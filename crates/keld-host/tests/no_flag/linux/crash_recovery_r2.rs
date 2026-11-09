//! GH-1081 round-2 research only (never merged), Linux under Xvfb.
//! (1) Reload of the stock scaffold page in the staged no-flag host: page-JS
//! `location.reload()`, F5, Ctrl+R and the WebKitGTK context-menu Reload, with a
//! real-URL reload as the negative control. (2) The retired generation's token
//! presented to the live, pre-HELLO successor listener. (3) An OS-typed draft
//! across a real Bun crash. No production hook: input is XTEST via xdotool,
//! observation is the page's own reports plus read-only X11 pixels.

use crate::crash_recovery_r1::{
    deadline, endpoint, host_windows, launch, probe_retired_token, quit_generation,
    ready_or_host_failure, stage, titled, unique_title_fixture,
};
use crate::crash_recovery_r2_probe::{
    DRAFT, Probe, await_successor, is_page_fetch, is_unload, same_document, scaffold_html,
};
use crate::recovery::accept_generation;
use crate::support::{
    PRODUCT_DEADLINE,
    control::{
        accept_control_or_host_failure, assert_nonzero_descendant, expect_ready_and_echoes,
        read_control_line,
    },
    process::{StrictGeneration, wait_for_strict_generation},
    project::ProductFixture,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufReader, Write as _},
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};

/// Bound for "a reloaded document reports": the real-URL control's successor
/// arrives far inside it (printed per iteration as `control_reload_ms`).
const SUCCESSOR_BOUND: Duration = Duration::from_secs(10);

fn census_json(script: &Path, mode: &[&str], prefix: &str) -> Value {
    let output = Command::new("python3")
        .arg("-I")
        .arg(script)
        .args(mode)
        .output()
        .expect("run X11 probe");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "X11 probe {mode:?}: {output:?}");
    serde_json::from_str(
        stdout
            .trim()
            .strip_prefix(prefix)
            .unwrap_or_else(|| panic!("X11 probe record {prefix}: {stdout}")),
    )
    .expect("X11 probe JSON")
}

fn pixels(script: &Path, window: u64) -> Value {
    census_json(script, &["pixels", &window.to_string()], "X11_PIXELS ")
}

fn popups(script: &Path) -> Vec<u64> {
    census_json(script, &["popups"], "X11_POPUPS ")
        .as_array()
        .expect("popup rows")
        .iter()
        .filter_map(|row| row["id"].as_u64())
        .collect()
}

/// One `xdotool` invocation (XTEST input to the focused window under Xvfb).
fn xdotool(args: &[&str]) -> Value {
    let output = Command::new("xdotool")
        .args(args)
        .output()
        .expect("run xdotool (installed by the GH-1081 workflow step)");
    json!({"args": args, "status": output.status.code(),
        "stderr": String::from_utf8_lossy(&output.stderr)})
}

/// Focus the host window and click its centre (a real XTEST pointer click).
fn focus_and_click(window: u64, geometry: &Value, button: &str) -> Vec<Value> {
    let id = window.to_string();
    let x = (geometry["width"].as_u64().unwrap_or(400) / 2).to_string();
    let y = (geometry["height"].as_u64().unwrap_or(300) / 2).to_string();
    vec![
        xdotool(&["windowfocus", &id]),
        xdotool(&["mousemove", "--window", &id, &x, &y]),
        xdotool(&["click", button]),
    ]
}

#[derive(Clone, Copy, Debug)]
enum Arm {
    JsReload,
    Key(&'static str),
    ContextMenu,
    RealUrlControl,
}

impl Arm {
    fn plans(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            Self::JsReload => (&["beat", "reload"], &[]),
            Self::Key(_) | Self::ContextMenu => (&[], &[]),
            Self::RealUrlControl => (&["real"], &["beat", "reload"]),
        }
    }
}

/// One fresh host launch, one reload trigger, every observation printed.
fn reload_arm(scaffold: &str, arm: Arm) -> Value {
    let (fixture, title, census) = unique_title_fixture();
    let (stage_plan, real_plan) = arm.plans();
    let mut probe = Probe::bind(scaffold, stage_plan, real_plan, false);
    fs::write(fixture.project.join("index.html"), probe.page()).expect("probe renderer");
    let control_path = fixture.root.path().join("gh1081-r2-control.sock");
    let listener = UnixListener::bind(&control_path).expect("bind r2 control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking r2 control");
    let staged = stage(&fixture);
    let mut host = launch(&staged, &control_path);
    let host_pid = host.id();
    let (mut reader, mut writer, generation, _) = accept_generation(&listener, &mut host);
    ready_or_host_failure(&mut reader, &mut host);
    let first = probe.beat(deadline());
    let (windows, rows) = host_windows(&census, host_pid, &title);
    let window = *windows
        .first()
        .unwrap_or_else(|| panic!("no titled X11 window: {rows}"));
    let mut current = first.clone();
    let mut inputs = Vec::new();
    let mut real_page = Value::Null;
    if let Arm::RealUrlControl = arm {
        // Release beat 0: the stage page navigates to the real http:// URL.
        probe.release();
        real_page = await_successor(&mut probe, first["doc"].as_str().unwrap_or(""), deadline())
            .expect("real-URL control page reported");
        current = real_page.clone();
    }
    // One released beat, so `before` is a live, idle, painted document.
    let before = probe.poll(deadline());
    let pixels_before = pixels(&census, window);
    let triggered = Instant::now();
    let mut popup_seen = Value::Null;
    match arm {
        Arm::JsReload | Arm::RealUrlControl => probe.release(),
        Arm::Key(key) => {
            inputs = focus_and_click(window, &pixels_before, "1");
            inputs.push(xdotool(&["key", key]));
        }
        Arm::ContextMenu => {
            let baseline = popups(&census);
            inputs = focus_and_click(window, &pixels_before, "3");
            let menu_deadline = Instant::now() + Duration::from_secs(5);
            let menu = loop {
                let now = popups(&census);
                if let Some(id) = now.iter().find(|id| !baseline.contains(id)) {
                    break Some(*id);
                }
                if Instant::now() >= menu_deadline {
                    break None;
                }
                std::thread::park_timeout(Duration::from_millis(20));
            };
            popup_seen = json!(menu);
            // GTK menus skip insensitive items (Back/Forward/Stop on a fresh
            // page): Down selects the first sensitive item, Reload.
            if menu.is_some() {
                inputs.push(xdotool(&["key", "Down", "Return"]));
            }
        }
    }
    let previous = current["doc"].as_str().unwrap_or("").to_owned();
    let successor = await_successor(&mut probe, &previous, triggered + SUCCESSOR_BOUND);
    let reload_ms = successor.as_ref().map(|_| triggered.elapsed().as_millis());
    // Settle remaining reports (unload beacons) without waiting further.
    while probe.try_next(Instant::now()).is_some() {}
    let pixels_after = pixels(&census, window);
    let (windows_after, _) = host_windows(&census, host_pid, &title);
    // Bun link health after the reload: one more echo on the same generation.
    writer.write_all(b"ECHO3\n").expect("request echo 3");
    writer.flush().expect("flush echo 3");
    let echo3 = read_control_line(&mut reader);
    let unloads = probe.count(is_unload);
    let fetches = probe.count(is_page_fetch);
    let record = json!({
        "arm": format!("{arm:?}"), "host": host_pid, "window": window,
        "windows_after": windows_after, "first": first, "real_page": real_page,
        "before": before, "pixels_before": pixels_before, "inputs": inputs,
        "popup": popup_seen, "successor": successor, "reload_ms": reload_ms,
        "unload_beacons": unloads, "real_page_fetches": fetches,
        "pixels_after": pixels_after, "echo3_after_reload": echo3,
        "documents": probe.documents(),
    });
    eprintln!("KELD_GH1081_R2_RELOAD {record}");
    quit_generation(reader, writer, &mut host, &generation.bun);
    record
}

/// Reload matrix on the stock scaffold page. The test asserts only the
/// oracle-validity facts (the window showed the dark scaffold before the
/// trigger; the control reloaded); every arm's outcome is recorded.
#[test]
fn gh1081_r2_linux_reload_matrix_on_scaffold_page() {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let arms = [
        Arm::RealUrlControl,
        Arm::JsReload,
        Arm::Key("F5"),
        Arm::Key("ctrl+r"),
        Arm::ContextMenu,
    ];
    let records: Vec<Value> = arms.iter().map(|arm| reload_arm(&scaffold, *arm)).collect();
    for record in &records {
        assert!(
            record["pixels_before"]["dark"].as_f64().unwrap_or(0.0) > 0.9,
            "oracle invalid: the scaffold page was not on screen before the trigger: {record}"
        );
        assert_eq!(record["echo3_after_reload"], "ECHO3", "{record}");
    }
    let control = &records[0];
    assert!(
        control["successor"]["real"] == true && control["successor"]["nav"] == "reload",
        "NC: the real-URL page did not reload into a new document: {control}"
    );
    assert_eq!(
        control["real_page_fetches"], 2,
        "NC: reload re-fetched the URL"
    );
}

fn hold_harness() -> String {
    let harness = include_str!("../../fixtures/t1b_harness.ts");
    let anchor = "const appSocket =\n";
    assert_eq!(harness.matches(anchor).count(), 1, "one app-link connect");
    harness.replacen(
        anchor,
        "// GH-1081 round 2 (research only): hold this generation before it\n\
         // connects its app-link, so its listener is live and unauthenticated.\n\
         await sendControl(`PRE_HELLO ${process.pid} ${KEL96_LINK}`);\n\
         if ((await receiveCommand()) !== \"GO\") throw new Error(\"GH1081 pre-HELLO hold\");\n\
         const appSocket =\n",
        1,
    )
}

/// A held generation: control accepted, `PRE_HELLO` read, nothing sent.
struct Held {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    link: String,
}

fn accept_held(listener: &UnixListener, host: &mut Child) -> Held {
    let control = accept_control_or_host_failure(listener, host, deadline());
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("held control deadline");
    let writer = control.try_clone().expect("held control writer");
    let mut reader = BufReader::new(control);
    let line = read_control_line(&mut reader);
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("PRE_HELLO"), "{line}");
    let _pid = fields.next().expect("held pid");
    let link = fields.next().expect("held link").to_owned();
    Held {
        reader,
        writer,
        link,
    }
}

fn release_held(
    mut held: Held,
    host: u32,
) -> (BufReader<UnixStream>, UnixStream, StrictGeneration) {
    held.writer
        .write_all(b"GO\n")
        .expect("release held generation");
    held.writer.flush().expect("flush GO");
    let hello = read_control_line(&mut held.reader);
    assert!(hello.starts_with("HELLO "), "{hello}");
    assert_nonzero_descendant(&read_control_line(&mut held.reader));
    let generation = wait_for_strict_generation(host, deadline());
    expect_ready_and_echoes(&mut held.reader);
    (held.reader, held.writer, generation)
}

/// F2 completion: G1's token presented to G2's live listener before G2's own
/// HELLO is refused (EOF, no reply bytes), and G2 then authenticates and
/// serves. NC (separate launch): G1's own token on G1's live listener is
/// accepted, and the same probe detects the reply bytes.
#[test]
fn gh1081_r2_linux_stale_token_refused_by_live_successor_listener() {
    let (fixture, _title, _census) = titled(ProductFixture::with_harness(&hold_harness()));
    let control_path = fixture.root.path().join("gh1081-r2-token.sock");
    let listener = UnixListener::bind(&control_path).expect("bind token control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking token control");
    let staged = stage(&fixture);
    let mut host = launch(&staged, &control_path);
    let host_pid = host.id();
    let g1_held = accept_held(&listener, &mut host);
    let g1_link = g1_held.link.clone();
    let (g1_reader, mut g1_writer, g1) = release_held(g1_held, host_pid);
    g1_writer.write_all(b"CRASH\n").expect("crash g1");
    g1_writer.flush().expect("flush crash");
    drop((g1_reader, g1_writer));
    crate::support::process::wait_process_identity_gone(&g1.bun, deadline());
    let g2_held = accept_held(&listener, &mut host);
    let g2_link = g2_held.link.clone();
    let g2_endpoint_live_before = Path::new(endpoint(&g2_link)).exists();
    let stale = probe_retired_token(&g1_link, &g2_link);
    let g2_endpoint_live_after_probe = Path::new(endpoint(&g2_link)).exists();
    let (mut g2_reader, mut g2_writer, g2) = release_held(g2_held, host_pid);
    g2_writer.write_all(b"ECHO3\n").expect("g2 echo 3");
    g2_writer.flush().expect("flush g2 echo 3");
    let g2_echo3 = read_control_line(&mut g2_reader);
    let g2_endpoint_after_hello = Path::new(endpoint(&g2_link)).exists();
    eprintln!(
        "KELD_GH1081_R2_TOKEN host={host_pid} g1_bun={} g2_bun={} g2_endpoint_live_before={g2_endpoint_live_before} g2_endpoint_live_after_probe={g2_endpoint_live_after_probe} g2_endpoint_after_hello={g2_endpoint_after_hello} stale_status={:?} stale_stdout={:?} stale_stderr={:?} g2_echo3={g2_echo3}",
        g1.bun.pid,
        g2.bun.pid,
        stale.status,
        String::from_utf8_lossy(&stale.stdout),
        String::from_utf8_lossy(&stale.stderr),
    );
    quit_generation(g2_reader, g2_writer, &mut host, &g2.bun);

    // NC: a valid token on a live listener must be detected as accepted.
    let staged = stage(&fixture);
    let mut nc_host = launch(&staged, &control_path);
    let nc_held = accept_held(&listener, &mut nc_host);
    let accepted = probe_retired_token(&nc_held.link, &nc_held.link);
    eprintln!(
        "KELD_GH1081_R2_TOKEN_NC status={:?} stdout={:?} stderr={:?}",
        accepted.status,
        String::from_utf8_lossy(&accepted.stdout),
        String::from_utf8_lossy(&accepted.stderr),
    );
    let _ = nc_host.kill();
    let _ = nc_host.wait();
    drop(nc_held);

    assert!(
        g2_endpoint_live_before,
        "G2 listener was not live while held"
    );
    assert!(
        stale.status.success() && stale.stdout == b"RETIRED_TOKEN_SERVER_EOF_NO_REPLY\n",
        "stale token not refused at HELLO by the live successor listener: {stale:?}"
    );
    assert_eq!(
        g2_echo3, "ECHO3",
        "successor serves after the refused probe"
    );
    assert!(
        !accepted.status.success()
            && String::from_utf8_lossy(&accepted.stderr)
                .contains("retired token received server reply bytes"),
        "NC: the probe cannot detect an accepted token: {accepted:?}"
    );
}

/// Task 3: real XTEST keystrokes into a textarea on the stock scaffold page,
/// read back by the page's own input reports, across one real Bun crash.
/// NC: a host relaunch is a new document with an empty draft.
#[test]
fn gh1081_r2_linux_typed_draft_survives_bun_crash() {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let (fixture, title, census) = unique_title_fixture();
    let mut probe = Probe::bind(&scaffold, &[], &[], true);
    fs::write(fixture.project.join("index.html"), probe.page()).expect("draft renderer");
    let control_path = fixture.root.path().join("gh1081-r2-draft.sock");
    let listener = UnixListener::bind(&control_path).expect("bind draft control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking draft control");
    let staged = stage(&fixture);
    let mut host = launch(&staged, &control_path);
    let host_pid = host.id();
    let (mut g1_reader, mut g1_writer, g1, _) = accept_generation(&listener, &mut host);
    ready_or_host_failure(&mut g1_reader, &mut host);
    let first = probe.beat(deadline());
    let (windows, rows) = host_windows(&census, host_pid, &title);
    let window = *windows
        .first()
        .unwrap_or_else(|| panic!("no titled X11 window: {rows}"));
    let geometry = pixels(&census, window);
    let mut inputs = focus_and_click(window, &geometry, "1");
    inputs.push(xdotool(&["type", "--delay", "25", DRAFT]));
    let typed = probe.await_draft(DRAFT, deadline());
    let before = probe.poll(deadline());
    g1_writer.write_all(b"CRASH\n").expect("crash g1");
    g1_writer.flush().expect("flush crash");
    let crashed = Instant::now();
    drop((g1_reader, g1_writer));
    crate::support::process::wait_process_identity_gone(&g1.bun, deadline());
    let (mut g2_reader, g2_writer, g2, _) = accept_generation(&listener, &mut host);
    ready_or_host_failure(&mut g2_reader, &mut host);
    let to_ready = crashed.elapsed();
    let after = probe.poll(deadline());
    let documents = probe.documents();
    let verdict = same_document(&before, &after);
    eprintln!(
        "KELD_GH1081_R2_DRAFT host={host_pid} g1_bun={} g2_bun={} crash_to_ready_ms={} first={first} typed={typed:?} before={before} after={after} documents={documents:?} inputs={inputs:?} verdict={verdict:?}",
        g1.bun.pid,
        g2.bun.pid,
        to_ready.as_millis()
    );
    quit_generation(g2_reader, g2_writer, &mut host, &g2.bun);
    let staged = stage(&fixture);
    let mut relaunch = launch(&staged, &control_path);
    let (mut r_reader, r_writer, r_gen, _) = accept_generation(&listener, &mut relaunch);
    ready_or_host_failure(&mut r_reader, &mut relaunch);
    let relaunched = await_successor(&mut probe, before["doc"].as_str().unwrap_or(""), deadline())
        .expect("relaunched document reported");
    let control_verdict = same_document(&before, &relaunched);
    eprintln!("KELD_GH1081_R2_DRAFT_NC relaunched={relaunched} verdict={control_verdict:?}");
    quit_generation(r_reader, r_writer, &mut relaunch, &r_gen.bun);

    assert!(
        typed.is_some(),
        "XTEST keystrokes did not reach the textarea: {inputs:?}"
    );
    assert_eq!(before["draft"], DRAFT, "draft before the crash");
    assert_eq!(
        before["trusted"].as_u64(),
        Some(DRAFT.len() as u64),
        "every input event was trusted: {before}"
    );
    verdict.expect("same renderer document across the Bun crash");
    assert_eq!(after["draft"], DRAFT, "draft intact after the Bun crash");
    assert_eq!(documents.len(), 1, "exactly one document across the crash");
    assert!(control_verdict.is_err(), "NC: a relaunch passed the oracle");
    assert_eq!(relaunched["draft"], "", "NC: relaunched draft is empty");
}
