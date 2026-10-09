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
//! AC5 and is human-gated. Without it no successor can be served: the
//! one-session listener consumed its locator when generation 1 authenticated,
//! so a successor's connect fails at once (`ENOENT`, `KELD-IPC-001`). Such
//! successors used to be provisioned anyway and crash-looped, so the breaker
//! tripped whenever the caller's teardown came late (GH-674). The session now
//! provisions none after its one admission, and
//! `a_window_phase_death_provisions_no_successor_the_listener_cannot_admit`
//! forces the late teardown.

#![allow(
    clippy::expect_used,
    reason = "These integration tests use expect to make process-lifecycle observations part of their assertion oracle"
)]

use std::fs;
use std::process::Command;

use keld_cli::create::create_project;
use keld_cli::dev::{run_dev_with_window, start_dev_session_with_stdout_markers};
use keld_core::HostOwnedHelloSession;
use std::time::{Duration, Instant};

/// Stderr the fixture emits before it is killed, so the assertion that the
/// captured tail reaches the developer has something real to find.
const BREADCRUMB: &str = "kel105-app-stderr-breadcrumb";

/// A stdout needle no generation prints (see [`await_supervision_end`]).
const NEVER_PRINTED: &str = "gh674-no-generation-prints-this";

/// Scaffolds the shipping template and prepends a pid breadcrumb.
///
/// The app itself is left stock on purpose: a hand-written fixture would prove
/// something about the fixture, not about the app `keld create` produces. The
/// returned generation path is written only by a generation that starts after
/// the first one, so its absence proves no successor was provisioned.
fn project_with_pid_breadcrumb(
    dir: &std::path::Path,
    name: &str,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let root = create_project(dir, name).expect("create");
    let pid_path = root.join("kel105-app.pid");
    let generation_path = root.join("kel105-generation-2");
    let main = root.join("src/main.ts");
    let scaffolded = fs::read_to_string(&main).expect("scaffolded main.ts");
    let pid_lit = pid_path.display().to_string();
    let generation_lit = generation_path.display().to_string();
    fs::write(
        &main,
        format!(
            "import {{ existsSync as kel105Exists, writeFileSync as kel105WritePid }} from \"node:fs\";\n\
             if (kel105Exists({pid_lit:?})) kel105WritePid({generation_lit:?}, \"restarted\");\n\
             kel105WritePid({pid_lit:?}, String(process.pid));\n\
             console.error({BREADCRUMB:?});\n{scaffolded}"
        ),
    )
    .expect("write fixture main");
    (root, pid_path, generation_path)
}

#[test]
fn window_phase_app_death_is_surfaced_not_reported_as_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("w{}", std::process::id());
    let (root, pid_path, generation_path) = project_with_pid_breadcrumb(dir.path(), &name);

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
        assert!(
            !generation_path.exists(),
            "restart marker must be absent before the kill"
        );
        kill_process(pid);
        // `kill -0` succeeds on a zombie, so a dead answer means the
        // supervisor already reaped the child, and its reaping thread records
        // the death before it reports a terminal outcome.
        let deadline = Instant::now() + Duration::from_secs(10);
        while process_is_alive(pid) {
            assert!(
                Instant::now() < deadline,
                "supervisor did not reap the killed generation"
            );
            std::thread::yield_now();
        }
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
    assert!(
        !generation_path.exists(),
        "no successor may be provisioned once the one-session listener is consumed"
    );
}

/// GH-674 regression: a window-phase death must not provision successors that
/// the one-session listener can never admit.
///
/// Generation 1's authentication consumed the listener's locator, so a
/// successor's connect fails at once (`KELD-IPC-001`, `ENOENT`). Before the
/// fix the supervisor restarted it anyway: two such crashes after the kill
/// tripped the breaker, so the verdict was `KELD-RUNTIME-002` whenever the
/// caller's teardown came more than about 60 ms (Apple M4) after the first
/// restart, and `KELD-RUNTIME-012` otherwise. Only the caller's timing decided.
///
/// This test forces the late teardown deterministically, without a sleep: it
/// tears down only after supervision has ended on its own, the latest teardown
/// there is. Before the fix that end is always the tripped breaker, so this
/// test fails every run; the immediate teardown of the test above was its
/// passing order. After the fix supervision stops at the one recorded crash.
#[test]
fn a_window_phase_death_provisions_no_successor_the_listener_cannot_admit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("g{}", std::process::id());
    let (root, pid_path, generation_path) = project_with_pid_breadcrumb(dir.path(), &name);
    let ready = format!("{name}: main process ready (IPC echo ok)");
    let session =
        start_dev_session_with_stdout_markers(&root, &[&ready]).expect("start host-owned session");
    session
        .wait_until_output_contains(&ready, Duration::from_secs(30))
        .expect("the stock template must complete HELLO + CALL");
    let pid: u32 = fs::read_to_string(&pid_path)
        .expect("the app must record its pid before it reports ready")
        .trim()
        .parse()
        .expect("pid breadcrumb must be a number");
    kill_process(pid);

    let ended = await_supervision_end(&session);
    assert!(
        ended.contains("KELD-RUNTIME-012") && !ended.contains("KELD-RUNTIME-002"),
        "supervision must stop at the killed generation's recorded crash, not a \
         crash loop of successors that cannot connect: {ended}"
    );

    let msg = session
        .shutdown()
        .expect_err("the app reported ready and then died")
        .to_string();
    assert!(msg.contains("KELD-CORE-033"), "{msg}");
    assert!(msg.contains("KELD-RUNTIME-012"), "{msg}");
    assert!(
        !msg.contains("KELD-RUNTIME-002"),
        "a late teardown must not turn one death into a crash loop: {msg}"
    );
    assert!(msg.contains(BREADCRUMB), "{msg}");
    assert!(
        !generation_path.exists(),
        "a successor was provisioned for a consumed one-session listener"
    );
    assert!(!process_is_alive(pid), "pid {pid} still live");
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
    let (root, pid_path, _generation_path) = project_with_pid_breadcrumb(dir.path(), &name);

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

/// Regression: status zero is still self-termination when the window path
/// requires the app to remain alive.
///
/// This is the shipping template with only its lifecycle-wait line changed. The real
/// Bun child completes HELLO + echo, prints the real ready marker, then exits
/// zero before the injected window phase returns. The process-status oracle
/// proves the host did not cause the exit.
#[test]
fn ready_then_exit_zero_fails_the_window_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("z{}", std::process::id());
    let (root, pid_path) = project_with_main_rewrite(dir.path(), &name, |scaffolded| {
        let lifecycle_wait = "    await windowsClosed;";
        assert!(
            scaffolded.contains(lifecycle_wait),
            "template shape changed; this fixture edits its lifecycle-wait line"
        );
        scaffolded.replace(lifecycle_wait, "    process.exit(0);")
    });

    let msg = window_error_after_child_exits(&root, &pid_path);
    assert!(msg.contains("KELD-CORE-033"), "{msg}");
    assert!(msg.contains("KELD-RUNTIME-012"), "{msg}");
    assert!(msg.contains("exited 0"), "{msg}");
}

/// Regression for the shipping template's existing `finally` shape.
///
/// `process.exit()` defaults to status zero and runs before the thrown error
/// can surface as an unhandled rejection. The ledger must record the child's
/// self-termination rather than treating the status as proof of host success.
#[test]
fn finally_process_exit_zero_fails_the_window_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("f{}", std::process::id());
    let (root, pid_path) = project_with_main_rewrite(dir.path(), &name, |scaffolded| {
        let lifecycle_wait = "    await windowsClosed;";
        let finally_close = "  session.close();";
        assert!(
            scaffolded.contains(lifecycle_wait),
            "template lifecycle-wait shape changed"
        );
        assert!(
            scaffolded.contains(finally_close),
            "template finally shape changed"
        );
        scaffolded
            .replace(
                lifecycle_wait,
                "    throw new Error(\"kel116-finally-boom\");",
            )
            .replace(finally_close, "  session.close();\n  process.exit();")
    });

    let msg = window_error_after_child_exits(&root, &pid_path);
    assert!(msg.contains("KELD-CORE-033"), "{msg}");
    assert!(msg.contains("KELD-RUNTIME-012"), "{msg}");
    assert!(msg.contains("exited 0"), "{msg}");
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

/// Regression for the crash baseline being re-recorded on a later readiness wait.
///
/// `wait_until_output_contains` matches against *cumulative* stdout, so a second
/// call for a marker the app already printed returns immediately. If that
/// re-recorded the baseline, a crash that happened after the app was live would
/// be counted as already-recovered and the run would report success.
///
/// Exactly one crash occurs: the one-session listener admits no successor
/// (GH-674), so the supervisor's terminal outcome stays `Stopped` and only the
/// ledger can dissent. That is what makes this falsify the baseline rule itself
/// rather than the crash-loop breaker.
#[test]
fn a_later_readiness_wait_does_not_forgive_a_post_ready_crash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("b{}", std::process::id());
    let root = create_project(dir.path(), &name).expect("create");
    let pid_path = root.join("kel105-app.pid");
    let main = root.join("src/main.ts");
    let scaffolded = fs::read_to_string(&main).expect("scaffolded main.ts");
    let pid_lit = pid_path.display().to_string();
    fs::write(
        &main,
        format!(
            "import {{ writeFileSync as kel105Write }} from \"node:fs\";\n\
             kel105Write({pid_lit:?}, String(process.pid));\n\
             {scaffolded}"
        ),
    )
    .expect("write pid-recording fixture");

    let ready = format!("{name}: main process ready (IPC echo ok)");
    let session =
        start_dev_session_with_stdout_markers(&root, &[&ready]).expect("start host-owned session");
    session
        .wait_until_output_contains(&ready, Duration::from_secs(30))
        .expect("the stock template must complete HELLO + CALL");

    let pid: u32 = fs::read_to_string(&pid_path)
        .expect("the app must record its pid before it reports ready")
        .trim()
        .parse()
        .expect("pid breadcrumb must be a number");
    kill_process(pid);

    // Await supervision's own end rather than polling process state: the
    // supervisor records the crash before it reports that end, so returning
    // proves the ledger already counted it.
    await_supervision_end(&session);

    // The trigger: this marker is already in buffered stdout, so the wait
    // returns at once — and must not re-baseline the ledger.
    session
        .wait_until_output_contains(&ready, Duration::from_secs(30))
        .expect("the ready marker is still in captured stdout");

    session.shutdown().expect_err(
        "a crash after the app was live must not be forgiven by a later readiness wait",
    );
}

/// Regression: an app that reports ready and *then* dies must fail the run.
///
/// This is KEL-105 reopening after #84. `keld-runtime` publishes stdout and its
/// `Exited` event *before* it calls `record_crash`, so by the time the host
/// notices the ready marker the crash is usually already in the ledger. A
/// baseline taken from the crash *count* then folds that post-ready death into
/// "recovered" and `keld dev` exits 0 over a dead app — the exact defect the
/// ticket exists to prevent. Ordering the marker against the ledger's recorded
/// stdout position is what separates "crashed, then printed" from "printed,
/// then crashed".
///
/// The fixture is the shipping template with only its lifecycle-wait line changed, so
/// generation 1 does a real HELLO + CALL and prints the real ready line before
/// dying. The one-session listener admits no successor (GH-674), so exactly
/// one crash occurs and the supervisor's terminal outcome stays `Stopped`.
#[test]
fn an_app_that_dies_after_reporting_ready_fails_the_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("d{}", std::process::id());
    let root = create_project(dir.path(), &name).expect("create");
    let main = root.join("src/main.ts");
    let scaffolded = fs::read_to_string(&main).expect("scaffolded main.ts");
    let lifecycle_wait = "    await windowsClosed;";
    assert!(
        scaffolded.contains(lifecycle_wait),
        "template shape changed; this fixture edits its lifecycle-wait line"
    );
    fs::write(
        &main,
        scaffolded.replace(lifecycle_wait, "    process.exit(1);"),
    )
    .expect("write die-after-ready fixture");

    let ready = format!("{name}: main process ready (IPC echo ok)");
    let session =
        start_dev_session_with_stdout_markers(&root, &[&ready]).expect("start host-owned session");

    // Await supervision's end WITHOUT waiting on the ready marker: that wait is
    // the thing under test, and running it here would record the baseline
    // early and hide the defect. The end proves generation 1 crashed *and*
    // that the supervisor already recorded it, because the ledger write
    // precedes the terminal event.
    await_supervision_end(&session);

    // Now the host looks for the ready marker for the first time — exactly the
    // ordering the product hits when an app dies immediately after reporting
    // ready.
    session
        .wait_until_output_contains(&ready, Duration::from_secs(30))
        .expect("generation 1's ready line is still in captured stdout");

    let err = session
        .shutdown()
        .expect_err("the app reported ready and then died; reporting success over it is KEL-105");
    let msg = err.to_string();
    assert!(msg.contains("KELD-CORE-033"), "{msg}");
    assert!(msg.contains("KELD-RUNTIME-012"), "{msg}");
}

/// Blocks until supervision ends on its own and returns how the session
/// reported that end.
///
/// `wait_until_output_contains` returns before its deadline only on a matched
/// needle or a terminal supervisor event. No generation prints
/// [`NEVER_PRINTED`], so only the terminal event can end this wait, and an
/// unmatched needle records no readiness baseline. The supervisor writes the
/// ledger before it sends that event, so the crash that ended supervision is
/// already counted when this returns. The deadline is only a kill switch.
fn await_supervision_end(session: &HostOwnedHelloSession) -> String {
    let ended = session
        .wait_until_output_contains(NEVER_PRINTED, Duration::from_secs(30))
        .expect_err("no generation prints this needle")
        .to_string();
    assert!(
        !ended.contains("KELD-CORE-032") && !ended.contains("KELD-CORE-034"),
        "supervision did not end on its own: {ended}"
    );
    ended
}

fn project_with_main_rewrite(
    dir: &std::path::Path,
    name: &str,
    rewrite: impl FnOnce(String) -> String,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = create_project(dir, name).expect("create");
    let pid_path = root.join("kel116-app.pid");
    let main = root.join("src/main.ts");
    let scaffolded = fs::read_to_string(&main).expect("scaffolded main.ts");
    let pid_lit = pid_path.display().to_string();
    let rewritten = rewrite(scaffolded);
    fs::write(
        &main,
        format!(
            "import {{ writeFileSync as kel116WritePid }} from \"node:fs\";\n\
             kel116WritePid({pid_lit:?}, String(process.pid));\n{rewritten}"
        ),
    )
    .expect("write self-terminating fixture");
    (root, pid_path)
}

fn window_error_after_child_exits(root: &std::path::Path, pid_path: &std::path::Path) -> String {
    let result = run_dev_with_window(root, |_title, _html| {
        let pid: u32 = fs::read_to_string(pid_path)
            .expect("the app must record its pid before it reports ready")
            .trim()
            .parse()
            .expect("pid breadcrumb must be a number");
        let deadline = Instant::now() + Duration::from_secs(10);
        while process_is_alive(pid) {
            assert!(
                Instant::now() < deadline,
                "self-terminating Bun child {pid} remained alive"
            );
            std::thread::yield_now();
        }
        Ok(())
    });

    result
        .expect_err("an app that self-terminated after ready must not report window success")
        .to_string()
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
