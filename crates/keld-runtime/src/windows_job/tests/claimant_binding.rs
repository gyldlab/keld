//! Windows process-object claimant identity and refusal-order tests.
//! Keeps the native launch, handle, and error oracle together.

use super::{
    Command, ERROR_INVALID_HANDLE, ProcessObservation, Stdio, WindowsClaimantRefusal,
    WindowsLaunchedProcess, WindowsProcessJob, bind_observed_claimant, blocking_stdio,
    claimant_by_pid, recorded_launch, same_kernel_object, system32,
};
use std::io;

#[test]
fn claimant_binding_accepts_the_exact_launched_process_before_and_after_resume() {
    let mut launch = recorded_launch(Some(blocking_stdio()));
    let process_id = launch.launched.child().id();
    let claimant = claimant_by_pid(process_id);
    assert_eq!(
        launch.launched.creation_time,
        claimant.creation_time(),
        "the launch record equals the creation time read through a second handle"
    );
    launch
        .launched
        .bind_claimant(&claimant)
        .expect("the exact suspended launch binds");

    let mut job = WindowsProcessJob::create().expect("create an attempt Job");
    let membership = job
        .assign_child(launch.launched.child())
        .expect("assign the launch before its resume");
    launch
        .launched
        .resume(&membership)
        .expect("resume the launch once");
    assert!(
        !claimant
            .has_exited()
            .expect("query the resumed launch through the claimant handle"),
        "the resumed interpreter waits on its stdin pipe"
    );
    launch
        .launched
        .bind_claimant(&claimant)
        .expect("the retained launch handle still binds after resume");
    launch
        .launched
        .bind_claimant(&claimant_by_pid(process_id))
        .expect("a claimant handle opened after resume binds");

    launch
        .launched
        .terminate(0)
        .expect("terminate the resumed launch");
    launch
        .launched
        .wait(10_000)
        .expect("reap the resumed launch");
}

#[test]
fn claimant_binding_refuses_a_second_instance_of_the_launched_image() {
    let launch = recorded_launch(None);
    let mut copy = Command::new(system32().join("cmd.exe"))
        .arg("/d")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start a same-user copy of the launched image");
    let claimant = claimant_by_pid(copy.id());
    let refusal = launch.launched.bind_claimant(&claimant);
    copy.kill().expect("terminate the same-image copy");
    let _ = copy.wait().expect("reap the same-image copy");
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::NotLaunchedProcess)),
        "{refusal:?}"
    );
}

#[test]
fn claimant_binding_refuses_a_forged_peer_carrying_the_launched_identity() {
    // Seam-injected: a peer whose process ID and creation time equal the
    // launch record but whose process object differs. Windows cannot give two
    // live processes one PID, so another real process handle carries the
    // launched identity numbers.
    let launch = recorded_launch(None);
    let other = claimant_by_pid(std::process::id());
    let recorded = ProcessObservation::of_launch(&launch.launched);
    let forged = ProcessObservation {
        process_id: recorded.process_id,
        creation_time: recorded.creation_time,
        ..ProcessObservation::of_peer(&other)
    };
    let refusal = bind_observed_claimant(forged, recorded);
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::NotLaunchedProcess)),
        "{refusal:?}"
    );
}

#[test]
fn claimant_binding_refuses_once_the_launch_handle_is_signaled() {
    let mut launch = recorded_launch(None);
    let claimant = claimant_by_pid(launch.launched.child().id());
    launch
        .launched
        .bind_claimant(&claimant)
        .expect("the live launch binds before termination");
    signal_launch(&mut launch.launched);
    let refusal = launch.launched.bind_claimant(&claimant);
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::LaunchExited)),
        "{refusal:?}"
    );
}

/// Terminates a recorded launch and waits until its retained handle is
/// signaled with the exit code the termination set.
fn signal_launch(launched: &mut WindowsLaunchedProcess) {
    launched.terminate(7).expect("terminate the launch");
    assert_eq!(
        launched
            .wait(10_000)
            .expect("observe the signaled launch handle"),
        7
    );
}

// Fact order (KEL-53 §4 "Candidate connect-back", *Acceptance*: "in order").
// Each cell fails more than one fact, so a reordered binding reports a
// different refusal.

#[test]
fn claimant_binding_compares_the_process_object_before_launch_liveness() {
    // Every fact fails: another process object, a signaled launch handle and
    // a different process ID. Only the comparison-first order reports
    // NotLaunchedProcess.
    let mut launch = recorded_launch(None);
    signal_launch(&mut launch.launched);
    let other = claimant_by_pid(std::process::id());
    assert_ne!(other.process_id, launch.launched.child().id());
    let refusal = launch.launched.bind_claimant(&other);
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::NotLaunchedProcess)),
        "{refusal:?}"
    );
}

#[test]
fn claimant_binding_checks_launch_liveness_before_the_recorded_identity() {
    // Seam-injected: the compared handles are one process object, opened
    // before the launch exits, while the launch handle is signaled and the
    // recorded creation time is wrong. Only liveness-before-identity reports
    // LaunchExited.
    let mut launch = recorded_launch(None);
    let peer = claimant_by_pid(launch.launched.child().id());
    signal_launch(&mut launch.launched);
    let claimant = ProcessObservation::of_peer(&peer);
    let recorded = ProcessObservation::of_launch(&launch.launched);
    assert!(
        same_kernel_object(claimant.handle, recorded.handle).expect("compare two live handles"),
        "the claimant handle still names the exited launch object"
    );
    let wrong_time = ProcessObservation {
        creation_time: claimant.creation_time + 1,
        ..recorded
    };
    let refusal = bind_observed_claimant(claimant, wrong_time);
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::LaunchExited)),
        "{refusal:?}"
    );
}

#[test]
fn claimant_binding_refuses_a_wrong_recorded_creation_time_or_process_id() {
    // Seam-injected: the recorded values differ from the kernel's while the
    // compared handles are one live process object.
    let launch = recorded_launch(None);
    let peer = claimant_by_pid(launch.launched.child().id());
    let claimant = ProcessObservation::of_peer(&peer);
    let recorded = ProcessObservation::of_launch(&launch.launched);
    bind_observed_claimant(claimant, recorded).expect("the unaltered record binds");
    let wrong_time = ProcessObservation {
        creation_time: recorded.creation_time + 1,
        ..recorded
    };
    let refusal = bind_observed_claimant(claimant, wrong_time);
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::LaunchIdentityMismatch)),
        "{refusal:?}"
    );
    let wrong_pid = ProcessObservation {
        process_id: recorded.process_id ^ 4,
        ..recorded
    };
    let refusal = bind_observed_claimant(claimant, wrong_pid);
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::LaunchIdentityMismatch)),
        "{refusal:?}"
    );
}

#[test]
fn claimant_binding_refuses_an_invalid_claimant_handle_with_a_typed_error() {
    // Seam-injected: safe callers cannot hold an invalid owned handle, so the
    // observation carries a null handle with the launched identity.
    let launch = recorded_launch(None);
    let recorded = ProcessObservation::of_launch(&launch.launched);
    let invalid = ProcessObservation {
        handle: std::ptr::null_mut(),
        ..recorded
    };
    match bind_observed_claimant(invalid, recorded) {
        Err(WindowsClaimantRefusal::Unverifiable { phase, source }) => {
            assert_eq!(phase, "claimant process-object comparison");
            assert_eq!(
                source.raw_os_error(),
                Some(ERROR_INVALID_HANDLE.cast_signed())
            );
        }
        other => panic!("an invalid claimant handle must refuse as unverifiable: {other:?}"),
    }
}

#[test]
fn claimant_refusals_render_runtime_017_with_fix_guidance() {
    let refusal = WindowsClaimantRefusal::NotLaunchedProcess;
    assert_eq!(
        refusal.to_string(),
        "KELD-RUNTIME-017: connect-back claimant refused: the connected process is not \
         the launched and retained process object. Disconnect this client and re-arm \
         the endpoint without consuming its one-shot; only the exact launched process \
         can claim, and the health deadline is not extended."
    );
    for refusal in [
        WindowsClaimantRefusal::LaunchExited,
        WindowsClaimantRefusal::LaunchIdentityMismatch,
        WindowsClaimantRefusal::Unverifiable {
            phase: "claimant process-object comparison",
            source: io::Error::from_raw_os_error(ERROR_INVALID_HANDLE.cast_signed()),
        },
    ] {
        let text = refusal.to_string();
        assert!(text.starts_with("KELD-RUNTIME-017: "), "{text}");
        assert!(text.contains("re-arm the endpoint"), "{text}");
    }
}
