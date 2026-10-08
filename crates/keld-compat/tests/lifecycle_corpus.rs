//! KEL-237: validate the bounded lifecycle corpus against KEL-74's evidence owner.
//!
//! The denominator binds the exact corpus bytes. Each mapped name must also have
//! one successful case result from its existing test runner: source comments,
//! helpers, ignored tests and a green suite with a missing case are not evidence.
//! These checks do not publish a compatibility score or instantiate a webview.
//!
//! gh566 T2: parsing, the digest, the pin, the registry and admission live in the
//! shared owner `support/corpus_manifest.rs`. The six case names below are named by
//! the published PR #242 receipts, so they keep their names at the crate root.

#![allow(clippy::expect_used, clippy::panic)] // test-only parsing/assertion context

#[path = "support/corpus_admission.rs"]
mod corpus_admission;
#[path = "support/corpus_manifest.rs"]
mod corpus_manifest;

use corpus_admission::{bun_case_passed, rust_case_passed};
use corpus_manifest::{
    Corpus, CorpusError, LIFECYCLE_V0, Runner, V0_CORPUS_SHA256, check_frozen_files, fixture_files,
};
use keld_compat::evidence::{OperationKind, Panel, Verdict};

/// Loads the committed lifecycle corpus through the shared owner.
fn corpus() -> Corpus {
    Corpus::load(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"))
}

/// Keep corpus identity and denominator membership independent of runner results.
#[test]
fn lifecycle_corpus_denominator_matches_exact_manifest_bytes() {
    let corpus = corpus();

    assert_eq!(corpus.id(), "electron-lifecycle-v0");
    assert_eq!(corpus.panel(), Panel::Showcase);
    assert_eq!(corpus.kind(), OperationKind::PrimaryWorkflow);
    assert!(
        corpus
            .scope()
            .contains("not median-app product compatibility"),
        "bounded corpus must not read as the product denominator"
    );
    assert_eq!(corpus.digest(), V0_CORPUS_SHA256);
    assert_eq!(corpus.denominator().corpus_sha256(), corpus.digest());
    assert_eq!(corpus.denominator().cells().len(), corpus.cells().len());

    let (manifest, denominator) =
        Corpus::fixture_bytes(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"));
    let mut mutated = manifest.clone();
    let index = mutated
        .iter()
        .position(|byte| *byte == b'e')
        .expect("corpus contains a byte to mutate");
    mutated[index] = b'E';
    let error = Corpus::parse(&LIFECYCLE_V0, &mutated, &denominator)
        .expect_err("a one-byte corpus mutation must invalidate the committed digest");
    assert!(
        matches!(error, CorpusError::DigestMismatch { .. }),
        "{error}"
    );
}

/// Validate metadata; the two execution tests below admit the actual mapped cases.
#[test]
fn lifecycle_corpus_cells_map_to_existing_behavioral_oracles() {
    let corpus = corpus();

    // The pin itself is checked by the owner against its one admitted-pin table.
    let mut runners = (0, 0);
    for cell in corpus.cells() {
        let target = LIFECYCLE_V0
            .targets
            .iter()
            .find(|target| target.path == cell.test_path)
            .expect("the owner admitted only registered targets");
        match target.runner {
            Runner::Libtest { .. } => runners.0 += 1,
            Runner::Bun => runners.1 += 1,
        }
        assert_eq!(
            cell.divergence.is_some(),
            cell.expected == Verdict::Fail,
            "{} pairs its verdict with its divergence",
            cell.key.operation_id
        );
    }
    assert_eq!(runners, (1, 2), "one Rust and two Bun lifecycle oracles");
}

/// Run only the existing Rust oracle target, never this validator recursively.
/// Cargo selects the current build artifact; no stale sibling executable is guessed.
/// Offline execution reuses dependencies already built for this integration target.
#[test]
fn lifecycle_corpus_rust_oracles_execute() {
    corpus_admission::admit_libtest(&[&corpus()]).unwrap_or_else(|error| panic!("{error}"));
}

/// A green Bun process alone is insufficient: every mapped case must have passed.
/// In particular, removing a test or changing it to test.skip/test.todo must fail.
#[test]
fn lifecycle_corpus_typescript_oracles_execute() {
    corpus_admission::admit_bun(&[&corpus()]).unwrap_or_else(|error| panic!("{error}"));
}

/// gh532 AC11: every committed lifecycle fixture file is byte-identical to origin/main.
#[test]
fn lifecycle_corpus_fixture_bytes_match_origin_main() {
    let files = fixture_files(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(files.len(), 15);
    check_frozen_files(&files).unwrap_or_else(|error| panic!("{error}"));

    let mut flipped = files.clone();
    let (_, report) = flipped
        .iter_mut()
        .find(|(path, _)| path == "report.md")
        .expect("report.md is committed");
    report[0] ^= 0x01;
    assert!(matches!(
        check_frozen_files(&flipped),
        Err(CorpusError::FixtureCensus { .. })
    ));

    let mut extra = files;
    extra.push(("evidence/extra.json".to_owned(), b"{}".to_vec()));
    assert!(matches!(
        check_frozen_files(&extra),
        Err(CorpusError::FixtureCensus { .. })
    ));
}

/// Reproduce the old substring false positives and reject absent/nonpass Rust cases.
#[test]
fn rust_case_results_reject_source_mentions_and_unexecuted_cases() {
    let source_only = "// #[test] fn mapped() {}\nfn mapped() {}\n";
    assert!(
        source_only.contains("mapped"),
        "old check accepted this source"
    );
    for output in [
        source_only,
        "",
        "test result: ok. 0 passed; 0 failed; 0 ignored;\n",
        "test mapped ... ignored\n",
        "test mapped ... FAILED\n",
        "test mapped_extra ... ok\n",
        "test mapped ... ok\ntest mapped ... ok\n",
    ] {
        assert!(
            !rust_case_passed(output, "mapped"),
            "false admission: {output}"
        );
    }
    assert!(rust_case_passed("test mapped ... ok\n", "mapped"));
}

/// Reject comments, helpers, skipped/todo and ambiguous Bun leaf-name results.
#[test]
fn bun_case_results_reject_source_mentions_and_unexecuted_cases() {
    let source_only = "// test(\"mapped\", () => {});\nfunction mapped() {}\n";
    assert!(
        source_only.contains("mapped"),
        "old check accepted this source"
    );
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
        assert!(
            !bun_case_passed(output, "mapped"),
            "false admission: {output}"
        );
    }
    assert!(bun_case_passed(
        "(pass) suite > mapped [1.00ms]\n",
        "mapped"
    ));
    assert!(bun_case_passed("(pass) mapped\n", "mapped"));
}
