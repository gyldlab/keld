//! #634: `keld_ipc::ChannelId::for_test`, the one constructor of an arbitrary
//! channel id outside `keld-ipc`, never reaches a production build.
//!
//! The constructor exists only with keld-ipc's non-default `test-channel-ids`
//! feature (spec `docs/specs/gh508-kipc-channel-table.md` §11). The oracle is
//! Cargo's own normalized view of every workspace manifest (`cargo metadata
//! --no-deps`, inherited `[workspace.dependencies]` features merged in): a
//! production build enables the feature only through a normal or build
//! dependency on `keld-ipc`, a feature that forwards it, or a keld-ipc feature
//! (such as `default`) that implies it. Dev-dependencies are the allowed path.
//!
//! It lives in `keld-cli` beside the channel-table scan for the same reason:
//! `tools/ci-inputs.json` routes any `crates/*` change to this package, so a
//! manifest that starts enabling the feature is checked on the PR that adds it.

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are the assertion oracles

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

const FEATURE: &str = "test-channel-ids";
const OWNER: &str = "keld-ipc";

fn workspace_metadata() -> Value {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata JSON")
}

fn packages(metadata: &Value) -> &[Value] {
    metadata["packages"].as_array().map_or(&[], Vec::as_slice)
}

fn strings(value: &Value) -> impl Iterator<Item = &str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn owner_dependencies(package: &Value) -> impl Iterator<Item = &Value> {
    package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|dependency| dependency["name"] == OWNER)
}

/// Every way a workspace manifest enables [`FEATURE`] for a production build,
/// as `"<package>: <how>"`.
fn production_enablers(metadata: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for package in packages(metadata) {
        let name = package["name"].as_str().unwrap_or("<unnamed>");
        let mut keys = Vec::new();
        for dependency in owner_dependencies(package) {
            let kind = dependency["kind"].as_str().unwrap_or("normal");
            if kind != "dev" {
                keys.push(dependency["rename"].as_str().unwrap_or(OWNER));
                if strings(&dependency["features"]).any(|feature| feature == FEATURE) {
                    found.push(format!("{name}: {kind} dependency on {OWNER}"));
                }
            }
        }
        let forwards: Vec<String> = keys
            .iter()
            .flat_map(|key| [format!("{key}/{FEATURE}"), format!("{key}?/{FEATURE}")])
            .collect();
        let Some(features) = package["features"].as_object() else {
            continue;
        };
        for (feature, implied) in features {
            for value in strings(implied) {
                let implies = (name == OWNER && value == FEATURE)
                    || forwards.iter().any(|forward| forward == value);
                if implies {
                    found.push(format!("{name}: feature `{feature}` enables `{value}`"));
                }
            }
        }
    }
    found.sort();
    found
}

/// Packages whose dev-dependencies enable [`FEATURE`]: the allowed path.
fn dev_enablers(metadata: &Value) -> Vec<&str> {
    packages(metadata)
        .iter()
        .filter(|package| {
            owner_dependencies(package).any(|dependency| {
                dependency["kind"] == "dev"
                    && strings(&dependency["features"]).any(|feature| feature == FEATURE)
            })
        })
        .filter_map(|package| package["name"].as_str())
        .collect()
}

fn package_mut<'a>(metadata: &'a mut Value, name: &str) -> &'a mut Value {
    metadata["packages"]
        .as_array_mut()
        .expect("packages")
        .iter_mut()
        .find(|package| package["name"] == name)
        .unwrap_or_else(|| panic!("workspace package {name}"))
}

/// The first dev-dependency of `package` on keld-ipc that enables [`FEATURE`].
fn dev_entry_mut<'a>(metadata: &'a mut Value, package: &str) -> &'a mut Value {
    package_mut(metadata, package)["dependencies"]
        .as_array_mut()
        .expect("dependencies")
        .iter_mut()
        .find(|dependency| {
            dependency["name"] == OWNER
                && dependency["kind"] == "dev"
                && strings(&dependency["features"]).any(|feature| feature == FEATURE)
        })
        .unwrap_or_else(|| panic!("{package} dev-dependency on {OWNER} with {FEATURE}"))
}

#[test]
fn no_workspace_manifest_enables_the_test_channel_id_feature_for_production() {
    let metadata = workspace_metadata();
    let enablers = production_enablers(&metadata);
    assert!(
        enablers.is_empty(),
        "{OWNER}'s `{FEATURE}` feature forges channel ids (#634); enable it only from \
         [dev-dependencies]: {enablers:#?}"
    );
}

/// Prerequisites that keep the zero-hit oracle above from being vacuous: the
/// metadata covers the workspace, keld-ipc declares the feature as non-default,
/// and the feature is in use, so a misplaced entry would be visible.
#[test]
fn the_feature_is_declared_non_default_and_used_only_by_dev_dependencies() {
    let metadata = workspace_metadata();
    let names: Vec<&str> = packages(&metadata)
        .iter()
        .filter_map(|package| package["name"].as_str())
        .collect();
    for required in [OWNER, "keld-core", "keld-host", "keld-native", "keld-cli"] {
        assert!(
            names.contains(&required),
            "{required} missing from {names:?}"
        );
    }
    let owner = &packages(&metadata)[names
        .iter()
        .position(|name| *name == OWNER)
        .expect("keld-ipc")];
    assert_eq!(owner["features"][FEATURE], json!([]));
    assert!(
        !strings(&owner["features"]["default"]).any(|feature| feature == FEATURE),
        "{FEATURE} must not be a default feature"
    );
    let users = dev_enablers(&metadata);
    assert!(users.contains(&OWNER), "{users:?}");
    assert!(users.contains(&"keld-core"), "{users:?}");
}

/// Each named negative control moves the real metadata one step toward a
/// production enablement and must produce exactly that finding.
#[test]
fn each_production_enablement_path_is_found() {
    let real = workspace_metadata();
    assert!(production_enablers(&real).is_empty());

    let mut normal = real.clone();
    dev_entry_mut(&mut normal, "keld-core")["kind"] = Value::Null;
    assert_eq!(
        production_enablers(&normal),
        vec!["keld-core: normal dependency on keld-ipc".to_owned()]
    );

    let mut build = real.clone();
    dev_entry_mut(&mut build, "keld-cli")["kind"] = json!("build");
    assert_eq!(
        production_enablers(&build),
        vec!["keld-cli: build dependency on keld-ipc".to_owned()]
    );

    for forward in [format!("{OWNER}/{FEATURE}"), format!("{OWNER}?/{FEATURE}")] {
        let mut forwarded = real.clone();
        package_mut(&mut forwarded, "keld-core")["features"]["probe"] = json!([forward]);
        assert_eq!(
            production_enablers(&forwarded),
            vec![format!("keld-core: feature `probe` enables `{forward}`")]
        );
    }

    let mut renamed = real.clone();
    let core = package_mut(&mut renamed, "keld-core");
    let normal_entry = core["dependencies"]
        .as_array_mut()
        .expect("dependencies")
        .iter_mut()
        .find(|dependency| dependency["name"] == OWNER && dependency["kind"].is_null())
        .expect("keld-core's normal dependency on keld-ipc");
    normal_entry["rename"] = json!("ipc");
    core["features"]["probe"] = json!([format!("ipc/{FEATURE}")]);
    assert_eq!(
        production_enablers(&renamed),
        vec![format!(
            "keld-core: feature `probe` enables `ipc/{FEATURE}`"
        )]
    );

    let mut defaulted = real;
    package_mut(&mut defaulted, OWNER)["features"]["default"] = json!([FEATURE]);
    assert_eq!(
        production_enablers(&defaulted),
        vec![format!("keld-ipc: feature `default` enables `{FEATURE}`")]
    );
}
