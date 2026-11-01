//! Independent checks for the GH-569 Electron reference artifact.
//!
//! These checks do not assert any Keld role behavior. The normalized artifact is
//! derived from the separately retained v44.4.5 macOS oracle output.

#![allow(clippy::expect_used, clippy::panic)] // test assertions and fixture diagnostics

use serde_json::{Value, json};

// GH565 links these Electron members; GH478 implements the parented async
// dialog subset. GH502 removes non-Electron helper exports. This source-shape
// observation does not prove member behavior or installed-module identity.
const BASELINE_MEMBERS: &[&str] = &[
    "BrowserWindow",
    "ClipboardItem",
    "Menu",
    "app",
    "clipboard",
    "dialog",
    "nativeImage",
    "nativeTheme",
    "screen",
    "session",
    "shell",
];
const BASELINE_HELPERS: &[&str] = &[
    "ELECTRON_VERSION_SHIM",
    "UNSUPPORTED_MEMBER_CODE",
    "UnsupportedMemberError",
    "isCallError",
    "isUnsupportedMemberError",
];

fn facade_baseline_matches(observed: &Value) -> bool {
    observed["namespace"] == json!(BASELINE_MEMBERS)
        && observed["defaultKeys"] == json!(BASELINE_MEMBERS)
}

#[test]
fn direct_facade_shape_rejects_missing_extra_and_wrong_side_exports() {
    let baseline = json!({"namespace": BASELINE_MEMBERS, "defaultKeys": BASELINE_MEMBERS});
    assert!(facade_baseline_matches(&baseline));
    for field in ["namespace", "defaultKeys"] {
        let mut missing = baseline.clone();
        missing[field]
            .as_array_mut()
            .expect("explicit keys")
            .remove(0);
        assert!(!facade_baseline_matches(&missing), "missing {field} member");
        let mut extra = baseline.clone();
        extra[field]
            .as_array_mut()
            .expect("explicit keys")
            .push(json!("inventedMember"));
        assert!(!facade_baseline_matches(&extra), "extra {field} member");
        for helper in BASELINE_HELPERS {
            let mut leaked = baseline.clone();
            leaked[field]
                .as_array_mut()
                .expect("explicit keys")
                .push(json!(helper));
            assert!(
                !facade_baseline_matches(&leaked),
                "non-Electron helper {helper} leaked into {field}"
            );
        }
    }
}

#[test]
fn direct_facade_observation_and_bounded_release_snapshot() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .parent()
        .expect("workspace");
    let output = std::process::Command::new("bun")
        .args([
            "--no-install",
            "packages/@keld/electron/fixtures/module_process_probe.ts",
        ])
        .current_dir(workspace)
        .output()
        .expect("launch direct facade probe");
    assert!(
        output.status.success(),
        "facade probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: Value = serde_json::from_slice(&output.stdout).expect("one facade observation");
    assert!(
        facade_baseline_matches(&observed),
        "unexpected direct facade observation: {observed}"
    );
    let mutation = std::process::Command::new("bun")
        .args([
            "--no-install",
            "packages/@keld/electron/fixtures/module_export_mutation_probe.ts",
        ])
        .current_dir(workspace)
        .output()
        .expect("launch named-export mutation");
    assert!(
        mutation.status.success(),
        "export mutation probe failed: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let changed: Value =
        serde_json::from_slice(&mutation.stdout).expect("mutated facade observation");
    assert!(
        !facade_baseline_matches(&changed),
        "one real named-export removal escaped the baseline check"
    );
    assert_eq!(
        changed["defaultKeys"], observed["defaultKeys"],
        "mutation changed the unrelated default object"
    );
    let namespace = observed["namespace"].as_array().expect("namespace keys");
    let defaults = observed["defaultKeys"].as_array().expect("default keys");
    assert_eq!(json!(namespace), json!(BASELINE_MEMBERS));
    assert_eq!(json!(defaults), json!(BASELINE_MEMBERS));
    assert_eq!(
        namespace, defaults,
        "public namespace and default member sets must match"
    );
    let snapshot_bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/module-process-corpus/electron-npm-versions.json"
    ))
    .expect("read bounded npm snapshot");
    let snapshot: Value =
        serde_json::from_slice(&snapshot_bytes).expect("parse bounded npm snapshot");
    let versions = snapshot["versions"].as_array().expect("version list");
    let actual = observed["electronVersion"]
        .as_str()
        .expect("shim version string");
    assert_eq!(
        actual, "0.0.1",
        "unexpected facade version: update reviewed observation"
    );
    let outside_current_npm_set = |candidate: &str| !versions.iter().any(|v| v == candidate);
    assert!(
        outside_current_npm_set(actual),
        "facade version matches bounded npm set"
    );
    assert!(
        !outside_current_npm_set("44.4.5"),
        "known-release mutation must be detected"
    );
    // These raw facts do not score as a KEL-74 pass: both are source-only / Keld rules.
}

fn resolver_baseline_matches(observed: &Value) -> bool {
    observed["electron"] == json!({"resolved": true})
        && [
            "electron/main",
            "electron/renderer",
            "electron/common",
            "electron/utility",
        ]
        .iter()
        .all(|name| observed[*name] == json!({"resolved": true}))
}

#[test]
fn copied_keld_package_resolution_observation() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .parent()
        .expect("workspace");
    let output = std::process::Command::new("bun")
        .args([
            "--no-install",
            "packages/@keld/electron/fixtures/module_resolve_probe.ts",
        ])
        .current_dir(workspace)
        .output()
        .expect("launch resolver probe");
    assert!(
        output.status.success(),
        "resolver probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: Value =
        serde_json::from_slice(&output.stdout).expect("resolver observation JSON");
    assert!(
        resolver_baseline_matches(&observed),
        "unexpected resolver observation: {observed}"
    );
    assert_eq!(observed["electron"], json!({"resolved": true}));
    for name in [
        "electron/main",
        "electron/renderer",
        "electron/common",
        "electron/utility",
    ] {
        assert_eq!(
            observed[name],
            json!({"resolved": true}),
            "unexpected resolver observation for {name}"
        );
    }
    for (argument, removed) in [
        ("remove-main", "electron/main"),
        ("remove-utility", "electron/utility"),
    ] {
        let mutated = std::process::Command::new("bun")
            .args([
                "--no-install",
                "packages/@keld/electron/fixtures/module_resolve_probe.ts",
                argument,
            ])
            .current_dir(workspace)
            .output()
            .expect("launch one-export mutation");
        assert!(
            mutated.status.success(),
            "mutation probe failed: {}",
            String::from_utf8_lossy(&mutated.stderr)
        );
        let changed: Value =
            serde_json::from_slice(&mutated.stdout).expect("mutation observation JSON");
        assert_eq!(
            changed[removed],
            json!({"resolved": false, "code": "MODULE_NOT_FOUND"})
        );
        assert!(
            !resolver_baseline_matches(&changed),
            "one-export mutation escaped the observed-status assertion"
        );
        for name in [
            "electron",
            "electron/main",
            "electron/renderer",
            "electron/common",
            "electron/utility",
        ] {
            if name != removed {
                assert_eq!(
                    changed[name], observed[name],
                    "mutation changed unrelated {name}"
                );
            }
        }
        assert_ne!(
            changed[removed], observed[removed],
            "one-export mutation must falsify observed status"
        );
    }
    // Resolution alone is not proof of module identity, installed graph, or a role.
}
