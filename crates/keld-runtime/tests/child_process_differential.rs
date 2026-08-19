//! KEL-77 T1: package-agnostic Node-versus-Bun differential harness for the
//! child-process lifecycle family. Spec:
//! `docs/specs/kel77-bun-child-process-differential.md`.
//!
//! # Why this exists
//!
//! Architecture 06 §1 ships a pinned Bun and states that "Bun's Node-compat is
//! the compat plan", and §1.1 has the host supervising Bun children. Nothing in
//! the repository measured whether Bun's `child_process` lifecycle actually
//! matches the Node contract a package was written against. This harness runs
//! one committed, package-agnostic fixture corpus under both runtimes and
//! compares each observation to a cited Node documentation sentence.
//!
//! # Oracle (upstream docs, not a mirror of the other arm)
//!
//! Deriving the expectation from the Node arm's live output would be a mirror,
//! not an oracle: it could not distinguish "Bun is wrong" from "Node changed",
//! and it would make the Node arm unfalsifiable. Every expectation below is a
//! sentence from <https://nodejs.org/docs/latest-v24.x/api/child_process.html>:
//!
//! - `'exit'`: "If the process exited, `code` is the final exit code of the
//!   process, otherwise `null`. If the process terminated due to receipt of a
//!   signal, `signal` is the string name of the signal, otherwise `null`. One of
//!   the two will always be non-`null`."
//! - `'close'`: "The `'close'` event will always emit after `'exit'` was already
//!   emitted, or `'error'` if the child process failed to spawn."
//! - `'error'`: emitted when "the process could not be spawned".
//! - `subprocess.kill()`: "This function returns `true` if `kill(2)` succeeds,
//!   and `false` otherwise."
//! - `subprocess.killed`: "Set to `true` after `subprocess.kill()` is used to
//!   successfully send a signal to the child process."
//! - Unspecified path — `process.exit()` "will cause the process to exit as
//!   quickly as possible even if there are still asynchronous operations
//!   pending, including I/O operations to `process.stdout`". A divergence on
//!   that path is recorded `Unknown`, never `Fail`.
//!
//! # Gating (see spec §4.2)
//!
//! Contracts both runtimes currently honor are asserted for both arms. The one
//! reproduced Bun defect is pinned by `bun_kill_after_exit_is_the_pinned_defect`,
//! which fails loudly when upstream fixes it. Unspecified paths are recorded,
//! never asserted as conformance.
//!
//! # Negative controls (executed; see PR body)
//!
//! - Inverting the `'close'`-after-`'exit'` comparison in `derive_verdict` makes
//!   `comparator_rejects_close_before_exit` fail.
//! - Replacing `derive_verdict` with a constant `Verdict::Pass` makes
//!   `comparator_can_emit_fail` and `bun_kill_after_exit_is_the_pinned_defect`
//!   fail.
//! - Making the fixture's `rawKillErrno` anything but `ESRCH` makes
//!   `comparator_requires_proof_the_child_is_gone` fail.

#![allow(clippy::expect_used, clippy::panic)] // test crate: expect/panic are the assertion oracle

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Map, Value};

/// Documentation revision every oracle sentence in this file was read from.
const ORACLE_REVISION: &str = "nodejs-docs-v24.x@2026-08-19";
/// Bytes the abrupt/drained flush fixture writes before exiting.
const DRAINED_WRITE_BYTES: u64 = 200_000;

/// Whether upstream documentation pins the observable this case measures.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Strength {
    /// A cited sentence fixes the expected value; a mismatch is a real defect.
    Specified,
    /// Documentation is silent or explicitly warns the path is lossy.
    Unspecified,
}

/// Result of comparing one observation to its oracle. Three states, never two:
/// collapsing `Unknown` into `Pass` would manufacture a compatibility claim.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verdict {
    Pass,
    Fail,
    Unknown,
}

/// One measured runtime.
#[derive(Clone, Copy)]
struct Arm {
    name: &'static str,
    exe: &'static str,
}

const NODE: Arm = Arm {
    name: "node",
    exe: "node",
};
const BUN: Arm = Arm {
    name: "bun",
    exe: "bun",
};

/// One evidence cell: an operation and the upstream sentence that judges it.
struct Case {
    operation_id: &'static str,
    oracle_id: &'static str,
    strength: Strength,
}

const CASES: &[Case] = &[
    Case {
        operation_id: "child-process.exit-code-propagation",
        oracle_id: "nodejs.child_process.exit-event-code",
        strength: Strength::Specified,
    },
    Case {
        operation_id: "child-process.signal-termination",
        oracle_id: "nodejs.child_process.exit-event-signal",
        strength: Strength::Specified,
    },
    Case {
        operation_id: "child-process.close-after-exit",
        oracle_id: "nodejs.child_process.close-after-exit-order",
        strength: Strength::Specified,
    },
    Case {
        operation_id: "child-process.spawn-failure-order",
        oracle_id: "nodejs.child_process.error-before-close-on-spawn-failure",
        strength: Strength::Specified,
    },
    Case {
        operation_id: "child-process.kill-after-exit",
        oracle_id: "nodejs.child_process.subprocess-kill-return",
        strength: Strength::Specified,
    },
    Case {
        operation_id: "child-process.stdout-flush-on-abrupt-exit",
        oracle_id: "nodejs.process.exit-discards-pending-stdout",
        strength: Strength::Unspecified,
    },
];

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/child-process")
}

/// Exact revision string of an arm, recorded with every verdict so a result is
/// never readable without the runtime it was measured against.
fn arm_revision(arm: Arm) -> String {
    // `bun --revision` includes the commit; node only offers `--version`.
    let flag = if arm.exe == "bun" {
        "--revision"
    } else {
        "--version"
    };
    let out = Command::new(arm.exe)
        .arg(flag)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "`{} {flag}` failed: {e}. KEL-77 requires both `node` and `bun` on PATH; \
             install the missing runtime rather than skipping the differential.",
                arm.exe
            )
        });
    assert!(
        out.status.success(),
        "`{} {flag}` exited {}",
        arm.exe,
        out.status
    );
    format!(
        "{}-{}",
        arm.name,
        String::from_utf8_lossy(&out.stdout).trim()
    )
}

/// Run one case under one arm and return its single JSON observation line.
fn run_case(arm: Arm, operation_id: &str) -> Map<String, Value> {
    let dir = corpus_dir();
    let out = Command::new(arm.exe)
        .arg(dir.join("driver.cjs"))
        .arg(operation_id)
        .current_dir(&dir)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "spawning `{} driver.cjs {operation_id}` failed: {e}. KEL-77 requires \
                 both `node` and `bun` on PATH.",
                arm.exe
            )
        });

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "[{}/{operation_id}] driver exited {}\nstdout: {stdout}\nstderr: {stderr}",
        arm.name,
        out.status
    );

    // Exactly one observation line. A fixture that prints nothing is a case
    // failure, never a silently absent record.
    let line = stdout
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or_else(|| {
            panic!(
                "[{}/{operation_id}] no observation line.\nstderr: {stderr}",
                arm.name
            )
        });
    let value: Value = serde_json::from_str(line).unwrap_or_else(|e| {
        panic!(
            "[{}/{operation_id}] observation is not JSON ({e}): {line}",
            arm.name
        )
    });
    let obs = value
        .as_object()
        .unwrap_or_else(|| panic!("[{}/{operation_id}] observation is not an object", arm.name))
        .clone();
    assert_eq!(
        obs.get("case").and_then(Value::as_str),
        Some(operation_id),
        "[{}] observation reports the wrong case: {line}",
        arm.name
    );
    obs
}

fn field<'a>(obs: &'a Map<String, Value>, key: &str) -> &'a str {
    obs.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("observation is missing string field `{key}`: {obs:?}"))
}

fn events(obs: &Map<String, Value>) -> Vec<&str> {
    obs.get("events")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("observation is missing `events`: {obs:?}"))
        .iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("non-string event: {v}"))
        })
        .collect()
}

fn u64_field(obs: &Map<String, Value>, key: &str) -> u64 {
    obs.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("observation is missing u64 field `{key}`: {obs:?}"))
}

/// Position of the first event whose name matches `name`.
fn index_of(evs: &[&str], name: &str) -> Option<usize> {
    let prefix = format!("{name}(");
    evs.iter().position(|e| e.starts_with(&prefix))
}

/// Compare one observation to its cited oracle. Pure over the observation, so
/// the negative-control tests can drive it with synthetic input.
fn derive_verdict(case: &Case, obs: &Map<String, Value>) -> (Verdict, String) {
    // An unspecified path can never produce a conformance judgement, whatever
    // the observation says.
    if case.strength == Strength::Unspecified {
        return (
            Verdict::Unknown,
            format!(
                "process.exit() is documented to discard pending stdout; abrupt={} \
                 drained={} bytes recorded as an observation, not a verdict",
                u64_field(obs, "abruptBytes"),
                u64_field(obs, "drainedBytes")
            ),
        );
    }

    let evs = events(obs);
    let (holds, detail): (bool, String) = match case.operation_id {
        "child-process.exit-code-propagation" => (
            evs == ["exit(7,null)", "close(7,null)"]
                && field(obs, "exitCode") == "7"
                && field(obs, "signalCode") == "null",
            format!(
                "events={evs:?} exitCode={} signalCode={}",
                field(obs, "exitCode"),
                field(obs, "signalCode")
            ),
        ),
        "child-process.signal-termination" => (
            evs == ["exit(null,SIGTERM)", "close(null,SIGTERM)"]
                && field(obs, "signalCode") == "SIGTERM"
                && field(obs, "exitCode") == "null",
            format!("events={evs:?} signalCode={}", field(obs, "signalCode")),
        ),
        // "'close' will always emit after 'exit' was already emitted".
        "child-process.close-after-exit" => {
            let exit_at = index_of(&evs, "exit");
            let close_at = index_of(&evs, "close");
            (
                matches!((exit_at, close_at), (Some(e), Some(c)) if c > e),
                format!("events={evs:?}"),
            )
        }
        // Spawn failure: 'error' fires, 'close' follows it, and 'exit' never fires.
        "child-process.spawn-failure-order" => {
            let error_at = index_of(&evs, "error");
            let close_at = index_of(&evs, "close");
            (
                evs.first().is_some_and(|e| *e == "error(ENOENT)")
                    && matches!((error_at, close_at), (Some(er), Some(c)) if c > er)
                    && index_of(&evs, "exit").is_none(),
                format!("events={evs:?}"),
            )
        }
        // kill(2) cannot succeed against a pid the OS reports as ESRCH, so
        // kill() must return false and `killed` must stay false.
        "child-process.kill-after-exit" => {
            let errno = field(obs, "rawKillErrno");
            assert_eq!(
                errno, "ESRCH",
                "the kill-after-exit oracle is only valid when the OS confirms the child \
                 is gone; raw process.kill(pid, 0) reported `{errno}`, so this observation \
                 cannot judge subprocess.kill()"
            );
            (
                field(obs, "killReturn") == "false" && field(obs, "killed") == "false",
                format!(
                    "killReturn={} killed={} rawKillErrno={errno}",
                    field(obs, "killReturn"),
                    field(obs, "killed")
                ),
            )
        }
        other => panic!("no oracle wired for case `{other}`"),
    };

    if holds {
        (Verdict::Pass, detail)
    } else {
        (Verdict::Fail, detail)
    }
}

fn case_by_id(operation_id: &str) -> &'static Case {
    CASES
        .iter()
        .find(|c| c.operation_id == operation_id)
        .unwrap_or_else(|| panic!("unknown case `{operation_id}`"))
}

// ---------------------------------------------------------------------------
// Acceptance §3.1 — both arms, every case, exactly one observation each.
// ---------------------------------------------------------------------------

#[test]
fn every_case_produces_one_observation_per_arm() {
    for arm in [NODE, BUN] {
        let revision = arm_revision(arm);
        assert!(!revision.is_empty(), "empty revision for {}", arm.name);
        for case in CASES {
            let obs = run_case(arm, case.operation_id);
            assert!(
                obs.contains_key("events"),
                "[{}/{}] observation without an event trace",
                arm.name,
                case.operation_id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Acceptance §3.2–§3.5 — contracts both runtimes currently honor. A regression
// on either arm turns this red, which is the point.
// ---------------------------------------------------------------------------

#[test]
fn specified_contracts_hold_on_both_arms() {
    let shared = [
        "child-process.exit-code-propagation",
        "child-process.signal-termination",
        "child-process.close-after-exit",
        "child-process.spawn-failure-order",
    ];
    for arm in [NODE, BUN] {
        let revision = arm_revision(arm);
        for operation_id in shared {
            let case = case_by_id(operation_id);
            let obs = run_case(arm, operation_id);
            let (verdict, detail) = derive_verdict(case, &obs);
            assert_eq!(
                verdict,
                Verdict::Pass,
                "[{revision}] {operation_id} violates oracle {}@{ORACLE_REVISION}: {detail}",
                case.oracle_id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Acceptance §3.6 + §3.8 — the one reproduced divergence, pinned in both
// directions so an upstream fix cannot be absorbed silently.
// ---------------------------------------------------------------------------

#[test]
fn node_kill_after_exit_satisfies_the_oracle() {
    let case = case_by_id("child-process.kill-after-exit");
    let obs = run_case(NODE, case.operation_id);
    let (verdict, detail) = derive_verdict(case, &obs);
    assert_eq!(
        verdict,
        Verdict::Pass,
        "[{}] Node is the reference implementation of its own documented \
         subprocess.kill() contract and must satisfy it: {detail}",
        arm_revision(NODE)
    );
}

#[test]
fn bun_kill_after_exit_is_the_pinned_defect() {
    let case = case_by_id("child-process.kill-after-exit");
    let revision = arm_revision(BUN);
    let obs = run_case(BUN, case.operation_id);
    let (verdict, detail) = derive_verdict(case, &obs);

    assert_eq!(
        verdict,
        Verdict::Fail,
        "[{revision}] subprocess.kill() after 'exit' now satisfies the Node contract \
         ({detail}). Upstream appears to have FIXED this. Required follow-up in the same \
         PR: delete this pinned-defect test, move `child-process.kill-after-exit` into \
         `specified_contracts_hold_on_both_arms`, and flip the recorded verdict for this \
         cell from `fail` to `pass`. Do not weaken this assertion to make it green."
    );
    // Pin the exact defect shape, so a *different* wrong answer is also caught.
    assert_eq!(field(&obs, "killReturn"), "true", "[{revision}] {detail}");
    assert_eq!(field(&obs, "killed"), "true", "[{revision}] {detail}");
}

#[test]
fn comparator_requires_proof_the_child_is_gone() {
    // The kill-after-exit verdict is only meaningful because the fixture also
    // asks the OS directly. Both arms must report ESRCH; if a runtime left the
    // child unreaped, kill(2) could legitimately succeed and the oracle would
    // not apply.
    for arm in [NODE, BUN] {
        let obs = run_case(arm, "child-process.kill-after-exit");
        assert_eq!(
            field(&obs, "rawKillErrno"),
            "ESRCH",
            "[{}] raw process.kill(pid, 0) did not report ESRCH; the child was not \
             actually gone, so this cell cannot judge subprocess.kill()",
            arm.name
        );
    }
}

// ---------------------------------------------------------------------------
// Acceptance §3.7 — unspecified path: record, never score.
// ---------------------------------------------------------------------------

#[test]
fn abrupt_exit_flush_is_unknown_and_the_drained_path_is_lossless() {
    let case = case_by_id("child-process.stdout-flush-on-abrupt-exit");
    for arm in [NODE, BUN] {
        let revision = arm_revision(arm);
        let obs = run_case(arm, case.operation_id);
        let (verdict, detail) = derive_verdict(case, &obs);
        assert_eq!(
            verdict,
            Verdict::Unknown,
            "[{revision}] a documented-lossy path must not produce a conformance \
             verdict: {detail}"
        );
        // The *specified* half of the same case: exiting from the write callback
        // is the documented way to flush, and it must deliver every byte on both
        // runtimes.
        assert_eq!(
            u64_field(&obs, "drainedBytes"),
            DRAINED_WRITE_BYTES,
            "[{revision}] the drained write path lost bytes; that path IS specified"
        );
    }
}

// ---------------------------------------------------------------------------
// Acceptance §3.12 — the comparator itself is falsifiable.
// ---------------------------------------------------------------------------

#[test]
fn comparator_can_emit_fail() {
    // Negative control: a deliberately-wrong observation must be judged Fail.
    // If derive_verdict were a constant Pass, this test is what catches it.
    let case = case_by_id("child-process.exit-code-propagation");
    let wrong: Map<String, Value> = serde_json::from_str(
        r#"{"case":"child-process.exit-code-propagation",
             "events":["exit(0,null)","close(0,null)"],
             "exitCode":"0","signalCode":"null"}"#,
    )
    .expect("synthetic observation parses");
    let (verdict, _) = derive_verdict(case, &wrong);
    assert_eq!(
        verdict,
        Verdict::Fail,
        "comparator accepted a wrong exit code"
    );

    // And the correct observation must still be judged Pass, so the control is
    // not passing merely because everything fails.
    let right: Map<String, Value> = serde_json::from_str(
        r#"{"case":"child-process.exit-code-propagation",
             "events":["exit(7,null)","close(7,null)"],
             "exitCode":"7","signalCode":"null"}"#,
    )
    .expect("synthetic observation parses");
    assert_eq!(derive_verdict(case, &right).0, Verdict::Pass);
}

#[test]
fn comparator_rejects_close_before_exit() {
    // Negative control for the ordering oracle specifically: inverting the
    // observed order must flip the verdict.
    let case = case_by_id("child-process.close-after-exit");
    let inverted: Map<String, Value> = serde_json::from_str(
        r#"{"case":"child-process.close-after-exit",
             "events":["close(7,null)","exit(7,null)"],
             "exitCode":"7","signalCode":"null"}"#,
    )
    .expect("synthetic observation parses");
    assert_eq!(derive_verdict(case, &inverted).0, Verdict::Fail);
}

// ---------------------------------------------------------------------------
// Acceptance §3.9 — every verdict is reported with the revision it was measured
// against. This prints the differential report consumed by the PR/handoff.
// ---------------------------------------------------------------------------

#[test]
fn differential_report_pins_revision_platform_and_oracle_for_every_cell() {
    let node_revision = arm_revision(NODE);
    let bun_revision = arm_revision(BUN);
    let platform = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    println!("KEL-77 child-process differential — {platform}/{arch}");
    println!("  arms: {node_revision} | {bun_revision}");
    println!("  oracle revision: {ORACLE_REVISION}");

    let mut rows = 0_usize;
    for case in CASES {
        for (arm, revision) in [(NODE, &node_revision), (BUN, &bun_revision)] {
            let obs = run_case(arm, case.operation_id);
            let (verdict, detail) = derive_verdict(case, &obs);
            println!(
                "  {:<44} {revision:<28} {verdict:?}  [{}] {detail}",
                case.operation_id, case.oracle_id
            );
            // Every cell carries the five data points KEL-77 requires: runtime
            // revision, platform, arch, oracle identity, and a three-state verdict.
            assert!(!revision.is_empty());
            assert!(!platform.is_empty() && !arch.is_empty());
            assert!(!case.oracle_id.is_empty());
            assert!(matches!(
                verdict,
                Verdict::Pass | Verdict::Fail | Verdict::Unknown
            ));
            rows += 1;
        }
    }
    assert_eq!(
        rows,
        CASES.len() * 2,
        "every case must be measured on both arms"
    );
}
