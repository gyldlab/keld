//! Real-Linux KEL-96/T4 no-flag host acceptance.

#![cfg(target_os = "linux")]
#![allow(clippy::expect_used, clippy::panic)] // process and filesystem observations are assertion oracles

#[path = "no_flag/linux/dev_lifecycle.rs"]
mod dev_lifecycle;
#[path = "no_flag/linux/lifecycle.rs"]
mod lifecycle;

#[path = "no_flag/linux/recovery.rs"]
mod recovery;
#[path = "no_flag/linux/staging.rs"]
mod staging;
#[path = "no_flag/linux/support/mod.rs"]
mod support;

#[test]
fn keld_dev_linux_helper() {
    let Some(project) = std::env::var_os("KELD_T4_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(std::path::Path::new(&project)).expect("shipping Linux keld dev helper");
}
