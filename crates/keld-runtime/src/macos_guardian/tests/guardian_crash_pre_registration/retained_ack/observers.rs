//! Independent kernel/JS observations for KGC1 before and after REGISTERED.
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use nix::unistd::{Pid, getpgid};

use super::super::super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, process_group::validate_guardian_child_group,
};
use super::super::super::test_support::kernel_process_image;
use super::super::GuardianCrashFixture;

pub(super) fn prove_held_registration(
    fixture: &GuardianCrashFixture,
    group: u32,
    marker: &Path,
) -> String {
    let group_i32 = i32::try_from(group).expect("positive native PGID");
    assert_eq!(
        fixture.primary.group_pid(),
        Some(group),
        "actual host owns the group before emitting REGISTERED"
    );
    let guardian = fixture.primary.guardian_pid();
    assert_ne!(Pid::from_raw(group_i32), nix::unistd::getpgrp());
    assert_eq!(
        getpgid(Some(Pid::from_raw(group_i32))),
        Ok(Pid::from_raw(group_i32)),
        "native first generation has its own process group"
    );
    validate_guardian_child_group(group, guardian)
        .expect("registered child is kernel-proven direct Guardian child");
    let harness_image = kernel_process_image(i32::try_from(std::process::id()).expect("PID"));
    let held_until = Instant::now() + Duration::from_millis(100);
    loop {
        assert_eq!(
            kernel_process_image(group_i32),
            harness_image,
            "KGC1 child exec'd Bun before the REGISTERED ACK"
        );
        assert!(!marker.exists(), "Bun script ran before CONTROL_REGISTERED");
        if Instant::now() >= held_until {
            break;
        }
        thread::yield_now();
    }
    harness_image
}

pub(super) fn await_executed_bun(marker: &Path, group: i32, pre_ack_image: &str) -> i32 {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        if let Ok(contents) = std::fs::read_to_string(marker) {
            let ids: Vec<i32> = contents
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect();
            if ids.len() == 2 {
                assert_eq!(ids[0], group, "real Bun is the group leader");
                assert_eq!(
                    getpgid(Some(Pid::from_raw(ids[1]))),
                    Ok(Pid::from_raw(group)),
                    "actual Bun descendant remains in Guardian's group"
                );
                assert_ne!(
                    kernel_process_image(group),
                    pre_ack_image,
                    "CONTROL_REGISTERED must permit Bun's executable image"
                );
                return ids[1];
            }
        }
        assert!(
            Instant::now() < deadline,
            "Bun never executed after KGC1 ACK"
        );
        thread::yield_now();
    }
}
