//! Negative source-only D-RECOVERY PE-shape tests. No native signer, complete
//! canonical directive, installed RecoveryPolicy, or role authority exists.

use super::super::fixture::{
    self, CERTIFICATE_TABLE, RAW_SIZE, SECTION_CHARACTERISTICS, VIRTUAL_SIZE, build, data,
    header_at, optional_of, put_u32, text, two_section_spec,
};
use super::dev::{dev_section, expected_dev};
use crate::host_identity::v2::{
    observe_local_development_v2_bytes, recovery::observe_local_development_recovery_bytes,
};
use crate::read_host_identity_bytes;

// Intentionally INCOMPLETE directive for testing PE-shape observation only.
// Admitting these raw bytes MUST NOT imply canonical D-62 JSON or recovery.
const UNQUALIFIED_JSON: &[u8] = br#"{"schema":"keld.windows.recovery-directive/v1"}"#;

fn rec_section(virtual_address: u32, raw_pointer: u32) -> fixture::Section {
    fixture::Section {
        name: *b".keldrec",
        virtual_size: u32::try_from(UNQUALIFIED_JSON.len())
            .expect("literal test directive fits its PE field"),
        virtual_address,
        raw_size: 0x200,
        raw_pointer,
        characteristics: 0x4000_0040,
        content: UNQUALIFIED_JSON.to_vec(),
    }
}

fn recovery_pe() -> Vec<u8> {
    build(&fixture::Spec {
        sections: vec![
            text(),
            data(),
            rec_section(0x3000, 0x600),
            dev_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    })
}

// The large boundary probes MUST remain fully valid at the shared PE Layout
// boundary. Otherwise a generic VA/raw-size refusal could falsely appear to
// test the private RecoveryDirective length limit.
fn large_recovery_pe(length: u32) -> Vec<u8> {
    let raw_size = u32::try_from(fixture::round_up(
        u64::from(length),
        fixture::FILE_ALIGNMENT,
    ))
    .expect("test raw length fits");
    let next_virtual = u32::try_from(fixture::round_up(
        0x3000 + u64::from(length),
        fixture::SECTION_ALIGNMENT,
    ))
    .expect("test virtual address fits");
    let next_raw = 0x600_u32
        .checked_add(raw_size)
        .expect("test raw address fits");
    build(&fixture::Spec {
        sections: vec![
            text(),
            data(),
            fixture::Section {
                virtual_size: length,
                raw_size,
                ..rec_section(0x3000, 0x600)
            },
            dev_section(next_virtual, next_raw),
        ],
        ..two_section_spec()
    })
}

#[test]
fn one_original_pe_can_be_observed_without_granting_any_recovery_authority() {
    let image = recovery_pe();
    super::valid_pe_fixture(&image);
    let observed = observe_local_development_recovery_bytes(&image)
        .expect("private unqualified section observation");
    assert_eq!(observed.expected_destination, expected_dev());
    assert_eq!(observed.directive_bytes, UNQUALIFIED_JSON);
    assert!(
        observe_local_development_v2_bytes(&image).is_err(),
        "the ordinary APP byte observer must reject a RecoveryImage"
    );
    assert!(
        read_host_identity_bytes(&image).is_err(),
        "the original v1 Host must reject both recovery and D4"
    );
}

#[test]
fn recovery_shape_refuses_missing_duplicate_swapped_and_nonadjacent_sections() {
    let missing = build(&fixture::Spec {
        sections: vec![text(), data(), dev_section(0x3000, 0x600)],
        ..two_section_spec()
    });
    assert!(observe_local_development_recovery_bytes(&missing).is_err());

    let duplicate = build(&fixture::Spec {
        sections: vec![
            text(),
            rec_section(0x2000, 0x400),
            rec_section(0x3000, 0x600),
            dev_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    super::valid_pe_fixture(&duplicate);
    assert!(observe_local_development_recovery_bytes(&duplicate).is_err());

    let swapped = build(&fixture::Spec {
        sections: vec![
            text(),
            data(),
            dev_section(0x3000, 0x600),
            rec_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    super::valid_pe_fixture(&swapped);
    assert!(observe_local_development_recovery_bytes(&swapped).is_err());

    let nonadjacent_data = fixture::Section {
        virtual_address: 0x3000,
        raw_pointer: 0x600,
        ..data()
    };
    let nonadjacent = build(&fixture::Spec {
        sections: vec![
            text(),
            rec_section(0x2000, 0x400),
            nonadjacent_data,
            dev_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    super::valid_pe_fixture(&nonadjacent);
    assert!(observe_local_development_recovery_bytes(&nonadjacent).is_err());
}

#[test]
fn recovery_shape_refuses_modified_header_padding_identity_and_legacy() {
    let mut image = recovery_pe();
    image[0x600 + UNQUALIFIED_JSON.len()] = 0x61;
    assert!(observe_local_development_recovery_bytes(&image).is_err());

    let mut image = recovery_pe();
    let offset = header_at(&image, 2) + SECTION_CHARACTERISTICS;
    put_u32(&mut image, offset, 0xC000_0040);
    assert!(observe_local_development_recovery_bytes(&image).is_err());

    let mut image = recovery_pe();
    let offset = header_at(&image, 2) + RAW_SIZE;
    put_u32(&mut image, offset, 0);
    assert!(observe_local_development_recovery_bytes(&image).is_err());

    let mut image = recovery_pe();
    let offset = header_at(&image, 2) + VIRTUAL_SIZE;
    put_u32(&mut image, offset, 0);
    assert!(observe_local_development_recovery_bytes(&image).is_err());

    let mut image = recovery_pe();
    image[0x800 + 97] = 1; // Production provider, not DEV.
    assert!(observe_local_development_recovery_bytes(&image).is_err());

    let legacy = build(&fixture::Spec {
        sections: vec![
            fixture::container_section(*b".keldeai", 0x1000, 0x200),
            data(),
            rec_section(0x3000, 0x600),
            dev_section(0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    super::valid_pe_fixture(&legacy);
    assert!(observe_local_development_recovery_bytes(&legacy).is_err());
}

#[test]
fn recovery_directive_boundary_refuses_65537_only_after_valid_pe_structure() {
    let admitted = large_recovery_pe(65_536);
    super::valid_pe_fixture(&admitted);
    assert_eq!(
        observe_local_development_recovery_bytes(&admitted)
            .expect("65536 is a valid unqualified byte length")
            .directive_bytes
            .len(),
        65_536
    );

    let overbound = large_recovery_pe(65_537);
    super::valid_pe_fixture(&overbound);
    assert!(matches!(
        observe_local_development_recovery_bytes(&overbound),
        Err(crate::PackError::IdentityContainerInvalid {
            detail: "recovery section size is outside directive bounds"
        })
    ));
}

#[test]
fn recovery_shape_refuses_wrong_raw_order_and_certificate_table_overlap() {
    let wrong_raw_order = build(&fixture::Spec {
        sections: vec![
            text(),
            data(),
            rec_section(0x3000, 0x800),
            dev_section(0x4000, 0x600),
        ],
        ..two_section_spec()
    });
    assert!(
        observe_local_development_recovery_bytes(&wrong_raw_order).is_err(),
        "earlier Recovery section cannot be physically after final D4"
    );

    let mut image = recovery_pe();
    let certificate_directory = optional_of(&image) + CERTIFICATE_TABLE;
    put_u32(&mut image, certificate_directory, 0x600);
    put_u32(&mut image, certificate_directory + 4, 8);
    assert!(
        observe_local_development_recovery_bytes(&image).is_err(),
        "certificate table cannot overlap either role section"
    );
}

#[cfg(windows)]
#[test]
fn recovery_observes_both_sections_from_same_borrowed_handle_and_any_cursor() {
    use crate::host_identity::v2::recovery::observe_local_development_recovery_handle;
    use std::io::{Seek as _, SeekFrom, Write as _};

    let mut file = tempfile::tempfile().expect("native isolated anonymous file");
    file.write_all(&recovery_pe())
        .expect("write unsigned recovery fixture");
    let initial = file.seek(SeekFrom::Start(13)).expect("set non-EOF cursor");
    let first =
        observe_local_development_recovery_handle(&file).expect("same original borrowed handle");
    assert_eq!(first.expected_destination, expected_dev());
    assert_eq!(first.directive_bytes, UNQUALIFIED_JSON);
    assert_ne!(
        file.stream_position()
            .expect("cursor after Windows positioned reads"),
        initial,
        "Windows seek_read changes the shared cursor; no preservation claim"
    );
    file.seek(SeekFrom::Start(37))
        .expect("set independent cursor");
    let second = observe_local_development_recovery_handle(&file)
        .expect("explicit section offsets independent of shared cursor");
    assert_eq!(second.directive_bytes, first.directive_bytes);
    assert_eq!(second.expected_destination, first.expected_destination);
}
