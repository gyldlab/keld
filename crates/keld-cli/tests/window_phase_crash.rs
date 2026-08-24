//! KEL-105 regression: a window-phase app-process death must not exit `keld dev` 0.
//!
//! # The defect this pins
//!
//! `run_dev` (`crates/keld-cli/src/dev.rs:185`) completes HELLO+CALL, then blocks
//! in `run_hello_window_html` for the whole window phase. When the supervised Bun
//! child dies mid-window the host observes nothing: the echo listener admits
//! exactly one authenticated session (`crates/keld-core/src/echo_link.rs:75`), so
//! every restart fails and the crash-loop breaker trips, but
//! `HostOwnedHelloSession::finish` drops the `Supervisor` — and with it the whole
//! terminal-outcome record — unread (`crates/keld-core/src/hello_session.rs:202`).
//! `shutdown()` is hard-coded `Ok(())` (`hello_session.rs:196`), so the `?` at
//! `dev.rs:208` is dead, `run_dev` returns the window's `Ok(())`, and `main.rs`
//! exits 0 with no diagnostic over a dead app process.
//!
//! # The observable this test pins
//!
//! `HostOwnedHelloSession::shutdown()` is the surfacing point, because it is the
//! call `run_dev` already makes at exactly the right moment — after the window
//! returns and *before* `finish()` destroys the supervisor — and its own doc
//! already anticipates becoming fallible in fact ("currently always `Ok`").
//! Making it report the terminal supervision outcome revives the existing `?`,
//! and `main.rs:88` already maps any `Err` to exit 1
//! (`docs/architecture/07-agent-experience.md` §7). No new exit code is needed.
//!
//! Three requirements, each separately falsifiable:
//!
//! 1. **Not silently successful.** `shutdown()` must be `Err` when the app
//!    process that completed HELLO died during the window phase.
//! 2. **Typed and registered.** The message must carry `KELD-CORE-033`, a new
//!    code. `KELD-CORE-031` is deliberately *not* reused: its registered fix is
//!    "Re-run `keld doctor` and fix the reported checks"
//!    (`docs/engineering/keld-error-codes.md`), which is wrong here — doctor
//!    passes, and the developer needs the crash instead.
//! 3. **Diagnostic, not just a code.** The message must nest the owning
//!    `keld-runtime` error's own `Display` — its `KELD-RUNTIME-002` code and its
//!    captured stderr tail — rather than a third hand-written copy of that text
//!    (AGENTS.md principle 3; `hello_session.rs:167` is the second copy).
//!
//! The surfaced outcome MUST be drain-independent. Shipping `run_dev` already
//! drains supervisor events before the window (`dev.rs:196`), and this test
//! drains again to await the terminal state without sleeping, so a fix that only
//! re-reads the event queue is not a fix. `keld-runtime` contracts the
//! drain-independent path (`Supervisor::wait_for_outcome`, whose Arc-backed
//! fallback is asserted by its own
//! `draining_crash_loop_event_does_not_erase_terminal_outcome`).
//!
//! # Scope
//!
//! This is the ticket's option (a), SURFACE. Option (b), RECOVER — minting a
//! fresh link generation so the restarted child can re-handshake — is KEL-96 AC5
//! and is deliberately kept out of the test's critical path: the fixture's
//! restarted generations refuse to connect rather than blocking on the retired
//! listener.

#![allow(clippy::expect_used)]

use std::fs;
use std::process::Command;
use std::time::Duration;

use keld_cli::create::create_project;
use keld_cli::dev::start_dev_session;

/// A needle no generation of any fixture here ever prints.
///
/// `wait_until_output_contains` returns early on a terminal supervisor event,
/// so waiting on an unprintable needle is how this test *awaits* the crash-loop
/// breaker instead of sleep-polling process state (AGENTS.md anti-flake).
const NEVER_PRINTED: &str = "kel105-marker-no-child-ever-prints";

#[test]
fn window_phase_app_death_is_surfaced_not_reported_as_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("w{}", std::process::id());
    let root = create_project(dir.path(), &name).expect("create");
    let refuse_marker = root.join("kel105-refuse-restart");
    let refuse_lit = refuse_marker.display().to_string();

    fs::write(
        root.join("src/main.ts"),
        format!(
            r#"
import {{ AppLinkSession }} from "./kipc";
import {{ existsSync }} from "node:fs";

// KEL-105 fixture. The host-owned echo listener admits exactly one
// authenticated session, so a supervisor-restarted generation can never
// re-handshake -- it would block on the retired listener instead of exiting.
// Refusing here keeps the crash loop deterministic and keeps that separate
// defect (ticket option (b) / KEL-96 AC5) out of this test's critical path.
const refuse = {refuse_lit:?};
if (existsSync(refuse)) {{
  console.error("kel105-restart-refused");
  process.exit(3);
}}

const link = process.env.KELD_APP_LINK;
if (!link) {{
  console.error("kel105-app-link-unset");
  process.exit(1);
}}
const session = await AppLinkSession.connect(link);
const response = await session.echo({{ message: "kel105", count: 1 }});
console.log(`ipc-echo ok: message=${{JSON.stringify(response.message)}} count=${{response.count}}`);
console.log("kel105-window-phase-ready");
// Park exactly like the shipping template: the host owns the window from here.
await new Promise(() => {{}});
"#
        ),
    )
    .expect("overwrite main");

    let session = start_dev_session(&root).expect("start host-owned session");
    session
        .wait_until_output_contains("kel105-window-phase-ready", Duration::from_secs(30))
        .expect("HELLO + CALL must complete before the window phase opens");
    let pid = session
        .current_pid()
        .expect("supervised Bun must be live once the window phase opens");
    assert!(
        process_is_alive(pid),
        "precondition: supervised Bun pid {pid} must be alive before the kill"
    );

    // ---- window-phase stand-in begins; shipping `run_dev` blocks in
    // `run_hello_window_html` across exactly this region (dev.rs:205). ----
    fs::write(&refuse_marker, "1").expect("restart-refusal marker");
    kill_process(pid);

    // Await the terminal supervision state; no sleep, no process polling.
    let terminal = session
        .wait_until_output_contains(NEVER_PRINTED, Duration::from_secs(30))
        .expect_err("supervision must reach a terminal state after the window-phase kill");
    let terminal = terminal.to_string();
    assert!(
        terminal.contains("KELD-RUNTIME-002"),
        "harness precondition failed: expected the crash-loop breaker to trip, got: {terminal}"
    );
    assert!(
        terminal.contains("kel105-restart-refused"),
        "harness precondition failed: restarted generations must have refused and crashed, got: {terminal}"
    );
    assert_eq!(
        session.current_pid(),
        None,
        "harness precondition failed: no app process may survive the crash loop"
    );
    // ---- window-phase stand-in ends; shipping `run_dev` runs `session.shutdown()?`
    // here (dev.rs:208) and returns the window's own result. ----

    let err = session.shutdown().expect_err(
        "KEL-105 defect: the app process that completed HELLO died during the window \
         phase and the crash-loop breaker tripped, yet the host reported success. \
         `run_dev` propagates this result (crates/keld-cli/src/dev.rs:208) and \
         `main.rs:88` exits 0, so `keld dev` is silently green over a dead app",
    );
    let msg = err.to_string();
    assert!(
        msg.contains("KELD-CORE-033"),
        "window-phase supervision failure must carry its own registered code \
         (KELD-CORE-031's registered fix, `re-run keld doctor`, is wrong here): {msg}"
    );
    assert!(
        msg.contains("KELD-RUNTIME-002"),
        "must nest the owning keld-runtime error rather than restate it: {msg}"
    );
    assert!(
        msg.contains("kel105-restart-refused"),
        "must carry the captured stderr so the developer can see the crash: {msg}"
    );
}

/// Guard: the fix must not make every teardown a failure.
///
/// A supervised app process that is still healthy when the window closes is the
/// shipping success path (`run_dev` -> exit 0). This passes on unfixed `main`
/// and must keep passing after the fix.
#[test]
fn healthy_window_phase_still_reports_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("h{}", std::process::id());
    let root = create_project(dir.path(), &name).expect("create");

    let session = start_dev_session(&root).expect("start host-owned session");
    let ready = format!("{name}: main process ready (IPC echo ok)");
    session
        .wait_until_output_contains(&ready, Duration::from_secs(30))
        .expect("stock template must complete HELLO + CALL");
    let pid = session
        .current_pid()
        .expect("supervised Bun must be live once the window phase opens");

    // Window-phase stand-in: nothing crashes.
    assert!(
        process_is_alive(pid),
        "precondition: supervised Bun pid {pid} must be alive across the window phase"
    );

    session
        .shutdown()
        .expect("a live app process across the window phase must not be reported as a failure");
    assert!(
        !process_is_alive(pid),
        "shutdown must reap Bun; pid {pid} still live"
    );
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        // A reaped pid makes `kill -0` write "No such process"; that is the
        // expected answer here, not test output worth printing.
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use std::os::windows::process::CommandExt;
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .creation_flags(0x0800_0000)
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()),
        Err(_) => false,
    }
}

/// Kills the supervised child the way an app crash would: uncatchable, no
/// cooperation from the fixture, so the supervisor observes a real non-zero
/// (Unix: signalled) exit.
#[cfg(unix)]
fn kill_process(pid: u32) {
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .expect("spawn kill -9");
    assert!(status.success(), "kill -9 {pid} failed: {status}");
}

/// Kills the supervised child the way an app crash would: uncatchable, no
/// cooperation from the fixture, so the supervisor observes a real non-zero exit.
#[cfg(windows)]
fn kill_process(pid: u32) {
    use std::os::windows::process::CommandExt;
    let status = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .creation_flags(0x0800_0000)
        .status()
        .expect("spawn taskkill");
    assert!(status.success(), "taskkill /F /PID {pid} failed: {status}");
}
