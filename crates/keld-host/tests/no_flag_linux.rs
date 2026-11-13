//! Real-Linux KEL-96/T4 no-flag host acceptance.

#![cfg(target_os = "linux")]
#![allow(clippy::expect_used, clippy::panic)] // process and filesystem observations are assertion oracles

#[path = "no_flag/linux/dev_lifecycle.rs"]
mod dev_lifecycle;
#[path = "no_flag/linux/lifecycle.rs"]
mod lifecycle;

#[path = "no_flag/linux/recovery.rs"]
mod recovery;
// GH-1081 round-1 research experiments only (never merged).
#[path = "no_flag/linux/crash_recovery_r1.rs"]
mod crash_recovery_r1;
#[path = "no_flag/crash_recovery_r1_heartbeat.rs"]
mod crash_recovery_r1_heartbeat;
#[path = "no_flag/linux/crash_recovery_r2.rs"]
mod crash_recovery_r2;
#[path = "no_flag/crash_recovery_r2_probe.rs"]
mod crash_recovery_r2_probe;
// GH-1081 round-4 KEL-79 origin experiments (research only, never merged).
#[path = "no_flag/linux/origin_r4.rs"]
mod origin_r4;
#[path = "no_flag/origin_r4_lab.rs"]
mod origin_r4_lab;
// GH-1081 round-5 KEL-79 origin experiments X4-X9 (research only, never merged).
#[path = "no_flag/linux/origin_r5.rs"]
mod origin_r5;
#[path = "no_flag/origin_r5_lab.rs"]
mod origin_r5_lab;
#[path = "no_flag/linux/staging.rs"]
mod staging;
#[path = "no_flag/linux/support/mod.rs"]
mod support;
#[path = "../../keld-wv/tests/fixtures/windows_renderer_http.rs"]
mod windows_renderer_http;

#[test]
fn keld_dev_linux_helper() {
    let Some(project) = std::env::var_os("KELD_T4_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(std::path::Path::new(&project)).expect("shipping Linux keld dev helper");
}
