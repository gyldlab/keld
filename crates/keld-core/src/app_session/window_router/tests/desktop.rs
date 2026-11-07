//! GH481 tests-first S1 Core writer contract. These bind a proposed
//! `AppWindowEvent::DesktopObserved` native-value shape; the root owner reviews
//! the final engine type when implementing, and must run real Mac/native tests.
//! No marker, native callback or fake Monitor observation is accepted as OS proof.

use super::*;
use keld_ipc::window::{DesktopFact, DesktopFacts};
use keld_wv::{AppDesktopDisplay, AppDesktopFacts, AppDesktopRect};

fn desktop_fixture() -> TestResult<Fixture> {
    let fixture = Fixture::new(Some(true))?;
    // Existing GH531 component fixtures intentionally keep their legacy
    // Subscribe behavior. Select approved S1 explicitly for these new cases.
    fixture
        .handle
        .windows
        .as_ref()
        .ok_or("window service is absent")?
        .lock()
        .map_err(|_| "window service lock is poisoned")?
        .desktop_required = true;
    Ok(fixture)
}

fn observed(dark: bool) -> AppDesktopFacts {
    AppDesktopFacts {
        primary_id: 17,
        displays: vec![AppDesktopDisplay {
            id: 17,
            bounds: AppDesktopRect {
                x: 0,
                y: 0,
                width: 1440,
                height: 900,
            },
            work_area: AppDesktopRect {
                x: 0,
                y: 24,
                width: 1440,
                height: 876,
            },
            scale_factor: 2.0,
        }],
        should_use_dark_colors: dark,
    }
}

fn expected(epoch: u64, value: Option<bool>) -> WindowEvent {
    WindowEvent::DesktopSnapshot {
        epoch,
        snapshot: value.map(|dark| DesktopFacts {
            primary_id: 17,
            displays: vec![DesktopFact {
                id: 17,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    width: 1440,
                    height: 900,
                },
                work_area: Rect {
                    x: 0,
                    y: 24,
                    width: 1440,
                    height: 876,
                },
                scale_factor: 2.0,
            }],
            should_use_dark_colors: dark,
        }),
    }
}

fn sample(fixture: &Fixture, value: Option<bool>) -> TestResult {
    fixture
        .handle
        .window_native_event(AppWindowEvent::DesktopObserved {
            facts: value.map(observed),
        })?;
    Ok(())
}

fn pending(fixture: &Fixture, correlation: u32) -> TestResult {
    fixture.handle.subscribe_window(
        &fixture.caller()?,
        CorrelationId(correlation),
        keld_ipc::channel_table::WINDOW.wire_id(),
    )?;
    Ok(())
}

fn accepted(fixture: &mut Fixture, correlation: u32) -> TestResult {
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Reply,
            LIFECYCLE_CHANNEL,
            CorrelationId(correlation)
        )
    );
    assert_eq!(
        decode::<LifecycleResponse>(&bytes)?,
        LifecycleResponse::Subscribed
    );
    Ok(())
}

fn unavailable(fixture: &mut Fixture, correlation: u32) -> TestResult {
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Err,
            LIFECYCLE_CHANNEL,
            CorrelationId(correlation)
        )
    );
    assert_eq!(decode::<CallError>(&bytes)?.code, "KELD-CORE-049");
    Ok(())
}

fn duplicate_refusal(fixture: &mut Fixture, correlation: u32) -> TestResult {
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Err,
            LIFECYCLE_CHANNEL,
            CorrelationId(correlation)
        )
    );
    assert_eq!(decode::<CallError>(&bytes)?.code, "KELD-IPC-005");
    Ok(())
}

#[test]
fn desktop_pending_subscribe_waits_for_native_sample_and_replays_before_real_raw_ready()
-> TestResult {
    let mut fixture = desktop_fixture()?;
    pending(&fixture, 71)?;
    assert!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("service")
            .lock()
            .expect("lock")
            .subscribed
            .is_none(),
        "a pending correlation is not an admitted subscriber"
    );
    pending(&fixture, 70)?;
    duplicate_refusal(&mut fixture, 70)?;
    sample(&fixture, Some(false))?;
    fact(&mut fixture, &expected(1, Some(false)))?;
    accepted(&mut fixture, 71)?;
    fixture.ready()?;
    // Repeated accepted Subscribe is idempotent and produces no second snapshot:
    // the first frame observed must already be the accepted lifecycle reply.
    pending(&fixture, 72)?;
    accepted(&mut fixture, 72)?;
    Ok(())
}

#[test]
fn desktop_duplicate_of_original_pending_correlation_retires_the_link() -> TestResult {
    let mut fixture = desktop_fixture()?;
    let caller = fixture.caller()?;
    pending(&fixture, 51)?;
    // Core cannot issue an ERR to the original c and later issue Subscribed(c):
    // both would settle the same WorkerLink correlation. Match the existing
    // fatal duplicate-window-CALL rule instead of replaying or minting a slot.
    drop(fixture.owner_commands);
    let failure = fixture
        .handle
        .subscribe_window(
            &caller,
            CorrelationId(51),
            keld_ipc::channel_table::WINDOW.wire_id(),
        )
        .expect_err("same-correlation pending Subscribe must retire");
    assert!(failure.to_string().contains("KELD-IPC-005"), "{failure}");
    assert!(fixture.handle.current.lock().expect("current").is_none());
    assert!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("service")
            .lock()
            .expect("lock")
            .pending_desktop
            .is_none(),
        "retired generation cannot retain a pending correlated reply"
    );
    assert!(
        fixture.commands.try_recv().is_err(),
        "duplicate subscription cannot enqueue window/native effects"
    );
    assert!(
        read_frame(&mut fixture.peer).is_err(),
        "retired original correlation receives link loss, never a forged second settlement"
    );
    // A late real native sample may update the Host's latest source, but has
    // no old-generation writer and cannot complete the retired c.
    fixture
        .handle
        .window_native_event(AppWindowEvent::DesktopObserved {
            facts: Some(observed(false)),
        })?;
    Ok(())
}

#[test]
fn desktop_real_init_first_raw_ready_then_subscribe_gets_latest_sample() -> TestResult {
    let mut fixture = desktop_fixture()?;
    // Native first-Init pushes a sample before its HostInitialized event. This
    // model performs exactly that Core sequence, without claiming an OS run.
    sample(&fixture, Some(false))?;
    fixture.ready()?;
    pending(&fixture, 81)?;
    fact(&mut fixture, &expected(1, Some(false)))?;
    accepted(&mut fixture, 81)?;
    Ok(())
}

#[test]
fn desktop_definitive_init_unavailability_ends_pending_correlation_without_any_replay() -> TestResult
{
    let mut fixture = desktop_fixture()?;
    pending(&fixture, 91)?;
    sample(&fixture, None)?;
    unavailable(&mut fixture, 91)?;
    assert!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("service")
            .lock()
            .expect("lock")
            .subscribed
            .is_none(),
        "failed desktop Init must not install a window subscriber"
    );
    pending(&fixture, 92)?;
    unavailable(&mut fixture, 92)?;
    assert!(
        fixture.commands.try_recv().is_err(),
        "no native work on denied Subscribe"
    );
    Ok(())
}

#[test]
fn desktop_unsubscribed_link_gets_zero_facts_until_its_own_admission() -> TestResult {
    let mut fixture = desktop_fixture()?;
    sample(&fixture, Some(false))?;
    sample(&fixture, Some(true))?;
    assert!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("service")
            .lock()
            .expect("lock")
            .subscribed
            .is_none(),
        "native sample must not grant a role"
    );
    pending(&fixture, 93)?;
    // If either earlier sample was broadcast, this frame/epoch and the
    // immediately following correlated reply cannot both match.
    fact(&mut fixture, &expected(2, Some(true)))?;
    accepted(&mut fixture, 93)?;
    assert_eq!(
        subscription_refusal(
            ReceiveClass::HostCallWithEvents,
            Authority::HostInternal,
            false,
            false
        ),
        Some(SubscribeRefusal::RoleNotAdmitted),
        "existing nonprimary role policy is unchanged"
    );
    Ok(())
}

#[test]
fn desktop_after_subscribe_update_is_ordered_after_subscribed_and_recovery_replays_latest()
-> TestResult {
    let mut fixture = desktop_fixture()?;
    sample(&fixture, Some(false))?;
    pending(&fixture, 101)?;
    fact(&mut fixture, &expected(1, Some(false)))?;
    accepted(&mut fixture, 101)?;
    fixture.ready()?;
    let window = fixture.build(102, 7)?;
    sample(&fixture, Some(true))?;
    fact(&mut fixture, &expected(2, Some(true)))?;
    fixture.handle.retire_generation(1)?;
    install_successor(&mut fixture)?;
    pending(&fixture, 103)?;
    fact(&mut fixture, &expected(2, Some(true)))?;
    fact(
        &mut fixture,
        &WindowEvent::Created {
            window,
            state: native_state(snapshot()),
            pending_close: None,
        },
    )?;
    accepted(&mut fixture, 103)?;
    Ok(())
}

#[test]
fn desktop_retirement_fences_pending_c_and_late_sample_after_caller_link_close() -> TestResult {
    let fixture = desktop_fixture()?;
    pending(&fixture, 111)?;
    fixture.handle.retire_generation(1)?;
    sample(&fixture, Some(false))?;
    let session = fixture
        .handle
        .windows
        .as_ref()
        .expect("service")
        .lock()
        .expect("lock");
    assert!(session.subscribed.is_none());
    assert!(fixture.handle.current.lock().expect("current").is_none());
    // The full 5,000-ms Bun call-start deadline is exercised by the
    // LifecycleLink/WorkerLink integration; here retirement is its
    // independently observable Host-side outcome.
    Ok(())
}

#[test]
fn desktop_epoch_and_payload_limit_fail_before_publication() {
    // The Core writer's actual bounded guards are independently testable even
    // though a valid <=32 fixed-shape display value encodes below 32 KiB.
    let mut epoch = super::super::DesktopEpoch(u64::MAX);
    assert!(epoch.advance().is_err(), "never wrap to zero after max");
    assert!(super::super::admit_desktop_payload_bytes(32 * 1024).is_ok());
    assert!(super::super::admit_desktop_payload_bytes(32 * 1024 + 1).is_err());
}
