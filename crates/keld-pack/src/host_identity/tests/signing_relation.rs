//! Test-only DEV signing byte relationship. No signature or custody authority.
use super::*;

fn relation(unsigned: &[u8], signed: &[u8]) -> Result<(), PackError> {
    let before = admit_structure(&Bytes(unsigned))?;
    let after = admit_structure(&Bytes(signed))?;
    if before.certificate != (0, 0) || before.raw_end != before.len {
        return Err(invalid("unsigned overlay or certificate"));
    }
    let (start, size) = after.certificate;
    let start = u64::from(start);
    let size = u64::from(size);
    // The existing Pack layout requires aligned section raw ranges, so no
    // additional pre-certificate alignment padding is needed in this profile.
    if start != before.len
        || start % 8 != 0
        || size < 8
        || size > 1024 * 1024
        || size % 8 != 0
        || start + size != after.len
        || before.nt != after.nt
    {
        return Err(invalid("unsupported signing table geometry"));
    }
    let certificate = &signed[to_index(start)..];
    if u64::from(le32(certificate, 0)) != size
        || le16(certificate, 4) != 0x0200
        || le16(certificate, 6) != 2
    {
        return Err(invalid("unsupported signing certificate entry"));
    }
    let optional = before.nt + COFF_END as u64;
    let checksum = optional + CHECKSUM as u64;
    let security = optional + CERTIFICATE_TABLE as u64;
    for (offset, (a, b)) in unsigned.iter().zip(signed).enumerate() {
        let offset = offset as u64;
        if a != b
            && !(checksum..checksum + 4).contains(&offset)
            && !(security..security + 8).contains(&offset)
        {
            return Err(invalid("signed bytes differ from reviewed source"));
        }
    }
    Ok(())
}

#[test]
fn signing_shape_does_not_accept_substituted_payload_or_overlay() {
    let unsigned = fixture::two_sections();
    let mut signed = unsigned.clone();
    let optional = fixture::optional_of(&signed);
    let end = u32::try_from(signed.len()).unwrap();
    fixture::put_u32(&mut signed, optional + fixture::CERTIFICATE_TABLE, end);
    fixture::put_u32(&mut signed, optional + fixture::CERTIFICATE_TABLE + 4, 16);
    fixture::put_u32(&mut signed, optional + fixture::CHECKSUM, 0);
    signed.extend_from_slice(&[16, 0, 0, 0, 0, 2, 2, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
    // Synthetic table bytes prove geometry only, not a PKCS#7 signature.
    assert!(relation(&unsigned, &signed).is_ok());
    let mut substituted = signed.clone();
    substituted[0x201] ^= 1;
    assert_eq!(
        super::refusal(relation(&unsigned, &substituted)).1,
        "signed bytes differ from reviewed source"
    );
    let mut overlay = unsigned.clone();
    overlay.extend_from_slice(&[0; 8]);
    assert_eq!(
        super::refusal(relation(&overlay, &signed)).1,
        "unsigned overlay or certificate"
    );
    let mut trailing = signed.clone();
    trailing.push(0);
    assert_eq!(
        super::refusal(relation(&unsigned, &trailing)).1,
        "unsupported signing table geometry"
    );
    let mut multiple = signed;
    fixture::put_u32(&mut multiple, end as usize, 8);
    assert_eq!(
        super::refusal(relation(&unsigned, &multiple)).1,
        "unsupported signing certificate entry"
    );
}
