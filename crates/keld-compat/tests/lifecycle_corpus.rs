//! KEL-237: validate the bounded lifecycle corpus against KEL-74's evidence owner.
//!
//! This file does not decide Electron compatibility. It proves that the committed
//! denominator names the exact corpus bytes and that every corpus cell maps to an
//! existing behavioral conformance test. The mapped tests remain the behavior oracles.

#![allow(clippy::expect_used, clippy::panic)] // test-only parsing/assertion context

use std::{collections::BTreeSet, path::PathBuf, process::Command};

use keld_compat::evidence::{OperationKind, Panel, parse_denominator};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const CORPUS_JSON: &[u8] = include_bytes!("../fixtures/lifecycle-corpus/corpus.json");
const DENOMINATOR_JSON: &[u8] = include_bytes!("../fixtures/lifecycle-corpus/denominator.json");
const RUST_LIFECYCLE_TESTS: &str = include_str!("electron_lifecycle.rs");
const TS_APP_TESTS: &str = include_str!("../../../packages/@keld/electron/src/app.test.ts");

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

fn sha256_uri(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/keld-compat -> workspace root")
        .to_path_buf()
}

fn test_source(path: &str) -> &'static str {
    match path {
        "crates/keld-compat/tests/electron_lifecycle.rs" => RUST_LIFECYCLE_TESTS,
        "packages/@keld/electron/src/app.test.ts" => TS_APP_TESTS,
        other => panic!("unregistered lifecycle corpus test path: {other}"),
    }
}

#[test]
fn lifecycle_corpus_denominator_matches_exact_manifest_bytes() {
    let corpus: Manifest = serde_json::from_slice(CORPUS_JSON).expect("lifecycle corpus JSON");
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

#[test]
fn lifecycle_corpus_cells_map_to_existing_behavioral_oracles() {
    let corpus: Manifest = serde_json::from_slice(CORPUS_JSON).expect("lifecycle corpus JSON");

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
        let source = test_source(&cell.test_path);
        assert!(
            source.contains(&cell.test_name),
            "{} must map to an existing behavioral test `{}` in {}",
            cell.operation_id,
            cell.test_name,
            cell.test_path
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

#[test]
fn lifecycle_corpus_typescript_oracles_execute() {
    let output = Command::new("bun")
        .args(["test", "./packages/@keld/electron/src/app.test.ts"])
        .current_dir(workspace_root())
        .output()
        .expect("spawn existing @keld/electron app test file — bun must be on PATH");

    assert!(
        output.status.success(),
        "mapped TypeScript lifecycle oracles failed. stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
