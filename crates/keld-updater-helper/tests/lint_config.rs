//! The helper's `clippy.toml` (KEL-270 T4d S9c). Clippy reads only the first
//! `clippy.toml` it finds, starting in this crate, so the helper's file must restate
//! every setting of the root file; and its `disallowed-methods` must keep naming every
//! process start that KEL-53 criterion 17 excludes from the elevated helper.
#![allow(clippy::panic)] // extra test crate: panic is an assertion oracle

use std::path::Path;

/// Every process start the elevated helper must not reach: the `keld-runtime` entry
/// points that start a Bun child or an app role, and the `std::process::Command`
/// methods that start a process directly (KEL-53 §4, §7 "17 (helper launch and
/// self-anchor)").
const DISALLOWED_PROCESS_STARTS: [&str; 9] = [
    "std::process::Command::spawn",
    "std::process::Command::output",
    "std::process::Command::status",
    "keld_runtime::Supervisor::start",
    "keld_runtime::Supervisor::start_with_stdout_markers",
    "keld_runtime::primary::PrimaryRoleSupervisor::start",
    "keld_runtime::primary::PrimaryRoleSupervisor::start_with_bound_generations",
    "keld_runtime::primary::PrimaryRoleSupervisor::start_with_bound_generations_gated",
    "keld_runtime::windows_lpac::WindowsLpacProfile::spawn_suspended",
];

/// The non-comment, non-blank lines of a `clippy.toml`.
fn settings(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn helper_settings() -> Vec<String> {
    settings(&Path::new(env!("CARGO_MANIFEST_DIR")).join("clippy.toml"))
}

#[test]
fn the_helper_clippy_config_restates_every_root_setting() {
    let root = settings(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clippy.toml"));
    let helper = helper_settings();
    assert!(!root.is_empty(), "the root clippy.toml holds settings");
    for setting in &root {
        assert!(
            helper.contains(setting),
            "crates/keld-updater-helper/clippy.toml must restate the root setting `{setting}`"
        );
    }
}

#[test]
fn the_helper_clippy_config_disallows_every_process_start() {
    let helper = helper_settings();
    for path in DISALLOWED_PROCESS_STARTS {
        let entry = format!("{{ path = \"{path}\", reason = \"");
        assert!(
            helper.iter().any(|line| line.starts_with(&entry)),
            "crates/keld-updater-helper/clippy.toml must disallow `{path}` with a reason"
        );
    }
}
