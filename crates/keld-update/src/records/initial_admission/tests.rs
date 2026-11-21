//! D-64/D-68 exact bytes and cross-record refusal oracles.
use super::*;

const VOLUME: &str = r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\";
const SID: &str = "S-1-5-21-100-200-300-1001";

fn anchor() -> RepositoryAnchor {
    RepositoryAnchor {
        repository_id: [0x71; 32],
        app_id: "com.gyldlab.testing".to_owned(),
        owner_sid: SID.to_owned(),
    }
}

fn object(kind: ObjectKind) -> NativeObjectObservation {
    NativeObjectObservation {
        volume_guid: VOLUME.to_owned(),
        device: 0x13,
        file: 0x19,
        kind,
    }
}

fn record() -> InitialAdmissionRecord {
    let mut identity = crate::tests::expected_identity();
    identity.app_id = anchor().app_id;
    identity.baseline.app_id = identity.app_id.clone();
    InitialAdmissionRecord {
        attempt_id: [0x2a; 32],
        repository_id: anchor().repository_id,
        owner_sid: SID.to_owned(),
        metadata_sequence: 1,
        metadata_generation_sha256: [0x01; 32],
        recovery_epoch: 0,
        host_sha256: [0x02; 32],
        catalogue_sha256: [0x03; 32],
        repository_policy_sha256: [0x04; 32],
        discovery_policy_sha256: [0x05; 32],
        manifest_sha256: [0x06; 32],
        package_content_blake3: [0x07; 32],
        package_transport_blake3: [0x08; 32],
        destination_identity: identity,
        destination_root_identity: DestinationRootBinding {
            kind: RootBindingKind::Absent,
            anchor: object(ObjectKind::Directory),
            remaining: vec!["GYLDLAB".to_owned(), "Keld".to_owned(), "v2".to_owned()],
        },
        source_identity: object(ObjectKind::RegularFile),
        phase: InitialAdmissionPhase::Admitted,
    }
}

fn pointer(record: &InitialAdmissionRecord) -> PendingRepositoryOperation {
    PendingRepositoryOperation {
        repository_id: record.repository_id,
        app_id: record.destination_identity.app_id.clone(),
        owner_sid: record.owner_sid.clone(),
        kind: PendingOperationKind::InitialAdmission,
        attempt_id: record.attempt_id,
    }
}

fn assert_refuses<T: std::fmt::Debug>(result: Result<T, UpdateError>) {
    let error = result.expect_err("closed record must refuse");
    assert_eq!(error.code(), "KELD-UPDATE-014", "{error}");
}

#[test]
fn admission_and_pointer_are_canonical_bounded_records_with_exact_order() {
    let journal = record();
    let journal_bytes = encode_initial_admission_record(&journal).expect("D64 encoding");
    assert!(
        journal_bytes.starts_with(br#"{"schema":"keld.windows.initial-admission/v1","attemptId":"#)
    );
    assert!(journal_bytes.ends_with(br#""phase":"admitted"}"#));
    assert!(journal_bytes.len() <= 32_768);
    assert_eq!(
        decode_initial_admission_record(&journal_bytes).expect("D64 decoding"),
        journal
    );
    let pointer = pointer(&journal);
    let pointer_bytes = encode_pending_repository_operation(&pointer).expect("D68 encoding");
    assert!(
        pointer_bytes
            .starts_with(br#"{"schema":"keld.repository-pending-operation/v1","repositoryId":"#)
    );
    assert!(pointer_bytes.ends_with(
        br#""attemptId":"2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"}"#
    ));
    assert!(pointer_bytes.len() <= 2_048);
    assert_eq!(
        decode_pending_repository_operation(&pointer_bytes).expect("D68 decoding"),
        pointer
    );
    assert_eq!(
        pointer.expected_journal_leaf(),
        "admissions/".to_owned() + &"2a".repeat(32) + ".json"
    );
}

#[test]
fn journal_refuses_unknown_duplicate_reordered_noncanonical_and_oversized_fields() {
    let source = String::from_utf8(encode_initial_admission_record(&record()).expect("journal"))
        .expect("canonical UTF-8");
    let cases = [
        source.replacen(
            r#""phase":"admitted""#,
            r#""phase":"admitted","phase":"aborted""#,
            1,
        ),
        source.replacen(
            r#""phase":"admitted""#,
            r#""extra":true,"phase":"admitted""#,
            1,
        ),
        source.replacen(r#""phase":"admitted""#, r#""phase":"bad-phase""#, 1),
        source.replacen(
            "keld.windows.initial-admission/v1",
            "keld.windows.initial-admission/v2",
            1,
        ),
        source.replacen("\"metadataSequence\":1", "\"metadataSequence\":0", 1),
        source.replacen("\"hostSha256\":\"", "\"hostSha256\":\"AA", 1),
        source.replacen("\"ownerSid\":\"", "\"ownerSid\":\"s-", 1),
        source.replacen(
            "\"sourceIdentity\":",
            "\"unexpected\":0,\"sourceIdentity\":",
            1,
        ),
        source.replacen(
            "\"sourceIdentity\":",
            "\"destinationRootIdentity\":null,\"sourceIdentity\":",
            1,
        ),
        format!(" {source}"),
        format!("{source}\n"),
    ];
    for candidate in &cases {
        assert_ne!(candidate, &source, "negative control must change bytes");
        assert_refuses(decode_initial_admission_record(candidate.as_bytes()));
    }
    assert_refuses(decode_initial_admission_record(&vec![b' '; 32_769]));
    let mut truncated = source.into_bytes();
    truncated.pop();
    assert_refuses(decode_initial_admission_record(&truncated));
}

#[test]
fn pointer_refuses_duplicate_unknown_reordered_invalid_kind_and_size() {
    let source = String::from_utf8(
        encode_pending_repository_operation(&pointer(&record())).expect("pointer"),
    )
    .expect("canonical UTF-8");
    for candidate in [
        source.replacen("\"attemptId\":", "\"other\":0,\"attemptId\":", 1),
        source.replacen("\"attemptId\":", "\"appId\":\"forged\",\"attemptId\":", 1),
        source.replacen("initial-admission", "unknown-operation", 1),
        source.replacen(
            "keld.repository-pending-operation/v1",
            "keld.repository-pending-operation/v2",
            1,
        ),
        source.replacen("2a2a2a2a", "2A2A2A2A", 1),
        format!(" {source}"),
        format!("{source}\n"),
    ] {
        assert_ne!(candidate, source);
        assert_refuses(decode_pending_repository_operation(candidate.as_bytes()));
    }
    assert_refuses(decode_pending_repository_operation(&vec![b'a'; 2_049]));
}

#[test]
fn d48_observations_fail_closed_for_source_and_destination_substitution() {
    let canonical = record();
    let mut source_is_directory = canonical.clone();
    source_is_directory.source_identity.kind = ObjectKind::Directory;
    assert_refuses(encode_initial_admission_record(&source_is_directory));

    let mut anchor_is_file = canonical.clone();
    anchor_is_file.destination_root_identity.anchor.kind = ObjectKind::RegularFile;
    assert_refuses(encode_initial_admission_record(&anchor_is_file));

    let mut wrong_volume = canonical.clone();
    wrong_volume.source_identity.volume_guid = VOLUME.to_uppercase();
    assert_refuses(encode_initial_admission_record(&wrong_volume));

    let mut bad_name = canonical.clone();
    bad_name.destination_root_identity.remaining = vec!["..".to_owned()];
    assert_refuses(encode_initial_admission_record(&bad_name));

    let mut too_many_parts = canonical.clone();
    too_many_parts.destination_root_identity.remaining = vec!["safe".to_owned(); 7];
    assert_refuses(encode_initial_admission_record(&too_many_parts));

    let mut oversized_component = canonical.clone();
    oversized_component.destination_root_identity.remaining = vec!["s".repeat(256)];
    assert_refuses(encode_initial_admission_record(&oversized_component));

    let mut inconsistent_existing = canonical.clone();
    inconsistent_existing.destination_root_identity.kind = RootBindingKind::Existing;
    assert_refuses(encode_initial_admission_record(&inconsistent_existing));
}

#[test]
fn d68_pointer_binds_exact_repository_app_owner_kind_and_attempt() {
    let expected = anchor();
    let journal = record();
    let good = pointer(&journal);
    assert_eq!(
        inspect_initial_admission(&expected, Some(&journal), Some(&good), &[]).expect("matched"),
        AdmissionInspection::AdmittedUnarmed
    );
    let mut foreign = good.clone();
    foreign.repository_id[0] ^= 1;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&journal),
        Some(&foreign),
        &[],
    ));
    foreign = good.clone();
    foreign.app_id = "com.other".to_owned();
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&journal),
        Some(&foreign),
        &[],
    ));
    foreign = good.clone();
    foreign.owner_sid = "S-1-5-21-999-888-777-1001".to_owned();
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&journal),
        Some(&foreign),
        &[],
    ));
    foreign = good.clone();
    foreign.attempt_id[0] ^= 1;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&journal),
        Some(&foreign),
        &[],
    ));
    foreign = good.clone();
    foreign.kind = PendingOperationKind::ManualRecovery;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&journal),
        Some(&foreign),
        &[],
    ));
    assert_refuses(inspect_initial_admission(&expected, None, Some(&good), &[]));
}

#[test]
fn orphan_record_and_terminal_retention_never_reconstruct_creation_authority() {
    let expected = anchor();
    let mut journal = record();
    assert_eq!(
        inspect_initial_admission(&expected, Some(&journal), None, &[]).expect("unarmed orphan"),
        AdmissionInspection::OrphanAdmittedNeedsIndependentNoEffectProof,
    );
    journal.phase = InitialAdmissionPhase::CreationArmed;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&journal),
        None,
        &[],
    ));
    journal.phase = InitialAdmissionPhase::BootstrapOwned;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&journal),
        None,
        &[],
    ));
    journal.phase = InitialAdmissionPhase::Aborted;
    assert_eq!(
        inspect_initial_admission(&expected, Some(&journal), None, &[]).expect("terminal"),
        AdmissionInspection::RetainedTerminalDiagnostic,
    );
}

#[test]
fn retained_a_and_terminal_b_are_pending_only_with_exact_b_pointer() {
    let expected = anchor();
    for retained_phase in [
        InitialAdmissionPhase::Complete,
        InitialAdmissionPhase::Aborted,
    ] {
        for pending_phase in [
            InitialAdmissionPhase::Complete,
            InitialAdmissionPhase::Aborted,
        ] {
            let mut b = record();
            b.phase = pending_phase;
            let mut a = record();
            a.attempt_id = [0x3b; 32];
            a.phase = retained_phase;
            assert_eq!(
                inspect_initial_admission(&expected, Some(&b), Some(&pointer(&b)), &[a.clone()])
                    .expect("pointed terminal B"),
                AdmissionInspection::TerminalCoherenceProofRequired,
            );
            assert_refuses(inspect_initial_admission(
                &expected,
                Some(&b),
                None,
                &[a.clone()],
            ));
            assert_refuses(inspect_initial_admission(
                &expected,
                Some(&b),
                Some(&pointer(&a)),
                &[a.clone()],
            ));
            assert_eq!(
                inspect_initial_admission(&expected, Some(&b), None, &[])
                    .expect("after retention rotation"),
                AdmissionInspection::RetainedTerminalDiagnostic,
            );
        }
    }
}

#[test]
fn bounded_retained_ledger_refuses_multiple_foreign_or_nonterminal_diagnostics() {
    let expected = anchor();
    let b = record();
    let mut a = record();
    a.attempt_id = [0x3b; 32];
    a.phase = InitialAdmissionPhase::Complete;
    let mut other = a.clone();
    other.attempt_id = [0x4c; 32];
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&b),
        Some(&pointer(&b)),
        &[a.clone(), other],
    ));
    let mut foreign_owner = a.clone();
    foreign_owner.owner_sid = "S-1-5-21-999-888-777-1001".to_owned();
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&b),
        Some(&pointer(&b)),
        &[foreign_owner],
    ));
    let mut foreign_repository = a.clone();
    foreign_repository.repository_id[0] ^= 1;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&b),
        Some(&pointer(&b)),
        &[foreign_repository],
    ));
    let mut not_terminal = a;
    not_terminal.phase = InitialAdmissionPhase::CreationArmed;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&b),
        Some(&pointer(&b)),
        &[not_terminal],
    ));
}
