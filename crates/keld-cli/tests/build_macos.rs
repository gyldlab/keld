//! Native CLI refusal controls for the first GH-1404 local packaging cell.

#![cfg(target_os = "macos")]
#![allow(clippy::expect_used)] // Integration fixture setup failures must fail the test.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn build_source() -> String {
    format!(
        "export default {{target:'macos-arm64',signing:'adhoc-local',bundleVersion:'2',minimumSystemVersion:'14.0',resources:[],host:{{path:'host',sha256:'{}',version:'1'}},bun:{{path:'bun',sha256:'{}',version:'2'}},outDir:'output'}} as const;",
        "a".repeat(64),
        "b".repeat(64)
    )
}

fn write_inputs(root: &Path) {
    fs::write(
        root.join("keld.config.ts"),
        "export default {name:'Demo',id:'dev.example.demo',version:'1.2.3',renderer:'index.html'} as const;",
    )
    .expect("write app authoring");
    fs::write(root.join("keld.build.ts"), build_source()).expect("write build authoring");
    fs::write(
        root.join("keld.permissions.jsonc"),
        r#"{"profile":"legacy"}"#,
    )
    .expect("write permission authoring");
}

fn renderer_source_app() -> &'static str {
    "export default {name:'Demo',id:'dev.example.demo',version:'1.2.3',entry:'src/main.ts',initialWindow:'none',rendererSource:{root:'client/views',entry:'editor/main.html'}} as const;"
}

fn graph_source_build() -> String {
    let declaration = format!(
        "graphSource:{{packageSha256:'{}',lockSha256:'{}',providerTrees:{{electron:'{}',api:'{}',kipc:'{}'}},preload:{{path:'startup/verified-preload.js',sha256:'{}'}}}},",
        "a".repeat(64),
        "b".repeat(64),
        "c".repeat(64),
        "d".repeat(64),
        "e".repeat(64),
        "f".repeat(64),
    );
    build_source().replace("outDir:'output'", &(declaration + "outDir:'output'"))
}

fn build(root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keld"))
        .args(["build", "--target", "macos-arm64"])
        .current_dir(root)
        .output()
        .expect("run native CLI")
}

fn refused_without_output(root: &Path, expected: &str) {
    let result = build(root);
    assert_eq!(
        result.status.code(),
        Some(1),
        "expected failed input admission: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains(expected), "{stderr}");
    assert!(
        !root.join("output").exists(),
        "build published an invalid artifact"
    );
    assert!(
        !root.join(".keld/dev").exists(),
        "release refusal must not create a development stage or lease"
    );
}

#[test]
fn missing_or_nonliteral_config_refuses_before_output_and_dev_staging() {
    let missing = tempfile::tempdir().expect("tempdir");
    write_inputs(missing.path());
    fs::remove_file(missing.path().join("keld.config.ts")).expect("remove required source");
    refused_without_output(missing.path(), "keld.config.ts");

    let dynamic = tempfile::tempdir().expect("tempdir");
    write_inputs(dynamic.path());
    fs::write(
        dynamic.path().join("keld.build.ts"),
        build_source().replace("outDir:'output'", "outDir: process.env.OUT"),
    )
    .expect("write malicious authoring");
    refused_without_output(dynamic.path(), "KELD-CORE-035");
}

#[test]
fn invalid_canonical_app_id_and_explicit_strict_refuse_before_pack() {
    let id = tempfile::tempdir().expect("tempdir");
    write_inputs(id.path());
    fs::write(
        id.path().join("keld.config.ts"),
        "export default {name:'Demo',id:'Dev.Example.Demo',version:'1.2.3'} as const;",
    )
    .expect("write invalid ID");
    refused_without_output(id.path(), "KELD-WV-009");

    let strict = tempfile::tempdir().expect("tempdir");
    write_inputs(strict.path());
    fs::write(
        strict.path().join("keld.permissions.jsonc"),
        r#"{"profile":"strict"}"#,
    )
    .expect("write strict profile");
    refused_without_output(strict.path(), "Strict containment");
}

#[test]
fn absent_profile_refuses_and_does_not_default_to_legacy_or_stage_dev() {
    let missing = tempfile::tempdir().expect("tempdir");
    write_inputs(missing.path());
    fs::write(missing.path().join("keld.permissions.jsonc"), "{}")
        .expect("write valid policy with no profile declaration");
    refused_without_output(missing.path(), "explicit root /profile");
}

#[test]
fn missing_policy_and_unsupported_signing_refuse_without_mutation() {
    let missing = tempfile::tempdir().expect("tempdir");
    write_inputs(missing.path());
    fs::remove_file(missing.path().join("keld.permissions.jsonc")).expect("remove required policy");
    refused_without_output(missing.path(), "no default policy is synthesized");

    let signing = tempfile::tempdir().expect("tempdir");
    write_inputs(signing.path());
    fs::write(
        signing.path().join("keld.build.ts"),
        build_source().replace("adhoc-local", "developer-id"),
    )
    .expect("write unsupported signing");
    refused_without_output(signing.path(), "adhoc-local");
}

#[test]
fn graph_source_must_be_paired_before_pack_writes_an_artifact() {
    let app_only = tempfile::tempdir().expect("isolated source-only project");
    write_inputs(app_only.path());
    fs::write(
        app_only.path().join("keld.config.ts"),
        renderer_source_app(),
    )
    .expect("write source-only app");
    refused_without_output(app_only.path(), "must be provided together");

    let build_only = tempfile::tempdir().expect("isolated build-only project");
    write_inputs(build_only.path());
    fs::write(
        build_only.path().join("keld.build.ts"),
        graph_source_build(),
    )
    .expect("write build-only graph expectations");
    refused_without_output(build_only.path(), "must be provided together");
}

#[test]
fn original_graph_config_rejects_noncanonical_source_before_any_packaging() {
    let source = tempfile::tempdir().expect("isolated noncanonical source");
    write_inputs(source.path());
    fs::write(
        source.path().join("keld.config.ts"),
        renderer_source_app().replace("editor/main.html", "../hidden.html"),
    )
    .expect("write invalid original source");
    fs::write(source.path().join("keld.build.ts"), graph_source_build())
        .expect("write complete expected data");
    refused_without_output(source.path(), "keld.config.ts rendererSource");
}
