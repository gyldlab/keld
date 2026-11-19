//! Exercise the selected watchdog through the real coordinator and terminal callback.

use super::super::{DesktopSubscribeClock, qualified_sample};
use super::{DesktopFixture, TestError, sample};
use crate::app_session::{
    AppReadyTrigger, AppWindowCommand, DirectPrimaryOwnerCommand, HostAppError,
    SESSION_CLI_LEASE_LOST, SESSION_DESKTOP_UI_ENTRY_TIMEOUT, SessionShutdownState,
    coordinate_window_events, tests::primary_test_stream_pair,
};
use keld_ipc::CorrelationId;
use keld_runtime::primary::{PrimaryRecoveryGate, PrimaryRoleConfig, PrimaryRoleSupervisor};
use std::sync::{Arc, OnceLock, atomic::Ordering, mpsc};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn core_rejects_positive_work_area_outside_the_primary_display() {
    let mut outside = sample();
    outside.displays[0].work_area.x = 1;
    assert!(
        qualified_sample(outside).is_none(),
        "a positive work area extending past its primary display cannot become a desktop snapshot"
    );
    let mut outside = sample();
    outside.displays[0].work_area.y = -1;
    assert!(qualified_sample(outside).is_none());
    let mut outside = sample();
    outside.displays[0].work_area.x = i32::MAX;
    outside.displays[0].work_area.width = u32::MAX;
    assert!(qualified_sample(outside).is_none());
    assert!(qualified_sample(sample()).is_some());
}

#[test]
fn coordinator_closes_pre_entry_timeout_while_native_holds_subscribe_progress()
-> Result<(), TestError> {
    let fixture = DesktopFixture::new()?;
    fixture.subscribe(41)?;
    {
        let mut session = fixture
            .router
            .windows
            .as_ref()
            .expect("selected Window owner")
            .lock()
            .expect("Registry");
        session
            .desktop
            .as_mut()
            .expect("desktop state")
            .pending
            .as_mut()
            .expect("authenticated Subscribe record")
            .clock = DesktopSubscribeClock::new(Instant::now() - Duration::from_secs(10));
    }

    // The actual Subscribe progress has reached the native command queue. A
    // held pre-entry UI has not emitted the corresponding AppWindowEvent.
    assert!(matches!(
        fixture.commands.try_recv()?,
        AppWindowCommand::HostWriterProgress
    ));
    let (_events_tx, events_rx) = mpsc::channel();
    let commands = fixture.router.window_commands.clone();
    let router = fixture.router.clone();
    let hold = fixture.router.shutdown.transition_guard();
    let coordinator = thread::spawn(move || {
        let entry = OnceLock::new();
        coordinate_window_events(
            &events_rx,
            &router,
            &commands,
            AppReadyTrigger::HostInitialized,
            |_| panic!("unentered selected UI cannot publish Ready"),
            Some((&entry, terminal_fixture)),
            None,
        )
    });
    let entered_cut = fixture.owner.recv_timeout(Duration::from_secs(2));
    let cut_while_held = matches!(&entered_cut, Ok(DirectPrimaryOwnerCommand::DenyRecovery));
    drop(hold); // Unblock a failing control before waiting for its cleanup.
    if !cut_while_held {
        assert!(matches!(
            fixture.owner.recv_timeout(Duration::from_secs(4))?,
            DirectPrimaryOwnerCommand::DenyRecovery
        ));
    }
    let DirectPrimaryOwnerCommand::Shutdown(reply) =
        fixture.owner.recv_timeout(Duration::from_secs(4))?
    else {
        panic!("exact startup terminal must shut down the current owner")
    };
    reply.send(Ok(()))?;
    let terminal = coordinator
        .join()
        .expect_err("the terminal never returns to the coordinator");
    assert_eq!(
        terminal.downcast_ref::<&'static str>(),
        Some(&"KELD-CORE-037")
    );
    assert!(
        cut_while_held,
        "the 10-second terminal depends on the held startup transition"
    );
    assert!(
        fixture
            .router
            .shutdown
            .reader_stop
            .load(std::sync::atomic::Ordering::Acquire)
    );
    assert_eq!(
        fixture.router.shutdown.cause(),
        SESSION_DESKTOP_UI_ENTRY_TIMEOUT
    );
    Ok(())
}

fn terminal_fixture(error: &HostAppError) -> ! {
    assert!(error.to_string().contains("10 s"), "{error}");
    std::panic::panic_any(error.code())
}

#[test]
fn cli_eof_reservation_wins_before_its_reader_stop_is_visible() {
    let shutdown = SessionShutdownState::new();
    let _native_transition = shutdown.transition_guard();
    assert!(shutdown.reserve_cause(SESSION_CLI_LEASE_LOST));
    assert!(
        !shutdown.reader_stop.load(Ordering::Acquire),
        "EOF has claimed its cause and has not reached the stop store"
    );
    assert!(
        !shutdown.claim_desktop_ui_entry_terminal(),
        "an earlier EOF reservation wins even while its reader stop is unset"
    );
    assert!(!shutdown.reader_stop.load(Ordering::Acquire));
    assert!(shutdown.accept_reserved_guarded(SESSION_CLI_LEASE_LOST));
    assert_eq!(shutdown.cause(), SESSION_CLI_LEASE_LOST);
}

#[test]
fn copied_entry_timeout_rechecks_the_fresh_entry_and_observed_eof() -> Result<(), TestError> {
    let fixture = DesktopFixture::new()?;
    fixture.subscribe(41)?;
    let expired = fixture.pending_deadline() - Duration::from_secs(20);
    let (original, correlation, _) = fixture
        .router
        .selected_desktop_startup_deadline(None, expired)?
        .expect("original pending timeout");
    assert!(
        !fixture
            .router
            .claim_desktop_ui_entry_timeout(&original, correlation, || {
                (expired, Some(expired - Duration::from_millis(1)))
            })?
    );
    assert!(!fixture.caller.reader_stop.load(Ordering::Acquire));
    fixture
        .router
        .latch_ready_writer_failure_for(&fixture.caller);
    assert!(fixture.router.pre_ready_link_failed.load(Ordering::Acquire));
    assert!(
        !fixture
            .router
            .claim_desktop_ui_entry_timeout(&original, correlation, || (expired, None),)?
    );
    assert!(!fixture.router.shutdown.reader_stop.load(Ordering::Acquire));
    Ok(())
}

#[test]
fn copied_entry_timeout_cannot_stop_a_replacement_or_duplicate_terminal() -> Result<(), TestError> {
    let fixture = DesktopFixture::new()?;
    fixture.subscribe(41)?;
    let expired = fixture.pending_deadline() - Duration::from_secs(20);
    let (original, correlation, _) = fixture
        .router
        .selected_desktop_startup_deadline(None, expired)?
        .expect("original pending timeout");
    fixture.router.retire_generation(fixture.caller.attempt)?;
    let (server, _peer) = primary_test_stream_pair();
    fixture
        .router
        .install_generation(fixture.caller.attempt + 1, server)
        .map_err(HostAppError::from)?;
    let successor = fixture
        .router
        .current_primary_writer_target()?
        .expect("actual new link");
    assert!(
        !fixture
            .router
            .claim_desktop_ui_entry_timeout(&original, correlation, || (expired, None),)?
    );
    assert!(!successor.caller().reader_stop.load(Ordering::Acquire));
    assert!(!fixture.router.shutdown.reader_stop.load(Ordering::Acquire));
    fixture.router.subscribe_window_ordered(
        successor.caller(),
        CorrelationId(42),
        keld_ipc::channel_table::WINDOW.wire_id(),
    )?;
    let next_expiry = fixture.pending_deadline() - Duration::from_secs(20);
    assert!(fixture.router.claim_desktop_ui_entry_timeout(
        &successor,
        CorrelationId(42),
        || (next_expiry, None),
    )?);
    assert!(!fixture.router.claim_desktop_ui_entry_timeout(
        &successor,
        CorrelationId(42),
        || (next_expiry, None),
    )?);
    assert!(successor.caller().reader_stop.load(Ordering::Acquire));
    successor.shutdown()?;
    let readers = std::mem::take(&mut *fixture.router.readers.lock().expect("reader owners"));
    for (_, reader) in readers {
        reader.join().expect("retired reader joins")?;
    }
    Ok(())
}

/// A real Runtime gate exposes its own armed/denied observation before either
/// terminal test hands its result back to the coordinator.
pub(super) fn arm_runtime_recovery(
    fixture: &DesktopFixture,
) -> Result<Arc<PrimaryRecoveryGate>, TestError> {
    let (runtime, recovery) = PrimaryRoleSupervisor::start_with_bound_generations_gated(
        PrimaryRoleConfig::primary("cmd.exe").args(["/C", "exit 0"]),
    )?;
    let recovery = Arc::new(recovery);
    assert!(recovery.arm());
    fixture.router.shutdown.transition_guard().direct_recovery = Some(Arc::clone(&recovery));
    runtime.shutdown();
    let _ = runtime.wait_for_outcome();
    Ok(recovery)
}
