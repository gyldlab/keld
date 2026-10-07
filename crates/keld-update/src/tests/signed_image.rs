//! `ExpectedAppIdentity::from_signed_image` over real open image handles (KEL-254 A3 §4,
//! task T3 Part B; KEL-19 container spec "Reader contract" and AC9).
//!
//! The host image is this test executable, a real linked PE32+ AMD64 image; keld-pack's
//! writer embeds the canonical payload, and each image is read through an anonymous
//! temporary file whose cursor sits at end of file, so the reader can neither reopen a
//! path nor rely on the cursor. Header offsets used to damage an image are restated from
//! the PE Format layout, independent of the reader under test.

use std::io::{Seek as _, SeekFrom, Write as _};

use super::*;

/// `e_lfanew`: file offset of the PE signature.
const E_LFANEW: usize = 0x3C;
/// Offsets relative to the PE signature.
const NUMBER_OF_SECTIONS: usize = 6;
const SIZE_OF_OPTIONAL_HEADER: usize = 20;
const OPTIONAL_HEADER: usize = 24;
const SECTION_HEADER_BYTES: usize = 40;
/// Offset of `VirtualSize` inside one section header.
const VIRTUAL_SIZE: usize = 8;

fn release_key() -> [u8; 32] {
    signing_key().verifying_key().to_bytes()
}

fn payload(app_id: &str, channel: &str, key: [u8; 32]) -> keld_pack::ExpectedAppIdentityPayload {
    keld_pack::ExpectedAppIdentityPayload::new(app_id, channel, TARGET, key)
        .expect("fixture fields are within keld-pack bounds")
}

/// This unsigned test executable, which keld-pack admits as a prebuilt host image.
fn unsigned_host() -> Vec<u8> {
    std::fs::read(std::env::current_exe().expect("test executable path"))
        .expect("read the test executable image")
}

fn embedded(payload: &keld_pack::ExpectedAppIdentityPayload) -> Vec<u8> {
    keld_pack::embed_host_identity(&unsigned_host(), payload)
        .expect("the linked test executable is an admissible host image")
}

/// An anonymous file (no path exists to reopen) holding `image`, cursor at end of file.
fn handle(image: &[u8]) -> std::fs::File {
    let mut file = tempfile::tempfile().expect("anonymous image file");
    file.write_all(image).expect("write image bytes");
    file.seek(SeekFrom::End(0)).expect("cursor to end of file");
    file
}

fn u16_at(image: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes(
        image[at..at + 2].try_into().expect("two bytes"),
    ))
}

fn u32_at(image: &[u8], at: usize) -> usize {
    usize::try_from(u32::from_le_bytes(
        image[at..at + 4].try_into().expect("four bytes"),
    ))
    .expect("u32 fits usize")
}

/// File offset of section header `index`.
fn section_header(image: &[u8], index: usize) -> usize {
    let nt = u32_at(image, E_LFANEW);
    nt + OPTIONAL_HEADER
        + u16_at(image, nt + SIZE_OF_OPTIONAL_HEADER)
        + index * SECTION_HEADER_BYTES
}

fn section_count(image: &[u8]) -> usize {
    u16_at(image, u32_at(image, E_LFANEW) + NUMBER_OF_SECTIONS)
}

fn pack_code_of(error: &UpdateError) -> &'static str {
    assert_code(error, "KELD-UPDATE-019");
    match error {
        UpdateError::ExpectedIdentityContainer { pack_code, detail } => {
            assert!(
                detail.starts_with(pack_code),
                "detail carries the keld-pack refusal: {detail}"
            );
            pack_code
        }
        other => panic!("expected KELD-UPDATE-019, got {other:?}"),
    }
}

fn invalid_detail(error: UpdateError) -> String {
    assert_code(&error, "KELD-UPDATE-017");
    match error {
        UpdateError::ExpectedIdentityInvalid { detail } => detail,
        other => panic!("expected KELD-UPDATE-017, got {other:?}"),
    }
}

#[test]
fn signed_image_payload_matches_its_decoded_expectation() {
    let canonical = payload(APP_ID, "beta", release_key());
    let from_image = ExpectedAppIdentity::from_signed_image(&handle(&embedded(&canonical)))
        .expect("one canonical container with a valid release key");
    let decoded = ExpectedAppIdentity::decode(&canonical.encode()).expect("payload decodes");
    assert_eq!(from_image.app_id, APP_ID);
    assert_eq!(from_image.channel, Channel::Beta);
    assert_eq!(from_image.target, TARGET);
    assert_eq!(
        from_image.signing_key_id,
        SigningKeyId::from_public_key(&release_key())
    );
    assert_eq!(from_image.app_id, decoded.app_id);
    assert_eq!(from_image.channel, decoded.channel);
    assert_eq!(from_image.target, decoded.target);
    assert_eq!(from_image.signing_key_id, decoded.signing_key_id);
}

#[test]
fn signed_image_applies_the_decode_channel_and_key_rules() {
    let error = ExpectedAppIdentity::from_signed_image(&handle(&embedded(&payload(
        APP_ID,
        "nightly",
        release_key(),
    ))))
    .expect_err("a canonical container with an unsupported channel is no expectation");
    assert_eq!(invalid_detail(error), "unsupported channel");

    let mut weak = [0_u8; 32];
    weak[0] = 1;
    let error = ExpectedAppIdentity::from_signed_image(&handle(&embedded(&payload(
        APP_ID, "stable", weak,
    ))))
    .expect_err("a weak key cannot anchor update trust");
    assert_eq!(invalid_detail(error), "Ed25519 public key is weak");
}

#[test]
fn signed_image_without_a_container_is_keld_update_019() {
    let error = ExpectedAppIdentity::from_signed_image(&handle(&unsigned_host()))
        .expect_err("a host without the container carries no expectation");
    assert_eq!(pack_code_of(&error), "KELD-PACK-007");
}

/// Renames `image`'s first section header to `name`. In an image that already carries
/// the container, renaming it to `.keldeai` makes two containers.
pub(crate) fn rename_first_section(image: &mut [u8], name: [u8; 8]) {
    let first = section_header(image, 0);
    image[first..first + 8].copy_from_slice(&name);
}

#[test]
fn signed_image_with_two_containers_is_keld_update_019() {
    let mut image = embedded(&payload(APP_ID, "stable", release_key()));
    // Rename the first section: the image now carries two sections named `.keldeai`.
    rename_first_section(&mut image, *b".keldeai");
    let error = ExpectedAppIdentity::from_signed_image(&handle(&image))
        .expect_err("a duplicated container is refused");
    assert_eq!(pack_code_of(&error), "KELD-PACK-008");
}

#[test]
fn signed_image_with_a_noncanonical_container_is_keld_update_019() {
    let canonical = payload(APP_ID, "stable", release_key());
    let host_length = unsigned_host().len();
    let mut image = embedded(&canonical);
    // The container's raw data starts at the unsigned image's end; the first byte after
    // the payload is zero padding up to FileAlignment.
    let padding = host_length + canonical.encode().len();
    assert!(padding < image.len(), "the payload is followed by padding");
    image[padding] = 1;
    let error = ExpectedAppIdentity::from_signed_image(&handle(&image))
        .expect_err("non-zero container padding is refused");
    assert_eq!(pack_code_of(&error), "KELD-PACK-011");
}

#[test]
fn signed_image_with_a_truncated_payload_is_keld_update_017() {
    // A key whose last byte is zero: dropping it from the payload leaves only zero
    // padding, so the container stays canonical while its payload is truncated.
    let mut key = release_key();
    key[31] = 0;
    let truncated = payload(APP_ID, "stable", key);
    let mut image = embedded(&truncated);
    let container = section_header(&image, section_count(&image) - 1);
    assert_eq!(&image[container..container + 8], b".keldeai");
    let length = u32_at(&image, container + VIRTUAL_SIZE);
    assert_eq!(length, truncated.encode().len());
    let shorter = u32::try_from(length - 1).expect("payload length fits u32");
    image[container + VIRTUAL_SIZE..container + VIRTUAL_SIZE + 4]
        .copy_from_slice(&shorter.to_le_bytes());
    let detail = invalid_detail(
        ExpectedAppIdentity::from_signed_image(&handle(&image))
            .expect_err("a truncated payload in a canonical container is refused"),
    );
    assert!(detail.starts_with("payload "), "{detail}");
}

#[test]
fn every_keld_pack_reader_refusal_keeps_its_code() {
    let io_error = || std::io::Error::other("injected read failure");
    for (error, pack_code) in [
        (keld_pack::PackError::UnsupportedHost, "KELD-PACK-001"),
        (
            keld_pack::PackError::InvalidMetadata { detail: "fixture" },
            "KELD-PACK-002",
        ),
        (
            keld_pack::PackError::SourceSizeMismatch {
                name: "fixture".to_owned(),
                expected: 1,
                observed: 0,
            },
            "KELD-PACK-003",
        ),
        (
            keld_pack::PackError::Processing {
                stage: "fixture",
                source: io_error(),
            },
            "KELD-PACK-004",
        ),
        (
            keld_pack::PackError::HostImageInvalid { detail: "fixture" },
            "KELD-PACK-006",
        ),
        (
            keld_pack::PackError::IdentityContainerMissing,
            "KELD-PACK-007",
        ),
        (
            keld_pack::PackError::IdentityContainerDuplicate,
            "KELD-PACK-008",
        ),
        (
            keld_pack::PackError::HostImageNotPristine { detail: "fixture" },
            "KELD-PACK-009",
        ),
        (
            keld_pack::PackError::HostImageNoRoom { detail: "fixture" },
            "KELD-PACK-010",
        ),
        (
            keld_pack::PackError::IdentityContainerInvalid { detail: "fixture" },
            "KELD-PACK-011",
        ),
        (
            keld_pack::PackError::IdentityContainerRead { source: io_error() },
            "KELD-PACK-012",
        ),
    ] {
        let rendered = error.to_string();
        let refusal = crate::provenance::container_refusal(&error);
        assert_eq!(pack_code_of(&refusal), pack_code);
        assert_eq!(
            refusal,
            UpdateError::ExpectedIdentityContainer {
                pack_code,
                detail: rendered,
            }
        );
    }
    let refusal =
        crate::provenance::container_refusal(&keld_pack::PackError::ExpectedIdentityInvalid {
            detail: "public key length or trailing bytes",
        });
    assert_eq!(
        invalid_detail(refusal),
        "payload public key length or trailing bytes"
    );
}

#[test]
fn container_error_has_stable_code_and_reinstall_guidance() {
    let error = UpdateError::ExpectedIdentityContainer {
        pack_code: "KELD-PACK-007",
        detail: "KELD-PACK-007: host image carries no container".to_owned(),
    };
    assert_eq!(error.code(), "KELD-UPDATE-019");
    let text = error.to_string();
    assert!(text.starts_with("KELD-UPDATE-019: "), "{text}");
    assert!(
        text.contains("(KELD-PACK-007: host image carries no container)"),
        "{text}"
    );
    assert!(text.contains("Reinstall the signed package"), "{text}");
    assert!(text.contains("`keld build`"), "{text}");
    assert!(text.contains("exactly one valid container"), "{text}");
    assert!(
        text.contains("rebuild `keld-host.exe` or `keld-updater-helper.exe`"),
        "{text}"
    );
    let guidance = text.replace("(KELD-PACK-007: host image carries no container)", "");
    assert!(
        !guidance.replace("keld-host.exe", "").contains("host"),
        "{text}"
    );
}
