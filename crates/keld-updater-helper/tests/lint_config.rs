//! Clippy reads only the first `clippy.toml` it finds, starting in this crate, so the
//! helper's file must restate every setting of the root file (KEL-270 T4d S9c).
#![allow(clippy::panic)] // extra test crate: panic is an assertion oracle

use std::path::Path;

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

#[test]
fn the_helper_clippy_config_restates_every_root_setting() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = settings(&manifest_dir.join("../../clippy.toml"));
    let helper = settings(&manifest_dir.join("clippy.toml"));
    assert!(!root.is_empty(), "the root clippy.toml holds settings");
    for setting in &root {
        assert!(
            helper.contains(setting),
            "crates/keld-updater-helper/clippy.toml must restate the root setting `{setting}`"
        );
    }
}
