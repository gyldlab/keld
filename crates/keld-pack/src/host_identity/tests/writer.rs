//! Writer contract: AC1 round trip and byte-diff oracle, AC3 exactly once, AC4 embed
//! before signing, AC5 room, AC7 determinism and golden digest (container spec §3, §7).

use super::fixture::{
    BOUND_IMPORT, CERTIFICATE_TABLE, CHECKSUM, NT, NUMBER_OF_SECTIONS, OPTIONAL,
    SECTION_HEADER_BYTES, SIZE_OF_IMAGE, SIZE_OF_INITIALIZED_DATA, Section, Spec, bss, build,
    container_section, data, get_u16, get_u32, golden_payload, header_at, nt_of, optional_of,
    put_u32, round_up, section_count, text, two_section_spec, two_sections, with_uninitialized,
};
use super::{embed, refusal};
use crate::{embed_host_identity, read_host_identity_bytes};
use std::ops::Range;

/// The container header for the golden payload (97 bytes) appended to the two-section
/// fixture, written by hand from the container format v1 table.
const TWO_SECTION_CONTAINER_HEADER: [u8; 40] = [
    0x2E, 0x6B, 0x65, 0x6C, 0x64, 0x65, 0x61, 0x69, // ".keldeai", no terminating NUL
    0x61, 0x00, 0x00, 0x00, // VirtualSize: L = 97
    0x00, 0x30, 0x00, 0x00, // VirtualAddress: the input SizeOfImage 0x3000
    0x00, 0x02, 0x00, 0x00, // SizeOfRawData: L rounded up to FileAlignment 0x200
    0x00, 0x06, 0x00, 0x00, // PointerToRawData: the input length E = 0x600
    0x00, 0x00, 0x00, 0x00, // PointerToRelocations
    0x00, 0x00, 0x00, 0x00, // PointerToLinenumbers
    0x00, 0x00, 0x00, 0x00, // NumberOfRelocations, NumberOfLinenumbers
    0x40, 0x00, 0x00, 0x40, // Characteristics: INITIALIZED_DATA | MEM_READ
];

/// The six ranges the writer contract lets change, located from the input alone.
fn changed_ranges(input: &[u8], output_len: usize) -> [Range<usize>; 6] {
    let nt = nt_of(input);
    let optional = optional_of(input);
    let table_end = header_at(input, section_count(input));
    [
        nt + NUMBER_OF_SECTIONS..nt + NUMBER_OF_SECTIONS + 2,
        optional + SIZE_OF_INITIALIZED_DATA..optional + SIZE_OF_INITIALIZED_DATA + 4,
        optional + SIZE_OF_IMAGE..optional + SIZE_OF_IMAGE + 4,
        optional + CHECKSUM..optional + CHECKSUM + 4,
        table_end..table_end + SECTION_HEADER_BYTES,
        input.len()..output_len,
    ]
}

/// Byte-diff oracle: every differing byte lies in one of the six listed ranges.
fn assert_only_listed_ranges_changed(input: &[u8], output: &[u8]) {
    let ranges = changed_ranges(input, output.len());
    for (offset, (before, after)) in input.iter().zip(output).enumerate() {
        assert!(
            before == after || ranges.iter().any(|range| range.contains(&offset)),
            "byte {offset:#x} changed outside the listed ranges ({before:#04x} -> {after:#04x})"
        );
    }
}

#[test]
fn ac1_two_section_host_round_trips_and_changes_only_the_listed_ranges() {
    let input = two_sections();
    assert_eq!(input.len(), 0x600);
    let output = embed(&input);
    assert_eq!(
        output.len(),
        input.len() + 0x200,
        "input plus SizeOfRawData"
    );
    assert_only_listed_ranges_changed(&input, &output);

    let optional = NT + OPTIONAL;
    assert_eq!(get_u16(&output, NT + NUMBER_OF_SECTIONS), 3);
    assert_eq!(get_u32(&output, optional + SIZE_OF_INITIALIZED_DATA), 0x400);
    assert_eq!(get_u32(&output, optional + SIZE_OF_IMAGE), 0x4000);
    assert_eq!(get_u32(&output, optional + CHECKSUM), 0);
    let header = header_at(&output, 2);
    assert_eq!(output[header..header + 40], TWO_SECTION_CONTAINER_HEADER);
    let payload = golden_payload().encode();
    assert_eq!(output[0x600..0x600 + payload.len()], payload[..]);
    assert!(
        output[0x600 + payload.len()..]
            .iter()
            .all(|&byte| byte == 0)
    );
    assert_eq!(
        read_host_identity_bytes(&output).expect("round trip"),
        golden_payload()
    );
}

#[test]
fn ac1_uninitialized_sections_have_no_raw_range_and_round_trip() {
    // Trailing `.bss`: E comes from `.data`, the container address follows `.bss`.
    let trailing = with_uninitialized();
    // Middle `.bss`: an empty section between two raw ranges.
    let middle = build(&Spec {
        sections: vec![
            text(),
            bss(0x2000),
            Section {
                virtual_address: 0x3000,
                ..data()
            },
        ],
        ..two_section_spec()
    });
    for (input, container_address) in [(trailing, 0x4000), (middle, 0x4000)] {
        assert_eq!(input.len(), 0x600);
        let output = embed(&input);
        assert_eq!(output.len(), 0x800);
        assert_only_listed_ranges_changed(&input, &output);
        let header = header_at(&output, 3);
        assert_eq!(output[header..header + 8], *b".keldeai");
        assert_eq!(get_u32(&output, header + 12), container_address);
        assert_eq!(get_u32(&output, header + 20), 0x600, "E ignores .bss");
        assert_eq!(
            get_u32(&output, optional_of(&output) + SIZE_OF_INITIALIZED_DATA),
            0x400,
            ".bss is not initialized data"
        );
        assert_eq!(
            read_host_identity_bytes(&output).expect("round trip"),
            golden_payload()
        );
    }
}

#[test]
fn ac1_maximum_file_alignment_round_trips() {
    let input = build(&Spec {
        headers: 0x1_0000,
        file_alignment: 0x1_0000,
        section_alignment: 0x1_0000,
        sections: vec![
            Section {
                virtual_address: 0x1_0000,
                raw_pointer: 0x1_0000,
                raw_size: 0x1_0000,
                content: vec![0xCC; 0x200],
                ..text()
            },
            Section {
                virtual_address: 0x2_0000,
                raw_pointer: 0x2_0000,
                raw_size: 0x1_0000,
                ..data()
            },
        ],
        ..two_section_spec()
    });
    let output = embed(&input);
    assert_eq!(output.len(), input.len() + 0x1_0000);
    assert_only_listed_ranges_changed(&input, &output);
    assert_eq!(
        read_host_identity_bytes(&output).expect("round trip"),
        golden_payload()
    );
}

#[test]
fn ac3_embedding_the_writers_own_output_refuses_with_008() {
    let once = embed(&two_sections());
    let twice = embed_host_identity(&once, &golden_payload());
    assert_eq!(refusal(twice).0, "KELD-PACK-008");
}

#[test]
fn ac3_a_preexisting_container_section_refuses_with_008() {
    let last = build(&Spec {
        sections: vec![
            text(),
            data(),
            container_section(*b".keldeai", 0x3000, 0x600),
        ],
        ..two_section_spec()
    });
    let middle = build(&Spec {
        sections: vec![
            text(),
            container_section(*b".keldeai", 0x2000, 0x400),
            Section {
                virtual_address: 0x3000,
                raw_pointer: 0x600,
                ..data()
            },
        ],
        ..two_section_spec()
    });
    for input in [last, middle] {
        assert_eq!(
            refusal(embed_host_identity(&input, &golden_payload())).0,
            "KELD-PACK-008"
        );
    }
}

#[test]
fn ac3_the_name_match_is_exact_and_case_sensitive() {
    for name in [*b".KELDEAI", *b".keldeaj", *b".keldea\0"] {
        let input = build(&Spec {
            sections: vec![text(), data(), container_section(name, 0x3000, 0x600)],
            ..two_section_spec()
        });
        let output = embed(&input);
        assert_eq!(
            read_host_identity_bytes(&output).expect("one exact-name container"),
            golden_payload()
        );
    }
}

#[test]
fn ac4_a_signed_or_appended_input_refuses_with_009() {
    let certificate = NT + OPTIONAL + CERTIFICATE_TABLE;
    let mut signed = build(&Spec {
        trailing: vec![0x30; 0x10],
        ..two_section_spec()
    });
    put_u32(&mut signed, certificate, 0x600);
    put_u32(&mut signed, certificate + 4, 0x10);
    let mut entry_only = two_sections();
    put_u32(&mut entry_only, certificate + 4, 0x10);
    let mut one_byte = two_sections();
    one_byte.push(0);
    let appended = build(&Spec {
        trailing: vec![0x5A; 0x200],
        ..two_section_spec()
    });
    for (input, detail) in [
        (signed, "the Certificate Table entry is present"),
        (entry_only, "the Certificate Table entry is present"),
        (one_byte, "bytes follow the last section's raw data"),
        (appended, "bytes follow the last section's raw data"),
    ] {
        assert_eq!(
            refusal(embed_host_identity(&input, &golden_payload())),
            ("KELD-PACK-009", detail)
        );
    }
}

/// `.text` with raw data, then `count - 1` uninitialized sections, with exactly the
/// header room the table needs plus the container's 40 bytes.
fn many_sections(count: usize) -> Vec<u8> {
    let table_end = NT + OPTIONAL + 240 + count * SECTION_HEADER_BYTES;
    let headers = u32::try_from(round_up(
        u64::try_from(table_end + 40).expect("fits"),
        0x200,
    ))
    .expect("headers fit");
    let mut sections = vec![Section {
        raw_pointer: headers,
        ..text()
    }];
    for index in 1..count {
        let address = 0x1000 + u32::try_from(index).expect("fits") * 0x1000;
        sections.push(bss(address));
    }
    build(&Spec {
        headers,
        sections,
        ..two_section_spec()
    })
}

/// Two sections with `e_lfanew` placed so exactly `slack` bytes follow the table.
fn with_slack(slack: usize) -> Vec<u8> {
    build(&Spec {
        nt: 0x200 - slack - (OPTIONAL + 240) - 2 * SECTION_HEADER_BYTES,
        ..two_section_spec()
    })
}

#[test]
fn ac5_ninety_five_sections_and_forty_bytes_of_slack_are_the_boundaries() {
    let output = embed(&many_sections(95));
    assert_eq!(section_count(&output), 96);
    assert_eq!(
        read_host_identity_bytes(&output).expect("96-section output"),
        golden_payload()
    );
    let output = embed(&with_slack(40));
    assert_eq!(
        read_host_identity_bytes(&output).expect("exact-room output"),
        golden_payload()
    );
    // A non-zero byte just after the 40 is not header room and is left untouched.
    let mut input = two_sections();
    let table_end = header_at(&input, 2);
    input[table_end + 40] = 0x01;
    let output = embed(&input);
    assert_only_listed_ranges_changed(&input, &output);
    assert_eq!(output[table_end + 40], 0x01);
}

#[test]
fn ac5_no_room_refuses_with_010() {
    let table_end = header_at(&two_sections(), 2);
    let mut nonzero_first = two_sections();
    nonzero_first[table_end] = 0x01;
    let mut nonzero_last = two_sections();
    nonzero_last[table_end + 39] = 0x01;
    let mut bound_import = two_sections();
    put_u32(&mut bound_import, NT + OPTIONAL + BOUND_IMPORT + 4, 0x20);
    for (input, detail) in [
        (
            many_sections(96),
            "the section table already holds the loader maximum of 96 sections",
        ),
        (
            with_slack(39),
            "fewer than 40 bytes lie between the section table and SizeOfHeaders",
        ),
        (
            nonzero_first,
            "the 40 bytes after the section table are not zero",
        ),
        (
            nonzero_last,
            "the 40 bytes after the section table are not zero",
        ),
        (bound_import, "the bound-import directory is present"),
    ] {
        assert_eq!(
            refusal(embed_host_identity(&input, &golden_payload())),
            ("KELD-PACK-010", detail)
        );
    }
}

#[test]
fn ac7_the_writer_is_deterministic_and_leaves_its_input_unchanged() {
    for input in [two_sections(), with_uninitialized(), many_sections(95)] {
        let before = input.clone();
        let first = embed(&input);
        let second = embed(&input);
        assert_eq!(first, second);
        assert_eq!(input, before);
    }
}

/// SHA-256 of the writer's output for the two-section fixture and the golden payload,
/// computed with coreutils `sha256sum` (independent of the `sha2` crate below). Every CI
/// OS must reproduce it.
const TWO_SECTION_OUTPUT_SHA256: &str =
    "fe96b9c4a5873499dd5ad7ebccaae64c05e062adb034cac8ce1f0d5f24392bfe";

#[test]
fn ac7_the_synthetic_output_matches_the_checked_in_golden_digest() {
    use sha2::{Digest, Sha256};
    let output = embed(&two_sections());
    let digest = format!("{:x}", Sha256::digest(&output));
    assert_eq!(digest, TWO_SECTION_OUTPUT_SHA256);
}
