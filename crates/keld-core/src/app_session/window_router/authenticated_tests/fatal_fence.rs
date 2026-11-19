//! A queued terminal failure must close the primary before the UI consumes it.
use super::{
    BOUND, RoleProcess, TestResult, command, publish_ready, read_control, router_for_bound,
};
use crate::app_session::{
    HostAppError, PrimaryOwnerUpdate, app_detail, window_router::NativePublicationDisposition,
    with_primary_caller_locked,
};
use keld_runtime::{SupervisorOutcome, primary::PrimaryRoleEvent};
use keld_wv::AppWindowCommand;
use std::sync::mpsc;
use std::time::Instant;

#[test]
fn queued_native_publication_fatal_refuses_authenticated_successor_before_ui_receipt() -> TestResult
{
    let mut role = RoleProcess::start()?;
    let (original, _old_control) = role.bind()?;
    let (owner_tx, owner_rx) = mpsc::channel();
    let (native_tx, native_rx) = mpsc::channel();
    let router = router_for_bound(original, owner_tx, native_tx)?;
    let handle = router.handle();
    let attempt = publish_ready(&role, &handle, &owner_rx)?;
    let target = handle
        .current_primary_writer_target()?
        .ok_or_else(|| std::io::Error::other("original authenticated writer absent after Ready"))?;
    let failure = app_detail("native publication fixture", "original source failure");
    let reported =
        handle.finish_native_publication(target.caller(), |_| true, Err(failure.into()))?;
    let NativePublicationDisposition::FatalQueued(cause) = reported else {
        return Err(std::io::Error::other("original native failure did not queue Fatal").into());
    };
    assert!(cause.to_string().contains("original source failure"));

    let mut old_publication_entered = false;
    with_primary_caller_locked(&handle.current, &handle.shutdown, target.caller(), |_| {
        old_publication_entered = true;
        Ok(())
    })?;

    // Leave the Fatal queued. Deliver a real Runtime revocation and authenticated
    // successor through Core while the UI cannot provide any terminal progress.
    role.supervisor.restart_generation(attempt);
    let deadline = Instant::now() + BOUND;
    loop {
        let event = role
            .supervisor
            .recv_event(deadline.saturating_duration_since(Instant::now()))
            .ok_or_else(|| std::io::Error::other("original generation was not revoked"))?;
        if matches!(event, PrimaryRoleEvent::Revoked { attempt: retired, .. } if retired == attempt)
        {
            handle
                .apply_generation_update(PrimaryOwnerUpdate::Role(event))
                .map_err(HostAppError::from)?;
            break;
        }
    }
    let (successor, mut successor_control) = role.bind()?;
    let successor_attempt = successor.attempt();
    assert!(successor_attempt > attempt);
    handle
        .apply_generation_update(PrimaryOwnerUpdate::Bound(successor))
        .map_err(HostAppError::from)?;
    assert!(
        handle.current.lock().expect("current owner").is_none(),
        "accepted Fatal allowed the authenticated successor to bind and begin replay"
    );
    assert!(
        !handle
            .readers
            .lock()
            .expect("registered readers")
            .contains_key(&successor_attempt)
    );
    assert!(
        !old_publication_entered,
        "original publication fence stayed open"
    );
    command(&mut successor_control, "WAIT-END")?;
    assert_eq!(
        read_control(&mut successor_control)?,
        "ENDED KELD-IPC-022 0"
    );
    assert!(matches!(
        native_rx.recv_timeout(BOUND)?,
        AppWindowCommand::Fatal
    ));
    assert!(
        native_rx.try_recv().is_err(),
        "the failure queues exactly one Fatal"
    );
    router.shutdown()?;
    assert!(matches!(role.finish()?, SupervisorOutcome::Stopped));
    Ok(())
}
