use super::{
    MAX_LOCAL_RECORD_BYTES, PointerKind, decode_complete, decode_floor, decode_pointer,
    decode_provenance, encode_complete, encode_floor, encode_pointer, encode_provenance,
};
use crate::tests::expected_identity;
use crate::{InstallOwner, InstallProvenance, UpdateError};

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
