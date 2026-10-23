//! GH-1081 round-1 research only (never merged): E6/E7/F2 on Linux under Xvfb.
//! Same X11 window and renderer document across one real Bun crash, and the
//! retired generation's endpoint and token refused after the restart.

use crate::crash_recovery_r1_heartbeat::{Beat, Heartbeat, same_document};
use crate::recovery::accept_generation;
use crate::support::{
    PRODUCT_DEADLINE,
    control::{expect_ready_and_echoes, read_control_line},
    process::{ProcessIdentity, wait_child, wait_child_output, wait_process_identity_gone},
    project::{DARK_BG, ProductFixture},
};
use serde_json::Value;
use std::{
    fs,
    io::{BufReader, Read as _, Write as _},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::Instant,
};

pub(crate) fn deadline() -> Instant {
    Instant::now() + PRODUCT_DEADLINE
}

pub(crate) fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// X11 top-level windows whose title is this run's unique title, bound to the
/// host PID when the toolkit publishes `_NET_WM_PID`.
pub(crate) fn host_windows(script: &Path, host: u32, title: &str) -> (Vec<u64>, Value) {
    let output = Command::new("python3")
        .arg("-I")
        .arg(script)
        .output()
        .expect("run X11 census");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "X11 census: {output:?}");
    let rows: Value = serde_json::from_str(
        stdout
            .trim()
            .strip_prefix("X11_CENSUS ")
            .expect("X11 census record"),
    )
    .expect("X11 census JSON");
    let mut ids: Vec<u64> = rows
        .as_array()
        .expect("X11 rows")
        .iter()
        .filter(|row| row["name"] == title && (row["pid"].is_null() || row["pid"] == host))
        .map(|row| row["id"].as_u64().expect("X11 window id"))
        .collect();
    ids.sort_unstable();
    (ids, rows)
}

/// The retired token on the successor endpoint: EOF with no reply bytes.
pub(crate) fn probe_retired_token(old_link: &str, fresh_link: &str) -> Output {
    let probe = Command::new("bun")
        .args(["-e", include_str!("../../fixtures/retired_token_probe.mjs")])
        .env(
            "KELD_RECOVERY_PROBE_TRANSPORT",
            repo().join("packages/@keld/kipc/src/transport.ts"),
        )
        .env("KELD_RECOVERY_PROBE_OLD", old_link)
        .env("KELD_RECOVERY_PROBE_FRESH", fresh_link)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("isolated stale-token probe");
    wait_child_output(probe, deadline())
}

pub(crate) fn endpoint(link: &str) -> &str {
    link.rsplit_once('#').expect("endpoint#token app link").0
}

pub(crate) fn launch(stage: &keld_cli::boot::DevBootStage, control: &Path) -> Child {
    Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", control)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch gh1081 Linux host")
}

/// `READY ECHO1 ECHO2`, or the host's own stderr when the generation never
/// reaches them (the host's diagnostic is the evidence, not the read timeout).
pub(crate) fn ready_or_host_failure(reader: &mut BufReader<UnixStream>, host: &mut Child) {
    let ready = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        expect_ready_and_echoes(reader);
    }));
    if let Err(failure) = ready {
        let _ = host.kill();
        let _ = host.wait();
        let mut stderr = String::new();
        if let Some(mut pipe) = host.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        eprintln!("KELD_GH1081_E6_HOST_STDERR {stderr}");
        std::panic::resume_unwind(failure);
    }
}

/// A product fixture whose window title is unique to this run, so the X11
/// census cannot match a concurrent test's host window.
pub(crate) fn unique_title_fixture() -> (ProductFixture, String, PathBuf) {
    titled(ProductFixture::new())
}

/// GH-1081 round 2: give an already-built product fixture the unique title.
pub(crate) fn titled(fixture: ProductFixture) -> (ProductFixture, String, PathBuf) {
    let leaf = fixture
        .root
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("UTF-8 fixture leaf")
        .to_owned();
    let title = format!("GH1081 Linux {}-{leaf}", std::process::id());
    fs::write(
        fixture.project.join("keld.config.ts"),
        format!(
            "export default {{\n  name: {title:?},\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
        ),
    )
    .expect("unique-title config");
    let census = fixture.root.path().join("gh1081-x11-census.py");
    fs::write(
        &census,
        include_str!("../../fixtures/crash_recovery_r1_x11_windows.py"),
    )
    .expect("write X11 census");
    (fixture, title, census)
}

pub(crate) fn stage(fixture: &ProductFixture) -> keld_cli::boot::DevBootStage {
    keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("stage gh1081 Linux host")
}

pub(crate) fn quit_generation(
    mut reader: BufReader<UnixStream>,
    mut writer: UnixStream,
    host: &mut Child,
    bun: &ProcessIdentity,
) {
    writer.write_all(b"QUIT\n").expect("request Quit");
    writer.flush().expect("flush Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = wait_child(host, deadline());
    assert!(status.success(), "gh1081 host exited with {status}");
    wait_process_identity_gone(bun, deadline());
}

/// Negative control for the document oracle: a fresh host launch's first beat.
fn relaunch_document(
    fixture: &ProductFixture,
    control: &Path,
    listener: &UnixListener,
    heartbeat: &mut Heartbeat,
) -> Beat {
    let staged = stage(fixture);
    let mut host = launch(&staged, control);
    let (mut reader, writer, generation, _) = accept_generation(listener, &mut host);
    ready_or_host_failure(&mut reader, &mut host);
    let relaunched = heartbeat.next(deadline());
    quit_generation(reader, writer, &mut host, &generation.bun);
    relaunched
}

/// Negative controls for both refusal probes: test-owned live listeners must be
/// reported as accepting. The kernel backlog completes a connect without an
/// accept, so the endpoint listener needs no thread.
fn live_listener_controls(root: &Path, old_link: &str) -> (bool, Output) {
    let live_path = root.join("gh1081-live.sock");
    let live = UnixListener::bind(&live_path).expect("bind live listener");
    let refused = UnixStream::connect(&live_path).is_err();
    drop(live);
    let reply_path = root.join("gh1081-reply.sock");
    let replying = UnixListener::bind(&reply_path).expect("bind replying listener");
    let reply_link = format!("{}#{}", reply_path.display(), "00".repeat(32));
    let replier = std::thread::spawn(move || {
        let (mut stream, _) = replying.accept().expect("accept one probe");
        let mut hello = [0_u8; 16];
        let _ = stream.read(&mut hello);
        let _ = stream.write_all(b"KI-reply-bytes");
    });
    let probed = probe_retired_token(old_link, &reply_link);
    // Releases the accept if the probe never connected; refused once it did.
    let _ = UnixStream::connect(&reply_path);
    replier.join().expect("replying listener thread");
    (refused, probed)
}

/// E6 + E7 + F2. Negative controls: a host relaunch must fail the document
/// oracle, and both refusal probes must detect a live, replying listener.
#[test]
fn gh1081_e6_e7_f2_linux_window_document_and_stale_authority() {
    let (fixture, title, census) = unique_title_fixture();
    let mut heartbeat = Heartbeat::bind();
    fs::write(
        fixture.project.join("index.html"),
        heartbeat.page(&title, DARK_BG),
    )
    .expect("heartbeat renderer");
    let control_path = fixture.root.path().join("gh1081-control.sock");
    let listener = UnixListener::bind(&control_path).expect("bind gh1081 control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking gh1081 control");
    let staged = stage(&fixture);
    let mut host = launch(&staged, &control_path);
    let host_pid = host.id();
    let (mut g1_reader, mut g1_writer, g1, g1_link) = accept_generation(&listener, &mut host);
    ready_or_host_failure(&mut g1_reader, &mut host);
    let initial = heartbeat.next(deadline());
    assert_eq!(initial.seq, 0);
    let before = heartbeat.poll(deadline());
    let (windows_before, rows_before) = host_windows(&census, host_pid, &title);

    g1_writer.write_all(b"CRASH\n").expect("crash g1");
    g1_writer.flush().expect("flush g1 crash");
    let started = Instant::now();
    drop((g1_reader, g1_writer));
    wait_process_identity_gone(&g1.bun, deadline());
    wait_process_identity_gone(&g1.descendant, deadline());
    let (mut g2_reader, g2_writer, g2, g2_link) = accept_generation(&listener, &mut host);
    ready_or_host_failure(&mut g2_reader, &mut host);
    let to_ready = started.elapsed();
    let after = heartbeat.poll(deadline());
    let (windows_after, rows_after) = host_windows(&census, host_pid, &title);
    let documents = heartbeat.documents();
    let verdict = same_document(&before, &after);
    // F2: the retired generation's authority after the restart.
    let old_endpoint_refused = UnixStream::connect(endpoint(&g1_link)).is_err();
    let stale_token = probe_retired_token(&g1_link, &g2_link);
    eprintln!(
        "KELD_GH1081_E6 host={host_pid} first_bun={} second_bun={} crash_to_successor_ready_ms={} before={before:?} after={after:?} documents={documents} session_storage={} windows_before={windows_before:?} windows_after={windows_after:?} x11_before={rows_before} x11_after={rows_after} old_endpoint_refused={old_endpoint_refused} stale_token_status={:?} stale_token_stdout={:?} stale_token_stderr={:?} verdict={verdict:?}",
        g1.bun.pid,
        g2.bun.pid,
        to_ready.as_millis(),
        after.storage,
        stale_token.status,
        String::from_utf8_lossy(&stale_token.stdout),
        String::from_utf8_lossy(&stale_token.stderr),
    );
    quit_generation(g2_reader, g2_writer, &mut host, &g2.bun);

    let relaunched = relaunch_document(&fixture, &control_path, &listener, &mut heartbeat);
    let control_verdict = same_document(&before, &relaunched);
    let (live_endpoint_refused, live_token) = live_listener_controls(fixture.root.path(), &g1_link);
    eprintln!(
        "KELD_GH1081_F2_NC relaunched={relaunched:?} verdict={control_verdict:?} live_endpoint_refused={live_endpoint_refused} live_token_status={:?} live_token_stderr={:?}",
        live_token.status,
        String::from_utf8_lossy(&live_token.stderr),
    );

    verdict.expect("E6: same renderer document across the Bun crash");
    assert_eq!(after.seq, before.seq + 1, "heartbeat sequence continued");
    assert_eq!(documents, 1, "E6: exactly one page-ready across the crash");
    assert!(control_verdict.is_err(), "NC: a relaunch passed the oracle");
    assert_ne!(g1.bun.pid, g2.bun.pid, "fresh Bun process");
    assert_ne!(g1_link, g2_link, "fresh app-link authority");
    assert!(
        old_endpoint_refused,
        "F2: retired endpoint accepted a connect"
    );
    // Observed in round 1 (10/10): the successor's endpoint is already gone
    // after its HELLO, so the probe cannot connect at all. Either refusal form
    // passes; a connected HELLO that receives reply bytes fails.
    let token_refused = (stale_token.status.success()
        && stale_token.stdout == b"RETIRED_TOKEN_SERVER_EOF_NO_REPLY\n")
        || (!stale_token.status.success()
            && String::from_utf8_lossy(&stale_token.stderr).contains("ENOENT"));
    assert!(
        token_refused,
        "F2: retired token neither refused at HELLO nor without a listener"
    );
    assert!(
        !live_endpoint_refused,
        "NC: endpoint probe cannot see a live listener"
    );
    assert!(
        !live_token.status.success()
            && String::from_utf8_lossy(&live_token.stderr)
                .contains("retired token received server reply bytes"),
        "NC: token probe cannot detect reply bytes: {live_token:?}"
    );
    // E7 last, so the document and authority verdicts are recorded first.
    assert!(
        !windows_before.is_empty(),
        "E7: no X11 window bound to the host title/PID: {rows_before}"
    );
    assert_eq!(windows_before, windows_after, "E7: X11 window id changed");
}
