//! GH-500 physical product-corpus design falsifiers before any real app-run target.
//!
//! These in-memory manifests exercise the existing GH-532/GH-566 parser and
//! native test-case admission, but MUST NOT be registered as product results.
//! A metadata-only green test cannot substitute for real draw.io Host+OS proof.

use keld_compat::evidence::{OperationKind, Panel, Platform, Verdict};
use serde_json::{Value, json};

use super::v1_fixture::{
    COMMIT, READY, RUST_PATH, V1_PRODUCT, bytes, citation, denominator, parse_with,
};
use crate::corpus_admission::{RunnerKind, check_cells, list_libtest, rust_case_listed};
use crate::corpus_manifest::{Corpus, CorpusError, REGISTRY, Registration};

type Operation<'a> = (&'a str, &'a str);

const INSTALL: &[Operation<'_>] = &[("drawio-desktop.install", "KEL-141 real distribution")];
const ACTIVATION: &[Operation<'_>] =
    &[("drawio-desktop.activate", "actual app page initialization")];
const WORKFLOW: &[Operation<'_>] = &[
    (
        "drawio-desktop.open",
        "actual selected file becomes active document",
    ),
    ("drawio-desktop.edit", "actual document model mutation"),
    ("drawio-desktop.save", "actual persisted bytes and reopen"),
    (
        "drawio-desktop.close-unsaved-prompt",
        "actual dirty-close Save/Cancel/Discard effects",
    ),
];

/// V1 parser-only proposal. Its intentionally nonexistent test names prevent
/// accidental promotion to a real registered/native product cell.
fn shape(corpus_id: &str, kind: &str, operations: &[Operation<'_>]) -> Value {
    let cells: Vec<Value> = operations
        .iter()
        .map(|(id, effect)| {
            json!({
                "operation_id": id,
                "oracle_id": format!("electron-v44.4.5.drawio-product.{}.unqualified", id),
                "expected_verdict": "unknown",
                "platforms": ["macos"],
                "test_path": RUST_PATH,
                "test_name": format!("GH500_UNQUALIFIED_NOT_EXECUTABLE_{id}"),
                "negative_control": format!(
                    "No real Mac product proof: suppress {} and require the future OS observer to fail",
                    effect
                ),
            })
        })
        .collect();
    json!({
        "schema": "keld.compat.corpus/v1",
        "corpus_id": corpus_id,
        "scope": "shape-only preflight, unqualified real Mac application evidence",
        "panel": "product",
        "kind": kind,
        "artifact_digest": "manifest_bytes",
        "engine": {"macos": "wkwebview"},
        "doc_snapshots": {},
        "upstream": {"electron_version": "44.4.5", "electron_commit": COMMIT},
        "cells": cells,
    })
}

/// The registered, real GH-566 libtest owner remains separate from an app target.
fn registration(corpus_id: &'static str) -> Registration {
    Registration {
        corpus_id,
        platforms: &[Platform::Macos],
        ..V1_PRODUCT
    }
}

const PARTITIONS: [(&str, &str, OperationKind, &[Operation<'_>]); 3] = [
    (
        "electron-apps-v0-install",
        "install",
        OperationKind::Install,
        INSTALL,
    ),
    (
        "electron-apps-v0-activation",
        "activation",
        OperationKind::Activation,
        ACTIVATION,
    ),
    (
        "electron-apps-v0-primary-workflow",
        "primary_workflow",
        OperationKind::PrimaryWorkflow,
        WORKFLOW,
    ),
];

#[test]
fn gh500_three_kind_homogeneous_physical_shapes_have_no_product_evidence() {
    let mut count = 0;
    // Query the actual compiled test target rather than accepting our own
    // invented test names or relying on the first empty-output rejection alone.
    let (live_cases, ignored_cases) =
        list_libtest("electron_lifecycle").expect("list the admitted real Rust target");
    for (corpus_id, token, expected_kind, ops) in PARTITIONS {
        let reg = registration(corpus_id);
        let proposed = shape(corpus_id, token, ops);
        let parsed = parse_with(&reg, &proposed, &[])
            .unwrap_or_else(|error| panic!("{corpus_id} v1 shape: {error}"));

        assert_eq!(parsed.id(), corpus_id);
        assert_eq!(parsed.panel(), Panel::Product);
        assert_eq!(parsed.kind(), expected_kind);
        assert_eq!(parsed.pin().version, "44.4.5");
        assert_eq!(parsed.pin().commit, COMMIT);
        assert_eq!(parsed.cells().len(), ops.len());

        for cell in parsed.cells() {
            assert_eq!(cell.expected, Verdict::Unknown);
            assert!(
                cell.citation.is_none(),
                "no false Electron citation from app source"
            );
            assert_eq!(cell.platforms, [Platform::Macos]);
            assert_eq!(cell.test_path, RUST_PATH);
            assert!(
                !rust_case_listed(&live_cases, &ignored_cases, &cell.test_name),
                "a metadata-only GH-500 placeholder must not be admitted as an actual app test"
            );
        }

        // An in-memory shape with no real app test output is NOT a registered
        // product operation. The unchanged admission owner must reject it.
        let refusal = check_cells(
            parsed.id(),
            parsed.cells(),
            parsed.registration(),
            Platform::Macos,
            RunnerKind::Libtest,
            &[],
        )
        .expect_err("no real Mac case can be admitted from manifest bytes alone");
        assert!(
            matches!(refusal, CorpusError::CaseNotAdmitted { .. }),
            "{corpus_id}: {refusal}"
        );
        assert!(
            REGISTRY
                .iter()
                .all(|registered| registered.corpus_id != corpus_id),
            "do not register unqualified product cells or imply product PASS"
        );
        count += ops.len();
    }
    assert_eq!(
        count, 6,
        "one install, one activation, four workflow operations"
    );
}

#[test]
fn gh500_product_corpus_wrong_kind_and_app_only_citation_fail_closed() {
    let corpus_id = "electron-apps-v0-primary-workflow";
    let reg = registration(corpus_id);
    let proposed = shape(corpus_id, "primary_workflow", WORKFLOW);

    // The v1 schema has one top-level kind. Per-cell mixed-kind overrides
    // have no schema owner and must be refused, even if internally plausible.
    let mut mixed = proposed.clone();
    mixed["cells"][0]["kind"] = json!("install");
    assert!(matches!(
        parse_with(&reg, &mixed, &[]),
        Err(CorpusError::Parse { .. })
    ));

    // The real GH-532 citation grammar does not admit arbitrary draw.io
    // source files as normative Electron documentation. Keep app-only UNKNOWN.
    for verdict in ["pass", "fail"] {
        let mut unsupported = proposed.clone();
        unsupported["cells"][0]["expected_verdict"] = json!(verdict);
        assert!(matches!(
            parse_with(&reg, &unsupported, &[]),
            Err(CorpusError::VerdictRule { .. })
        ));
    }

    // An application source URL does not become a normative Electron-doc
    // citation merely because the quoted text and its SHA-256 are well formed.
    // Exercise the actual citation URL owner, not only the missing-citation rule.
    let mut app_source = proposed.clone();
    app_source["cells"][0]["expected_verdict"] = json!("pass");
    app_source["cells"][0]["doc_citation"] = citation(READY);
    app_source["cells"][0]["doc_citation"]["url"] = json!(
        "https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L849"
    );
    assert!(matches!(
        parse_with(&reg, &app_source, &[]),
        Err(CorpusError::CitationUrl { .. })
    ));

    // Desktop's package-lock resolves Electron 44.5.1, but only the 44.4.5
    // normative oracle is admitted here; don't quietly repin the schema.
    let mut wrong_pin = proposed;
    wrong_pin["upstream"]["electron_version"] = json!("44.5.1");
    assert!(matches!(
        parse_with(&reg, &wrong_pin, &[]),
        Err(CorpusError::UnadmittedPin { .. })
    ));
}

#[test]
fn gh500_manifest_byte_mutation_refuses_changed_denominator() {
    let corpus_id = "electron-apps-v0-install";
    let reg = registration(corpus_id);
    let proposed = shape(corpus_id, "install", INSTALL);
    let committed_bytes = bytes(&proposed);
    let original_denominator = denominator(&committed_bytes, &proposed);
    let valid = Corpus::parse(&reg, &committed_bytes, &original_denominator)
        .unwrap_or_else(|error| panic!("original shape should parse: {error}"));
    assert_eq!(valid.panel(), Panel::Product);

    // Change one literal source byte without changing the committed denominator.
    // The only digest owner must reject before parsing or scoring.
    let source = String::from_utf8(committed_bytes).expect("fixture is UTF-8");
    let changed = source.replacen("shape-only", "shapE-only", 1);
    assert_ne!(changed, source);
    let wrong = Corpus::parse(&reg, changed.as_bytes(), &original_denominator);
    assert!(
        matches!(wrong, Err(CorpusError::DigestMismatch { .. })),
        "changing one manifest byte without updating the denominator must fail"
    );
}
