//! Public-main local admission after Quit; native handler-entry absence is unobserved.
use super::project::{CONTENT, Exercise, Policy, PublicFsProject};
use super::{
    LaunchEvidence, observer::Observation, outcomes::assert_component_outcomes, run_launch,
};
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use serde_json::{Value, json};
use std::fs;
use std::net::TcpStream;
use std::path::Path;
use std::process::Output;
use std::time::Instant;

#[test]
fn public_app_fs_after_quit_is_locally_refused_without_effect_and_relaunches() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let retired = run_launch(&fixture, 0, Exercise::PublicAppRetirement);
    // The existing ordinary public-main roundtrip proves the next launch is healthy.
    let relaunched = run_launch(&fixture, 1, Exercise::PublicAppComponent);
    for key in ["documentNonce", "javascriptToken"] {
        let document_identity = |evidence: &LaunchEvidence| {
            evidence
                .reports
                .iter()
                .find(|report| report["phase"] == "page-ready")
                .expect("actual launch document")[key]
                .as_str()
                .expect("renderer-created identity")
                .to_owned()
        };
        assert_ne!(
            document_identity(&retired),
            document_identity(&relaunched),
            "healthy relaunch reused old volatile {key}"
        );
    }
    assert_ne!(
        retired.family.expect("retired process family").bun,
        relaunched.family.expect("healthy process family").bun,
        "healthy relaunch reused retired Bun identity"
    );
    eprintln!(
        "KELD_KEL140_PUBLIC_RETIREMENT_ACCEPTANCE launches=2 input_actions=0 local_closed=true unchanged_allowed_bytes=true native_entry_absence=unknown renderer_action_acceptance=false"
    );
}

/// Independent native host/Guardian/Bun and filesystem retirement acceptance
/// when the operator's CoreGraphics screen-capture preflight is unavailable.
/// This does NOT substitute for the separate native-window presentation test.
#[test]
fn public_fs_quit_and_relaunch_reap_families_without_graphics_capture() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let retired =
        run_launch_without_window_census(&fixture, 0, Exercise::PublicAppRetirement, false);
    let relaunched =
        run_launch_without_window_census(&fixture, 1, Exercise::PublicAppComponent, false);
    assert_ne!(
        retired.family.expect("retired native family").bun,
        relaunched.family.expect("relaunch native family").bun,
        "fresh launch reused the native supervised Bun identity"
    );
}

/// The live shipping host must still exit after an accepted-but-silent HTTP
/// connection is held open by an independent peer of the test observer. This
/// reproduces the Quit-lane starvation schedule with real Bun and Guardian.
#[test]
fn public_fs_relaunch_quit_survives_accepted_idle_observer_connection() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let retired =
        run_launch_without_window_census(&fixture, 0, Exercise::PublicAppRetirement, false);
    let relaunched =
        run_launch_without_window_census(&fixture, 1, Exercise::PublicAppComponent, true);
    assert_ne!(
        retired.family.expect("retired native family").bun,
        relaunched.family.expect("relaunch native family").bun,
        "a successful post-Quit relaunch must mint a fresh native Bun family"
    );
}

pub(super) fn run_launch_without_window_census(
    fixture: &PublicFsProject,
    launch: usize,
    exercise: Exercise,
    hold_silent_observer_peer: bool,
) -> LaunchEvidence {
    let (observer, accept_receipts, app_result_ack_release) = if hold_silent_observer_peer {
        let (observer, receipts, release) = Observation::bind_with_app_result_gate();
        (observer, Some(receipts), Some(release))
    } else {
        (Observation::bind(), None, None)
    };
    let prepared = match exercise {
        Exercise::PublicAppRetirement => fixture.prepare_retirement(launch, observer.port),
        Exercise::PublicAppComponent => fixture.prepare_component(launch, observer.port),
        _ => panic!("only actual native public-main retirement scenarios"),
    };
    let stage = &prepared.stage;
    let mut cleanup = super::spawn_shipping_host(stage);
    let host = cleanup.cli.as_mut().expect("native host custody");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain host-only dev lease");
    let mut evidence = LaunchEvidence::default();
    super::await_ready(&observer, &mut cleanup, &mut evidence, host_pid, &prepared);
    if !evidence
        .reports
        .iter()
        .any(|report| report["phase"] == "app-result")
    {
        let result = observer
            .reports
            .recv_timeout(EVENT_DEADLINE)
            .expect("real public FS component result before Quit");
        assert_eq!(result["phase"], "app-result", "{result}");
        evidence.reports.push(result);
    }
    assert_component_outcomes(
        &evidence.reports,
        fixture.policy,
        &prepared.target,
        &fixture.outside,
        &fixture.scope,
        evidence.family.expect("independent native Bun census").bun,
    );
    // The opted-in fixture has reported app-result but withheld its HTTP
    // response. Bun still awaits that response and cannot issue /quit yet.
    // Park an independently accepted byte-silent peer first, then release the
    // response so /quit is necessarily queued behind that silent connection.
    let idle = accept_receipts.as_ref().map(|receipts| {
        let stream = TcpStream::connect(("127.0.0.1", observer.port))
            .expect("test-owned silent observer connection");
        let expected = stream.local_addr().expect("test-owned peer address");
        let deadline = Instant::now() + EVENT_DEADLINE;
        loop {
            let accepted = receipts
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("fixture accepted test-owned silent peer");
            if accepted == expected {
                break;
            }
        }
        stream
    });
    if let Some(release) = app_result_ack_release {
        release
            .send(())
            .expect("release real Bun app-result acknowledgement");
    }
    observer.quit.send(()).expect("release real public Quit");
    let output = wait_child_output(
        cleanup.cli.take().expect("retained real shipping host"),
        EVENT_DEADLINE,
    );
    // Hold the independently accepted silent peer until the shipping host
    // actually exits, then release it before fixture observer Drop/join.
    drop(idle);
    super::assert_product_quit(&output, &evidence, stage.root(), host_pid, exercise);
    evidence.capture_profile_diagnostics(fixture, &output);
    if exercise == Exercise::PublicAppRetirement {
        assert_after_quit(&output, &evidence, &prepared.target);
    }
    cleanup.bun_group = None;
    evidence
}

pub(super) fn assert_after_quit(output: &Output, evidence: &LaunchEvidence, target: &Path) {
    // This runs after product cleanup and before fixture Drop/fallback cleanup.
    // File effects are an independent OS oracle, not inferred from app output.
    assert_eq!(
        fs::read(target).expect("allowed target after public Quit"),
        CONTENT,
        "post-Quit write changed independently observed allowed bytes"
    );
    let stderr = std::str::from_utf8(&output.stderr).expect("retirement output UTF-8");
    let actual: Vec<Value> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("KELD_KEL140_PUBLIC_FS_RETIRED "))
        .map(|line| {
            assert!(line.len() <= 4096, "bounded public retirement output");
            serde_json::from_str(line).expect("actual public retirement JSON")
        })
        .collect();
    assert_eq!(
        actual.len(),
        1,
        "one actual post-Quit public outcome record"
    );
    assert_eq!(
        actual[0],
        json!({
            "phase":"app-retirement",
            "pid":evidence.family.expect("OS-attributed Bun family").bun,
            "componentOnly":true,
            "outcomes":[
                {"action":"write","status":"rejected","errorName":"Error",
                    "message":"KELD-IPC-001: session is closed",
                    "codePresent":false,"callError":false},
                {"action":"read","status":"rejected","errorName":"Error",
                    "message":"KELD-IPC-001: session is closed",
                    "codePresent":false,"callError":false}
            ]
        }),
        "post-Quit methods must report the exact ordinary local closed Error"
    );
}
