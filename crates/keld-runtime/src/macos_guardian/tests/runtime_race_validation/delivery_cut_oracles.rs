//! Scenarios own fault injection, observable cleanup and independent admission assertions.
use std::thread;
use std::time::Instant;

use super::delivery_assertions::{assert_cleanup_route, assert_cut_outcome};
use super::delivery_fixture::{CutFixture, CutObservation};
use super::records_and_cut_types::{COLLECTION, DeliveryCut, RelayCommand, RelayReceipt};
use super::{GuardedPrimaryUpdate, RoleEvent, assert_gone};

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
            loop {
                if rustix::io::ioctl_fionread(
                    fixture
                        .plane
                        .consumption_probe
                        .as_ref()
                        .expect("observation alias"),
                )
                .expect("existing safe available-byte primitive")
                    == 0
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "valid prefix not consumed by sole control reader"
                );
                thread::yield_now();
            }
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
            fixture
                .phase_release
                .send(())
                .expect("release only after actual Q ACK");
        }
    }
    let observation = fixture.collect(&response);
    assert_observation(cut, &observation, began);
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
