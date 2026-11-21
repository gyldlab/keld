//! KEL-135 canonical app-ID, D-64/D-68 field-order and D-48 GUID negative controls.

use super::*;

fn assert_local_detail<T: std::fmt::Debug>(result: Result<T, UpdateError>, expected: &str) {
    match result.expect_err("record must refuse with the specified reason") {
        UpdateError::LocalRecordInvalid { detail } => {
            assert_eq!(detail, expected, "the test must reach the intended guard");
        }
        other => panic!("expected local-record refusal {expected:?}, got {other:?}"),
    }
}

#[test]
fn kel135_noncanonical_app_ids_refuse_both_record_codecs_on_encode_and_decode() {
    const CANONICAL: &str = "com.gyldlab.testing";
    const INVALID: &[&str] = &[
        "Other.App",
        "other/app",
        "org..test",
        "é",
        "foo_bar",
        "-leading",
        "trailing-",
        "good.-bad",
        "good.bad-",
    ];
    let journal_source =
        String::from_utf8(encode_initial_admission_record(&record()).expect("canonical admission"))
            .expect("ASCII journal");
    let pointer_source = String::from_utf8(
        encode_pending_repository_operation(&pointer(&record())).expect("canonical pointer"),
    )
    .expect("ASCII pointer");
    assert_eq!(journal_source.matches(CANONICAL).count(), 2);
    assert_eq!(pointer_source.matches(CANONICAL).count(), 1);

    let too_long = "a".repeat(256);
    for invalid in INVALID
        .iter()
        .copied()
        .chain(std::iter::once(too_long.as_str()))
    {
        let mut journal = record();
        journal.destination_identity.app_id = invalid.to_owned();
        journal.destination_identity.baseline.app_id = invalid.to_owned();
        assert_local_detail(
            encode_initial_admission_record(&journal),
            "application identity must use KEL-135 canonical lowercase ASCII dot segments",
        );
        let mutated_journal = journal_source.replace(CANONICAL, invalid);
        assert!(serde_json::from_str::<serde_json::Value>(&mutated_journal).is_ok());
        assert_local_detail(
            decode_initial_admission_record(mutated_journal.as_bytes()),
            "application identity must use KEL-135 canonical lowercase ASCII dot segments",
        );

        let mut pending = pointer(&record());
        pending.app_id = invalid.to_owned();
        assert_local_detail(
            encode_pending_repository_operation(&pending),
            "application identity must use KEL-135 canonical lowercase ASCII dot segments",
        );
        let mutated_pointer = pointer_source.replace(CANONICAL, invalid);
        assert!(serde_json::from_str::<serde_json::Value>(&mutated_pointer).is_ok());
        assert_local_detail(
            decode_pending_repository_operation(mutated_pointer.as_bytes()),
            "application identity must use KEL-135 canonical lowercase ASCII dot segments",
        );
    }
}

#[test]
fn kel135_canonical_app_ids_roundtrip_both_record_codecs_at_length_boundary() {
    let max_length = "a".repeat(255);
    for valid in ["a", "0", "dev.keld-1.fixture", max_length.as_str()] {
        let mut admission = record();
        admission.destination_identity.app_id = valid.to_owned();
        admission.destination_identity.baseline.app_id = valid.to_owned();
        let journal_bytes = encode_initial_admission_record(&admission).expect("valid KEL-135 app");
        assert_eq!(
            decode_initial_admission_record(&journal_bytes).expect("canonical D64"),
            admission
        );

        let pending = pointer(&admission);
        let pointer_bytes = encode_pending_repository_operation(&pending).expect("valid D68 app");
        assert_eq!(
            decode_pending_repository_operation(&pointer_bytes).expect("canonical D68"),
            pending
        );
    }
}

#[test]
fn reordered_admission_and_pointer_fields_refuse_only_for_noncanonical_order() {
    let journal = String::from_utf8(encode_initial_admission_record(&record()).expect("journal"))
        .expect("UTF-8");
    let schema = r#""schema":"keld.windows.initial-admission/v1""#;
    let attempt = format!(r#""attemptId":"{}""#, "2a".repeat(32));
    let expected_prefix = format!("{{{schema},{attempt},");
    let reordered_prefix = format!("{{{attempt},{schema},");
    assert!(journal.starts_with(&expected_prefix));
    let reordered_journal = journal.replacen(&expected_prefix, &reordered_prefix, 1);
    assert_ne!(reordered_journal, journal);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reordered_journal).expect("swapped JSON"),
        serde_json::from_str::<serde_json::Value>(&journal).expect("original JSON")
    );
    assert_local_detail(
        decode_initial_admission_record(reordered_journal.as_bytes()),
        "record differs from its canonical typed encoding",
    );

    let pointer = String::from_utf8(
        encode_pending_repository_operation(&pointer(&record())).expect("pointer"),
    )
    .expect("UTF-8");
    let schema = r#""schema":"keld.repository-pending-operation/v1""#;
    let repository = format!(r#""repositoryId":"{}""#, "71".repeat(32));
    let expected_prefix = format!("{{{schema},{repository},");
    let reordered_prefix = format!("{{{repository},{schema},");
    assert!(pointer.starts_with(&expected_prefix));
    let reordered_pointer = pointer.replacen(&expected_prefix, &reordered_prefix, 1);
    assert_ne!(reordered_pointer, pointer);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reordered_pointer).expect("swapped JSON"),
        serde_json::from_str::<serde_json::Value>(&pointer).expect("original JSON")
    );
    assert_local_detail(
        decode_pending_repository_operation(reordered_pointer.as_bytes()),
        "record differs from its canonical typed encoding",
    );
}

#[test]
fn uppercase_guid_digits_are_rejected_with_the_volume_prefix_unchanged() {
    let upper_guid = VOLUME.replace("89ab-cdef", "89AB-CDEF");
    assert_ne!(upper_guid, VOLUME);
    assert!(upper_guid.starts_with(r"\\?\Volume{"));
    assert!(upper_guid.ends_with(r"}\"));
    let mut changed = record();
    changed.source_identity.volume_guid = upper_guid.clone();
    assert_local_detail(
        encode_initial_admission_record(&changed),
        "native observation has malformed lowercase volume GUID",
    );

    let canonical =
        String::from_utf8(encode_initial_admission_record(&record()).expect("canonical journal"))
            .expect("UTF-8");
    let old = serde_json::to_string(VOLUME).expect("canonical JSON volume");
    let replacement = serde_json::to_string(&upper_guid).expect("mutated JSON volume");
    assert_eq!(canonical.matches(&old).count(), 2);
    let mutated = canonical.replacen(&old, &replacement, 1);
    assert!(serde_json::from_str::<serde_json::Value>(&mutated).is_ok());
    assert_local_detail(
        decode_initial_admission_record(mutated.as_bytes()),
        "native observation has malformed lowercase volume GUID",
    );
}
