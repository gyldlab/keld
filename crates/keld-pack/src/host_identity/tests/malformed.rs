//! AC6: each independent header mutation refuses with `KELD-PACK-006` from both the
//! writer (on the input) and the reader (on the canonical output), never panics, and
//! never reads or allocates by the file size (container spec §7 row 6).

use super::fixture::{
    CHARACTERISTICS, FILE_ALIGNMENT_FIELD, MACHINE, MAGIC, NUMBER_OF_RVA_AND_SIZES,
    NUMBER_OF_SECTIONS, OPTIONAL, RAW_POINTER, RAW_SIZE, SECTION_ALIGNMENT_FIELD, SIZE_OF_HEADERS,
    SIZE_OF_IMAGE, SIZE_OF_INITIALIZED_DATA, SIZE_OF_OPTIONAL_HEADER, Section, Spec,
    VIRTUAL_ADDRESS, VIRTUAL_SIZE, build, data, get_u16, get_u32, golden_payload, header_at, nt_of,
    optional_of, put_u16, put_u32, round_up, section_count, text, two_section_spec, two_sections,
};
use super::{embed, refusal};
use crate::{embed_host_identity, read_host_identity_bytes};

pub(super) type Mutation = fn(&mut Vec<u8>);

fn set_coff_u16(image: &mut [u8], field: usize, value: u16) {
    let at = nt_of(image) + field;
    put_u16(image, at, value);
}

fn set_optional_u32(image: &mut [u8], field: usize, value: u32) {
    let at = optional_of(image) + field;
    put_u32(image, at, value);
}

fn set_section_u32(image: &mut [u8], index: usize, field: usize, value: u32) {
    let at = header_at(image, index) + field;
    put_u32(image, at, value);
}

fn section_u32(image: &[u8], index: usize, field: usize) -> u32 {
    get_u32(image, header_at(image, index) + field)
}

/// The section whose raw range ends last (`.data` in the input, the container after).
fn highest_raw(image: &[u8]) -> usize {
    (0..section_count(image))
        .max_by_key(|&index| {
            u64::from(section_u32(image, index, RAW_POINTER))
                + u64::from(section_u32(image, index, RAW_SIZE))
        })
        .expect("at least one section")
}

fn last(image: &[u8]) -> usize {
    section_count(image) - 1
}

const FILE_ALIGNMENT_DETAIL: &str = "FileAlignment is not a power of two from 512 to 65536";
const SECTION_ALIGNMENT_DETAIL: &str =
    "SectionAlignment is not a power of two of at least 4096 and FileAlignment";
const HEADERS_DETAIL: &str = "SizeOfHeaders is not FileAlignment-aligned inside the image";
const COFF_DETAIL: &str = "PE signature or COFF header lies beyond the end of the image";
const EXECUTABLE_DETAIL: &str = "image is not an executable image or is a DLL";
const COUNT_DETAIL: &str = "NumberOfSections is not 1 to 96";
const ONE_ZERO_DETAIL: &str = "section has exactly one of SizeOfRawData and PointerToRawData zero";
const RAW_ALIGN_DETAIL: &str = "section raw data is not FileAlignment-aligned";
const OVERLAP_DETAIL: &str = "section raw ranges overlap";
const BEYOND_DETAIL: &str = "section raw data lies beyond the end of the image";
const SIZE_OF_IMAGE_DETAIL: &str =
    "SizeOfImage is not the last section's virtual end rounded up to SectionAlignment";

/// One row per §7 row 6 mutation: name, mutation, expected `KELD-PACK-006` detail.
#[allow(clippy::too_many_lines)] // One flat table keeps every row next to its oracle.
pub(super) fn cases() -> Vec<(&'static str, Mutation, &'static str)> {
    vec![
        (
            "shorter than the DOS header",
            |image| image.truncate(63),
            "image is shorter than the DOS header",
        ),
        ("MZ", |image| image[1] = b'X', "missing MZ signature"),
        (
            "e_lfanew below 64",
            |image| put_u32(image, 0x3C, 63),
            "e_lfanew points inside the DOS header",
        ),
        (
            "e_lfanew beyond the file",
            |image| {
                let len = u32::try_from(image.len()).expect("fits");
                put_u32(image, 0x3C, len);
            },
            COFF_DETAIL,
        ),
        (
            "e_lfanew overflow",
            |image| put_u32(image, 0x3C, 0xFFFF_FFF0),
            COFF_DETAIL,
        ),
        (
            "PE signature",
            |image| {
                let at = nt_of(image) + 3;
                image[at] = 1;
            },
            "missing PE signature",
        ),
        (
            "Machine i386",
            |image| set_coff_u16(image, MACHINE, 0x014C),
            "machine is not AMD64",
        ),
        (
            "Machine ARM64",
            |image| set_coff_u16(image, MACHINE, 0xAA64),
            "machine is not AMD64",
        ),
        (
            "DLL bit",
            |image| set_coff_u16(image, CHARACTERISTICS, 0x2022),
            EXECUTABLE_DETAIL,
        ),
        (
            "executable bit clear",
            |image| set_coff_u16(image, CHARACTERISTICS, 0x0020),
            EXECUTABLE_DETAIL,
        ),
        (
            "SizeOfOptionalHeader",
            |image| set_coff_u16(image, SIZE_OF_OPTIONAL_HEADER, 224),
            "SizeOfOptionalHeader is not 240",
        ),
        (
            "optional header beyond the file",
            |image| {
                let len = optional_of(image) + 239;
                image.truncate(len);
            },
            "optional header lies beyond the end of the image",
        ),
        (
            "Magic PE32",
            |image| {
                let at = optional_of(image) + MAGIC;
                put_u16(image, at, 0x10B);
            },
            "optional header is not PE32+",
        ),
        (
            "NumberOfRvaAndSizes",
            |image| set_optional_u32(image, NUMBER_OF_RVA_AND_SIZES, 15),
            "NumberOfRvaAndSizes is not 16",
        ),
        (
            "FileAlignment not a power of two",
            |image| set_optional_u32(image, FILE_ALIGNMENT_FIELD, 0x300),
            FILE_ALIGNMENT_DETAIL,
        ),
        (
            "FileAlignment 256",
            |image| set_optional_u32(image, FILE_ALIGNMENT_FIELD, 0x100),
            FILE_ALIGNMENT_DETAIL,
        ),
        (
            "FileAlignment 131072",
            |image| set_optional_u32(image, FILE_ALIGNMENT_FIELD, 0x2_0000),
            FILE_ALIGNMENT_DETAIL,
        ),
        (
            "SectionAlignment below 4096",
            |image| set_optional_u32(image, SECTION_ALIGNMENT_FIELD, 0x800),
            SECTION_ALIGNMENT_DETAIL,
        ),
        (
            "SectionAlignment not a power of two",
            |image| set_optional_u32(image, SECTION_ALIGNMENT_FIELD, 0x1800),
            SECTION_ALIGNMENT_DETAIL,
        ),
        (
            "SectionAlignment below FileAlignment",
            |image| set_optional_u32(image, FILE_ALIGNMENT_FIELD, 0x2000),
            SECTION_ALIGNMENT_DETAIL,
        ),
        (
            "SizeOfHeaders misaligned",
            |image| set_optional_u32(image, SIZE_OF_HEADERS, 0x300),
            HEADERS_DETAIL,
        ),
        (
            "SizeOfHeaders beyond the file",
            |image| {
                let beyond = round_up(image.len() as u64, 0x200) + 0x200;
                set_optional_u32(image, SIZE_OF_HEADERS, u32::try_from(beyond).expect("fits"));
            },
            HEADERS_DETAIL,
        ),
        (
            "NumberOfSections zero",
            |image| set_coff_u16(image, NUMBER_OF_SECTIONS, 0),
            COUNT_DETAIL,
        ),
        (
            "NumberOfSections 97",
            |image| set_coff_u16(image, NUMBER_OF_SECTIONS, 97),
            COUNT_DETAIL,
        ),
        (
            "section table beyond SizeOfHeaders",
            |image| set_coff_u16(image, NUMBER_OF_SECTIONS, 10),
            "section table ends after SizeOfHeaders",
        ),
        (
            "only SizeOfRawData zero",
            |image| set_section_u32(image, 0, RAW_SIZE, 0),
            ONE_ZERO_DETAIL,
        ),
        (
            "only PointerToRawData zero",
            |image| set_section_u32(image, 0, RAW_POINTER, 0),
            ONE_ZERO_DETAIL,
        ),
        (
            "misaligned PointerToRawData",
            |image| set_section_u32(image, 0, RAW_POINTER, 0x210),
            RAW_ALIGN_DETAIL,
        ),
        (
            "misaligned SizeOfRawData",
            |image| set_section_u32(image, 0, RAW_SIZE, 0x1F0),
            RAW_ALIGN_DETAIL,
        ),
        (
            "raw data before SizeOfHeaders",
            |image| set_optional_u32(image, SIZE_OF_HEADERS, 0x400),
            "section raw data starts before SizeOfHeaders",
        ),
        (
            "same raw pointer",
            |image| set_section_u32(image, 1, RAW_POINTER, 0x200),
            OVERLAP_DETAIL,
        ),
        (
            "partly overlapping raw ranges",
            |image| set_section_u32(image, 0, RAW_SIZE, 0x400),
            OVERLAP_DETAIL,
        ),
        (
            "raw range beyond the file",
            |image| {
                let index = highest_raw(image);
                let size = section_u32(image, index, RAW_SIZE);
                set_section_u32(image, index, RAW_SIZE, size + 0x200);
            },
            BEYOND_DETAIL,
        ),
        (
            "raw end overflows 32 bits",
            |image| {
                let index = highest_raw(image);
                set_section_u32(image, index, RAW_POINTER, 0xFFFF_FE00);
            },
            BEYOND_DETAIL,
        ),
        (
            "zero VirtualSize",
            |image| set_section_u32(image, 0, VIRTUAL_SIZE, 0),
            "section VirtualSize is zero",
        ),
        (
            "misaligned VirtualAddress",
            |image| set_section_u32(image, 0, VIRTUAL_ADDRESS, 0x1100),
            "section VirtualAddress is not SectionAlignment-aligned",
        ),
        (
            "non-adjacent virtual addresses",
            |image| {
                let address = section_u32(image, 1, VIRTUAL_ADDRESS);
                set_section_u32(image, 1, VIRTUAL_ADDRESS, address + 0x1000);
            },
            "section virtual addresses are not ascending and adjacent",
        ),
        (
            "wrong SizeOfImage",
            |image| {
                let size = get_u32(image, optional_of(image) + SIZE_OF_IMAGE);
                set_optional_u32(image, SIZE_OF_IMAGE, size + 0x1000);
            },
            SIZE_OF_IMAGE_DETAIL,
        ),
        (
            // With 32-bit wrapping the last section would end at 0 and match.
            "virtual end overflows 32 bits",
            |image| {
                let index = last(image);
                let address = section_u32(image, index, VIRTUAL_ADDRESS);
                set_section_u32(image, index, VIRTUAL_SIZE, 0u32.wrapping_sub(address));
                set_optional_u32(image, SIZE_OF_IMAGE, 0);
            },
            SIZE_OF_IMAGE_DETAIL,
        ),
    ]
}

#[test]
fn ac6_each_header_mutation_refuses_with_006_from_the_writer_and_the_reader() {
    let input = two_sections();
    let output = embed(&input);
    for (name, mutate, detail) in cases() {
        let mut bad_input = input.clone();
        mutate(&mut bad_input);
        assert_eq!(
            refusal(embed_host_identity(&bad_input, &golden_payload())),
            ("KELD-PACK-006", detail),
            "writer: {name}"
        );
        let mut bad_output = output.clone();
        mutate(&mut bad_output);
        assert_eq!(
            refusal(read_host_identity_bytes(&bad_output)),
            ("KELD-PACK-006", detail),
            "reader: {name}"
        );
    }
}

#[test]
fn ac6_a_written_field_that_would_overflow_32_bits_refuses_with_006() {
    const OVERFLOW: &str = "a written header field would overflow 32 bits";
    let at = optional_of(&two_sections()) + SIZE_OF_INITIALIZED_DATA;
    let mut top = two_sections();
    put_u32(&mut top, at, 0xFFFF_FFFF - 0x200);
    assert!(embed_host_identity(&top, &golden_payload()).is_ok());
    put_u32(&mut top, at, 0xFFFF_FFFF - 0x1FF);
    assert_eq!(
        refusal(embed_host_identity(&top, &golden_payload())),
        ("KELD-PACK-006", OVERFLOW)
    );
    // SizeOfImage 0xFFFF_F000: the container would end past the 32-bit address space.
    let high = build(&Spec {
        sections: vec![
            Section {
                virtual_address: 0xFFFF_D000,
                ..text()
            },
            Section {
                virtual_address: 0xFFFF_E000,
                ..data()
            },
        ],
        ..two_section_spec()
    });
    assert_eq!(
        get_u32(&high, optional_of(&high) + SIZE_OF_IMAGE),
        0xFFFF_F000
    );
    assert_eq!(
        refusal(embed_host_identity(&high, &golden_payload())),
        ("KELD-PACK-006", OVERFLOW)
    );
}

#[test]
fn ac6_every_truncation_refuses_without_panicking() {
    let input = two_sections();
    let output = embed(&input);
    for len in 0..input.len() {
        assert!(embed_host_identity(&input[..len], &golden_payload()).is_err());
    }
    for len in 0..output.len() {
        assert!(read_host_identity_bytes(&output[..len]).is_err());
    }
    assert_eq!(get_u16(&output, nt_of(&output) + NUMBER_OF_SECTIONS), 3);
}

#[test]
fn ac6_the_fixture_offsets_hit_the_two_section_layout() {
    // Guards the table above: each section index it names exists in both images.
    let input = two_sections();
    assert_eq!(header_at(&input, 0), two_section_spec().nt + OPTIONAL + 240);
    assert_eq!(highest_raw(&input), 1);
    assert_eq!(highest_raw(&embed(&input)), 2);
}
