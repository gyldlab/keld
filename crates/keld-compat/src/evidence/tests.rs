use super::{
    Arch, ArtifactClass, AuthorityProfile, CivilDate, Denominator, EVIDENCE_SCHEMA, EvidenceError,
    EvidenceRecord, FAT_CIGAM, FAT_CIGAM_64, FAT_MAGIC, FAT_MAGIC_64, MAX_EVIDENCE_BYTES,
    OperationKind, Panel, Platform, Scoreboard, Verdict, Waiver, classify_artifact,
    parse_denominator, parse_evidence, score,
};

mod evidence_uri;

const DIGEST_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DIGEST_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const AS_OF: CivilDate = CivilDate {
    year: 2026,
    month: 8,
    day: 19,
};

fn valid_evidence_json() -> String {
    format!(
        r#"{{
  "schema": "keld.compat.evidence/v1",
  "artifact": {{
    "sha256": "{DIGEST_A}",
    "platform": "macos",
    "arch": "aarch64"
  }},
  "revisions": {{
    "keld": "67f39cdc898254f1e0c9cd50800f242ae7a4c493",
    "bun": "1.3.14",
    "engine": "wkwebview"
  }},
  "authority_profile": "strict_bun",
  "operation": {{
    "id": "hello.window.open",
    "kind": "primary_workflow",
    "oracle": {{
      "id": "hello-window-visible",
      "revision": "kel26-macos"
    }}
  }},
  "result": "pass",
  "evidence_uri": "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493"
}}"#
    )
}

fn valid_denominator_json() -> String {
    format!(
        r#"{{
  "schema": "keld.compat.denominator/v1",
  "panel": "product",
  "corpus_id": "phase2-hello",
  "corpus_sha256": "{DIGEST_B}",
  "kind": "primary_workflow",
  "cells": [
    {{ "operation_id": "hello.window.open", "oracle_id": "hello-window-visible" }},
    {{ "operation_id": "hello.ipc.echo", "oracle_id": "kipc-echo-roundtrip" }}
  ]
}}"#
    )
}

fn parse_ok() -> EvidenceRecord {
    parse_evidence(valid_evidence_json().as_bytes()).expect("valid evidence")
}

fn second_pass() -> EvidenceRecord {
    pass_with_authority(CELL_ECHO, "strict_bun")
}

/// The two `valid_denominator_json` cells as `(operation_id, oracle_id)`.
const CELL_WINDOW: (&str, &str) = ("hello.window.open", "hello-window-visible");
const CELL_ECHO: (&str, &str) = ("hello.ipc.echo", "kipc-echo-roundtrip");

/// The four pre-T2 `authority_profile` strings and the variant each must
/// parse to. A literal oracle, independent of `parse_authority`/`as_str`.
const OTHER_AUTHORITY: [(&str, AuthorityProfile); 4] = [
    ("strict_bun", AuthorityProfile::StrictBun),
    (
        "sandboxed_addon_worker",
        AuthorityProfile::SandboxedAddonWorker,
    ),
    ("legacy_sandbox_off", AuthorityProfile::LegacySandboxOff),
    (
        "user_approved_tool_child",
        AuthorityProfile::UserApprovedToolChild,
    ),
];

/// All five v1 `authority_profile` strings with their literal variants.
fn all_authority() -> impl Iterator<Item = (&'static str, AuthorityProfile)> + Clone {
    OTHER_AUTHORITY
        .into_iter()
        .chain([("unverified", AuthorityProfile::Unverified)])
}

/// `valid_evidence_json` moved to `cell` and labelled with the literal
/// `authority_profile` string `profile`; digest and engine are unchanged.
fn evidence_json_with_authority(cell: (&str, &str), profile: &str) -> String {
    let label = format!(r#""authority_profile": "{profile}""#);
    let json = valid_evidence_json()
        .replace(r#""authority_profile": "strict_bun""#, &label)
        .replace(CELL_WINDOW.0, cell.0)
        .replace(CELL_WINDOW.1, cell.1);
    assert!(json.contains(&label), "fixture must carry {label}");
    json
}

fn pass_with_authority(cell: (&str, &str), profile: &str) -> EvidenceRecord {
    parse_evidence(evidence_json_with_authority(cell, profile).as_bytes())
        .expect("closed v1 record")
}

/// Two-cell showcase board: the panel that may publish a percentage, so
/// only identity consistency can withhold it.
fn score_showcase(window_profile: &str, echo_profile: &str) -> Scoreboard {
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.panel = Panel::Showcase;
    let records = [
        pass_with_authority(CELL_WINDOW, window_profile),
        pass_with_authority(CELL_ECHO, echo_profile),
    ];
    score(&denom, &records, AS_OF).expect("score")
}

#[test]
fn parse_evidence_accepts_closed_v1_record() {
    let record = parse_ok();
    assert_eq!(record.artifact.sha256, DIGEST_A);
    assert_eq!(record.artifact.platform, Platform::Macos);
    assert_eq!(record.artifact.arch, Arch::Aarch64);
    assert_eq!(record.revisions.bun, "1.3.14");
    assert_eq!(record.authority_profile, AuthorityProfile::StrictBun);
    assert_eq!(record.operation.kind, OperationKind::PrimaryWorkflow);
    assert_eq!(record.result, Verdict::Pass);
    assert!(record.waiver.is_none());
}

#[test]
fn parse_rejects_oversize() {
    let mut bytes = valid_evidence_json().into_bytes();
    bytes.resize(MAX_EVIDENCE_BYTES + 1, b' ');
    let err = parse_evidence(&bytes).expect_err("oversize");
    assert_eq!(err.code(), "KELD-COMPAT-001");
    assert!(err.to_string().contains("Split the ledger"));
}

#[test]
fn parse_rejects_non_utf8() {
    let err = parse_evidence(&[0xFF, 0xFE, 0x00]).expect_err("latin1");
    assert_eq!(err.code(), "KELD-COMPAT-002");
    assert!(err.to_string().contains("Re-encode"));
}

#[test]
fn parse_rejects_trailing_junk() {
    let mut json = valid_evidence_json();
    json.push_str(" true");
    let err = parse_evidence(json.as_bytes()).expect_err("junk");
    assert_eq!(err.code(), "KELD-COMPAT-003");
    assert!(err.to_string().contains("no trailing bytes"));
}

#[test]
fn parse_rejects_unknown_schema() {
    let json = valid_evidence_json().replace(EVIDENCE_SCHEMA, "keld.compat.evidence/v0");
    let err = parse_evidence(json.as_bytes()).expect_err("schema");
    assert_eq!(err.code(), "KELD-COMPAT-004");
    assert!(err.to_string().contains(EVIDENCE_SCHEMA));
}

#[test]
fn parse_rejects_unknown_field_and_bad_digest() {
    let extra = valid_evidence_json().replacen('{', r#"{"extra":1,"#, 1);
    let err = parse_evidence(extra.as_bytes()).expect_err("unknown field");
    assert_eq!(err.code(), "KELD-COMPAT-005");
    let bad = valid_evidence_json().replace(DIGEST_A, "sha256:DEADBEEF");
    let err = parse_evidence(bad.as_bytes()).expect_err("digest");
    assert_eq!(err.code(), "KELD-COMPAT-005");
    assert!(err.to_string().contains("closed field set"));
}

#[test]
fn waived_requires_owner_reason_expiry() {
    let json = valid_evidence_json().replace(r#""result": "pass""#, r#""result": "waived""#);
    let err = parse_evidence(json.as_bytes()).expect_err("missing waiver");
    assert_eq!(err.code(), "KELD-COMPAT-006");
    assert!(err.to_string().contains("owner, reason"));
}

#[test]
fn waiver_forbidden_on_pass() {
    let mut json = valid_evidence_json();
    json = json.replacen(
        r#""result": "pass""#,
        r#""result": "pass",
  "waiver": {"owner":"a","reason":"b","expires_on":"2026-12-31"}"#,
        1,
    );
    let err = parse_evidence(json.as_bytes()).expect_err("extra waiver");
    assert_eq!(err.code(), "KELD-COMPAT-006");
}

#[test]
fn civil_date_rejects_signed_chunks() {
    // Rust integer FromStr accepts a leading '+'; YYYY-MM-DD must not.
    let err = CivilDate::parse("+026-+1-+1").expect_err("signed chunks");
    assert_eq!(err.code(), "KELD-COMPAT-006");
    assert!(err.to_string().contains("YYYY-MM-DD"), "{err}");
    assert!(CivilDate::parse("2026-08-19").is_ok());
}

#[test]
fn parse_accepts_waived_with_complete_waiver() {
    let mut json = valid_evidence_json();
    json = json.replace(r#""result": "pass""#, r#""result": "waived""#);
    json = json.replacen(
        r#""evidence_uri""#,
        r#""waiver": {"owner":"release","reason":"engine gap","expires_on":"2026-12-31"},
  "evidence_uri""#,
        1,
    );
    let record = parse_evidence(json.as_bytes()).expect("waived");
    assert_eq!(record.result, Verdict::Waived);
    assert_eq!(record.waiver.expect("waiver").owner, "release");
}

#[test]
fn rejects_turn_citation_and_tmp_path() {
    for uri in ["turn0file3", "file:///tmp/out.json", "/tmp/keld-out.json"] {
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            uri,
        );
        let err = parse_evidence(json.as_bytes()).expect_err(uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        assert!(err.to_string().contains("non-normative leads"), "{uri}");
    }
}

#[test]
fn denominator_rejects_empty_and_duplicate_cells() {
    let empty = valid_denominator_json().replace(
        r#"cells": [
    { "operation_id": "hello.window.open", "oracle_id": "hello-window-visible" },
    { "operation_id": "hello.ipc.echo", "oracle_id": "kipc-echo-roundtrip" }
  ]"#,
        r#"cells": []"#,
    );
    let err = parse_denominator(empty.as_bytes()).expect_err("empty");
    assert_eq!(err.code(), "KELD-COMPAT-008");
    let dup = valid_denominator_json().replace("hello.ipc.echo", "hello.window.open");
    let dup = dup.replace("kipc-echo-roundtrip", "hello-window-visible");
    let err = parse_denominator(dup.as_bytes()).expect_err("dup");
    assert_eq!(err.code(), "KELD-COMPAT-008");
    assert!(err.to_string().contains("unique cells"));
}

#[test]
fn partial_corpus_refuses_percent_and_complete() {
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    let board = score(&denom, &[parse_ok()], AS_OF).expect("score");
    assert_eq!(board.passed, 1);
    assert_eq!(board.missing, 1);
    assert_eq!(board.denominator, 2);
    assert_eq!(board.unweighted_percent, None);
    assert!(!board.complete);
    assert_eq!(
        board.claim,
        format!("1/2 of product corpus phase2-hello@{DIGEST_B} (primary_workflow)")
    );
    assert!(!board.claim.contains("100% compatible"));
    assert!(!board.claim.contains("fully compatible"));
}

#[test]
fn full_pass_is_complete_and_still_names_denominator() {
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.panel = Panel::Showcase;
    let board = score(&denom, &[parse_ok(), second_pass()], AS_OF).expect("score");
    assert_eq!(board.unweighted_percent, Some(100));
    assert!(board.complete);
    assert_eq!(
        board.claim,
        format!("2/2 of showcase corpus phase2-hello@{DIGEST_B} (primary_workflow)")
    );
    assert!(!board.claim.contains("100% compatible"));
}

#[test]
fn extra_records_cannot_shrink_denominator() {
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    let mut stray = valid_evidence_json();
    stray = stray.replace("hello.window.open", "other.op");
    stray = stray.replace("hello-window-visible", "other-oracle");
    let stray = parse_evidence(stray.as_bytes()).expect("stray");
    let board = score(&denom, &[parse_ok(), stray], AS_OF).expect("score");
    assert_eq!(board.missing, 1);
    assert_eq!(board.unweighted_percent, None);
    assert!(!board.complete);
}

#[test]
fn unknown_cell_refuses_percent() {
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    let mut unknown = valid_evidence_json();
    unknown = unknown.replace(r#""result": "pass""#, r#""result": "unknown""#);
    let unknown = parse_evidence(unknown.as_bytes()).expect("unknown");
    let board = score(&denom, &[unknown, second_pass()], AS_OF).expect("score");
    assert_eq!(board.unknown, 1);
    assert_eq!(board.passed, 1);
    assert_eq!(board.unweighted_percent, None);
    assert!(!board.complete);
}

#[test]
fn expired_waiver_is_not_silent() {
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    let mut json = valid_evidence_json();
    json = json.replace(r#""result": "pass""#, r#""result": "waived""#);
    json = json.replacen(
        r#""evidence_uri""#,
        r#""waiver": {"owner":"release","reason":"gap","expires_on":"2026-01-01"},
  "evidence_uri""#,
        1,
    );
    let waived = parse_evidence(json.as_bytes()).expect("parse waiver");
    let err = score(&denom, &[waived], AS_OF).expect_err("expired");
    assert_eq!(err.code(), "KELD-COMPAT-006");
}

#[test]
fn waiver_is_valid_through_its_inclusive_expiry_day() {
    // `Waiver::expires_on` is the inclusive last valid date: scoring on that
    // day admits the waiver; the next day rejects it.
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.cells.truncate(1);
    let mut record = parse_ok();
    record.result = Verdict::Waived;
    record.waiver = Some(Waiver {
        owner: "release".to_owned(),
        reason: "gap".to_owned(),
        expires_on: AS_OF,
    });
    let board = score(&denom, std::slice::from_ref(&record), AS_OF)
        .expect("waiver must be valid on its expiry day");
    assert_eq!(board.waived, 1);
    assert_eq!(board.passed, 0);

    let day_after = CivilDate {
        day: AS_OF.day + 1,
        ..AS_OF
    };
    let err = score(&denom, &[record], day_after).expect_err("expired the day after");
    assert_eq!(err.code(), "KELD-COMPAT-006");
    assert!(err.to_string().contains("expired on 2026-08-19"), "{err}");
}

#[test]
fn empty_cells_score_is_not_complete_and_not_one_hundred_percent() {
    // parse_denominator already rejects `cells: []`. score must too: a
    // hand-built empty Vec used to yield passed==n==0, complete=true, "0/0".
    let denom = Denominator {
        panel: Panel::Product,
        corpus_id: "phase2-hello".to_owned(),
        corpus_sha256: DIGEST_B.to_owned(),
        kind: OperationKind::PrimaryWorkflow,
        cells: Vec::new(),
    };
    match score(&denom, &[], AS_OF) {
        Ok(board) => panic!(
            "empty cells must not score: complete={} percent={:?} claim={}",
            board.complete, board.unweighted_percent, board.claim
        ),
        Err(err) => {
            assert_eq!(err.code(), "KELD-COMPAT-008");
            assert!(err.to_string().contains("unique cells"), "{err}");
        }
    }
}

#[test]
fn duplicate_denom_cells_cannot_double_count_one_pass() {
    // parse_denominator rejects duplicate cells; a hand-built vec used to
    // count one Pass twice (2/2 complete from a single record).
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    let cell = denom.cells[0].clone();
    denom.cells = vec![cell.clone(), cell];
    match score(&denom, &[parse_ok()], AS_OF) {
        Ok(board) => panic!(
            "duplicate cells must not score: passed={} n={} complete={} percent={:?}",
            board.passed, board.denominator, board.complete, board.unweighted_percent
        ),
        Err(err) => {
            assert_eq!(err.code(), "KELD-COMPAT-008");
            assert!(err.to_string().contains("unique cells"), "{err}");
        }
    }
}

#[test]
fn tmp_and_turn_uri_rejected_at_score() {
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.cells.truncate(1);
    for uri in ["turn0file3", "/tmp/keld-out.json", "file:///tmp/out.json"] {
        let mut record = parse_ok();
        record.evidence_uri = uri.to_owned();
        let err = score(&denom, &[record], AS_OF).expect_err(uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
}

#[test]
fn mixed_artifact_identity_refuses_complete_and_percent() {
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.panel = Panel::Showcase;
    let first = parse_ok();
    let mut second = second_pass();
    second.artifact.sha256 = DIGEST_B.to_owned();
    second.authority_profile = AuthorityProfile::LegacySandboxOff;
    second.revisions.engine = "cef".to_owned();
    let board = score(&denom, &[first, second], AS_OF).expect("mixed stitch still counts");
    assert_eq!(board.passed, 2);
    assert_eq!(board.denominator, 2);
    assert!(
        !board.complete,
        "mixed digest/profile/engine must not be complete"
    );
    assert_eq!(board.unweighted_percent, None);
    assert!(!board.claim.contains("100%"));
}

#[test]
fn unverified_is_never_merged_with_another_profile() {
    // gh532 AC13. Negative controls: `parse_authority` mapping
    // "unverified" to `LegacySandboxOff` or `StrictBun`, and a profile
    // comparison that treats `Unverified` as equal to another variant.
    for (other, _) in OTHER_AUTHORITY {
        // Control: the same two records under one shared profile are
        // complete, so only the profile difference withholds below.
        let control = score_showcase(other, other);
        assert!(
            control.complete(),
            "{other}+{other} control must be complete"
        );
        assert_eq!(control.unweighted_percent(), Some(100), "{other}+{other}");
        for (window, echo) in [("unverified", other), (other, "unverified")] {
            let board = score_showcase(window, echo);
            assert_eq!(
                (board.passed(), board.missing()),
                (2, 0),
                "{window}+{echo}: the unverified run is counted, not dropped"
            );
            assert!(!board.complete(), "{window}+{echo} must not be complete");
            assert_eq!(board.unweighted_percent(), None, "{window}+{echo}");
        }
    }
}

#[test]
fn scoreboard_reports_unverified_only_for_an_all_unverified_board() {
    // gh532 AC14. Negative controls: an accessor that returns the first
    // record's profile for a mixed board, one that reports
    // `LegacySandboxOff` or `StrictBun` for an all-`unverified` board, and
    // a claim or rendered profile that names `strict_bun` or
    // `legacy_sandbox_off` for an `unverified` run.
    let board = score_showcase("unverified", "unverified");
    assert_eq!((board.passed(), board.missing()), (2, 0));
    assert_eq!(
        board.authority_profile(),
        Some(AuthorityProfile::Unverified)
    );
    assert_eq!(
        board.authority_profile().map(AuthorityProfile::as_str),
        Some("unverified")
    );
    // KEL-74 §4.3 rule 7: the claim format is unchanged and names no profile.
    assert_eq!(
        board.claim(),
        format!("2/2 of showcase corpus phase2-hello@{DIGEST_B} (primary_workflow)")
    );
    for promoted in ["strict_bun", "legacy_sandbox_off"] {
        assert!(!board.claim().contains(promoted), "{}", board.claim());
    }
    for (other, variant) in OTHER_AUTHORITY {
        let uniform = score_showcase(other, other);
        assert_eq!(
            uniform.authority_profile(),
            Some(variant),
            "{other}+{other}"
        );
        // Both cell orders, so neither a first- nor a last-record
        // accessor can report one profile for a mixed board.
        for (window, echo) in [("unverified", other), (other, "unverified")] {
            let mixed = score_showcase(window, echo);
            assert_eq!(mixed.authority_profile(), None, "{window}+{echo}");
        }
    }
}

#[test]
fn contributing_identity_requires_one_profile_digest_and_engine() {
    // Each case differs in exactly one identity dimension, so deleting
    // any one comparison in `shared_authority_profile` or
    // `contributing_identity_consistent` fails here.
    for (left, _) in all_authority() {
        for (right, _) in all_authority().filter(|(text, _)| *text != left) {
            let board = score_showcase(left, right);
            assert!(!board.complete(), "{left}+{right}");
            assert_eq!(board.unweighted_percent(), None, "{left}+{right}");
            assert_eq!(board.authority_profile(), None, "{left}+{right}");
        }
    }
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.panel = Panel::Showcase;
    let mut digest = pass_with_authority(CELL_ECHO, "legacy_sandbox_off");
    digest.artifact.sha256 = DIGEST_B.to_owned();
    let mut engine = pass_with_authority(CELL_ECHO, "legacy_sandbox_off");
    engine.revisions.engine = "cef".to_owned();
    for (dimension, echo) in [("digest", digest), ("engine", engine)] {
        let window = pass_with_authority(CELL_WINDOW, "legacy_sandbox_off");
        let board = score(&denom, &[window, echo], AS_OF).expect("score");
        assert!(!board.complete(), "{dimension}");
        assert_eq!(board.unweighted_percent(), None, "{dimension}");
        assert_eq!(
            board.authority_profile(),
            Some(AuthorityProfile::LegacySandboxOff),
            "{dimension} mismatch alone does not mix profiles"
        );
    }
    // Records that fill no committed cell contribute no profile.
    denom.cells.truncate(1);
    let foreign_kind = evidence_json_with_authority(CELL_WINDOW, "unverified")
        .replace(r#""kind": "primary_workflow""#, r#""kind": "install""#);
    let foreign_kind = parse_evidence(foreign_kind.as_bytes()).expect("install record");
    let outside = pass_with_authority(CELL_ECHO, "unverified");
    let window = pass_with_authority(CELL_WINDOW, "strict_bun");
    let records = [window, foreign_kind.clone(), outside.clone()];
    let board = score(&denom, &records, AS_OF).expect("score");
    assert!(board.complete());
    assert_eq!(board.authority_profile(), Some(AuthorityProfile::StrictBun));
    let board = score(&denom, &[foreign_kind, outside], AS_OF).expect("score");
    assert_eq!(board.missing(), 1);
    assert_eq!(board.authority_profile(), None, "no contributing record");
}

#[test]
fn authority_vocabulary_round_trips_and_unverified_is_distinct() {
    // gh532 AC15. Negative controls: swapping any two `as_str` arms fails
    // the round trip, and a sixth, unknown string is still KELD-COMPAT-005.
    // §4.4: `unverified` is a fifth v1 arm, not a schema bump.
    assert_eq!(EVIDENCE_SCHEMA, "keld.compat.evidence/v1");
    for (text, variant) in all_authority() {
        let record = pass_with_authority(CELL_WINDOW, text);
        assert_eq!(record.authority_profile(), variant, "{text}");
        assert_eq!(variant.as_str(), text);
    }
    for (text, variant) in OTHER_AUTHORITY {
        assert_ne!(AuthorityProfile::Unverified, variant, "{text}");
    }
    for unknown in ["verified", "Unverified", "unverified_sandbox_off", ""] {
        let json = evidence_json_with_authority(CELL_WINDOW, unknown);
        let err = parse_evidence(json.as_bytes()).expect_err(unknown);
        assert_eq!(err.code(), "KELD-COMPAT-005", "{unknown}");
        assert!(
            err.to_string()
                .contains(&format!("unknown authority_profile `{unknown}`")),
            "{err}"
        );
    }
}

#[test]
fn product_panel_omits_percent_for_uncommitted_corpus() {
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.cells.truncate(1);
    denom.corpus_id = "toy-uncommitted".to_owned();
    let board = score(&denom, &[parse_ok()], AS_OF).expect("score");
    assert_eq!(board.passed, 1);
    assert_eq!(board.denominator, 1);
    assert_eq!(board.unweighted_percent, None);
    assert!(
        !board.complete,
        "uncommitted product 1/1 must not be complete; consumers key off complete"
    );
    assert!(!board.claim.contains("100%"));
    assert!(!board.claim.contains("100% compatible"));
    assert!(!board.claim.contains("fully compatible"));
    assert!(
        board
            .claim
            .contains("1/1 of product corpus toy-uncommitted")
    );
}

#[test]
fn showcase_one_cell_pass_publishes_percent_and_complete() {
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.panel = Panel::Showcase;
    denom.cells.truncate(1);
    let board = score(&denom, &[parse_ok()], AS_OF).expect("score");
    assert_eq!(board.unweighted_percent, Some(100));
    assert!(board.complete);
    assert!(board.claim.contains("1/1 of showcase corpus"));
    assert!(!board.claim.contains("100% compatible"));
}

#[test]
fn scoreboard_struct_fields_are_not_public() {
    // Independent of score(): a `pub complete` / `pub unweighted_percent`
    // field is the same minting hole the Denominator list had. rustc
    // privacy is the seal; this test fails if those fields are re-exported.
    let src = include_str!("../evidence.rs");
    let marker = "pub struct Scoreboard {";
    let start = src.find(marker).expect("Scoreboard struct");
    let after = &src[start + marker.len()..];
    let end = after.find('}').expect("struct close");
    let fields = &after[..end];
    assert!(
        !fields.contains("pub "),
        "Scoreboard fields must stay private so callers cannot mint 100%/complete:\n{fields}"
    );
    assert!(
        fields.contains("unweighted_percent: Option<u8>"),
        "{fields}"
    );
    assert!(fields.contains("complete: bool"), "{fields}");
}

#[test]
fn constructed_pass_with_unexpired_waiver_is_not_a_pass() {
    // parse_evidence rejects Pass+waiver JSON; public fields let a caller
    // construct it anyway. score must not treat that as a pass.
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.cells.truncate(1);
    let mut record = parse_ok();
    record.result = Verdict::Pass;
    record.waiver = Some(Waiver {
        owner: "release".to_owned(),
        reason: "gap".to_owned(),
        expires_on: CivilDate {
            year: 2026,
            month: 12,
            day: 31,
        },
    });
    match score(&denom, &[record], AS_OF) {
        Ok(board) => panic!(
            "Pass+waiver must not score as pass: complete={} passed={} percent={:?}",
            board.complete, board.passed, board.unweighted_percent
        ),
        Err(err) => {
            assert_eq!(err.code(), "KELD-COMPAT-006");
        }
    }
}

#[test]
fn duplicate_cell_records_error() {
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    let err = score(&denom, &[parse_ok(), parse_ok()], AS_OF).expect_err("dup");
    assert_eq!(err.code(), "KELD-COMPAT-009");
    assert!(err.to_string().contains("one record"));
}

#[test]
fn classify_artifact_magic_classes() {
    assert_eq!(
        classify_artifact(&[0xCF, 0xFA, 0xED, 0xFE, 0x00]),
        ArtifactClass::MachO
    );
    // FAT_MAGIC / FAT_CIGAM (32-bit fat) and FAT_MAGIC_64 / FAT_CIGAM_64.
    assert_eq!(classify_artifact(&FAT_MAGIC), ArtifactClass::MachO);
    assert_eq!(classify_artifact(&FAT_CIGAM), ArtifactClass::MachO);
    assert_eq!(classify_artifact(&FAT_MAGIC_64), ArtifactClass::MachO);
    assert_eq!(classify_artifact(&FAT_CIGAM_64), ArtifactClass::MachO);
    assert_eq!(classify_artifact(b"MZ\x90\x00"), ArtifactClass::Pe);
    assert_eq!(classify_artifact(b"\x7FELF...."), ArtifactClass::Elf);
    assert_eq!(classify_artifact(b"\0asm\x01\x00"), ArtifactClass::Wasm);
    assert_eq!(classify_artifact(b""), ArtifactClass::Unknown);
    assert_eq!(classify_artifact(b"PK\x03\x04"), ArtifactClass::Unknown);
}

#[test]
fn error_display_table_has_code_and_fix() {
    let cases: [(EvidenceError, &str, &str); 9] = [
        (
            EvidenceError::TooLarge { len: 9, max: 8 },
            "KELD-COMPAT-001",
            "Split the ledger",
        ),
        (EvidenceError::NotUtf8, "KELD-COMPAT-002", "Re-encode"),
        (
            EvidenceError::InvalidJson {
                detail: "x".to_owned(),
            },
            "KELD-COMPAT-003",
            "no trailing bytes",
        ),
        (
            EvidenceError::UnsupportedSchema {
                found: "v0".to_owned(),
            },
            "KELD-COMPAT-004",
            EVIDENCE_SCHEMA,
        ),
        (
            EvidenceError::InvalidRecord {
                detail: "x".to_owned(),
            },
            "KELD-COMPAT-005",
            "closed field set",
        ),
        (
            EvidenceError::InvalidWaiver {
                detail: "x".to_owned(),
            },
            "KELD-COMPAT-006",
            "owner, reason",
        ),
        (
            EvidenceError::NonNormativeEvidence {
                detail: "x".to_owned(),
            },
            "KELD-COMPAT-007",
            "non-normative leads",
        ),
        (
            EvidenceError::InvalidDenominator {
                detail: "x".to_owned(),
            },
            "KELD-COMPAT-008",
            "unique cells",
        ),
        (
            EvidenceError::DuplicateCell {
                cell: "a/b".to_owned(),
            },
            "KELD-COMPAT-009",
            "one record",
        ),
    ];
    for (err, code, fix) in cases {
        let text = err.to_string();
        assert_eq!(err.code(), code);
        assert!(text.contains(code), "{text}");
        assert!(text.contains(fix), "{text}");
    }
}

#[test]
fn foreign_kind_record_cannot_fill_committed_cell() {
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.cells.truncate(1);
    let json =
        valid_evidence_json().replace(r#""kind": "primary_workflow""#, r#""kind": "install""#);
    let foreign = parse_evidence(json.as_bytes()).expect("install record");
    assert_eq!(foreign.operation.kind, OperationKind::Install);
    let board = score(&denom, &[foreign], AS_OF).expect("score");
    assert_eq!(board.missing, 1);
    assert_eq!(board.passed, 0);
    assert_eq!(board.unweighted_percent, None);
    assert!(!board.complete);
}

#[test]
fn rejects_malformed_https_ports() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    for uri in [
        format!("https://github.com:abc/gyldlab/keld/commit/{sha}"),
        format!("https://github.com:65536/gyldlab/keld/commit/{sha}"),
        format!("https://github.com:/gyldlab/keld/commit/{sha}"),
        format!("https://[2001:db8::1]:65536/org/repo/commit/{sha}"),
    ] {
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &uri,
        );
        let err = parse_evidence(json.as_bytes()).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
    let ok = format!("https://github.com:443/gyldlab/keld/commit/{sha}");
    let json = valid_evidence_json().replace(
        "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
        &ok,
    );
    parse_evidence(json.as_bytes()).expect("decimal u16 port is still a public https location");
}

#[test]
fn rejects_trailing_dot_github_raw_cdn_live_ref() {
    let hexfile = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let live = format!("https://raw.githubusercontent.com./gyldlab/keld/main/{hexfile}");
    let json = valid_evidence_json().replace(
        "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
        &live,
    );
    let err = parse_evidence(json.as_bytes()).expect_err(&live);
    assert_eq!(err.code(), "KELD-COMPAT-007");
    let pinned = format!("https://raw.githubusercontent.com./gyldlab/keld/{sha}/README.md");
    let json = valid_evidence_json().replace(
        "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
        &pinned,
    );
    parse_evidence(json.as_bytes())
        .expect("trailing FQDN dot on a pinned GitHub raw CDN ref is still a pin");
}
