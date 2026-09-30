use super::{
    MAX_LOCAL_RECORD_BYTES, PointerKind, decode_activation_journal, decode_complete, decode_floor,
    decode_pointer, decode_provenance, encode_activation_journal, encode_complete, encode_floor,
    encode_pointer, encode_provenance, lifecycle_installation_id,
};
use crate::records::{ActivationJournal, ActivationPhase};
use crate::tests::expected_identity;
use crate::{DirectInstallMode, InstallOwner, InstallProvenance, UpdateError};

const VOLUME: &str = r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\";

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
    assert_eq!(
        decode_pointer(PointerKind::Current, &current).expect("current bytes"),
        artifact
    );
    assert_eq!(
        decode_pointer(PointerKind::LastKnownGood, &good).expect("LKG bytes"),
        artifact
    );
    assert!(matches!(
        decode_pointer(PointerKind::Current, &good),
        Err(UpdateError::LocalRecordInvalid { .. })
    ));
    assert!(matches!(
        decode_pointer(PointerKind::LastKnownGood, &current),
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
        phase: ActivationPhase::PublishPending,
    }
}

#[test]
fn activation_journal_uses_exact_canonical_bytes_and_roundtrips_all_context() {
    let journal = activation_journal();
    let bytes = encode_activation_journal(&journal).expect("activation journal");
    let text = String::from_utf8(bytes.clone()).expect("UTF-8");
    assert!(text.starts_with(r#"{"schema":"keld.activation-journal/v1","attempt_id":"1111"#));
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
        text.replace("keld.activation-journal/v1", "keld.activation-journal/v2"),
        text.replace(
            r#""schema":"keld.activation-journal/v1""#,
            r#""schema":"keld.activation-journal/v1","unknown":0"#,
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
