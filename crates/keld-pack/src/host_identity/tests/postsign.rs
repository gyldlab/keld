//! Pure step-7 contracts. Certificate bodies are deliberately not valid signatures.

use super::{embed, fixture, refusal};
use crate::{PackError, check_host_identity_post_sign, read_host_identity_bytes};

fn candidate(unsigned: &[u8]) -> Vec<u8> {
    let mut bytes = unsigned.to_vec();
    let directory = fixture::optional_of(unsigned) + 144;
    fixture::put_u32(
        &mut bytes,
        directory,
        u32::try_from(unsigned.len()).unwrap(),
    );
    fixture::put_u32(&mut bytes, directory + 4, 16);
    // Exact independent WIN_CERTIFICATE fixture: 16 bytes, v2, PKCS signed-data type.
    bytes.extend_from_slice(&[16, 0, 0, 0, 0, 2, 2, 0, 0xA5, 1, 2, 3, 4, 5, 6, 7]);
    bytes
}

fn check(unsigned: &[u8], signed: &[u8]) -> Result<(), PackError> {
    check_host_identity_post_sign(unsigned, signed, &fixture::golden_payload())
}

#[test]
fn canonical_envelope_is_integrity_not_authentication() {
    let unsigned = embed(&fixture::two_sections());
    let mut signed = candidate(&unsigned);
    fixture::put_u32(&mut signed, fixture::optional_of(&unsigned) + 64, u32::MAX);
    check(&unsigned, &signed).unwrap();
    // Certificate contents are opaque to this checker, not authenticated or DER parsed.
    *signed.last_mut().unwrap() ^= 0xFF;
    check(&unsigned, &signed).unwrap();
    assert_eq!(
        read_host_identity_bytes(&signed).unwrap(),
        fixture::golden_payload()
    );
}

#[test]
fn rejects_every_non_exempt_pre_certificate_byte_change() {
    let unsigned = embed(&fixture::two_sections());
    let optional = fixture::optional_of(&unsigned);
    for offset in 0..unsigned.len() {
        if (optional + 64..optional + 68).contains(&offset)
            || (optional + 144..optional + 152).contains(&offset)
        {
            continue;
        }
        let mut signed = candidate(&unsigned);
        signed[offset] ^= 1;
        let (code, detail) = refusal(check(&unsigned, &signed));
        assert_eq!(code, "KELD-PACK-015", "offset {offset:#x}");
        assert!(
            detail.contains("outside the checksum"),
            "offset {offset:#x}: {detail}"
        );
    }
}

#[test]
fn baseline_must_be_unsigned_exact_and_match_expected_payload() {
    let unsigned = embed(&fixture::two_sections());
    let signed = candidate(&unsigned);
    assert_eq!(refusal(check(&signed, &signed)).0, "KELD-PACK-015");
    let mut overlay = unsigned.clone();
    overlay.push(0);
    assert_eq!(refusal(check(&overlay, &signed)).0, "KELD-PACK-015");
    let wrong = crate::ExpectedAppIdentityPayload::new(
        "com.other.app",
        "stable",
        "windows-x64",
        fixture::golden_key(),
    )
    .unwrap();
    assert_eq!(
        refusal(check_host_identity_post_sign(&unsigned, &signed, &wrong)),
        (
            "KELD-PACK-015",
            "the baseline payload differs from the expected identity"
        ),
    );
    assert_eq!(refusal(check(&[], &signed)).0, "KELD-PACK-006");
    assert_eq!(
        refusal(check(&fixture::two_sections(), &signed)).0,
        "KELD-PACK-007"
    );
}

#[test]
fn table_must_start_at_raw_end_and_end_at_eof() {
    let unsigned = embed(&fixture::two_sections());
    let directory = fixture::optional_of(&unsigned) + 144;
    let raw_end = u32::try_from(unsigned.len()).unwrap();
    for (start, size) in [
        (0, 0),
        (raw_end, 0),
        (0, 16),
        (raw_end - 8, 24),
        (raw_end + 8, 8),
        (u32::MAX, 16),
        (raw_end, u32::MAX),
        (raw_end, 15),
    ] {
        let mut signed = candidate(&unsigned);
        fixture::put_u32(&mut signed, directory, start);
        fixture::put_u32(&mut signed, directory + 4, size);
        assert_eq!(refusal(check(&unsigned, &signed)).0, "KELD-PACK-015");
    }
    let mut gap = candidate(&unsigned);
    gap.splice(unsigned.len()..unsigned.len(), [0; 8]);
    fixture::put_u32(&mut gap, directory, raw_end + 8);
    assert_eq!(refusal(check(&unsigned, &gap)).0, "KELD-PACK-015");
    let mut overlay = candidate(&unsigned);
    overlay.push(0);
    assert_eq!(refusal(check(&unsigned, &overlay)).0, "KELD-PACK-015");
    let signed = candidate(&unsigned);
    // Every truncation, including partial WIN_CERTIFICATE headers and body.
    for end in 0..signed.len() {
        assert_eq!(refusal(check(&unsigned, &signed[..end])).0, "KELD-PACK-015");
    }
}

#[test]
fn rejects_second_entry_and_noncanonical_certificate_headers() {
    let unsigned = embed(&fixture::two_sections());
    let start = unsigned.len();
    let directory = fixture::optional_of(&unsigned) + 144;
    let mut second = candidate(&unsigned);
    second.extend_from_slice(&[16, 0, 0, 0, 0, 2, 2, 0, 9, 8, 7, 6, 5, 4, 3, 2]);
    fixture::put_u32(&mut second, directory + 4, 32);
    // Regression: the existing container reader intentionally accepts this envelope.
    assert_eq!(
        read_host_identity_bytes(&second).unwrap(),
        fixture::golden_payload()
    );
    assert_eq!(
        refusal(check(&unsigned, &second)),
        (
            "KELD-PACK-015",
            "the table is not one canonical WIN_CERTIFICATE entry"
        )
    );
    for length in [0, 1, 7, 8, 9, 15, 17, 24, u32::MAX] {
        let mut signed = candidate(&unsigned);
        fixture::put_u32(&mut signed, start, length);
        assert_eq!(refusal(check(&unsigned, &signed)).0, "KELD-PACK-015");
    }
    for (field, value) in [(4, 0), (4, 0x0100), (6, 0), (6, 1)] {
        let mut signed = candidate(&unsigned);
        fixture::put_u16(&mut signed, start + field, value);
        assert_eq!(refusal(check(&unsigned, &signed)).0, "KELD-PACK-015");
    }
    let mut empty = candidate(&unsigned);
    empty.truncate(start + 8);
    fixture::put_u32(&mut empty, directory + 4, 8);
    fixture::put_u32(&mut empty, start, 8);
    assert_eq!(refusal(check(&unsigned, &empty)).0, "KELD-PACK-015");
}

#[test]
fn shifted_baseline_header_controls_exemptions() {
    let mut spec = fixture::two_section_spec();
    spec.nt = 0x80;
    let unsigned = embed(&fixture::build(&spec));
    let mut signed = candidate(&unsigned);
    fixture::put_u32(&mut signed, 0x80 + 24 + 64, 0x1234_5678);
    check(&unsigned, &signed).unwrap();
    // The default-layout checksum is not exempt on this image.
    signed[0x40 + 24 + 64] ^= 1;
    assert_eq!(refusal(check(&unsigned, &signed)).0, "KELD-PACK-015");
}

#[test]
fn post_sign_error_has_honest_guidance_and_no_source() {
    let error = PackError::PostSignIntegrity {
        detail: "changed text byte",
    };
    assert_eq!(error.code(), "KELD-PACK-015");
    assert!(error.to_string().contains("changed text byte"));
    assert!(
        error
            .to_string()
            .contains("does not establish signature trust")
    );
    assert!(std::error::Error::source(&error).is_none());
}
