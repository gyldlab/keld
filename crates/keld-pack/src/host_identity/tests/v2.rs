//! Private D-4 .keldea2 PE section and recovery-role negative controls.
//! These are synthetic unsigned byte fixtures; none is a signed Host.

use super::fixture::{
    self, CERTIFICATE_TABLE, CHECKSUM, RAW_SIZE, SECTION_CHARACTERISTICS, VIRTUAL_SIZE, build,
    container_section, data, get_u32, header_at, optional_of, put_u32, text, two_section_spec,
    two_sections,
};
use crate::expected_identity_v2::{ExpectedAppIdentityV2, V2IdentityFields, V2SigningProfile};
use crate::host_identity::v2::{embed_ordinary_v2, read_ordinary_v2_bytes};
use crate::{ExpectedAppIdentityPayload, PackError, embed_host_identity, read_host_identity_bytes};

mod dev;

fn payload() -> ExpectedAppIdentityV2 {
    ExpectedAppIdentityV2::new(V2IdentityFields {
        app_id: "com.example.app",
        channel: "stable",
        target: "windows-x64",
        update_public_key: [0x11; 32],
        signing_profile: V2SigningProfile::PublicTrustOid("1.2.840.113556.1.8000.2554"),
        issuer_policy_sha256: [0x22; 32],
        repository_id: [0x33; 32],
        repository_policy_sha256: [0x44; 32],
        discovery_policy_sha256: [0x55; 32],
    })
    .expect("independent synthetic v2 bytes")
}

fn canonical() -> Vec<u8> {
    embed_ordinary_v2(&two_sections(), &payload()).expect("private v2 writer")
}

fn valid_pe_fixture(image: &[u8]) {
    use crate::host_identity::{Bytes, admit_structure};
    admit_structure(&Bytes(image)).expect("the test must reach the v2 identity/role gate");
}

fn refused(result: Result<ExpectedAppIdentityV2, PackError>) {
    if let Ok(value) = result {
        panic!("unexpected v2 identity admission: {value:?}");
    }
}

fn container_detail(result: Result<ExpectedAppIdentityV2, PackError>) -> &'static str {
    match result {
        Err(PackError::IdentityContainerInvalid { detail }) => detail,
        other => panic!("expected a v2 section-form refusal, got {other:?}"),
    }
}

#[test]
fn ordinary_v2_writer_has_one_final_section_and_exact_retained_bytes() {
    let input = two_sections();
    let image = canonical();
    assert_eq!(image.len(), input.len() + 0x200);
    assert_eq!(fixture::section_count(&image), 3);
    assert_eq!(
        &image[header_at(&image, 2)..header_at(&image, 2) + 8],
        b".keldea2"
    );
    assert_eq!(get_u32(&image, header_at(&image, 2) + VIRTUAL_SIZE), 258);
    assert_eq!(read_ordinary_v2_bytes(&image).expect("read"), payload());
    assert_eq!(
        &image[..0x40],
        &input[..0x40],
        "original DOS bytes preserved"
    );
    // The old v1 reader must not adopt the new v2-only section or domain.
    assert!(read_host_identity_bytes(&image).is_err());
}

#[test]
fn old_v1_payload_and_writer_remain_unchanged_when_private_v2_feature_is_on() {
    let old =
        ExpectedAppIdentityPayload::new("com.example.app", "stable", "windows-x64", [0x11; 32])
            .expect("existing v1 contract");
    let old_image = embed_host_identity(&two_sections(), &old).expect("unchanged v1 writer");
    assert_eq!(
        read_host_identity_bytes(&old_image).expect("unchanged v1 reader"),
        old
    );
    assert_eq!(
        &old_image[header_at(&old_image, 2)..header_at(&old_image, 2) + 8],
        b".keldeai"
    );
    refused(read_ordinary_v2_bytes(&old_image));
}

#[test]
fn mixed_v1_v2_and_duplicate_v2_sections_refuse() {
    let standalone_v2 = build(&fixture::Spec {
        sections: vec![text(), data(), fixture::v2_container_section(0x3000, 0x600)],
        ..two_section_spec()
    });
    assert_eq!(
        read_ordinary_v2_bytes(&standalone_v2).expect("independent valid v2 section"),
        payload()
    );
    let mixed = build(&fixture::Spec {
        sections: vec![
            text(),
            data(),
            container_section(*b".keldeai", 0x3000, 0x600),
            fixture::v2_container_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    valid_pe_fixture(&mixed);
    assert!(matches!(
        read_ordinary_v2_bytes(&mixed),
        Err(PackError::IdentityContainerDuplicate)
    ));
    let reversed = build(&fixture::Spec {
        sections: vec![
            text(),
            data(),
            fixture::v2_container_section(0x3000, 0x600),
            container_section(*b".keldeai", 0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    valid_pe_fixture(&reversed);
    assert!(matches!(
        read_ordinary_v2_bytes(&reversed),
        Err(PackError::IdentityContainerDuplicate)
    ));
    assert!(
        matches!(
            read_host_identity_bytes(&reversed),
            Err(PackError::IdentityContainerDuplicate)
        ),
        "v1 must refuse mixed sections under D-WIRE feature"
    );
    let duplicate = build(&fixture::Spec {
        sections: vec![
            text(),
            data(),
            fixture::v2_container_section(0x3000, 0x600),
            fixture::v2_container_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    valid_pe_fixture(&duplicate);
    assert!(matches!(
        read_ordinary_v2_bytes(&duplicate),
        Err(PackError::IdentityContainerDuplicate)
    ));
    assert!(embed_ordinary_v2(&reversed, &payload()).is_err());
}

#[test]
fn missing_only_and_nonfinal_v2_sections_fail_at_the_correct_boundary() {
    assert_eq!(
        container_detail(read_ordinary_v2_bytes(&two_sections())),
        "the v2 identity section is absent"
    );
    let only = build(&fixture::Spec {
        sections: vec![fixture::v2_container_section(0x1000, 0x200)],
        ..two_section_spec()
    });
    valid_pe_fixture(&only);
    assert_eq!(
        container_detail(read_ordinary_v2_bytes(&only)),
        "the v2 identity must be the final PE section"
    );
    let not_final = build(&fixture::Spec {
        sections: vec![
            text(),
            fixture::v2_container_section(0x2000, 0x400),
            fixture::Section {
                virtual_address: 0x3000,
                raw_pointer: 0x600,
                ..data()
            },
        ],
        ..two_section_spec()
    });
    valid_pe_fixture(&not_final);
    assert_eq!(
        container_detail(read_ordinary_v2_bytes(&not_final)),
        "the v2 identity must be the final PE section"
    );
}

#[test]
fn dedicated_recovery_section_cannot_be_reinterpreted_as_ordinary_host() {
    // The RecoveryImage directive is independently specified and does not
    // authorize ordinary catalogue or application launch.
    let recovery = build(&fixture::Spec {
        sections: vec![
            text(),
            data(),
            container_section(*b".keldrec", 0x3000, 0x600),
            fixture::v2_container_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    valid_pe_fixture(&recovery);
    assert!(matches!(
        read_ordinary_v2_bytes(&recovery),
        Err(PackError::IdentityContainerInvalid {
            detail: "a RecoveryImage cannot be read as an ordinary Host"
        })
    ));
    assert!(read_host_identity_bytes(&recovery).is_err());
    assert!(embed_ordinary_v2(&recovery, &payload()).is_err());
}

#[test]
fn invalid_v2_header_fields_and_padding_never_decode() {
    let mut altered = canonical();
    let at = header_at(&altered, 2) + SECTION_CHARACTERISTICS;
    put_u32(&mut altered, at, 0xC000_0040);
    assert_eq!(
        container_detail(read_ordinary_v2_bytes(&altered)),
        "the v2 section attributes are not read-only initialized data"
    );

    let mut altered = canonical();
    let at = header_at(&altered, 2) + RAW_SIZE;
    put_u32(&mut altered, at, 0);
    refused(read_ordinary_v2_bytes(&altered));

    let mut altered = canonical();
    altered[0x600 + 258] = 1; // FileAlignment padding, not payload.
    assert_eq!(
        container_detail(read_ordinary_v2_bytes(&altered)),
        "v2 padding is nonzero"
    );

    let mut altered = canonical();
    altered[0x600] ^= 1; // The exact v2 payload domain.
    refused(read_ordinary_v2_bytes(&altered));

    let mut altered = canonical();
    let at = header_at(&altered, 2) + VIRTUAL_SIZE;
    put_u32(&mut altered, at, 791);
    refused(read_ordinary_v2_bytes(&altered));
}

#[test]
fn signed_pe_shape_is_readable_but_never_claimed_to_be_authenticated() {
    // Synthetic certificate-table bytes are neither an embedded signature
    // nor a WinTrust receipt. This checks only the PE reader range geometry.
    let mut image = canonical();
    let raw_end = u32::try_from(image.len()).expect("bounded test image");
    image.extend_from_slice(&[0x30; 16]);
    let cert = optional_of(&image) + CERTIFICATE_TABLE;
    put_u32(&mut image, cert, raw_end);
    put_u32(&mut image, cert + 4, 16);
    let checksum = optional_of(&image) + CHECKSUM;
    put_u32(&mut image, checksum, 0x1234);
    assert_eq!(
        read_ordinary_v2_bytes(&image).expect("container bytes only"),
        payload()
    );
    assert!(embed_ordinary_v2(&image, &payload()).is_err());
}

#[test]
fn half_present_certificate_table_directory_refuses_before_payload_decoding() {
    // A nonzero certificate offset with zero size is not an unsigned image
    // (both fields zero) or a complete signed-image directory (both nonzero).
    // The offset-only case at EOF previously passed the ordinary v2 reader.
    let mut offset_only = canonical();
    let certificate = optional_of(&offset_only) + CERTIFICATE_TABLE;
    let image_end = u32::try_from(offset_only.len()).expect("bounded synthetic PE");
    put_u32(&mut offset_only, certificate, image_end);
    valid_pe_fixture(&offset_only);
    assert_eq!(
        container_detail(read_ordinary_v2_bytes(&offset_only)),
        "v2 certificate directory has only one of offset and size"
    );

    let mut size_only = canonical();
    let certificate = optional_of(&size_only) + CERTIFICATE_TABLE;
    put_u32(&mut size_only, certificate + 4, 16);
    valid_pe_fixture(&size_only);
    assert_eq!(
        container_detail(read_ordinary_v2_bytes(&size_only)),
        "v2 certificate directory has only one of offset and size"
    );
}

#[test]
fn nonpristine_image_and_embedding_second_time_are_denied() {
    let image = canonical();
    assert!(embed_ordinary_v2(&image, &payload()).is_err());
    let mut overlay = two_sections();
    overlay.push(0x7f);
    assert!(embed_ordinary_v2(&overlay, &payload()).is_err());
}

#[cfg(windows)]
#[test]
fn native_positioned_read_uses_the_same_held_file_not_a_path_reopen() {
    use crate::host_identity::v2::read_ordinary_v2_handle;
    use std::io::{Seek as _, SeekFrom, Write as _};
    let mut file = tempfile::tempfile().expect("native private temporary file");
    file.write_all(&canonical())
        .expect("write synthetic PE bytes");
    file.seek(SeekFrom::End(0)).expect("place cursor after PE");
    assert_eq!(
        read_ordinary_v2_handle(&file).expect("positioned read"),
        payload()
    );
    // A successful PE parse has still not verified a signer or a production Host.
}
