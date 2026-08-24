//! KEL-105 regression: a window-phase app-process death must not exit `keld dev` 0.
//!
//! # The defect this pins
//!
//! `run_dev` completes HELLO+CALL, then blocks in `run_hello_window_html` for the
//! whole window phase. When the supervised Bun child dies mid-window the host
//! observes nothing, and `keld dev` exits 0 with no diagnostic over a dead app.
//!
//! Surfacing only the crash-loop breaker is not a fix. `KELD-RUNTIME-002`
//! requires three crashes inside a 30s sliding window
//! (`RestartPolicy::default()` and `crash_times.retain`), so a single death —
//! or a slower crash cadence — leaves the terminal outcome a clean `Stopped`
//! while the developer has no running app. Whether the breaker happens to trip
//! depends on how fast the restarted generations fail, which is timing the host
//! must not depend on.
//!
//! This test therefore kills the app **once** and never manufactures a crash
//! loop: a fixture that supplies three crashes tests the breaker, not the
//! defect. The verdict must come from durable crash state instead.
//!
//! # The observable this test pins
//!
//! `run_dev_with_window` is `run_dev` with only the GUI injected, so the
//! KEL-105 seam — reap Bun, read the supervision verdict, choose the returned
//! status — runs here exactly as it ships. `main.rs` maps any `Err` to exit 1
//! (`docs/architecture/07-agent-experience.md` §7), so an `Err` here is the
//! process exiting non-zero. Asserting on the seam rather than on
//! `shutdown()` alone is deliberate: a fix that computes the verdict and then
//! drops it on the floor still returns `Ok(())` and would otherwise pass.
//!
//! Four requirements, each separately falsifiable:
//!
//! 1. **Not silently successful.** The run must be `Err` when the app process
//!    that completed HELLO died during the window phase.
//! 2. **Typed and registered.** `KELD-CORE-033`. `KELD-CORE-031` is deliberately
//!    not reused: its registered fix is "Re-run `keld doctor`", which is wrong
//!    here — doctor passes, and the developer needs the crash.
//! 3. **Diagnostic, not just a code.** The message must nest the owning
//!    `keld-runtime` error's own `Display` — its `KELD-RUNTIME-012` code and the
//!    captured stderr — rather than a third hand-written copy of that text
//!    (AGENTS.md principle 3).
//! 4. **One fix, not two.** The crash diagnostic must not be wrapped in
//!    `KELD-CLI-031`, whose registered fix ("re-run `keld doctor`") contradicts
//!    it.
//!
//! The verdict MUST be drain-independent. Shipping `run_dev` already drains
//! supervisor events before the window, so a fix that only re-reads the event
//! queue is not a fix; `keld-runtime` publishes the crash as durable ledger
//! state instead.
//!
//! # Scope
//!
//! This is the ticket's option (a), SURFACE. Option (b), RECOVER — minting a
//! fresh link generation so the restarted child can re-handshake — is KEL-96
//! AC5 and is human-gated; without it the restarted generation hangs, which is
//! precisely the condition documented above.

#![allow(clippy::expect_used)]

use std::fs;
use std::process::Command;

use keld_cli::create::create_project;
use keld_cli::dev::run_dev_with_window;

/// Stderr the fixture emits before it is killed, so the assertion that the
/// captured tail reaches the developer has something real to find.
const BREADCRUMB: &str = "kel105-app-stderr-breadcrumb";

/// Scaffolds the shipping template and prepends a pid breadcrumb.
///
/// The app itself is left stock on purpose: a hand-written fixture would prove
/// something about the fixture, not about the app `keld create` produces.
fn project_with_pid_breadcrumb(
    dir: &std::path::Path,
    name: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = create_project(dir, name).expect("create");
    let pid_path = root.join("kel105-app.pid");
    let main = root.join("src/main.ts");
    let scaffolded = fs::read_to_string(&main).expect("scaffolded main.ts");
    let pid_lit = pid_path.display().to_string();
    fs::write(
        &main,
        format!(
            "import {{ writeFileSync as kel105WritePid }} from \"node:fs\";\n\
             kel105WritePid({pid_lit:?}, String(process.pid));\n\
             console.error({BREADCRUMB:?});\n{scaffolded}"
        ),
    )
    .expect("write fixture main");
    (root, pid_path)
}

#[test]
fn window_phase_app_death_is_surfaced_not_reported_as_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("w{}", std::process::id());
    let (root, pid_path) = project_with_pid_breadcrumb(dir.path(), &name);

    let mut killed = None;
    let result = run_dev_with_window(&root, |_title, _html| {
        // Stands exactly where tao's `run_return` does: the host owns the
        // window here and observes nothing about the app process.
        let pid: u32 = fs::read_to_string(&pid_path)
            .expect("the app must record its pid before it reports ready")
            .trim()
            .parse()
            .expect("pid breadcrumb must be a number");
        assert!(
            process_is_alive(pid),
            "precondition: supervised Bun pid {pid} must be alive before the kill"
        );
        kill_process(pid);
        killed = Some(pid);
        Ok(())
    });

    let pid = killed.expect("the window phase must have run");
    let err = result.expect_err(
        "KEL-105 defect: the app process that completed HELLO died during the window \
         phase, yet `keld dev` returned success. `main.rs` exits 0 on `Ok`, so the \
         command is silently green over a dead app",
    );
    let msg = err.to_string();
    assert!(
        msg.contains("KELD-CORE-033"),
        "window-phase supervision failure must carry its own registered code: {msg}"
    );
    assert!(
        msg.contains("KELD-RUNTIME-012"),
        "must nest the owning keld-runtime error rather than restate it: {msg}"
    );
    assert!(
        msg.contains(BREADCRUMB),
        "must carry the captured stderr so the developer can see the crash: {msg}"
    );
    assert!(
        !msg.contains("KELD-CLI-031") && !msg.contains("keld doctor"),
        "the crash diagnostic must not be wrapped in a code whose registered fix \
         contradicts it: {msg}"
    );
    assert!(
        !process_is_alive(pid),
        "teardown must reap the app process; pid {pid} still live"
    );
}

/// Guard: the fix must not make every run a failure.
///
/// A supervised app process that is still healthy when the window closes is the
/// shipping success path (`run_dev` -> exit 0). Driven through the same seam,
/// so a fix that unconditionally reports failure fails here.
#[test]
fn healthy_window_phase_still_exits_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("s{}", std::process::id());
    let (root, pid_path) = project_with_pid_breadcrumb(dir.path(), &name);

    let mut observed = None;
    run_dev_with_window(&root, |_title, _html| {
        let pid: u32 = fs::read_to_string(&pid_path)
            .expect("pid breadcrumb")
            .trim()
            .parse()
            .expect("pid breadcrumb must be a number");
        assert!(process_is_alive(pid), "app must be live across the window");
        observed = Some(pid);
        Ok(())
    })
    .expect("a live app process across the window phase must exit 0");
    let pid = observed.expect("the window phase must have run");
    assert!(
        !process_is_alive(pid),
        "teardown must reap the app process; pid {pid} still live"
    );
}

/// Guard: a crash the supervisor *recovers* from before the app is ready stays
/// a success (KEL-70 AC1/AC3). Without this, "any crash fails the run" would
/// look like a valid fix for KEL-105 while silently breaking recovery.
#[test]
fn crash_recovered_before_ready_still_reports_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("r{}", std::process::id());
    let root = create_project(dir.path(), &name).expect("create");
    let marker = root.join("kel105-crash-once");
    let marker_lit = marker.display().to_string();
    let main = root.join("src/main.ts");
    let scaffolded = fs::read_to_string(&main).expect("scaffolded main.ts");
    fs::write(
        &main,
        format!(
            "import {{ existsSync as kel105Exists, writeFileSync as kel105Write }} from \"node:fs\";\n\
             if (!kel105Exists({marker_lit:?})) {{ kel105Write({marker_lit:?}, \"1\"); \
             console.error(\"kel105-crash-once\"); process.exit(1); }}\n{scaffolded}"
        ),
    )
    .expect("write once-crashing fixture");

    run_dev_with_window(&root, |_title, _html| Ok(()))
        .expect("a crash the supervisor recovered from before ready is not a failure");
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
