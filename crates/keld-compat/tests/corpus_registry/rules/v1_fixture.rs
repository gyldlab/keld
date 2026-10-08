//! In-test v1 fixtures (gh566 §7). The manifest is built as JSON, its denominator is
//! derived from it, snapshots live in memory, and records are built per cell. Nothing
//! here is committed corpus data (gh532 §5: AC16 cases use in-test snapshots).

use std::io;

use keld_compat::evidence::{EvidenceRecord, Platform, parse_evidence};
use serde_json::{Value, json};

use crate::corpus_manifest::{
    Corpus, CorpusError, Registration, Runner, Shape, TestTarget, arch_token, platform_token,
    sha256_uri,
};

/// The v1 pin commit (gh532 rule 1).
pub const COMMIT: &str = "694f45852a0f1726cd23bfd379854de489cccb65";
/// The frozen v0 pin commit, inadmissible for v1.
pub const V0_COMMIT: &str = "07e460719c75b2ec5ee4893f7d2192ef31c7b8c2";
/// The one cited page.
pub const PAGE: &str = "docs/api/app.md";
/// An in-test snapshot of that page: LF only, with three citable sentences.
pub const SNAPSHOT: &str = "# app\n\n### `app.whenReady()`\n\nReturns `Promise<void>` - fulfilled when Electron is initialized.\n\n### `app.quit()`\n\nTry to close all windows.\n\n### `app.exit()`\n\nExits immediately with `exitCode`.\n";
/// The sentence the pass cell cites.
pub const READY: &str = "Returns `Promise<void>` - fulfilled when Electron is initialized.";
/// The sentence the red cell cites.
pub const QUIT: &str = "Try to close all windows.";
/// The sentence the divergence cell cites.
pub const EXIT: &str = "Exits immediately with `exitCode`.";
/// The red cell's implementing ticket.
pub const TICKET: &str = "GH-445";

const RUST: TestTarget = TestTarget {
    path: "crates/keld-compat/tests/electron_lifecycle.rs",
    runner: Runner::Libtest {
        target: "electron_lifecycle",
    },
};
const BUN: TestTarget = TestTarget {
    path: "packages/@keld/electron/src/app.test.ts",
    runner: Runner::Bun,
};
const ALL: &[Platform] = &[Platform::Macos, Platform::Linux, Platform::Windows];

/// A v1 showcase registration for the in-test corpus.
pub const V1: Registration = Registration {
    corpus_id: "electron-app-v1-fixture",
    fixture_dir: "fixtures/in-test-only",
    shape: Shape::V1,
    platforms: ALL,
    targets: &[RUST, BUN],
};

/// A v1 product registration for the in-test corpus.
pub const V1_PRODUCT: Registration = Registration {
    corpus_id: "electron-apps-v1-fixture",
    ..V1
};

/// The libtest target path cells map.
pub const RUST_PATH: &str = RUST.path;
/// The Bun target path cells map.
pub const BUN_PATH: &str = BUN.path;

/// A citation of `quote` on the pinned page, with its correct digest.
pub fn citation(quote: &str) -> Value {
    json!({
        "url": format!("https://github.com/electron/electron/blob/{COMMIT}/{PAGE}#appwhenready"),
        "quote": quote,
        "quote_sha256": sha256_uri(quote.as_bytes()),
    })
}

/// One v1 cell mapped to `path`.
pub fn cell(operation: &str, verdict: &str, path: &str, platforms: &[&str]) -> Value {
    json!({
        "operation_id": operation,
        "oracle_id": format!("electron-v44.4.5.{operation}"),
        "expected_verdict": verdict,
        "platforms": platforms,
        "test_path": path,
        "test_name": format!("{operation}_case"),
        "negative_control": format!("Breaking {operation} fails its mapped test."),
    })
}

/// The base manifest: a cited pass cell, a red cell (macOS only), a divergence cell
/// and an uncited unknown cell. Cell indexes: 0 pass, 1 red, 2 divergence, 3 unknown.
pub fn manifest() -> Value {
    let mut pass = cell(
        "app.when-ready",
        "pass",
        RUST_PATH,
        &["macos", "linux", "windows"],
    );
    pass["doc_citation"] = citation(READY);
    let mut red = cell("app.quit", "fail", RUST_PATH, &["macos"]);
    red["doc_citation"] = citation(QUIT);
    red["implementing_ticket"] = json!(TICKET);
    let mut divergence = cell("app.exit", "fail", BUN_PATH, &["macos", "linux", "windows"]);
    divergence["doc_citation"] = citation(EXIT);
    divergence["intentional_divergence"] = json!("Keld exits after flushing the kipc link.");
    let unknown = cell(
        "app.name",
        "unknown",
        BUN_PATH,
        &["macos", "linux", "windows"],
    );
    json!({
        "schema": "keld.compat.corpus/v1",
        "corpus_id": V1.corpus_id,
        "scope": "in-test v1 fixture; not median-app product compatibility",
        "panel": "showcase",
        "kind": "primary_workflow",
        "artifact_digest": "manifest_bytes",
        "engine": {
            "macos": "headless-lifecycle-conformance",
            "linux": "headless-lifecycle-conformance",
            "windows": "headless-lifecycle-conformance",
        },
        "doc_snapshots": { PAGE: sha256_uri(SNAPSHOT.as_bytes()) },
        "upstream": { "electron_version": "44.4.5", "electron_commit": COMMIT },
        "cells": [pass, red, divergence, unknown],
    })
}

/// The base manifest after `change`.
pub fn edit(change: impl FnOnce(&mut Value)) -> Value {
    let mut manifest = manifest();
    change(&mut manifest);
    manifest
}

/// The manifest's bytes.
pub fn bytes(manifest: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(manifest).expect("manifest JSON")
}

/// A KEL-74 denominator that agrees with `manifest_bytes` and the manifest's cells.
pub fn denominator(manifest_bytes: &[u8], manifest: &Value) -> Vec<u8> {
    let cells: Vec<Value> = manifest["cells"]
        .as_array()
        .map(|cells| {
            cells
                .iter()
                .map(|cell| {
                    json!({ "operation_id": cell["operation_id"], "oracle_id": cell["oracle_id"] })
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::to_vec_pretty(&json!({
        "schema": "keld.compat.denominator/v1",
        "panel": manifest["panel"],
        "corpus_id": manifest["corpus_id"],
        "corpus_sha256": sha256_uri(manifest_bytes),
        "kind": manifest["kind"],
        "cells": cells,
    }))
    .expect("denominator JSON")
}

/// The in-memory snapshot store: the page under the pinned commit.
pub fn store() -> Vec<(String, Vec<u8>)> {
    vec![(
        format!("doc-snapshots/{COMMIT}/{PAGE}"),
        SNAPSHOT.as_bytes().to_vec(),
    )]
}

/// Parses raw manifest bytes through `reg` against `store`, with a matching denominator.
pub fn parse_bytes(
    reg: &Registration,
    manifest_bytes: &[u8],
    manifest: &Value,
    store: &[(String, Vec<u8>)],
) -> Result<Corpus, CorpusError> {
    let reader = |rel: &str| {
        store
            .iter()
            .find(|(path, _)| path == rel)
            .map(|(_, bytes)| bytes.clone())
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    };
    Corpus::parse_with_snapshots(
        reg,
        manifest_bytes,
        &denominator(manifest_bytes, manifest),
        &reader,
    )
}

/// Parses `manifest` through `reg` against `store`.
pub fn parse_with(
    reg: &Registration,
    manifest: &Value,
    store: &[(String, Vec<u8>)],
) -> Result<Corpus, CorpusError> {
    parse_bytes(reg, &bytes(manifest), manifest, store)
}

/// Parses `manifest` through the showcase registration and the base store.
pub fn parse(manifest: &Value) -> Result<Corpus, CorpusError> {
    parse_with(&V1, manifest, &store())
}

/// Parses `manifest`, which must be accepted.
pub fn accepted(manifest: &Value) -> Corpus {
    parse(manifest).unwrap_or_else(|error| panic!("{error}"))
}

/// Parses `manifest`, which must be rejected.
pub fn rejected(manifest: &Value) -> CorpusError {
    parse(manifest).expect_err("mutation must be rejected")
}

/// A record for cell `index` of `corpus` on `platform`, as JSON to edit.
pub fn record_value(
    corpus: &Corpus,
    index: usize,
    platform: Platform,
    result: &str,
    label: &str,
) -> Value {
    let cell = &corpus.cells()[index];
    let arch = match platform {
        Platform::Macos => arch_token(keld_compat::evidence::Arch::Aarch64),
        Platform::Linux | Platform::Windows => arch_token(keld_compat::evidence::Arch::X86_64),
    };
    json!({
        "schema": "keld.compat.evidence/v1",
        "artifact": {
            "sha256": corpus.digest(),
            "platform": platform_token(platform),
            "arch": arch,
        },
        "revisions": {
            "keld": "38db257ba2d1f377bd2e24f7bc871faca895c6d5",
            "bun": "1.4.2+744846f84",
            "engine": "headless-lifecycle-conformance@38db257ba2d1f377bd2e24f7bc871faca895c6d5",
        },
        "authority_profile": label,
        "operation": {
            "id": cell.key.operation_id,
            "kind": "primary_workflow",
            "oracle": {
                "id": cell.key.oracle_id,
                "revision": corpus.pin().oracle_revision(),
            },
        },
        "result": result,
        "evidence_uri": "sha256:741490530fde052e312e938d6969d31e151da1f74b86e25067ca2a7632200355",
    })
}

/// Parses an edited record value.
pub fn to_record(value: &Value) -> EvidenceRecord {
    parse_evidence(&serde_json::to_vec(value).expect("record JSON")).expect("schema-valid record")
}

/// A record for cell `index` of `corpus` on `platform`.
pub fn record(
    corpus: &Corpus,
    index: usize,
    platform: Platform,
    result: &str,
    label: &str,
) -> EvidenceRecord {
    to_record(&record_value(corpus, index, platform, result, label))
}

/// The harness label of v1 conformance records.
pub const HARNESS: &str = "legacy_sandbox_off";

/// The expected macOS harness run: pass, fail (red), fail (divergence), unknown.
pub fn macos_run(corpus: &Corpus) -> Vec<EvidenceRecord> {
    ["pass", "fail", "fail", "unknown"]
        .iter()
        .enumerate()
        .map(|(index, result)| record(corpus, index, Platform::Macos, result, HARNESS))
        .collect()
}
