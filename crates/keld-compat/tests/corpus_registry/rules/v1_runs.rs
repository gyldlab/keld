//! v1 record runs, product runs, the pending/divergence split and per-platform
//! admission (gh532 AC2, AC7–AC9, AC12, AC17; gh566 C5, C6, C10, D7, D11, D13).

use keld_compat::evidence::{AuthorityProfile, CellKey, EvidenceRecord, Platform};
use serde_json::{Value, json};

use super::v1_fixture::{
    BUN_PATH, HARNESS, RUST_PATH, TICKET, V1_PRODUCT, accepted, edit, macos_run, manifest,
    parse_with, record, record_value, store, to_record,
};
use crate::corpus_admission::{RunnerKind, RunnerOutput, check_admission};
use crate::corpus_manifest::{
    Corpus, CorpusError, DIVERGENCE_LABEL, PENDING_LABEL, ProductReceipt, ProfileState,
    V1_RECORDS_AS_OF, validate_committed_runs,
};

fn harness(corpus: &Corpus, records: &[EvidenceRecord]) -> Result<(), CorpusError> {
    corpus
        .validate_harness_run(records, V1_RECORDS_AS_OF)
        .map(|_| ())
}

fn field_of(error: &CorpusError) -> Option<&'static str> {
    match error {
        CorpusError::RecordMismatch { field, .. } => Some(field),
        _ => None,
    }
}

/// The macOS run with record `index` replaced by an edit of its JSON.
fn run_with(corpus: &Corpus, index: usize, change: impl FnOnce(&mut Value)) -> Vec<EvidenceRecord> {
    let mut run = macos_run(corpus);
    let mut value = record_value(corpus, index, Platform::Macos, "pass", HARNESS);
    value["result"] = json!(["pass", "fail", "fail", "unknown"][index]);
    change(&mut value);
    run[index] = to_record(&value);
    run
}

/// gh566 C5 (v1) and C10 (records): each record agrees with its v1 cell and platform.
#[test]
fn records_v1_agree_with_their_cell_and_platforms() {
    let corpus = accepted(&manifest());
    harness(&corpus, &macos_run(&corpus)).unwrap_or_else(|error| panic!("{error}"));

    let red_pass = run_with(&corpus, 1, |r| r["result"] = json!("pass"));
    assert_eq!(
        field_of(&harness(&corpus, &red_pass).expect_err("red pass")),
        Some("result")
    );
    let uncited_pass = run_with(&corpus, 3, |r| r["result"] = json!("pass"));
    assert_eq!(
        field_of(&harness(&corpus, &uncited_pass).expect_err("uncited pass")),
        Some("result")
    );

    // The red cell declares macOS only: on Linux its record must be `unknown`.
    let linux = |result: &str| {
        vec![
            record(&corpus, 0, Platform::Linux, "pass", HARNESS),
            record(&corpus, 1, Platform::Linux, result, HARNESS),
        ]
    };
    assert_eq!(
        field_of(&harness(&corpus, &linux("fail")).expect_err("undeclared lane")),
        Some("result")
    );
    harness(&corpus, &linux("unknown")).unwrap_or_else(|error| panic!("{error}"));

    // Two cells that share an operation but not an oracle each take their own record.
    let shared = edit(|m| {
        let mut twin = m["cells"][0].clone();
        twin["oracle_id"] = json!("electron-v44.4.5.app.when-ready-twin");
        m["cells"].as_array_mut().expect("cells").push(twin);
    });
    let corpus = accepted(&shared);
    let mut run = macos_run(&corpus);
    run.push(record(&corpus, 4, Platform::Macos, "pass", HARNESS));
    let board = corpus
        .validate_harness_run(&run, V1_RECORDS_AS_OF)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_board();
    assert_eq!(board.passed(), 2);
}

/// gh532 AC2, AC7 and AC9 (records) and the v1 harness label: the record revision, the
/// artifact digest and the engine identity bind the corpus; a strict label is refused.
#[test]
fn records_v1_carry_the_pin_digest_engine_and_harness_label() {
    let corpus = accepted(&manifest());
    let revision = run_with(&corpus, 0, |r| {
        r["operation"]["oracle"]["revision"] =
            json!("electron-v44.4.5@07e460719c75b2ec5ee4893f7d2192ef31c7b8c2");
    });
    assert!(matches!(
        harness(&corpus, &revision),
        Err(CorpusError::PinMismatch { .. })
    ));
    let digest = run_with(&corpus, 0, |r| {
        r["artifact"]["sha256"] = json!(format!("sha256:{}", "0".repeat(64)));
    });
    assert_eq!(
        field_of(&harness(&corpus, &digest).expect_err("digest")),
        Some("artifact.sha256")
    );
    let engine = run_with(&corpus, 0, |r| {
        r["revisions"]["engine"] = json!("wkwebview@38db257ba2d1f377bd2e24f7bc871faca895c6d5");
    });
    assert_eq!(
        field_of(&harness(&corpus, &engine).expect_err("engine")),
        Some("revisions.engine")
    );

    // A platform absent from the engine map is rejected.
    let mac_engine = accepted(&edit(|m| {
        m["engine"] = json!({ "macos": "headless-lifecycle-conformance" });
    }));
    let linux = vec![record(&mac_engine, 0, Platform::Linux, "pass", HARNESS)];
    assert_eq!(
        field_of(&harness(&mac_engine, &linux).expect_err("no engine entry")),
        Some("revisions.engine")
    );

    for label in ["strict_bun", "unverified", "sandboxed_addon_worker"] {
        let relabelled = run_with(&corpus, 0, |r| r["authority_profile"] = json!(label));
        assert!(
            matches!(
                harness(&corpus, &relabelled),
                Err(CorpusError::LabelMismatch { label: found, .. }) if found == label
            ),
            "{label}"
        );
    }
}

fn product() -> Corpus {
    let manifest = edit(|m| {
        m["corpus_id"] = json!(V1_PRODUCT.corpus_id);
        m["panel"] = json!("product");
    });
    parse_with(&V1_PRODUCT, &manifest, &store()).unwrap_or_else(|error| panic!("{error}"))
}

fn keys(corpus: &Corpus, indexes: &[usize]) -> Vec<CellKey> {
    indexes
        .iter()
        .map(|index| corpus.cells()[*index].key.clone())
        .collect()
}

/// gh532 AC8 and AC12: a product record's label follows its receipt state exactly, and
/// each rejection names the state and the label.
#[test]
fn product_run_labels_follow_the_receipt_state() {
    let corpus = product();
    let cells = keys(&corpus, &[0]);
    for state in [
        ProfileState::Unverified,
        ProfileState::Legacy,
        ProfileState::Strict,
    ] {
        for label in [
            AuthorityProfile::Unverified,
            AuthorityProfile::LegacySandboxOff,
            AuthorityProfile::StrictBun,
            AuthorityProfile::SandboxedAddonWorker,
            AuthorityProfile::UserApprovedToolChild,
        ] {
            let receipt = ProductReceipt {
                state,
                cells: &cells,
            };
            let records = [record(&corpus, 0, Platform::Macos, "pass", label.as_str())];
            let result = corpus.validate_product_run(&receipt, &records, V1_RECORDS_AS_OF);
            if label == state.required_label() {
                result.unwrap_or_else(|error| panic!("{}: {error}", state.token()));
            } else {
                assert!(
                    matches!(
                        &result,
                        Err(CorpusError::LabelMismatch { receipt_state, label: found, .. })
                            if *receipt_state == state.token() && *found == label.as_str()
                    ),
                    "{} × {}: {result:?}",
                    state.token(),
                    label.as_str()
                );
            }
        }
    }
}

/// gh532 AC8 and gh566 D7: every receipt cell has a record, and no record names a cell
/// the receipt did not run. Without the coverage check, cell B would score as a pass.
#[test]
fn product_run_covers_exactly_the_receipt_cells() {
    let corpus = product();
    let only_a = keys(&corpus, &[0]);
    let unverified = ProductReceipt {
        state: ProfileState::Unverified,
        cells: &only_a,
    };
    assert!(matches!(
        corpus.validate_product_run(&unverified, &[], V1_RECORDS_AS_OF),
        Err(CorpusError::MissingProductRecord {
            receipt_state: "unverified",
            ..
        })
    ));
    let both = [
        record(&corpus, 0, Platform::Macos, "pass", "unverified"),
        record(&corpus, 3, Platform::Macos, "unknown", "unverified"),
    ];
    assert!(matches!(
        corpus.validate_product_run(&unverified, &both, V1_RECORDS_AS_OF),
        Err(CorpusError::UncoveredProductRecord { .. })
    ));
    let a_and_b = keys(&corpus, &[0, 3]);
    let covered = ProductReceipt {
        state: ProfileState::Unverified,
        cells: &a_and_b,
    };
    corpus
        .validate_product_run(&covered, &both, V1_RECORDS_AS_OF)
        .unwrap_or_else(|error| panic!("{error}"));
}

/// gh566 C6 and D7: each run kind refuses the other panel, and committed product records
/// fail closed until X02-T5's receipt reader exists.
#[test]
fn product_records_need_receipt() {
    let showcase = accepted(&manifest());
    let product = product();
    let receipt_cells = keys(&product, &[0]);
    let receipt = ProductReceipt {
        state: ProfileState::Unverified,
        cells: &receipt_cells,
    };
    assert!(matches!(
        showcase.validate_product_run(&receipt, &[], V1_RECORDS_AS_OF),
        Err(CorpusError::NotProductPanel { .. })
    ));
    let product_run = vec![record(&product, 0, Platform::Macos, "pass", HARNESS)];
    assert!(matches!(
        product.validate_harness_run(&product_run, V1_RECORDS_AS_OF),
        Err(CorpusError::NotHarnessPanel { .. })
    ));
    assert!(matches!(
        validate_committed_runs(&product, &[product_run]),
        Err(CorpusError::ProductRecordsNeedReceipt { .. })
    ));
    assert_eq!(validate_committed_runs(&product, &[]), Ok(0));
    assert_eq!(
        validate_committed_runs(&showcase, &[macos_run(&showcase)]),
        Ok(1)
    );
}

/// gh532 AC17 (amended by A1): `FailSplit` counts pending cells apart from divergence
/// cells, from the manifest key, and its `Display` is the one renderer. The expected
/// text is built from the owner constants, so the C9 census passes on this test.
#[test]
fn fail_split_counts_pending_apart_from_divergence() {
    // The labels are pinned independently of `Display`, which uses the same constants,
    // so swapping or renaming them fails here (C9-allowed forms).
    assert_eq!(PENDING_LABEL, concat!("Pending ", "implementation"));
    assert_eq!(DIVERGENCE_LABEL, concat!("Intentional ", "divergence"));
    let corpus = accepted(&manifest());
    let run = corpus
        .validate_harness_run(&macos_run(&corpus), V1_RECORDS_AS_OF)
        .unwrap_or_else(|error| panic!("{error}"));
    let split = run.fail_split();
    assert_eq!(split.pending().len(), 1);
    assert_eq!(
        split.pending().values().next().map(String::as_str),
        Some(TICKET)
    );
    assert_eq!(split.divergence().len(), 1);
    assert!(split.divergence().contains(&corpus.cells()[2].key));
    assert_eq!(
        split.to_string(),
        format!("{PENDING_LABEL}: 1 ({TICKET})\n{DIVERGENCE_LABEL}: 1\n")
    );

    // An unrun red lane counts in neither set; the zero count has no parenthesis.
    let unrun = run_with(&corpus, 1, |r| r["result"] = json!("unknown"));
    let run = corpus
        .validate_harness_run(&unrun, V1_RECORDS_AS_OF)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        run.fail_split().to_string(),
        format!("{PENDING_LABEL}: 0\n{DIVERGENCE_LABEL}: 1\n")
    );

    // Two red cells on one ticket: two pending cells, one listed key.
    let two_red = edit(|m| {
        let mut second = m["cells"][1].clone();
        second["operation_id"] = json!("app.relaunch");
        second["oracle_id"] = json!("electron-v44.4.5.app.relaunch");
        m["cells"].as_array_mut().expect("cells").push(second);
    });
    let corpus = accepted(&two_red);
    let mut run = macos_run(&corpus);
    run.push(record(&corpus, 4, Platform::Macos, "fail", HARNESS));
    let split = corpus
        .validate_harness_run(&run, V1_RECORDS_AS_OF)
        .unwrap_or_else(|error| panic!("{error}"))
        .fail_split()
        .to_string();
    assert_eq!(
        split,
        format!("{PENDING_LABEL}: 2 ({TICKET})\n{DIVERGENCE_LABEL}: 1\n")
    );
}

fn output(path: &str, text: String) -> RunnerOutput {
    RunnerOutput {
        path: path.to_owned(),
        text,
    }
}

/// gh566 C10 and D13: admission runs per declared platform. An undeclared cell is
/// listed `unknown`; a declared cell whose case is `cfg`-gated out or skipped fails.
#[test]
fn platforms_admission_lists_undeclared_cells_as_unknown() {
    let corpus = accepted(&manifest());
    let corpora = [&corpus];
    let rust_ok = |names: &[&str]| {
        names
            .iter()
            .map(|name| format!("test {name}_case ... ok"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    // On Linux the red cell (macOS only) is unknown; the pass cell must still pass.
    let linux = check_admission(
        &corpora,
        Platform::Linux,
        RunnerKind::Libtest,
        &[output(RUST_PATH, rust_ok(&["app.when-ready"]))],
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        linux.unknown,
        vec![(corpus.id().to_owned(), corpus.cells()[1].key.clone())]
    );
    // On macOS the red cell is declared, so a missing (cfg-gated) case fails.
    assert!(matches!(
        check_admission(
            &corpora,
            Platform::Macos,
            RunnerKind::Libtest,
            &[output(RUST_PATH, rust_ok(&["app.when-ready"]))],
        ),
        Err(CorpusError::CaseNotAdmitted { .. })
    ));
    let mac = check_admission(
        &corpora,
        Platform::Macos,
        RunnerKind::Libtest,
        &[output(RUST_PATH, rust_ok(&["app.when-ready", "app.quit"]))],
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(mac.unknown.is_empty());
    // A Bun case skipped on a declared platform fails.
    let skipped = "(skip) app > app.exit_case\n(pass) app > app.name_case [1.00ms]\n".to_owned();
    assert!(matches!(
        check_admission(
            &corpora,
            Platform::Windows,
            RunnerKind::Bun,
            &[output(BUN_PATH, skipped)]
        ),
        Err(CorpusError::CaseNotAdmitted { .. })
    ));
}
