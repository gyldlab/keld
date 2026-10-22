//! Scenarios own fault injection, observable cleanup and independent admission assertions.
use std::os::fd::AsFd;
use std::thread;
use std::time::Instant;

use super::delivery_assertions::{assert_cleanup_route, assert_cut_outcome};
use super::delivery_fixture::{CutFixture, CutObservation};
use super::records_and_cut_types::{COLLECTION, DeliveryCut, RelayCommand, RelayReceipt};
use super::{GUARDIAN_SHUTDOWN_DEADLINE, GuardedPrimaryUpdate, RoleEvent, assert_gone};

#[test]
fn prepared_delivery_zero_bytes_retires_exact_unclaimed_host_lease() {
    run_delivery_cut(DeliveryCut::Zero);
}

#[test]
fn prepared_delivery_partial_prefix_keeps_framing_clock_and_typed_timeout() {
    run_delivery_cut(DeliveryCut::Partial);
}

#[test]
fn prepared_delivery_full_before_lease_uses_normal_supervisor_revoke() {
    run_delivery_cut(DeliveryCut::FullBeforeLease);
}

#[test]
fn prepared_delivery_post_lease_pre_spawn_uses_normal_supervisor_revoke() {
    run_delivery_cut(DeliveryCut::LeaseBeforeSpawn);
}

fn run_delivery_cut(cut: DeliveryCut) {
    let mut fixture = CutFixture::start(cut);
    let response = fixture.release_after_clear();
    let mut began = None;
    let mut released = None;
    match cut {
        DeliveryCut::Zero => {
            fixture.accept_actual_q();
            fixture
                .plane
                .commands
                .send(RelayCommand::AwaitClosure)
                .expect("zero-byte closure collection");
        }
        DeliveryCut::Partial => {
            fixture
                .plane
                .commands
                .send(RelayCommand::Prefix)
                .expect("one canonical prefix");
            began = match fixture
                .plane
                .receipts
                .recv_timeout(COLLECTION)
                .expect("actual prefix write receipt")
            {
                RelayReceipt::PrefixSent(time) => Some(time),
                _ => panic!("unexpected prefix receipt"),
            };
            let deadline = Instant::now() + COLLECTION;
            await_native_prefix_consumed(
                fixture
                    .plane
                    .consumption_probe
                    .as_ref()
                    .expect("observation alias"),
                deadline,
                |_| {},
            );
            drop(fixture.plane.consumption_probe.take());
            fixture.accept_actual_q();
            fixture
                .plane
                .commands
                .send(RelayCommand::AwaitClosure)
                .expect("partial suffix remains withheld until actual failure/EOF");
        }
        DeliveryCut::FullBeforeLease | DeliveryCut::LeaseBeforeSpawn => {
            fixture
                .plane
                .commands
                .send(RelayCommand::Full)
                .expect("full canonical Prepared2 release");
            fixture
                .phase_entered
                .recv_timeout(COLLECTION)
                .expect("exact factory/returned-lease boundary");
            fixture.accept_actual_q();
            released = Some(Instant::now());
            fixture
                .phase_release
                .send(())
                .expect("release only after actual Q ACK");
        }
    }
    let observation = fixture.collect(&response);
    assert_observation(cut, &observation, began);
    if let Some(released) = released {
        // The never-spawned successor has no child to report a PID. Its
        // revoke must not wait out the pre-exec PID receipt deadline, or an
        // orderly host shutdown exceeds its own Guardian exit deadline.
        let revoke = observation.terminal_at.duration_since(released);
        assert!(
            revoke < GUARDIAN_SHUTDOWN_DEADLINE / 4,
            "unspawned successor revoke stalled {revoke:?} on the PID receipt deadline"
        );
    }
}

#[derive(Clone, Copy)]
enum NativePrefixProbeStage {
    BeforeNativeProbe,
    AfterNativeProbe(u64),
}

fn await_native_prefix_consumed(
    probe: &impl AsFd,
    deadline: Instant,
    mut observe: impl FnMut(NativePrefixProbeStage),
) {
    loop {
        assert!(
            Instant::now() < deadline,
            "valid prefix probe must start within original collection deadline"
        );
        observe(NativePrefixProbeStage::BeforeNativeProbe);
        let available =
            rustix::io::ioctl_fionread(probe).expect("existing safe available-byte primitive");
        observe(NativePrefixProbeStage::AfterNativeProbe(available));
        assert!(
            Instant::now() < deadline,
            "native prefix absence completed after native probe original deadline"
        );
        if available == 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "valid prefix not consumed by sole control reader"
        );
        thread::yield_now();
    }
}

#[test]
fn late_real_native_prefix_absence_cannot_cross_original_probe_deadline() {
    use std::io::{Read as _, Write as _};
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    let (probe, mut peer) = UnixStream::pair().expect("own actual Unix socket pair");
    peer.write_all(b"PART")
        .expect("queue real prefix before original deadline");
    assert_eq!(
        rustix::io::ioctl_fionread(&probe).expect("real queued bytes"),
        4
    );
    let mut actual_reader = probe.try_clone().expect("own read-only-consumption alias");
    actual_reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("bound real native prefix reader");
    let deadline = Instant::now() + Duration::from_millis(350);
    let (go_tx, go_rx) = mpsc::channel();
    let (actual_tx, actual_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        go_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("controller authorized actual native consumption");
        let mut prefix = [0_u8; 4];
        actual_reader
            .read_exact(&mut prefix)
            .expect("consume true queued Unix bytes");
        actual_tx
            .send((prefix, Instant::now()))
            .expect("report actual native prefix receipt");
    });
    let before_probe_at = std::cell::Cell::new(None);
    let native_zero_probe_at = std::cell::Cell::new(None);
    let actual_consumed_at = std::cell::Cell::new(None);
    let released = std::cell::Cell::new(false);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        await_native_prefix_consumed(&probe, deadline, |stage| match stage {
            NativePrefixProbeStage::BeforeNativeProbe if !released.replace(true) => {
                before_probe_at.set(Some(Instant::now()));
                go_tx.send(()).expect("release original native reader");
                let (bytes, instant) = actual_rx
                    .recv_timeout(Duration::from_secs(2))
                    .expect("true native prefix must be consumed by test-owned reader");
                assert_eq!(bytes, *b"PART");
                actual_consumed_at.set(Some(instant));
                while Instant::now() < deadline + Duration::from_millis(40) {
                    thread::yield_now();
                }
            }
            NativePrefixProbeStage::AfterNativeProbe(0) => {
                native_zero_probe_at.set(Some(Instant::now()));
            }
            NativePrefixProbeStage::BeforeNativeProbe
            | NativePrefixProbeStage::AfterNativeProbe(_) => {}
        });
    }));
    reader
        .join()
        .expect("join actual native prefix consumer even after failing oracle");
    assert!(
        before_probe_at
            .get()
            .is_some_and(|instant| instant < deadline),
        "the original kernel-read entry must occur before collection cutoff"
    );
    assert!(
        actual_consumed_at
            .get()
            .is_some_and(|instant| instant < deadline),
        "actual prefix consumption must complete before cutoff"
    );
    assert!(
        native_zero_probe_at
            .get()
            .is_some_and(|instant| instant > deadline),
        "actual kernel zero-available probe must occur after original deadline"
    );
    let panic = result.expect_err("late native zero-available probe cannot grant success");
    let reason = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("expected native probe deadline panic");
    assert!(
        reason.contains("after native probe"),
        "post-probe original-clock fence must reject actual late native result: {reason}"
    );
}

#[test]
fn timely_real_native_prefix_consumption_still_completes() {
    use std::io::{Read as _, Write as _};
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    let (probe, mut peer) = UnixStream::pair().expect("own actual Unix socket pair");
    peer.write_all(b"DATA").expect("queue real native bytes");
    let mut actual_reader = probe.try_clone().expect("clone test-owned native reader");
    actual_reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("bound actual reader");
    let deadline = Instant::now() + Duration::from_secs(2);
    let (go_tx, go_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        go_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("consumer release within test budget");
        let mut consumed = [0_u8; 4];
        actual_reader
            .read_exact(&mut consumed)
            .expect("actually consume Unix bytes");
        consumed
    });
    let released = std::cell::Cell::new(false);
    let observed_positive = std::cell::Cell::new(false);
    let observed_zero = std::cell::Cell::new(false);
    await_native_prefix_consumed(&probe, deadline, |stage| match stage {
        NativePrefixProbeStage::AfterNativeProbe(available) if available > 0 => {
            observed_positive.set(true);
            if !released.replace(true) {
                go_tx.send(()).expect("release sole native reader");
            }
        }
        NativePrefixProbeStage::AfterNativeProbe(0) => observed_zero.set(true),
        NativePrefixProbeStage::BeforeNativeProbe | NativePrefixProbeStage::AfterNativeProbe(_) => {
        }
    });
    assert_eq!(reader.join().expect("join owned native consumer"), *b"DATA");
    assert!(
        observed_positive.get() && observed_zero.get(),
        "actual native FIONREAD must change from positive to zero"
    );
    assert!(
        Instant::now() < deadline,
        "positive fixture remains inside original collection budget"
    );
}

fn assert_observation(cut: DeliveryCut, observed: &CutObservation, began: Option<Instant>) {
    observed
        .relay
        .as_ref()
        .expect("real control EOF forwarded after guardian-side owner drop");
    observed
        .host
        .as_ref()
        .expect("actual exact host lease cleanup completed before EOF success");
    assert!(!observed.started_two && observed.current_pid.is_none());
    assert!(observed.current_group_pid.is_none());
    assert_eq!(observed.ledger.self_termination_count, 1);
    assert_eq!(
        observed.ledger.last_self_termination.map(|entry| entry.pid),
        Some(observed.initial_pid)
    );
    assert_gone(Some(observed.initial_pid));
    assert!(
        observed
            .ledger
            .last_self_termination
            .is_some_and(|entry| entry.exit_code == Some(1))
    );
    assert!(observed.output.stdout.contains("KEL143_RACE_G1"));
    assert!(!observed.output.stdout.contains("KEL143_RACE_BAD_G2") && observed.no_effect);
    assert!(observed.updates.iter().any(|update| matches!(
        update,
        GuardedPrimaryUpdate::Role(RoleEvent::Provisioned { attempt: 2, .. })
    )));
    assert!(observed.updates.iter().any(|update| matches!(
        update,
        GuardedPrimaryUpdate::Role(RoleEvent::Revoked {
            attempt: 2,
            cause: crate::role::RoleRevocationCause::Shutdown,
            ..
        })
    )));
    assert!(!observed.updates.iter().any(
        |update| matches!(update, GuardedPrimaryUpdate::Bound(bound) if bound.attempt() == 2)
    ));
    assert!(!observed.updates.iter().any(|update| matches!(
        update,
        GuardedPrimaryUpdate::Role(
            RoleEvent::Spawned { attempt: 2, .. } | RoleEvent::LinkBound { attempt: 2, .. }
        )
    )));
    assert!(
        observed.endpoint_gone,
        "exact minted successor listener survived"
    );
    assert_cleanup_route(cut, &observed.receipts);
    assert_cut_outcome(cut, &observed.terminal, &observed.outcome, began);
    println!(
        "KEL143_PREPARED_CUT cut={cut:?} actual_mint=true q_ack=true exact_revoked=2 listener_absent={} g2_effect_absent={} direct_child_reaped=true",
        observed.endpoint_gone, observed.no_effect
    );
}
