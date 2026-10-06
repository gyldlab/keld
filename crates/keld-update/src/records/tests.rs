#[cfg(windows)]
use super::lifecycle_installation_id;
use super::{
    MAX_LOCAL_RECORD_BYTES, PointerKind, decode_activation_journal, decode_complete, decode_floor,
    decode_pointer, decode_provenance, encode_activation_journal, encode_complete, encode_floor,
    encode_pointer, encode_provenance,
};
use crate::records::{
    ActivationFailureClass, ActivationJournal, ActivationPhase, AttemptOwner, AttemptOwnership,
    InitiatingLogon,
};
use crate::tests::expected_identity;
use crate::{DirectInstallMode, InstallOwner, InstallProvenance, UpdateError};

const VOLUME: &str = r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\";

/// Owner facts of the checked-in v2 golden vectors: the logon session LUID with
/// `HighPart` 1 and `LowPart` `0x0002a5f3`, logged on at 2026-10-06T08:00:00Z, and an
/// owner process created 42 seconds later (both as FILETIME 100 ns ticks since 1601).
const GOLDEN_OWNERSHIP: AttemptOwnership = AttemptOwnership {
    initiating_logon: InitiatingLogon::new(0x0000_0001_0002_a5f3, 134_357_472_000_000_000),
    attempt_owner: AttemptOwner::new(4242, 134_357_472_420_000_000),
};

#[test]
fn floor_has_exact_canonical_bytes_and_distinct_pointer_schemas() {
    assert_eq!(
        encode_floor("1.2.3+installer").expect("floor"),
        br#"{"schema":"keld.version-floor/v1","version":"1.2.3+installer"}"#
    );
    assert_eq!(
        decode_floor(br#"{"schema":"keld.version-floor/v1","version":"1.2.3+installer"}"#)
            .expect("golden floor"),
        "1.2.3+installer"
    );
    let artifact = expected_identity().baseline;
    let current = encode_pointer(PointerKind::Current, &artifact).expect("current");
    let good = encode_pointer(PointerKind::LastKnownGood, &artifact).expect("LKG");
    let previous = encode_pointer(PointerKind::PreviousKnownGood, &artifact).expect("previous LKG");
    assert_eq!(
        decode_pointer(PointerKind::Current, &current).expect("current bytes"),
        artifact
    );
    assert_eq!(
        decode_pointer(PointerKind::LastKnownGood, &good).expect("LKG bytes"),
        artifact
    );
    assert_eq!(
        decode_pointer(PointerKind::PreviousKnownGood, &previous).expect("previous LKG bytes"),
        artifact
    );
    assert!(previous.starts_with(br#"{"schema":"keld.previous-known-good/v1""#));
    assert!(matches!(
        decode_pointer(PointerKind::Current, &good),
        Err(UpdateError::LocalRecordInvalid { .. })
    ));
    assert!(matches!(
        decode_pointer(PointerKind::LastKnownGood, &current),
        Err(UpdateError::LocalRecordInvalid { .. })
    ));
    assert!(matches!(
        decode_pointer(PointerKind::LastKnownGood, &previous),
        Err(UpdateError::LocalRecordInvalid { .. })
    ));
}

#[test]
fn strict_local_codec_rejects_duplicates_unknown_fields_and_noncanonical_bytes() {
    let cases: &[&[u8]] = &[
        b"",
        b"null",
        b"\xff",
        b"{}",
        br#"{"schema":"keld.version-floor/v2","version":"1.0.0"}"#,
        br#"{"schema":"keld.version-floor/v1","version":"1.0.0","version":"2.0.0"}"#,
        br#"{"schema":"keld.version-floor/v1","version":"1.0.0","extra":0}"#,
        br#"{"version":"1.0.0","schema":"keld.version-floor/v1"}"#,
        br#"{ "schema":"keld.version-floor/v1","version":"1.0.0"}"#,
        br#"{"schema":"keld.version-floor/v1","version":"\u0031.0.0"}"#,
        br#"{"schema":"keld.version-floor/v1","version":"01.0.0"}"#,
        b"{\"schema\":\"keld.version-floor/v1\",\"version\":\"1.0.0\"}\n",
    ];
    for bytes in cases {
        let error = decode_floor(bytes).expect_err("strict rejection");
        assert_eq!(error.code(), "KELD-UPDATE-014", "{bytes:?}");
    }
    assert!(decode_floor(&vec![b' '; MAX_LOCAL_RECORD_BYTES + 1]).is_err());
    let mut truncated = encode_floor("1.0.0").expect("floor");
    truncated.pop();
    assert!(decode_floor(&truncated).is_err());
}

#[test]
fn provenance_roundtrip_preserves_every_identity_and_trusted_scope_field() {
    let provenance = InstallProvenance {
        identity: expected_identity(),
        owner: InstallOwner::Direct,
    };
    let bytes = encode_provenance(&provenance, &[0xab; 32], VOLUME).expect("provenance");
    let decoded = decode_provenance(&bytes).expect("canonical provenance");
    assert_eq!(decoded.provenance, provenance);
    assert_eq!(decoded.publisher_scope, [0xab; 32]);
    assert_eq!(decoded.volume_guid, VOLUME);
    let text = String::from_utf8(bytes).expect("UTF-8");
    for altered in [
        text.replace(&"ab".repeat(32), &"AB".repeat(32)),
        text.replace("windows-system-users-rx-v1", "owner-private"),
        text.replace("strict-distinct-os-principals", "legacy-same-user"),
        text.replace("\"owner\":\"direct\"", "\"owner\":\"managed\""),
        text.replacen("\"app_id\":", "\"unknown\":0,\"app_id\":", 1),
    ] {
        assert!(decode_provenance(altered.as_bytes()).is_err(), "{altered}");
    }
}

#[cfg(windows)]
#[test]
fn lifecycle_installation_id_binds_the_versioned_canonical_provenance() {
    let provenance = InstallProvenance {
        identity: expected_identity(),
        owner: InstallOwner::Direct,
    };
    let canonical =
        encode_provenance(&provenance, &[0xab; 32], VOLUME).expect("canonical v2 provenance");
    assert_eq!(
        canonical,
        br#"{"schema":"keld.install-provenance/v2","owner":"direct","protection_profile":"windows-system-users-rx-v1","identity":{"install_mode":"machine-seamless-direct","app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","install_root":"C:\\Program Files\\KeldFixture","update_root":"C:\\ProgramData\\KeldFixture\\updates","signing_key_id":"0871f3aabc26e4582c508af5c03884e6a96f0989d1dd8cfb49cd17ed25792433","baseline":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"profile_digest":"0202020202020202020202020202020202020202020202020202020202020202","principal_model":"strict-distinct-os-principals"},"publisher_scope":"abababababababababababababababababababababababababababababababab","volume_guid":"\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\"}"#,
        "canonical provenance v2 bytes and field order are frozen by this vector"
    );
    let expected_length = u64::try_from(canonical.len()).expect("bounded record length");
    let mut expected = blake3::Hasher::new();
    expected.update(b"keld.installation-binding/provenance-v2/v1\0");
    expected.update(&expected_length.to_le_bytes());
    expected.update(&canonical);
    let expected = *expected.finalize().as_bytes();
    let actual = lifecycle_installation_id(&provenance, &[0xab; 32], VOLUME)
        .expect("domain-separated lifecycle ID");
    assert_eq!(actual, expected);
    assert_eq!(
        crate::error::hex_digest(&actual),
        "884410cbb07068c6a1d2799859cab6db99bdbd0114639362276f853c8b5454ba"
    );

    let mut changed = provenance.clone();
    changed.identity.install_root.push("-relocated");
    assert_ne!(
        lifecycle_installation_id(&changed, &[0xab; 32], VOLUME).expect("changed root ID"),
        actual
    );
    assert_ne!(
        lifecycle_installation_id(&provenance, &[0xac; 32], VOLUME).expect("changed publisher ID"),
        actual
    );
    assert_ne!(
        lifecycle_installation_id(
            &provenance,
            &[0xab; 32],
            r"\\?\Volume{11234567-89ab-cdef-0123-456789abcdef}\",
        )
        .expect("changed volume ID"),
        actual
    );
    changed = provenance.clone();
    changed.identity.install_mode = DirectInstallMode::MachineUacDirect;
    assert_ne!(
        lifecycle_installation_id(&changed, &[0xab; 32], VOLUME).expect("changed install mode ID"),
        actual
    );
}

#[test]
fn provenance_binds_each_explicit_mode_to_its_own_profile_and_refuses_v1() {
    let mut provenance = InstallProvenance {
        identity: expected_identity(),
        owner: InstallOwner::Direct,
    };
    for (mode, profile) in [
        (
            DirectInstallMode::PerUserDirect,
            "windows-per-user-role-rx-v1",
        ),
        (
            DirectInstallMode::MachineUacDirect,
            "windows-administrators-system-users-rx-v1",
        ),
        (
            DirectInstallMode::MachineSeamlessDirect,
            "windows-system-users-rx-v1",
        ),
    ] {
        provenance.identity.install_mode = mode;
        let bytes = encode_provenance(&provenance, &[0xab; 32], VOLUME).expect("mode record");
        let text = String::from_utf8(bytes.clone()).expect("UTF-8");
        assert!(text.contains(profile), "{text}");
        assert_eq!(
            decode_provenance(&bytes).expect("mode read").provenance,
            provenance
        );
        let mismatched_profile = text.replace(profile, "windows-owner-private-v0");
        assert!(decode_provenance(mismatched_profile.as_bytes()).is_err());
    }

    let legacy =
        String::from_utf8(encode_provenance(&provenance, &[0xab; 32], VOLUME).expect("v2 record"))
            .expect("UTF-8")
            .replace("keld.install-provenance/v2", "keld.install-provenance/v1")
            .replace(r#""install_mode":"machine-seamless-direct","#, "");
    assert!(decode_provenance(legacy.as_bytes()).is_err());
}

#[test]
fn local_roots_refuse_aliases_and_roundtrip_verbatim_drive_text_losslessly() {
    let mut provenance = InstallProvenance {
        identity: expected_identity(),
        owner: InstallOwner::Direct,
    };
    for root in [
        "relative",
        r"C:relative",
        r"\\server\share\install",
        r"\\.\C:\install",
        r"C:\install\.\next",
        r"C:\install\..\next",
        r"C:\install\\next",
        "C:/install",
        "C:\\install\\",
        "C:\\install\0",
    ] {
        provenance.identity.install_root = root.into();
        assert!(
            encode_provenance(&provenance, &[0; 32], VOLUME).is_err(),
            "{root:?}"
        );
    }
    provenance.identity.install_root = r"\\?\C:\Keld~1\Install".into();
    let bytes = encode_provenance(&provenance, &[0; 32], VOLUME).expect("verbatim root");
    assert_eq!(
        decode_provenance(&bytes).expect("verbatim read").provenance,
        provenance
    );
}

#[test]
fn complete_record_checks_exact_artifact_and_integer_bounds() {
    let artifact = expected_identity().baseline;
    for content_size in [1, keld_pack::MAX_ARTIFACT_BYTES] {
        let bytes = encode_complete(&artifact, content_size).expect("boundary");
        let complete = decode_complete(&bytes).expect("boundary read");
        assert_eq!(complete.artifact, artifact);
        assert_eq!(complete.content_size, content_size);
    }
    for content_size in [0, keld_pack::MAX_ARTIFACT_BYTES + 1] {
        assert!(encode_complete(&artifact, content_size).is_err());
    }
    let text = String::from_utf8(encode_complete(&artifact, 1).expect("complete")).expect("UTF-8");
    for number in ["0", "-1", "1.0", "1e0", "9007199254740992"] {
        assert!(
            decode_complete(
                text.replace("\"content_size\":1", &format!("\"content_size\":{number}"))
                    .as_bytes()
            )
            .is_err(),
            "{number}"
        );
    }
}

#[test]
fn codec_enforces_the_exact_record_size_ceiling_on_write_and_read() {
    let mut artifact = expected_identity().baseline;
    let base_length = encode_pointer(PointerKind::Current, &artifact)
        .expect("pointer")
        .len();
    artifact
        .app_id
        .push_str(&"x".repeat(MAX_LOCAL_RECORD_BYTES - base_length));
    let exact = encode_pointer(PointerKind::Current, &artifact).expect("exact maximum");
    assert_eq!(exact.len(), MAX_LOCAL_RECORD_BYTES);
    assert_eq!(
        decode_pointer(PointerKind::Current, &exact).expect("exact maximum read"),
        artifact
    );
    artifact.app_id.push('x');
    assert!(encode_pointer(PointerKind::Current, &artifact).is_err());
    let mut oversized = exact;
    oversized.push(b' ');
    assert!(decode_pointer(PointerKind::Current, &oversized).is_err());
}

fn activation_journal() -> ActivationJournal {
    let baseline = expected_identity().baseline;
    let mut previous = baseline.clone();
    previous.version = "0.9.0".to_owned();
    previous.content_blake3 = [0x33; 32];
    let mut candidate = baseline.clone();
    candidate.version = "1.1.0".to_owned();
    candidate.content_blake3 = [0x44; 32];
    ActivationJournal {
        attempt_id: [0x11; 32],
        candidate,
        rollback_target: baseline.clone(),
        prior_floor: baseline.version.clone(),
        prior_last_known_good: baseline,
        prior_previous_known_good: Some(previous),
        helper_image_blake3: [0x55; 32],
        health_channel_id: [0x66; 32],
        lifecycle_channel_id: [0x88; 32],
        ownership: Some(GOLDEN_OWNERSHIP),
        phase: ActivationPhase::PublishPending,
    }
}

/// The four persisted phases of the golden fixture, with each phase's v2 golden bytes.
fn golden_phases() -> [(&'static str, ActivationPhase, &'static [u8]); 4] {
    [
        (
            "publish-pending",
            ActivationPhase::PublishPending,
            include_bytes!("golden/journal-v2-publish-pending.json"),
        ),
        (
            "awaiting-health",
            ActivationPhase::AwaitingHealth,
            include_bytes!("golden/journal-v2-awaiting-health.json"),
        ),
        (
            "health-accepted",
            ActivationPhase::HealthAccepted {
                health_receipt_digest: [0x77; 32],
            },
            include_bytes!("golden/journal-v2-health-accepted.json"),
        ),
        (
            "rollback-pending",
            ActivationPhase::RollbackPending {
                failure: ActivationFailureClass::HealthRejected,
            },
            include_bytes!("golden/journal-v2-rollback-pending.json"),
        ),
    ]
}

#[test]
fn activation_journal_v2_golden_vectors_are_exact_for_every_phase() {
    for (name, phase, golden) in golden_phases() {
        let mut journal = activation_journal();
        journal.phase = phase;
        assert_eq!(
            String::from_utf8(encode_activation_journal(&journal).expect("v2 journal"))
                .expect("UTF-8"),
            std::str::from_utf8(golden).expect("UTF-8 golden"),
            "{name}: the v2 encoding is frozen by its checked-in golden vector"
        );
        assert_eq!(
            decode_activation_journal(golden).expect("v2 golden decodes"),
            journal,
            "{name}: the golden vector decodes to every journaled fact"
        );
    }
    // The spec fixes the owner encoding independently of the encoder under test.
    let golden = std::str::from_utf8(golden_phases()[0].2).expect("UTF-8 golden");
    assert!(golden.starts_with(r#"{"schema":"keld.activation-journal/v2","attempt_id":"1111"#));
    assert!(golden.contains(concat!(
        r#""lifecycle_channel_id":"8888888888888888888888888888888888888888888888888888888888888888","#,
        r#""initiating_logon":{"authentication_id":"000000010002a5f3","logon_time":"01dd5568af7ac000"},"#,
        r#""attempt_owner":{"owner_process_id":4242,"owner_creation_time":"01dd5568c8837100"},"#,
        r#""phase":{"phase":"publish-pending"}}"#,
    )));
    // The wire review approves the golden that the governing spec states, byte for byte.
    let spec = include_str!("../../../../docs/specs/kel53-full-package-activation.md");
    assert!(
        spec.contains(&format!("```json\n{golden}\n```")),
        "KEL-53 §4 states exactly the checked-in publish-pending golden"
    );
}

#[test]
fn a_v1_journal_decodes_without_owner_facts_and_keeps_its_landed_bytes() {
    // The retained v1 seed shares the golden fixture; only its owner facts are absent.
    let landed = include_bytes!("../../fuzz/corpus/activation_journal/canonical-awaiting-health");
    let mut journal = activation_journal();
    journal.phase = ActivationPhase::AwaitingHealth;
    journal.ownership = None;
    let decoded = decode_activation_journal(landed).expect("a landed v1 journal still decodes");
    assert_eq!(decoded, journal);
    assert_eq!(
        decoded.ownership, None,
        "a v1 record carries no attempt owner or initiating logon, so it admits no claim"
    );
    assert_eq!(
        encode_activation_journal(&journal).expect("finishing a v1 attempt"),
        landed,
        "phase writes that finish a v1 attempt keep the landed v1 bytes"
    );
    // A re-mint record that supplies both facts is v2.
    journal.ownership = Some(GOLDEN_OWNERSHIP);
    assert_eq!(
        encode_activation_journal(&journal).expect("v2 journal"),
        golden_phases()[1].2
    );
}

#[test]
fn every_v2_refusal_vector_refuses_as_an_invalid_local_record() {
    const EXPECTED: [&str; 52] = [
        "v2-missing-initiating-logon",
        "v2-missing-attempt-owner",
        "v2-missing-both-owner-fields",
        "v2-null-initiating-logon",
        "v2-null-attempt-owner",
        "v1-with-owner-fields",
        "v1-with-initiating-logon-only",
        "v1-with-attempt-owner-only",
        "unknown-schema-v3",
        "owner-objects-swapped",
        "owner-after-phase",
        "logon-keys-swapped",
        "owner-keys-swapped",
        "whitespace-in-owner",
        "missing-authentication-id",
        "missing-logon-time",
        "missing-owner-process-id",
        "missing-owner-creation-time",
        "duplicate-initiating-logon",
        "duplicate-attempt-owner",
        "duplicate-authentication-id",
        "duplicate-logon-time",
        "duplicate-owner-process-id",
        "duplicate-owner-creation-time",
        "unknown-field-in-initiating-logon",
        "unknown-field-in-attempt-owner",
        "unknown-top-level-owner-field",
        "zero-logon-time",
        "zero-owner-creation-time",
        "zero-owner-process-id",
        "negative-logon-time-min",
        "negative-logon-time-minus-one",
        "owner-process-id-above-u32",
        "owner-process-id-negative",
        "owner-process-id-fraction",
        "owner-process-id-exponent",
        "owner-process-id-leading-zero",
        "owner-process-id-string",
        "authentication-id-uppercase",
        "authentication-id-15-digits",
        "authentication-id-17-digits",
        "authentication-id-non-hex",
        "authentication-id-0x-prefix",
        "authentication-id-signed",
        "authentication-id-number",
        "authentication-id-escaped",
        "logon-time-uppercase",
        "logon-time-15-digits",
        "logon-time-number",
        "owner-creation-time-uppercase",
        "owner-creation-time-17-digits",
        "owner-creation-time-non-hex",
    ];
    let fixture = std::str::from_utf8(include_bytes!("golden/journal-v2-refusals.txt"))
        .expect("UTF-8 refusal vectors");
    assert!(
        !fixture.contains('\r'),
        "refusal vectors are exact LF-separated bytes"
    );
    let golden = golden_phases()[0].2;
    let mut labels = Vec::new();
    for line in fixture.lines() {
        let (label, record) = line.split_once('\t').expect("label<TAB>record");
        assert_ne!(
            record.as_bytes(),
            golden,
            "{label}: a refusal vector differs"
        );
        let error = decode_activation_journal(record.as_bytes())
            .expect_err("every refusal vector must refuse");
        assert_eq!(error.code(), "KELD-UPDATE-014", "{label}: {error}");
        labels.push(label);
    }
    assert_eq!(
        labels, EXPECTED,
        "every refusal vector is checked exactly once"
    );
}

#[test]
fn owner_fact_boundaries_encode_exact_h16_and_json_integers() {
    let cases = [
        (
            InitiatingLogon::new(0, 1),
            AttemptOwner::new(1, 1),
            concat!(
                r#""initiating_logon":{"authentication_id":"0000000000000000","logon_time":"0000000000000001"},"#,
                r#""attempt_owner":{"owner_process_id":1,"owner_creation_time":"0000000000000001"}"#,
            ),
        ),
        (
            InitiatingLogon::new(u64::MAX, i64::MAX),
            AttemptOwner::new(u32::MAX, u64::MAX),
            concat!(
                r#""initiating_logon":{"authentication_id":"ffffffffffffffff","logon_time":"7fffffffffffffff"},"#,
                r#""attempt_owner":{"owner_process_id":4294967295,"owner_creation_time":"ffffffffffffffff"}"#,
            ),
        ),
    ];
    for (initiating_logon, attempt_owner, expected) in cases {
        let mut journal = activation_journal();
        journal.ownership = Some(AttemptOwnership {
            initiating_logon,
            attempt_owner,
        });
        let bytes = encode_activation_journal(&journal).expect("boundary owner facts");
        assert!(
            String::from_utf8_lossy(&bytes).contains(expected),
            "{expected}"
        );
        assert_eq!(
            decode_activation_journal(&bytes).expect("boundary read"),
            journal
        );
    }
}

#[test]
fn the_encoder_refuses_owner_facts_that_name_no_process_or_session() {
    for (label, initiating_logon, attempt_owner) in [
        (
            "zero process ID",
            GOLDEN_OWNERSHIP.initiating_logon,
            AttemptOwner::new(0, 1),
        ),
        (
            "zero creation time",
            GOLDEN_OWNERSHIP.initiating_logon,
            AttemptOwner::new(4242, 0),
        ),
        (
            "zero logon time",
            InitiatingLogon::new(1, 0),
            GOLDEN_OWNERSHIP.attempt_owner,
        ),
        (
            "negative logon time",
            InitiatingLogon::new(1, -1),
            GOLDEN_OWNERSHIP.attempt_owner,
        ),
        (
            "minimum logon time",
            InitiatingLogon::new(1, i64::MIN),
            GOLDEN_OWNERSHIP.attempt_owner,
        ),
    ] {
        let mut journal = activation_journal();
        journal.ownership = Some(AttemptOwnership {
            initiating_logon,
            attempt_owner,
        });
        let error = encode_activation_journal(&journal).expect_err(label);
        assert_eq!(error.code(), "KELD-UPDATE-014", "{label}: {error}");
    }
}

#[test]
fn activation_journal_uses_exact_canonical_bytes_and_roundtrips_all_context() {
    let journal = activation_journal();
    let bytes = encode_activation_journal(&journal).expect("activation journal");
    let text = String::from_utf8(bytes.clone()).expect("UTF-8");
    assert!(text.starts_with(r#"{"schema":"keld.activation-journal/v2","attempt_id":"1111"#));
    assert!(text.ends_with(r#""phase":{"phase":"publish-pending"}}"#));
    assert_eq!(
        decode_activation_journal(&bytes).expect("canonical journal"),
        journal
    );

    let mut accepted = journal;
    accepted.phase = ActivationPhase::HealthAccepted {
        health_receipt_digest: [0x77; 32],
    };
    let accepted_bytes = encode_activation_journal(&accepted).expect("accepted journal");
    assert!(
        String::from_utf8_lossy(&accepted_bytes)
            .contains(r#""phase":{"phase":"health-accepted","health_receipt_digest":"7777"#)
    );
    assert_eq!(
        decode_activation_journal(&accepted_bytes).expect("accepted journal read"),
        accepted
    );
}

#[test]
fn retained_activation_journal_fuzz_seeds_cover_each_phase() {
    let seeds: [(&str, &[u8], ActivationPhase); 4] = [
        (
            "publish-pending",
            &include_bytes!("../../fuzz/corpus/activation_journal/canonical-publish-pending")[..],
            ActivationPhase::PublishPending,
        ),
        (
            "awaiting-health",
            &include_bytes!("../../fuzz/corpus/activation_journal/canonical-awaiting-health")[..],
            ActivationPhase::AwaitingHealth,
        ),
        (
            "health-accepted",
            &include_bytes!("../../fuzz/corpus/activation_journal/canonical-health-accepted")[..],
            ActivationPhase::HealthAccepted {
                health_receipt_digest: [0x77; 32],
            },
        ),
        (
            "rollback-pending",
            &include_bytes!("../../fuzz/corpus/activation_journal/canonical-rollback-pending")[..],
            ActivationPhase::RollbackPending {
                failure: crate::records::ActivationFailureClass::HealthRejected,
            },
        ),
    ];
    for (name, bytes, expected_phase) in seeds {
        let decoded = decode_activation_journal(bytes).expect("retained canonical fuzzer seed");
        assert_eq!(decoded.attempt_id, [0x11; 32], "{name}");
        assert_eq!(decoded.candidate.version, "1.1.0", "{name}");
        assert_eq!(decoded.phase, expected_phase, "{name}");
        assert_eq!(
            decoded.ownership, None,
            "{name}: a v1 seed has no owner facts"
        );
    }
    // The v2 seeds start mutation from every v2 phase shape and equal the goldens.
    let v2_seeds: [&[u8]; 4] = [
        include_bytes!("../../fuzz/corpus/activation_journal/canonical-v2-publish-pending"),
        include_bytes!("../../fuzz/corpus/activation_journal/canonical-v2-awaiting-health"),
        include_bytes!("../../fuzz/corpus/activation_journal/canonical-v2-health-accepted"),
        include_bytes!("../../fuzz/corpus/activation_journal/canonical-v2-rollback-pending"),
    ];
    for (seed, (name, phase, golden)) in v2_seeds.into_iter().zip(golden_phases()) {
        assert_eq!(seed, golden, "{name}: the v2 seed is the golden vector");
        let decoded = decode_activation_journal(seed).expect("retained v2 fuzzer seed");
        assert_eq!(decoded.phase, phase, "{name}");
        assert_eq!(decoded.ownership, Some(GOLDEN_OWNERSHIP), "{name}");
    }
}

#[test]
fn activation_journal_rejects_noncanonical_or_substituted_context() {
    let bytes = encode_activation_journal(&activation_journal()).expect("journal");
    let text = String::from_utf8(bytes).expect("UTF-8");
    let duplicate_attempt = text.replacen(
        r#""attempt_id":""#,
        r#""attempt_id":"0000000000000000000000000000000000000000000000000000000000000000","attempt_id":""#,
        1,
    );
    let duplicate_lifecycle = text.replace(&"88".repeat(32), &"66".repeat(32));
    let cases = [
        text.replace("keld.activation-journal/v2", "keld.activation-journal/v3"),
        text.replace(
            r#""schema":"keld.activation-journal/v2""#,
            r#""schema":"keld.activation-journal/v2","unknown":0"#,
        ),
        duplicate_attempt,
        text.replace("1.1.0", "1.0.0"),
        text.replace("1.1.0", "01.1.0"),
        text.replace(r#""prior_floor":"1.0.0""#, r#""prior_floor":"0.5.0""#),
        duplicate_lifecycle,
        text.replace("candidate", "other-app-candidate"),
        text.replace("rollback_target", "untrusted_rollback_target"),
        text.replace("publish-pending", "future-phase"),
        format!("{{ {text}"),
    ];
    for altered in cases {
        assert!(
            decode_activation_journal(altered.as_bytes()).is_err(),
            "accepted substituted journal: {altered}"
        );
    }
}

#[test]
fn activation_journal_encoder_refuses_floor_below_known_good_history() {
    let mut journal = activation_journal();
    journal.prior_floor = "0.5.0".to_owned();
    assert!(
        encode_activation_journal(&journal).is_err(),
        "encoded journal history cannot place the prior floor below its rollback artifacts"
    );
}

#[test]
fn activation_journal_rejects_previous_known_good_not_older_than_prior_lkg() {
    let mut journal = activation_journal();
    journal.prior_floor = "2.0.0".to_owned();
    journal.candidate.version = "3.0.0".to_owned();
    journal
        .prior_previous_known_good
        .as_mut()
        .expect("prior previous")
        .version = "2.0.0".to_owned();
    assert!(
        encode_activation_journal(&journal).is_err(),
        "historical previous slot cannot equal or outrank its prior LKG"
    );
}
