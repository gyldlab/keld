use crate::support::DARK_BG;
use crate::support::EVENT_DEADLINE;
use crate::support::TITLE;
use crate::support::control::ReapOnDrop;
use crate::support::control::accept_before;
use crate::support::control::parse_pid;
use crate::support::control::read_control_line;
use crate::support::control::wait_child_output;
use crate::support::native_window::{await_no_native_windows, native_windows};
use crate::support::process::{await_process_gone, process_exists, session_dirs_for};
use crate::support::product::ProductFixture;
use crate::support::renderer::NavigationBlocker;
use std::cell::Cell;
use std::fs;
use std::io::BufReader;
use std::net::TcpStream;
use std::os::unix::net::UnixListener;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

#[test]
fn stalled_initial_navigation_rolls_back_window_link_and_process_group() {
    let fixture = ProductFixture::new("navigation-timeout");
    let mut blocker = NavigationBlocker::bind();
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{TITLE}</title><img src=\"http://127.0.0.1:{}/never\">\n",
            blocker.port
        ),
    )
    .expect("stalled renderer");
    let stage = fixture.stage();
    let control_path = fixture.root.path().join("navigation-timeout.sock");
    let listener = UnixListener::bind(&control_path).expect("bind navigation control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking navigation control");
    let mut child = ReapOnDrop::new(
        Command::new(stage.host())
            .env("KELD_DEV_LEASE", "stdin-v1")
            .env("KELD_T1B_CONTROL", &control_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch navigation-timeout host"),
    );
    // Hold the dev lease throughout the original Host exit wait. Closing it
    // before WK's 15s navigation deadline initiates ordinary successful Quit.
    let _dev_lease_writer = child.child_mut().stdin.take();
    let host_pid = child.child_mut().id();
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("control deadline");
    let mut observations = BufReader::new(control);
    let hello = read_control_line(&mut observations);
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let bun_pid = parse_pid(fields.next(), &hello);
    let app_link = fields.next().expect("navigation app link");
    let session_dir = PathBuf::from(app_link.rsplit_once('#').expect("app link token").0)
        .parent()
        .expect("session directory")
        .to_path_buf();
    let descendant = read_control_line(&mut observations);
    let descendant_pid = parse_pid(descendant.split_whitespace().nth(1), &descendant);
    blocker
        .connected
        .recv_timeout(EVENT_DEADLINE)
        .expect("WKWebView requested stalled resource");
    await_no_native_windows(host_pid, TITLE);
    // The Child reaper inside wait_child_output remains authoritative. Even
    // when its original deadline panics, release and join our socket owner
    // before resuming the first failure with its original diagnostics.
    let host_exit = catch_unwind(AssertUnwindSafe(|| child.wait_output(EVENT_DEADLINE)));
    let join_result = blocker.stop_and_join();
    let output = host_exit.unwrap_or_else(|failure| resume_unwind(failure));
    join_result.expect("navigation blocker joins");

    assert!(!output.status.success(), "stalled navigation became Ready");
    let stderr = String::from_utf8(output.stderr).expect("navigation stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(stderr.contains("initial renderer navigation"), "{stderr}");
    await_process_gone(bun_pid);
    await_process_gone(descendant_pid);
    assert!(
        !session_dir.exists(),
        "navigation rollback left app-link locator"
    );
    assert!(
        native_windows(host_pid, TITLE).is_empty(),
        "navigation rollback left native window"
    );
}

#[test]
fn initial_navigation_pre_wait_panic_reaps_owned_child_and_joins_blocker() {
    // Real native subprocess and socket cleanup before and after accepted
    // navigation. Neither panic may transfer an unowned Child or worker.
    for accepted in [false, true] {
        let host_pid = Cell::new(None);
        let blocked_port = Cell::new(None);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let blocker = NavigationBlocker::bind();
            blocked_port.set(Some(blocker.port));
            let mut child = ReapOnDrop::new(
                Command::new("/bin/sleep")
                    .arg("2")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("native disposable pre-wait child"),
            );
            host_pid.set(Some(child.child_mut().id()));
            if accepted {
                let _peer = TcpStream::connect(("127.0.0.1", blocker.port))
                    .expect("native accepted navigation socket");
                blocker
                    .connected
                    .recv_timeout(EVENT_DEADLINE)
                    .expect("accepted native socket before injected unwind");
            }
            panic!("intentional pre-wait failure");
        }));
        assert_eq!(
            result
                .expect_err("injected pre-wait panic must propagate")
                .downcast_ref::<&str>(),
            Some(&"intentional pre-wait failure")
        );
        assert!(
            !process_exists(host_pid.get().expect("owned child identity")),
            "pre-wait unwind abandoned a live Child"
        );
        assert!(
            TcpStream::connect(("127.0.0.1", blocked_port.get().expect("owned fixture port")))
                .is_err(),
            "pre-wait unwind left a detached navigation accept listener"
        );
    }
}

#[test]
fn pre_ready_bun_crash_is_startup_failure_not_a_recovered_window() {
    let fixture = ProductFixture::new("t3-pre-ready-crash");
    let stage = fixture.stage();
    let attempt_marker = fixture.root.path().join("pre-ready-attempt");
    let mut child = Command::new(stage.host())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T3_CRASH_BEFORE_HELLO", "1")
        .env("KELD_T3_PRE_READY_MARKER", &attempt_marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch pre-Ready crash host");
    let _dev_lease_writer = child.stdin.take();
    let host_pid = child.id();
    let output = wait_child_output(child, EVENT_DEADLINE);
    assert!(!output.status.success(), "pre-Ready crash became success");
    let stderr = String::from_utf8(output.stderr).expect("pre-Ready stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(
        stderr.contains("before its initial authenticated generation bound"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("KELD-RUNTIME-002"),
        "pre-Ready crash restarted to breaker: {stderr}"
    );
    assert_eq!(native_windows(host_pid, TITLE), [] as [u32; 0]);
    assert_eq!(session_dirs_for(host_pid), [] as [std::path::PathBuf; 0]);
    let attempts = fs::read_dir(fixture.root.path())
        .expect("list pre-Ready attempts")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("pre-ready-attempt.")
        })
        .count();
    assert_eq!(attempts, 1, "pre-Ready failure provisioned a successor");
}

#[test]
fn dev_lease_loss_before_initial_bound_is_an_orderly_startup_cancellation() {
    let fixture = ProductFixture::new("lease-loss-before-bound");
    let stage = fixture.stage();
    let mut child = Command::new(stage.host())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch leased host");
    let host_pid = child.id();
    // The CLI lease ends before the host can register its Guardian, so the
    // first unbound-generation poll observes the loss. Before the Guardian's
    // first PREPARE is admitted, only an orderly host S has an acknowledgment.
    drop(child.stdin.take());
    let output = wait_child_output(child, EVENT_DEADLINE);
    let stderr = String::from_utf8(output.stderr).expect("cancellation stderr UTF-8");
    assert!(
        output.status.success(),
        "lease loss before Bound was not an orderly cancellation: {stderr}"
    );
    assert!(!stderr.contains("KELD-"), "{stderr}");
    assert_eq!(native_windows(host_pid, TITLE), [] as [u32; 0]);
    assert_eq!(session_dirs_for(host_pid), [] as [std::path::PathBuf; 0]);
}
