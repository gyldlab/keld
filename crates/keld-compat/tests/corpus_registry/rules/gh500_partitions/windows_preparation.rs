//! GH-1183 W0 preparation, inside the existing GH-500 fixture family.
//!
//! No native operation runs here. These cases exercise the existing corpus,
//! evidence and admission owners. `ProductRecordsNeedReceipt` is a deliberate
//! blanket refusal, not a document-byte, generation or native-origin verifier.

use std::collections::BTreeSet;

use keld_compat::evidence::{EvidenceError, Platform, Verdict, parse_evidence};
use serde_json::{Value, json};

use super::super::v1_fixture::{bytes, parse_with, record_value, to_record};
use super::{PARTITIONS, registration, shape};
use crate::corpus_admission::{RunnerKind, check_cells, rust_case_passed};
use crate::corpus_manifest::{
    Corpus, CorpusError, ProductReceipt, ProfileState, REGISTRY, Registration, V1_RECORDS_AS_OF,
    sha256_uri, validate_committed_runs,
};

const INPUT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/drawio-windows-preparation/input.drawio"
));
const EDITED: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/drawio-windows-preparation/edited.drawio"
));
const OTHER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/drawio-windows-preparation/other.drawio"
));
const WINDOWS_HEADLESS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/lifecycle-corpus/receipts/windows-x86_64.json"
));
const MAC_HEADLESS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/lifecycle-corpus/receipts/macos-aarch64.json"
));

/// An execution-plan row, not a registration, receipt or scoring denominator.
/// The partition and cell indexes consume GH-500's existing operation identities.
struct PreparedCase {
    name: &'static str,
    partition: usize,
    cell: usize,
    action: &'static str,
    disk_after: Option<&'static [u8]>,
    dependencies: &'static str,
}

const CASES: &[PreparedCase] = &[
    PreparedCase {
        name: "launch",
        partition: 1,
        cell: 0,
        action: "First attempt: original main/preload -> actual draw.io page, live role and native window",
        disk_after: None,
        dependencies: "W2 T3 managed Create; W1 trusted preload; W3 B22 entry/policy; W4 first mirror sample; W6 Windows explicit profile",
    },
    PreparedCase {
        name: "open",
        partition: 2,
        cell: 0,
        action: "Native Open selects input.drawio; original rendererReq reqId receives exact bytes; editor has w0-box",
        disk_after: Some(INPUT),
        dependencies: "Launch gates; W5 parented Open/caller custody; W1 mainResp; W6 actual file access",
    },
    PreparedCase {
        name: "edit",
        partition: 2,
        cell: 1,
        action: "Actual editor changes w0-box label to W0 edited 1183; same active document becomes dirty",
        disk_after: Some(INPUT),
        dependencies: "Launch gates; W1 app bridge/current dirty state; W3 attributed settled observation",
    },
    PreparedCase {
        name: "save-existing",
        partition: 2,
        cell: 2,
        action: "Save named document; original reqId settles; independent file read equals captured original-caller save payload",
        disk_after: Some(EDITED),
        dependencies: "W1 save/result path; W6 real write/profile; W3 B21 if eval observes result; W5 not required for a named path",
    },
    PreparedCase {
        name: "save-as",
        partition: 2,
        cell: 2,
        action: "Native Save As selects fresh destination; matching request writes and reopens exact payload",
        disk_after: Some(EDITED),
        dependencies: "Save-existing gates plus W5 native Save dialog, parent generation and cancellation",
    },
    PreparedCase {
        name: "dirty-close-cancel",
        partition: 2,
        cell: 3,
        action: "Windows button 2; same HWND/controller/window/profile remain; dirty model survives; subsequent Save works",
        disk_after: Some(INPUT),
        dependencies: "W5 sync prompt/index; W2 T3 veto/identity retention; W1 actual isModified continuation",
    },
    PreparedCase {
        name: "dirty-close-save",
        partition: 2,
        cell: 3,
        action: "Windows button 0; same document saves correct bytes before one accepted close/release",
        disk_after: Some(EDITED),
        dependencies: "W5 prompt and unnamed-document Save dialog; W1/W3 original caller; W6 write; W2 T3 save-before-destroy",
    },
    PreparedCase {
        name: "dirty-close-discard",
        partition: 2,
        cell: 3,
        action: "Windows button 1; draft cleanup settles; saved input bytes unchanged; exactly one destroy/release",
        disk_after: Some(INPUT),
        dependencies: "W5 prompt; W1 discard continuation; W6 backup/draft descendants; W2 T3 terminal release",
    },
    PreparedCase {
        name: "cleanup",
        partition: 2,
        cell: 3,
        action: "Observe owned process/window/controller/profile release and no late effects before emergency harness cleanup",
        disk_after: None,
        dependencies: "W2 managed final-window path and lead PR1397 qualification; W6 descendants/profile; consumed GH1073 regressions",
    },
    PreparedCase {
        name: "fresh-relaunch",
        partition: 1,
        cell: 0,
        action: "New process/session/generation opens independently verified saved bytes and completes another healthy operation",
        disk_after: Some(EDITED),
        dependencies: "Cleanup plus W1-W4 fresh initialization and W6 fresh admission; preserve GH814 first-attempt failures",
    },
];

fn windows_shape(partition: usize) -> (Registration, Value) {
    let (id, kind, _, operations) = PARTITIONS[partition];
    let reg = Registration {
        platforms: &[Platform::Windows],
        ..registration(id)
    };
    let mut value = shape(id, kind, operations);
    value["scope"] =
        json!("GH1183 in-memory Windows preparation; no native execution or product evidence");
    value["engine"] = json!({"windows": "webview2"});
    for cell in value["cells"].as_array_mut().expect("GH500 cells") {
        cell["platforms"] = json!(["windows"]);
        cell["negative_control"] =
            json!("Missing actual Windows original-app observation must remain unadmitted");
    }
    (reg, value)
}

fn windows_corpus(partition: usize) -> Corpus {
    let (reg, value) = windows_shape(partition);
    parse_with(&reg, &value, &[]).expect("existing owner accepts only the UNKNOWN in-memory shape")
}

fn unqualified_record(corpus: &Corpus, partition: usize, cell: usize) -> Value {
    let mut value = record_value(corpus, cell, Platform::Windows, "unknown", "unverified");
    value["operation"]["kind"] = json!(PARTITIONS[partition].1);
    // This is a synthetic fixture identity, never the measured engine or a product build.
    value["revisions"]["engine"] = json!("webview2@fixture-only");
    value
}

fn expect_product_refusal(corpus: &Corpus, value: &Value) {
    let error = validate_committed_runs(corpus, &[vec![to_record(value)]])
        .expect_err("no trusted product collector exists");
    assert!(
        matches!(error, CorpusError::ProductRecordsNeedReceipt { .. }),
        "the existing product admission boundary must refuse: {error}"
    );
}

/// Only existing structural checks; this must never be used as product admission.
fn shape_only(corpus: &Corpus, value: &Value) -> Result<(), CorpusError> {
    let record = to_record(value);
    let keys = [keld_compat::evidence::CellKey {
        operation_id: record.operation().id.clone(),
        oracle_id: record.operation().oracle.id.clone(),
    }];
    let receipt = ProductReceipt {
        state: ProfileState::Unverified,
        cells: &keys,
    };
    let run = corpus.validate_product_run(&receipt, &[record], V1_RECORDS_AS_OF)?;
    assert!(!run.board().complete());
    assert_eq!(run.board().unweighted_percent(), None);
    Ok(())
}

#[test]
fn gh1183_prepared_cases_remain_unknown_and_require_actual_windows_targets() {
    let mut names = BTreeSet::new();
    for case in CASES {
        assert!(names.insert(case.name), "duplicate prepared attempt");
        let corpus = windows_corpus(case.partition);
        let cell = &corpus.cells()[case.cell];
        assert_eq!(cell.expected, Verdict::Unknown);
        assert_eq!(cell.platforms, [Platform::Windows]);
        assert!(REGISTRY.iter().all(|reg| reg.corpus_id != corpus.id()));
        let refusal = check_cells(
            corpus.id(),
            std::slice::from_ref(cell),
            corpus.registration(),
            Platform::Windows,
            RunnerKind::Libtest,
            &[],
        )
        .expect_err("an execution-plan row is not an executed product target");
        assert!(matches!(refusal, CorpusError::CaseNotAdmitted { .. }));
        expect_product_refusal(
            &corpus,
            &unqualified_record(&corpus, case.partition, case.cell),
        );
        let disk = case
            .disk_after
            .map_or_else(|| "no file-write oracle".to_owned(), sha256_uri);
        println!(
            "PREPARED / UNKNOWN | {} | {} | fixture disk={} | action={} | blocked by={}",
            case.name, cell.key.operation_id, disk, case.action, case.dependencies
        );
    }
    assert_eq!(
        names,
        BTreeSet::from([
            "launch",
            "open",
            "edit",
            "save-existing",
            "save-as",
            "dirty-close-cancel",
            "dirty-close-save",
            "dirty-close-discard",
            "cleanup",
            "fresh-relaunch",
        ]),
        "every requested native case must stay visible, even when it shares a corpus cell"
    );
    // Every GH500 activation/workflow cell is represented; cleanup and relaunch
    // are subcases, not extra scoreable cells or an install claim.
    let covered: BTreeSet<_> = CASES
        .iter()
        .map(|case| (case.partition, case.cell))
        .collect();
    let required: BTreeSet<_> = PARTITIONS
        .iter()
        .enumerate()
        .skip(1)
        .flat_map(|(partition, (_, _, _, operations))| {
            (0..operations.len()).map(move |cell| (partition, cell))
        })
        .collect();
    assert_eq!(covered, required);
}

#[test]
fn gh1183_wrong_digest_and_engine_fail_specific_existing_reader_checks() {
    let corpus = windows_corpus(2);
    let baseline = unqualified_record(&corpus, 2, 2);
    shape_only(&corpus, &baseline).expect("healthy UNKNOWN shape, not authentic native proof");
    for (field, replacement) in [
        ("artifact.sha256", sha256_uri(INPUT)),
        ("revisions.engine", "wkwebview@fixture-only".to_owned()),
        ("revisions.engine", "mock@fixture-only".to_owned()),
    ] {
        let mut changed = baseline.clone();
        let (parent, key) = field.split_once('.').expect("existing record field");
        changed[parent][key] = json!(replacement);
        let error = shape_only(&corpus, &changed).expect_err("one identity fault must be rejected");
        assert!(
            matches!(error, CorpusError::RecordMismatch { field: actual, .. } if actual == field)
        );
        expect_product_refusal(&corpus, &changed);
    }
    shape_only(&corpus, &baseline).expect("healthy shape still works after each negative");
}

#[test]
fn gh1183_same_success_text_from_windows_mac_mock_or_hello_never_admits_product() {
    let corpus = windows_corpus(2);
    let case_name = &corpus.cells()[2].test_name;
    let success = format!("test {case_name} ... ok\n");
    assert!(
        rust_case_passed(&success, case_name),
        "a text-only check can be imitated"
    );
    // Keep the exact success line while replacing the referenced evidence.
    // The first two attachments are existing committed lifecycle receipts,
    // not original draw.io product observations. The last two are explicit fakes.
    for (source, attachment) in [
        ("windows-headless", WINDOWS_HEADLESS),
        ("macos-headless", MAC_HEADLESS),
        (
            "mock",
            b"synthetic fixture only: saved successfully".as_slice(),
        ),
        (
            "hello-world",
            b"synthetic hello-world page: saved successfully".as_slice(),
        ),
    ] {
        assert!(rust_case_passed(&success, case_name));
        let mut value = unqualified_record(&corpus, 2, 2);
        value["evidence_uri"] = json!(sha256_uri(attachment));
        shape_only(&corpus, &value).expect("shape cannot authenticate the attachment");
        expect_product_refusal(&corpus, &value);
        println!("REFUSED / ProductRecordsNeedReceipt | same success text | {source}");
    }
}

#[test]
fn gh1183_wrong_document_bytes_and_stale_generation_cannot_bypass_product_gate() {
    assert_ne!(INPUT, EDITED);
    assert_ne!(OTHER, EDITED);
    let corpus = windows_corpus(2);
    // Opaque test attachments are NOT a new receipt format or generation authority.
    // Change only the named input; neither attachment can become authenticated.
    let current = b"UNTRUSTED TEST ATTACHMENT: saved successfully; role G2, current G2";
    let stale = b"UNTRUSTED TEST ATTACHMENT: saved successfully; role G1, current G2";
    for (name, healthy, faulty) in [
        ("wrong-saved-bytes-original", EDITED, INPUT),
        ("wrong-saved-bytes-other-document", EDITED, OTHER),
        ("stale-generation", current.as_slice(), stale.as_slice()),
    ] {
        let mut baseline = unqualified_record(&corpus, 2, 2);
        baseline["evidence_uri"] = json!(sha256_uri(healthy));
        let mut changed = baseline.clone();
        changed["evidence_uri"] = json!(sha256_uri(faulty));
        assert_ne!(
            changed["evidence_uri"], baseline["evidence_uri"],
            "{name} changed bytes"
        );
        shape_only(&corpus, &changed).expect("opaque locator is not authenticated provenance");
        expect_product_refusal(&corpus, &changed);
        expect_product_refusal(&corpus, &baseline);
        println!(
            "REFUSED / ProductRecordsNeedReceipt | {name} | per-field native check NOT IMPLEMENTED"
        );
    }
}

#[test]
fn gh1183_no_self_declared_generation_or_origin_can_extend_closed_evidence_schema() {
    let corpus = windows_corpus(2);
    let baseline = unqualified_record(&corpus, 2, 2);
    for (field, claim) in [
        ("generation", json!({"role": 2, "document": 7})),
        ("origin", json!("host")),
        ("saved_document_sha256", json!(sha256_uri(EDITED))),
    ] {
        let mut changed = baseline.clone();
        changed[field] = claim;
        let error = parse_evidence(&bytes(&changed))
            .expect_err("self-declared provenance is not part of the owned schema");
        assert!(
            matches!(&error, EvidenceError::InvalidRecord { detail }
            if detail.contains("unknown field") && detail.contains(field)),
            "unowned field {field} must be refused specifically: {error}"
        );
    }
}

#[test]
fn gh1183_app_lock_version_cannot_replace_or_extend_the_normative_pin() {
    let (reg, baseline) = windows_shape(2);
    let corpus = parse_with(&reg, &baseline, &[]).expect("current owned pin");
    assert_eq!(corpus.pin().version, "44.4.5");
    let mut repinned = baseline.clone();
    repinned["upstream"]["electron_version"] = json!("44.5.1");
    assert!(matches!(
        parse_with(&reg, &repinned, &[]),
        Err(CorpusError::UnadmittedPin { .. })
    ));
    let mut second_pin = baseline;
    second_pin["upstream"]["application_electron_version"] = json!("44.5.1");
    assert!(matches!(
        parse_with(&reg, &second_pin, &[]),
        Err(CorpusError::Parse { .. })
    ));
}

#[test]
fn gh1183_app_only_pass_and_undeclared_mac_cell_are_refused() {
    let corpus = windows_corpus(2);
    let baseline = unqualified_record(&corpus, 2, 2);
    let mut asserted_pass = baseline.clone();
    asserted_pass["result"] = json!("pass");
    assert!(matches!(
        shape_only(&corpus, &asserted_pass),
        Err(CorpusError::RecordMismatch {
            field: "result",
            ..
        })
    ));
    let mut mac = baseline;
    mac["artifact"]["platform"] = json!("macos");
    mac["artifact"]["arch"] = json!("aarch64");
    mac["revisions"]["engine"] = json!("wkwebview@fixture-only");
    assert!(matches!(
        shape_only(&corpus, &mac),
        Err(CorpusError::RecordMismatch {
            field: "revisions.engine",
            ..
        })
    ));
    expect_product_refusal(&corpus, &asserted_pass);
    expect_product_refusal(&corpus, &mac);
}
