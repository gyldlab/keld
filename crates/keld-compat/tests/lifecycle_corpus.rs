//! KEL-237: validate the bounded lifecycle corpus against KEL-74's evidence owner.
//!
//! The denominator binds the exact corpus bytes. Each mapped name must also have
//! one successful case result from its existing test runner: source comments,
//! helpers, ignored tests and a green suite with a missing case are not evidence.
//! These checks do not publish a compatibility score or instantiate a webview.

#![allow(clippy::expect_used, clippy::panic)] // test-only parsing/assertion context

use std::{collections::BTreeSet, path::PathBuf, process::Command};

use keld_compat::evidence::{OperationKind, Panel, parse_denominator};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const CORPUS_JSON: &[u8] = include_bytes!("../fixtures/lifecycle-corpus/corpus.json");
const DENOMINATOR_JSON: &[u8] = include_bytes!("../fixtures/lifecycle-corpus/denominator.json");
const RUST_TEST_PATH: &str = "crates/keld-compat/tests/electron_lifecycle.rs";
const TS_TEST_PATH: &str = "packages/@keld/electron/src/app.test.ts";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    corpus_id: String,
    scope: String,
    panel: String,
    kind: String,
    upstream: Upstream,
    cells: Vec<CorpusCell>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Upstream {
    electron_version: String,
    electron_commit: String,
    app_docs: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusCell {
    operation_id: String,
    oracle_id: String,
    expected_verdict: String,
    test_path: String,
    test_name: String,
    negative_control: String,
    #[serde(default)]
    intentional_divergence: Option<String>,
}

/// Parse the committed manifest without accepting undeclared fields.
fn manifest() -> Manifest {
    serde_json::from_slice(CORPUS_JSON).expect("lifecycle corpus JSON")
}

/// Hash the exact bytes, including whitespace, using the existing workspace pin.
fn sha256_uri(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Locate the source workspace independently of the test runner's working directory.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/keld-compat -> workspace root")
        .to_path_buf()
}

/// Require exactly one successful libtest pretty-format case, not a substring.
/// Ignored, missing, similarly named and duplicate records fail closed.
fn rust_case_passed(stdout: &str, name: &str) -> bool {
    let expected = format!("test {name} ... ok");
    stdout.lines().filter(|line| *line == expected).count() == 1
}

/// Require one successful Bun no-color console record for an exact leaf name.
/// Strip only the optional timing suffix and the runner's describe hierarchy.
/// A reporter-format change fails closed; source text is never a fallback.
fn bun_case_passed(stderr: &str, name: &str) -> bool {
    stderr
        .lines()
        .filter_map(|line| line.strip_prefix("(pass) "))
        .map(|line| {
            let full_name = match line.rsplit_once(" [") {
                Some((label, timing)) if timing.ends_with(']') => label,
                _ => line,
            };
            full_name.rsplit(" > ").next().unwrap_or(full_name)
        })
        .filter(|found| *found == name)
        .count()
        == 1
}

/// Keep corpus identity and denominator membership independent of runner results.
#[test]
fn lifecycle_corpus_denominator_matches_exact_manifest_bytes() {
    let corpus = manifest();
    let denominator = parse_denominator(DENOMINATOR_JSON).expect("KEL-74 denominator");

    assert_eq!(corpus.corpus_id, "electron-lifecycle-v0");
    assert_eq!(corpus.panel, "showcase");
    assert_eq!(corpus.kind, "primary_workflow");
    assert!(
        corpus
            .scope
            .contains("not median-app product compatibility"),
        "bounded corpus must not read as the product denominator"
    );

    assert_eq!(denominator.panel(), Panel::Showcase);
    assert_eq!(denominator.kind(), OperationKind::PrimaryWorkflow);
    assert_eq!(denominator.corpus_id(), corpus.corpus_id);
    assert_eq!(denominator.corpus_sha256(), sha256_uri(CORPUS_JSON));

    let manifest_cells: BTreeSet<_> = corpus
        .cells
        .iter()
        .map(|cell| (cell.operation_id.as_str(), cell.oracle_id.as_str()))
        .collect();
    assert_eq!(
        manifest_cells.len(),
        corpus.cells.len(),
        "lifecycle corpus cells must be unique"
    );

    let denominator_cells: BTreeSet<_> = denominator
        .cells()
        .iter()
        .map(|cell| (cell.operation_id.as_str(), cell.oracle_id.as_str()))
        .collect();
    assert_eq!(
        denominator_cells, manifest_cells,
        "denominator must contain exactly the committed lifecycle corpus cells"
    );

    let mut mutated = CORPUS_JSON.to_vec();
    let index = mutated
        .iter()
        .position(|byte| *byte == b'e')
        .expect("corpus contains a byte to mutate");
    mutated[index] = b'E';
    assert_ne!(
        sha256_uri(&mutated),
        denominator.corpus_sha256(),
        "a one-byte corpus mutation must invalidate the committed digest"
    );
}

/// Validate metadata; the two execution tests below admit the actual mapped cases.
#[test]
fn lifecycle_corpus_cells_map_to_existing_behavioral_oracles() {
    let corpus = manifest();

    assert_eq!(corpus.upstream.electron_version, "44.3.0");
    assert_eq!(
        corpus.upstream.electron_commit,
        "07e460719c75b2ec5ee4893f7d2192ef31c7b8c2"
    );
    assert!(
        corpus
            .upstream
            .app_docs
            .contains(&corpus.upstream.electron_commit),
        "upstream oracle must be an immutable Electron source URL"
    );

    for cell in &corpus.cells {
        assert!(
            !cell.negative_control.trim().is_empty(),
            "{} must name a falsifier",
            cell.operation_id
        );
        assert!(
            matches!(cell.test_path.as_str(), RUST_TEST_PATH | TS_TEST_PATH),
            "{} names an unregistered test target: {}",
            cell.operation_id,
            cell.test_path
        );
        assert!(
            !cell.test_name.trim().is_empty() && !cell.test_name.contains(['\r', '\n']),
            "{} must name one non-empty test case",
            cell.operation_id
        );

        match cell.expected_verdict.as_str() {
            "pass" => assert!(
                cell.intentional_divergence.is_none(),
                "passing cell {} must not hide a divergence",
                cell.operation_id
            ),
            "fail" => assert!(
                cell.intentional_divergence
                    .as_deref()
                    .is_some_and(|reason| !reason.trim().is_empty()),
                "failing cell {} must name the intentional divergence",
                cell.operation_id
            ),
            other => panic!(
                "{} uses unsupported bounded-corpus verdict {other}",
                cell.operation_id
            ),
        }
    }
}

/// Run only the existing Rust oracle target, never this validator recursively.
/// Cargo selects the current build artifact; no stale sibling executable is guessed.
/// Offline execution reuses dependencies already built for this integration target.
#[test]
fn lifecycle_corpus_rust_oracles_execute() {
    let output = Command::new(env!("CARGO"))
        .args([
            "test",
            "--offline",
            "--color",
            "never",
            "-p",
            "keld-compat",
            "--test",
            "electron_lifecycle",
            "--",
            "--format",
            "pretty",
            "--color",
            "never",
        ])
        .env_remove("RUST_TEST_NOCAPTURE")
        .current_dir(workspace_root())
        .output()
        .expect("run existing electron_lifecycle target with Cargo");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Rust lifecycle oracles failed. stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    for cell in manifest().cells.iter().filter(|cell| cell.test_path == RUST_TEST_PATH) {
        assert!(
            rust_case_passed(&stdout, &cell.test_name),
            "{} needs exactly one executed, passing Rust test `{}`. stdout:\n{stdout}",
            cell.operation_id,
            cell.test_name
        );
    }
}

/// A green Bun process alone is insufficient: every mapped case must have passed.
/// In particular, removing a test or changing it to test.skip/test.todo must fail.
#[test]
fn lifecycle_corpus_typescript_oracles_execute() {
    let output = Command::new("bun")
        .args(["test", "./packages/@keld/electron/src/app.test.ts"])
        .env("NO_COLOR", "1")
        .env_remove("FORCE_COLOR")
        .current_dir(workspace_root())
        .output()
        .expect("spawn existing @keld/electron app test file — bun must be on PATH");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TypeScript lifecycle oracles failed. stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    for cell in manifest().cells.iter().filter(|cell| cell.test_path == TS_TEST_PATH) {
        assert!(
            bun_case_passed(&stderr, &cell.test_name),
            "{} needs exactly one executed, passing Bun test `{}`. stderr:\n{stderr}",
            cell.operation_id,
            cell.test_name
        );
    }
}

/// Reproduce the old substring false positives and reject absent/nonpass Rust cases.
#[test]
fn rust_case_results_reject_source_mentions_and_unexecuted_cases() {
    let source_only = "// #[test] fn mapped() {}\nfn mapped() {}\n";
    assert!(source_only.contains("mapped"), "old check accepted this source");
    for output in [
        source_only,
        "",
        "test result: ok. 0 passed; 0 failed; 0 ignored;\n",
        "test mapped ... ignored\n",
        "test mapped ... FAILED\n",
        "test mapped_extra ... ok\n",
        "test mapped ... ok\ntest mapped ... ok\n",
    ] {
        assert!(!rust_case_passed(output, "mapped"), "false admission: {output}");
    }
    assert!(rust_case_passed("test mapped ... ok\n", "mapped"));
}

/// Reject comments, helpers, skipped/todo and ambiguous Bun leaf-name results.
#[test]
fn bun_case_results_reject_source_mentions_and_unexecuted_cases() {
    let source_only = "// test(\"mapped\", () => {});\nfunction mapped() {}\n";
    assert!(source_only.contains("mapped"), "old check accepted this source");
    for output in [
        source_only,
        "",
        "0 pass\n0 fail\n",
        "(skip) suite > mapped\n",
        "(todo) suite > mapped\n",
        "(fail) suite > mapped\n",
        "(pass) suite > mapped_extra [1.00ms]\n",
        "(pass) first > mapped\n(pass) second > mapped\n",
    ] {
        assert!(!bun_case_passed(output, "mapped"), "false admission: {output}");
    }
    assert!(bun_case_passed("(pass) suite > mapped [1.00ms]\n", "mapped"));
    assert!(bun_case_passed("(pass) mapped\n", "mapped"));
}
