//! The elevated helper's normal dependency closure holds no crate that `deny.toml` bans
//! (KEL-53 §4 "Helper launch and self-anchor", criterion 17), checked with `cargo tree`
//! independently of the `cargo-deny` run in CI. `deny.toml` owns the list.
#![cfg(windows)]
#![allow(clippy::expect_used)] // extra test crate: expect is an assertion oracle
#![allow(clippy::disallowed_methods)] // test-only: Command::output runs `cargo tree`, the independent edge-set observer

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// The crate names of `deny.toml`'s `[bans] deny` entries. Every non-comment line of
/// the list must be one `{ crate = "…", … }` entry, so a reformatted entry fails here
/// instead of silently leaving the check.
fn banned() -> BTreeSet<String> {
    let config = std::fs::read_to_string(Path::new(MANIFEST_DIR).join("deny.toml"))
        .expect("read the helper's deny.toml");
    let (_, list) = config
        .split_once("\ndeny = [")
        .expect("deny.toml has a `deny = [` list");
    let (list, _) = list.split_once("\n]").expect("the deny list closes");
    let entries: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let names: BTreeSet<String> = entries
        .iter()
        .filter_map(|line| line.strip_prefix("{ crate = \""))
        .filter_map(|rest| rest.split_once('"'))
        .map(|(name, _)| name.to_owned())
        .collect();
    assert_eq!(
        names.len(),
        entries.len(),
        "every deny.toml ban must be one distinct `{{ crate = \"…\" }}` line: {entries:?}"
    );
    names
}

/// Every package in the helper's normal closure for the Windows MSVC target.
fn normal_closure() -> BTreeSet<String> {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "--locked",
            "--color",
            "never",
            "-p",
            "keld-updater-helper",
            "-e",
            "normal",
            "--target",
            "x86_64-pc-windows-msvc",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .current_dir(MANIFEST_DIR)
        .output()
        .expect("run cargo tree");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 cargo tree output");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_normal_closure_holds_no_banned_crate() {
    let banned = banned();
    for name in [
        "keld-core",
        "keld-wv",
        "keld-host",
        "keld-cli",
        "keld-native",
    ] {
        assert!(
            banned.contains(name),
            "deny.toml must ban {name} (KEL-53 §4)"
        );
    }
    let closure = normal_closure();
    for edge in ["keld-guard", "keld-ipc", "keld-runtime", "keld-update"] {
        assert!(
            closure.contains(edge),
            "{edge} is one of the helper's normal edges: {closure:?}"
        );
    }
    let found: Vec<&String> = closure.intersection(&banned).collect();
    assert!(
        found.is_empty(),
        "banned crates in the helper's normal closure: {found:?}"
    );
}
