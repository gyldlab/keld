use crate::support::DARK_BG;
use crate::support::EVENT_DEADLINE;
use crate::support::TITLE;
use crate::support::control::accept_before;
use crate::support::control::parse_pid;
use crate::support::control::read_control_line;
use crate::support::control::wait_child_output;
use crate::support::native_window::{await_no_native_windows, native_windows};
use crate::support::process::{await_process_gone, session_dirs_for};
use crate::support::product::ProductFixture;
use crate::support::renderer::NavigationBlocker;
use std::fs;
use std::io::BufReader;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

#[test]
fn stalled_initial_navigation_rolls_back_window_link_and_process_group() {
    let fixture = ProductFixture::new("navigation-timeout");
    let blocker = NavigationBlocker::bind();
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
    let mut child = Command::new(stage.host())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T1B_CONTROL", &control_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch navigation-timeout host");
    let dev_lease_writer = child.stdin.take();
    let host_pid = child.id();
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
    drop(dev_lease_writer);
    let output = wait_child_output(child, EVENT_DEADLINE);
    blocker
        .release
        .send(())
        .expect("release blocked navigation");
    blocker.handle.join().expect("navigation blocker joins");

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
    assert!(native_windows(host_pid, TITLE).is_empty());
    assert!(session_dirs_for(host_pid).is_empty());
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
