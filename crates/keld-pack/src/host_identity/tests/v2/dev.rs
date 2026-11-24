//! Synthetic DEV provider-2 PE observations, never signer or admission proof.
//! Literal fixture bytes do not use the production PE writer or D-WIRE encoder.

use super::super::fixture::{
    self, Section, Spec, VIRTUAL_SIZE, build, container_section, data, header_at, put_u32, text,
    two_section_spec, two_sections,
};
use crate::expected_identity_v2::{ExpectedAppIdentityV2, V2IdentityFields, V2SigningProfile};
use crate::host_identity::v2::{
    embed_ordinary_v2, observe_local_development_v2_bytes, read_ordinary_v2_bytes,
};
use crate::{PackError, read_host_identity_bytes};

const APP_SPKI: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn expected_dev() -> ExpectedAppIdentityV2 {
    ExpectedAppIdentityV2::new(V2IdentityFields {
        app_id: "com.example.app",
        channel: "stable",
        target: "windows-x64",
        update_public_key: [0x11; 32],
        signing_profile: V2SigningProfile::LocalDevelopmentAppSpkiSha256(APP_SPKI),
        issuer_policy_sha256: [0x22; 32],
        repository_id: [0x33; 32],
        repository_policy_sha256: [0x44; 32],
        discovery_policy_sha256: [0x55; 32],
    })
    .expect("canonical synthetic DEV fields")
}

fn dev_section(virtual_address: u32, raw_pointer: u32) -> Section {
    let mut bytes = b"keld.expected-app-identity/v2\0\
        \x0fcom.example.app\
        \x06stable\
        \x0bwindows-x64"
        .to_vec();
    bytes.extend_from_slice(&[0x11; 32]);
    bytes.push(2); // Local DEV provider, not Public Trust.
    bytes.extend_from_slice(b"\x00\x40");
    bytes.extend_from_slice(APP_SPKI.as_bytes());
    bytes.extend_from_slice(&[0x22; 32]);
    bytes.extend_from_slice(&[0x33; 32]);
    bytes.extend_from_slice(&[0x44; 32]);
    bytes.extend_from_slice(&[0x55; 32]);
    bytes.extend_from_slice(&[1, 1, 1, 3]); // Direct, per-user, strict, DEV root.
    assert_eq!(bytes.len(), 296, "independent literal D-4 DEV payload");
    Section {
        name: *b".keldea2",
        virtual_size: 296,
        virtual_address,
        raw_size: 0x200,
        raw_pointer,
        characteristics: 0x4000_0040,
        content: bytes,
    }
}

fn dev_pe() -> Vec<u8> {
    build(&Spec {
        sections: vec![text(), data(), dev_section(0x3000, 0x600)],
        ..two_section_spec()
    })
}

fn expected_refusal(result: Result<ExpectedAppIdentityV2, PackError>, detail: &'static str) {
    assert!(
        matches!(
            result,
            Err(PackError::ExpectedIdentityInvalid { detail: actual }) if actual == detail
        ),
        "expected {detail} byte refusal"
    );
}

#[test]
fn dev_pe_observation_is_positive_only_for_explicit_dev_reader() {
    let image = dev_pe();
    super::valid_pe_fixture(&image);
    assert_eq!(
        observe_local_development_v2_bytes(&image).expect("DEV bytes only"),
        expected_dev()
    );
    expected_refusal(read_ordinary_v2_bytes(&image), "v2 provider");
    assert!(read_host_identity_bytes(&image).is_err());
    assert!(
        embed_ordinary_v2(&two_sections(), &expected_dev()).is_err(),
        "the original production writer must still reject DEV on readback"
    );

    let production_pe = build(&Spec {
        sections: vec![text(), data(), fixture::v2_container_section(0x3000, 0x600)],
        ..two_section_spec()
    });
    assert!(read_ordinary_v2_bytes(&production_pe).is_ok());
    expected_refusal(
        observe_local_development_v2_bytes(&production_pe),
        "v2 provider",
    );
}

#[test]
fn dev_pe_observer_refuses_mixed_provider_root_spki_padding_and_trailing_bytes() {
    let mut image = dev_pe();
    image[0x600 + 97] = 1;
    expected_refusal(observe_local_development_v2_bytes(&image), "v2 provider");

    let mut image = dev_pe();
    image[0x600 + 295] = 2;
    expected_refusal(observe_local_development_v2_bytes(&image), "v2 root policy");

    let mut image = dev_pe();
    image[0x600 + 100] = b'A';
    expected_refusal(
        observe_local_development_v2_bytes(&image),
        "v2 APP SPKI SHA256",
    );

    let mut image = dev_pe();
    image[0x600 + 296] = 1;
    assert!(matches!(
        observe_local_development_v2_bytes(&image),
        Err(PackError::IdentityContainerInvalid {
            detail: "v2 padding is nonzero"
        })
    ));

    let mut image = dev_pe();
    let virtual_size = header_at(&image, 2) + VIRTUAL_SIZE;
    put_u32(&mut image, virtual_size, 297); // Include one zero padding byte as payload.
    expected_refusal(
        observe_local_development_v2_bytes(&image),
        "v2 trailing bytes",
    );
}

#[test]
fn dev_pe_observer_refuses_legacy_and_recovery_role_sections() {
    let mixed = build(&Spec {
        sections: vec![
            text(),
            data(),
            container_section(*b".keldeai", 0x3000, 0x600),
            dev_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    assert!(matches!(
        observe_local_development_v2_bytes(&mixed),
        Err(PackError::IdentityContainerDuplicate)
    ));

    let recovery = build(&Spec {
        sections: vec![
            text(),
            data(),
            container_section(*b".keldrec", 0x3000, 0x600),
            dev_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    assert!(matches!(
        observe_local_development_v2_bytes(&recovery),
        Err(PackError::IdentityContainerInvalid {
            detail: "a RecoveryImage cannot be read as an ordinary Host"
        })
    ));
}

#[cfg(windows)]
#[test]
fn dev_observer_reads_original_borrowed_file_without_path_reopen() {
    use crate::host_identity::v2::{observe_local_development_v2_handle, read_ordinary_v2_handle};
    use std::io::{Seek as _, SeekFrom, Write as _};

    let mut file = tempfile::tempfile().expect("isolated native temporary file");
    file.write_all(&dev_pe()).expect("write synthetic DEV PE");
    let original_cursor = file
        .seek(SeekFrom::Start(13))
        .expect("cursor deliberately not at EOF");
    assert_eq!(
        observe_local_development_v2_handle(&file).expect("read same retained file"),
        expected_dev()
    );
    // Windows FileExt::seek_read advances the shared cursor to the end of its
    // last read. The parser's success must NOT depend on or preserve the cursor.
    assert_ne!(
        file.stream_position()
            .expect("cursor after positioned read"),
        original_cursor,
        "the non-EOF probe must expose Windows cursor movement"
    );
    file.seek(SeekFrom::Start(37))
        .expect("change borrowed cursor before a second independent read");
    expected_refusal(read_ordinary_v2_handle(&file), "v2 provider");
    file.seek(SeekFrom::Start(61))
        .expect("change cursor before repeating DEV observation");
    assert_eq!(
        observe_local_development_v2_handle(&file).expect("DEV read independent of cursor"),
        expected_dev(),
        "the same original file bytes must parse independently of cursor position"
    );
}
