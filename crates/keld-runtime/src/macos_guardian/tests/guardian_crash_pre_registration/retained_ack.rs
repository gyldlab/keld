//! Actual KGC1 `CONTROL_REGISTERED` withheld after host-native group custody.
//! The Guardian's real Bun process cannot exec before that acknowledgment.
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use super::super::super::process_group::validate_guardian_child_group;
use super::super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, generation_host::GenerationRegistrationFence,
};
use super::super::test_support::verified_real_bun;
use super::{GuardianCrashFixture, group_and_child_gone, guarded_role_command};

mod observers;
use self::observers::{await_executed_bun, prove_held_registration};

fn held_registered_ack(release_ack: bool) {
    let bun = verified_real_bun();
    let temp = tempfile::tempdir().expect("owned native KGC1 ACK test fixture");
    let marker = temp.path().join("bun-and-descendant.pid");
    let command = guarded_role_command(None, &marker, &bun);
    let pending =
        GuardianBootstrap::spawn_supervised(command).expect("real KGC1 Guardian bootstrap");
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let primary = pending
        .register_guarded_primary_until_with_registration_fence(
            Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
            GenerationRegistrationFence {
                attempt: 1,
                entered: entered_tx,
                release: release_rx,
                received_prepare: None,
                received_spawned: None,
            },
        )
        .expect("real authenticated persistent Guardian");
    let mut fixture = GuardianCrashFixture {
        primary,
        group_birth: None,
    };
    let group = entered_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("host retained native group, before sending REGISTERED");
    let group_i32 = i32::try_from(group).expect("positive native PGID");
    fixture.group_birth = Some(
        validate_guardian_child_group(group, fixture.primary.guardian_pid())
            .expect("host-retained group must still be original Guardian's direct child"),
    );
    let harness_image = prove_held_registration(&fixture, group, &marker);
    let mut descendant = None;
    if release_ack {
        release_tx
            .send(())
            .expect("release genuine KGC1 REGISTERED");
        let native_descendant = await_executed_bun(&marker, group_i32, &harness_image);
        descendant = Some(native_descendant);
    }
    // Kill only the retained Guardian. The host's registered-group fallback
    // must retire the pre-exec leader or the real Bun+sleep group.
    fixture
        .primary
        .child
        .kill()
        .expect("SIGKILL retained native Guardian");
    let _ = release_tx.send(());
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let error = loop {
        match fixture.primary.poll_fatal() {
            Ok(()) => {
                assert!(Instant::now() < deadline, "missed actual Guardian SIGKILL");
                thread::yield_now();
            }
            Err(error) => break error,
        }
    };
    assert!(error.to_string().contains("guardian"), "{error}");
    group_and_child_gone(group_i32, descendant);
    if !release_ack {
        assert!(
            !marker.exists(),
            "Bun ran despite withheld KGC1 ACK and Guardian death"
        );
    }
    fixture.group_birth = None;
}

#[test]
fn real_kgc1_guardian_sigkill_while_registered_ack_is_held_denies_bun_exec() {
    held_registered_ack(false);
}

#[test]
fn real_kgc1_registered_ack_release_executes_bun_then_guardian_reaps_group() {
    held_registered_ack(true);
}
