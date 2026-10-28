//! GH-569 behavioral consumer: shared corpus owner plus actual selected observations.
//! This consumer is deliberately not itself a registered execution target.
#![allow(clippy::expect_used, clippy::panic)] // test-only corpus and subprocess assertions

#[path = "support/corpus_admission.rs"]
mod corpus_admission;
#[path = "support/corpus_manifest.rs"]
mod corpus_manifest;

use corpus_manifest::{Corpus, MODULE_PROCESS_V1, host_platform};
use keld_compat::evidence::{Platform, Verdict};
use serde_json::{Value, json};
use std::process::Command;

fn run_bun(script: &str) -> Value {
    let output = Command::new("bun")
        .args(["--no-install", script])
        .current_dir(corpus_manifest::workspace_root())
        .output()
        .expect("launch raw probe");
    assert!(
        output.status.success(),
        "raw probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("one raw probe JSON")
}

fn observation(output: &str) -> Value {
    let prefix = "GH569 early role observation: ";
    let lines: Vec<_> = output
        .lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .collect();
    assert_eq!(lines.len(), 1, "missing or duplicate role observation");
    serde_json::from_str(lines[0]).expect("role observation JSON")
}

fn verdict(value: bool) -> Verdict {
    if value { Verdict::Pass } else { Verdict::Fail }
}
fn undefined(value: &Value) -> bool {
    value["kind"] == "undefined"
}
fn true_or_undefined(value: &Value) -> bool {
    undefined(value) || (value["kind"] == "boolean" && value["value"] == true)
}

#[test]
fn actual_observations_match_manifest_statuses() {
    let corpus = Corpus::load(&MODULE_PROCESS_V1).expect("load validated GH569 corpus");
    if host_platform() != Platform::Macos {
        assert!(
            corpus
                .cells()
                .iter()
                .all(|cell| !cell.platforms.contains(&host_platform()))
        );
        eprintln!("GH569 macOS observations unrun on this host: unknown");
        return;
    }
    let output =
        corpus_admission::run_registered_exact(&corpus, "crates/keld-host/tests/no_flag_macos.rs")
            .expect("selected shipping-role case");
    let role = observation(&output);
    let resolver = run_bun("packages/@keld/electron/fixtures/module_resolve_probe.ts");
    let facade = run_bun("packages/@keld/electron/fixtures/module_process_probe.ts");
    let argv = role["argv"].as_array().expect("role argv");
    let default_app = &role["defaultApp"];
    let offset = if default_app["kind"] == "boolean" && default_app["value"] == true {
        2
    } else {
        1
    };
    // This particular staged launch supplied no user arguments.
    let empty_user_args = argv.get(offset..).is_some_and(<[Value]>::is_empty);
    let aliases = [
        "electron",
        "electron/main",
        "electron/renderer",
        "electron/common",
    ];
    for name in aliases.into_iter().chain(["electron/utility"]) {
        let found = resolver[name]["resolved"]
            .as_bool()
            .expect("well-formed resolver result");
        if !found {
            assert_eq!(
                resolver[name]["code"], "MODULE_NOT_FOUND",
                "unexpected resolver failure for {name}"
            );
        }
    }
    let all_aliases_resolve = aliases
        .iter()
        .all(|name| resolver[*name]["resolved"] == true);
    // A missing alias falsifies identity; all resolving cannot alone establish identity.
    let alias_verdict = if all_aliases_resolve {
        Verdict::Unknown
    } else {
        Verdict::Fail
    };
    let _utility_raw = resolver["electron/utility"]["resolved"]
        .as_bool()
        .expect("utility raw result");
    let namespace = facade["namespace"]
        .as_array()
        .expect("actual namespace keys");
    let defaults = facade["defaultKeys"]
        .as_array()
        .expect("actual default keys");
    let parity_raw = namespace == defaults;
    let version_raw = facade["electronVersion"]
        .as_str()
        .expect("actual facade version");
    eprintln!("GH569 raw facade parity={parity_raw}, version={version_raw}");
    for cell in corpus.cells() {
        let actual = match cell.key.operation_id.as_str() {
            "resolver.documented-aliases.identity" => alias_verdict,
            "resolver.utility.identity"
            | "resolver.esm-export-set"
            | "process.versions.electron.honesty" => Verdict::Unknown,
            "process.early.type" => {
                verdict(role["type"]["kind"] == "string" && role["type"]["value"] == "browser")
            }
            "process.argv.default-app.empty-user-args" => verdict(empty_user_args),
            "process.default-app.domain" => verdict(true_or_undefined(default_app)),
            "process.mas.domain" => verdict(true_or_undefined(&role["mas"])),
            "process.windows-store.domain" => verdict(true_or_undefined(&role["windowsStore"])),
            "process.sandboxed.domain" => verdict(true_or_undefined(&role["sandboxed"])),
            "process.context-isolated.main" => verdict(undefined(&role["contextIsolated"])),
            "process.resources-path.string" => verdict(role["resourcesPath"]["kind"] == "string"),
            other => panic!("unhandled GH569 cell {other}"),
        };
        assert_eq!(
            actual, cell.expected,
            "observed status differs for {}",
            cell.key.operation_id
        );
    }
}

#[test]
fn malformed_and_duplicate_observations_fail_closed() {
    assert!(std::panic::catch_unwind(|| observation("no marker")).is_err());
    assert!(
        std::panic::catch_unwind(|| observation("GH569 early role observation: {bad}")).is_err()
    );
    assert!(
        std::panic::catch_unwind(|| observation(
            "GH569 early role observation: {}\nGH569 early role observation: {}\n"
        ))
        .is_err()
    );
}

const GOLDEN_SHA256: &str =
    "sha256:c4a2bb0f48906511fa88f4e6655c05e5780f5aa9a8b10ff13cb74e0ce9dae277";
fn user_arguments_match(argv: &[&str], default_app: Option<bool>, expected: &[&str]) -> bool {
    let offset = if default_app == Some(true) { 2 } else { 1 };
    argv.get(offset..) == Some(expected)
}

#[test]
fn reference_integrity_and_independent_argument_pair() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/module-process-corpus/electron-reference.json"
    );
    let bytes = std::fs::read(path).expect("read normalized Electron oracle");
    assert_eq!(
        corpus_manifest::sha256_uri(&bytes),
        GOLDEN_SHA256,
        "reference bytes changed"
    );
    let value: Value = serde_json::from_slice(&bytes).expect("parse Electron oracle");
    assert_eq!(value["electron"], "44.4.5");
    assert_eq!(value["earlyType"], "browser");
    assert_eq!(
        value["facts"]["defaultApp"],
        json!({"kind":"boolean","value":true})
    );
    for flag in ["mas", "windowsStore", "sandboxed", "contextIsolated"] {
        assert_eq!(value["facts"][flag], json!({"kind":"undefined"}), "{flag}");
    }
    assert_eq!(value["facts"]["resourcesPath"]["kind"], "string");
    let argv: Vec<&str> = value["argv"]
        .as_array()
        .expect("argv array")
        .iter()
        .map(|v| v.as_str().expect("argument string"))
        .collect();
    // The sentinels are supplied by the launch command, not derived from argv.
    assert!(user_arguments_match(
        &argv,
        Some(true),
        &["gh569-alpha", "gh569-beta"]
    ));
    assert!(!user_arguments_match(
        &argv,
        None,
        &["gh569-alpha", "gh569-beta"]
    ));
    let text = std::str::from_utf8(&bytes).expect("reference UTF-8");
    let corrupted = text.replacen(
        "\"kind\": \"boolean\",\n      \"value\": true",
        "\"kind\": \"undefined\"",
        1,
    );
    assert_ne!(
        corrupted.as_bytes(),
        bytes,
        "one defaultApp mutation applied"
    );
    assert_ne!(
        corpus_manifest::sha256_uri(corrupted.as_bytes()),
        GOLDEN_SHA256
    );
    let changed: Value = serde_json::from_str(&corrupted).expect("parse mutated golden");
    let changed_argv: Vec<&str> = changed["argv"]
        .as_array()
        .expect("mutated argv")
        .iter()
        .map(|value| value.as_str().expect("argument string"))
        .collect();
    let changed_default = &changed["facts"]["defaultApp"];
    let default_app = match changed_default["kind"].as_str() {
        Some("undefined") => None,
        Some("boolean") => Some(changed_default["value"].as_bool().expect("boolean value")),
        _ => panic!("invalid mutated defaultApp"),
    };
    assert!(
        !user_arguments_match(&changed_argv, default_app, &["gh569-alpha", "gh569-beta"]),
        "mutated golden must independently fail its argv relation"
    );
}

#[test]
fn both_documented_argument_shapes_and_single_field_corruptions() {
    let expected = ["gh569-alpha", "gh569-beta"];
    let a = ["runtime", "entry", "gh569-alpha", "gh569-beta"];
    let b = ["executable", "gh569-alpha", "gh569-beta"];
    assert!(user_arguments_match(&a, Some(true), &expected));
    assert!(user_arguments_match(&b, None, &expected));
    assert!(!user_arguments_match(&a, None, &expected));
    assert!(!user_arguments_match(&b, Some(true), &expected));
}

#[test]
fn pinned_oracle_fixture_bytes_match_the_recorded_run() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/module-process-corpus/oracle");
    for (name, expected) in [
        (
            "main.cjs",
            "858ceb86eb6ea9cae3c0b3157bf17cf262bb7384887ec8f483472fded0e8f58d",
        ),
        (
            "package.json",
            "f8dfa9033e28294aca8c8fae69b8313529f2ce69b2634b7ca1643fb39599ff00",
        ),
        (
            "early-index.js",
            "a53c5ad791ca282c4cc76b8952e42eece58855a47fc1a12d530a9585b4eaa6f0",
        ),
    ] {
        let bytes = std::fs::read(base.join(name)).expect("read oracle fixture file");
        assert_eq!(
            corpus_manifest::sha256_uri(&bytes),
            format!("sha256:{expected}"),
            "oracle fixture changed: {name}"
        );
    }
}

#[test]
fn release_snapshot_integrity() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/module-process-corpus/electron-npm-versions.json"
    ))
    .expect("release snapshot");
    assert_eq!(
        corpus_manifest::sha256_uri(&bytes),
        "sha256:4e16df9857fd00f0b1d027675ccdc7c7804969e0ffe52d8f792e7b410df920a4"
    );
}

#[test]
fn unsupported_host_is_refused_before_exact_case_spawn() {
    let corpus = Corpus::load(&MODULE_PROCESS_V1).expect("validated GH569 corpus");
    let result = corpus_admission::run_registered_exact_on(
        &corpus,
        "crates/keld-host/tests/no_flag_macos.rs",
        Platform::Linux,
    );
    assert!(
        matches!(
            result,
            Err(corpus_manifest::CorpusError::InvalidTarget { .. })
        ),
        "unadmitted platform must not spawn the Host case: {result:?}"
    );
}
