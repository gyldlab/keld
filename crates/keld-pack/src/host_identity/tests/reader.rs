//! Reader contract: AC9 refusals, the canonical-form boundaries, and the bounded-read
//! property (container spec §4 "Reader contract", §7 row 9).

use super::fixture::{
    CERTIFICATE_TABLE, CHECKSUM, LINE_NUMBER_COUNT, LINE_NUMBERS_POINTER, RAW_POINTER, RAW_SIZE,
    RELOCATION_COUNT, RELOCATIONS_POINTER, SECTION_CHARACTERISTICS, SIZE_OF_IMAGE, Section, Spec,
    VIRTUAL_ADDRESS, VIRTUAL_SIZE, build, container_section, data, get_u32, golden_key,
    golden_payload, header_at, optional_of, put_u16, put_u32, text, two_section_spec, two_sections,
};
use super::malformed::cases;
use super::{embed, refusal};
use crate::host_identity::{ImageSource, read_container};
use crate::{ExpectedAppIdentityPayload, PackError, embed_host_identity, read_host_identity_bytes};
use std::cell::Cell;

const LENGTH: &str = "the container VirtualSize is outside the payload length bounds";
const RAW_SIZE_DETAIL: &str =
    "the container SizeOfRawData is not VirtualSize rounded up to FileAlignment";
const FIELDS: &str = "the container relocation or line-number fields are not zero";
const PADDING: &str = "the container padding is not zero";
const CERTIFICATE: &str = "the certificate table overlaps the container or ends beyond the image";

/// The canonical two-section output; its container header is section 2 and its raw
/// data is `0x600..0x800` holding the 97-byte golden payload.
fn canonical() -> Vec<u8> {
    embed(&two_sections())
}

fn container_u32(image: &mut [u8], field: usize, value: u32) {
    let at = header_at(image, 2) + field;
    put_u32(image, at, value);
}

fn with_certificate(mut image: Vec<u8>, appended: usize, start: u32, size: u32) -> Vec<u8> {
    image.resize(image.len() + appended, 0x30);
    let at = optional_of(&image) + CERTIFICATE_TABLE;
    put_u32(&mut image, at, start);
    put_u32(&mut image, at + 4, size);
    image
}

#[test]
fn ac9_no_container_refuses_with_007() {
    assert_eq!(
        refusal(read_host_identity_bytes(&two_sections())).0,
        "KELD-PACK-007"
    );
}

#[test]
fn ac9_two_containers_refuse_with_008() {
    let input = build(&Spec {
        sections: vec![
            text(),
            data(),
            container_section(*b".keldeai", 0x3000, 0x600),
            container_section(*b".keldeai", 0x4000, 0x800),
        ],
        ..two_section_spec()
    });
    assert_eq!(refusal(read_host_identity_bytes(&input)).0, "KELD-PACK-008");
}

#[test]
fn ac9_each_non_canonical_container_field_refuses_with_011() {
    let mut characteristics = canonical();
    container_u32(&mut characteristics, SECTION_CHARACTERISTICS, 0xC000_0040);
    let mut relocations_pointer = canonical();
    container_u32(&mut relocations_pointer, RELOCATIONS_POINTER, 1);
    let mut line_numbers_pointer = canonical();
    container_u32(&mut line_numbers_pointer, LINE_NUMBERS_POINTER, 1);
    let mut relocation_count = canonical();
    let at = header_at(&relocation_count, 2) + RELOCATION_COUNT;
    put_u16(&mut relocation_count, at, 1);
    let mut line_number_count = canonical();
    let at = header_at(&line_number_count, 2) + LINE_NUMBER_COUNT;
    put_u16(&mut line_number_count, at, 1);
    let mut below_minimum = canonical();
    container_u32(&mut below_minimum, VIRTUAL_SIZE, 67);
    let mut above_maximum = canonical();
    container_u32(&mut above_maximum, VIRTUAL_SIZE, 401);
    let mut raw_size = canonical();
    raw_size.resize(0xA00, 0);
    container_u32(&mut raw_size, RAW_SIZE, 0x400);
    let mut no_raw_range = canonical();
    container_u32(&mut no_raw_range, RAW_SIZE, 0);
    container_u32(&mut no_raw_range, RAW_POINTER, 0);
    let mut first_padding = canonical();
    first_padding[0x600 + 97] = 0x01;
    let mut last_padding = canonical();
    last_padding[0x7FF] = 0x01;
    let overlapping = with_certificate(canonical(), 8, 0x7F8, 0x10);
    let beyond = with_certificate(canonical(), 0x10, 0x800, 0x18);

    for (name, image, detail) in [
        (
            "characteristics",
            characteristics,
            "the container Characteristics are not 0x40000040",
        ),
        ("PointerToRelocations", relocations_pointer, FIELDS),
        ("PointerToLinenumbers", line_numbers_pointer, FIELDS),
        ("NumberOfRelocations", relocation_count, FIELDS),
        ("NumberOfLinenumbers", line_number_count, FIELDS),
        ("L = 67", below_minimum, LENGTH),
        ("L = 401", above_maximum, LENGTH),
        ("SizeOfRawData", raw_size, RAW_SIZE_DETAIL),
        ("both raw fields zero", no_raw_range, RAW_SIZE_DETAIL),
        ("first padding byte", first_padding, PADDING),
        ("last padding byte", last_padding, PADDING),
        ("certificate table overlaps", overlapping, CERTIFICATE),
        ("certificate table beyond the image", beyond, CERTIFICATE),
    ] {
        assert_eq!(
            refusal(read_host_identity_bytes(&image)),
            ("KELD-PACK-011", detail),
            "{name}"
        );
    }
}

#[test]
fn ac9_container_position_rules_refuse_with_011() {
    let not_last = build(&Spec {
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
    let only = build(&Spec {
        sections: vec![container_section(*b".keldeai", 0x1000, 0x200)],
        ..two_section_spec()
    });
    let file_order = build(&Spec {
        sections: vec![
            text(),
            Section {
                raw_pointer: 0x600,
                ..data()
            },
            container_section(*b".keldeai", 0x3000, 0x400),
        ],
        ..two_section_spec()
    });
    for (image, detail) in [
        (not_last, "the container is not the last section header"),
        (only, "the container is the only section"),
        (
            file_order,
            "the container raw data is not the last raw data in the image",
        ),
    ] {
        assert_eq!(
            refusal(read_host_identity_bytes(&image)),
            ("KELD-PACK-011", detail)
        );
    }
}

#[test]
fn ac9_structure_rules_reach_the_container_first_with_006() {
    // Reader step 1 admits the whole table, container included, before step 3 inspects
    // the container, so these step 3 clauses surface as the step 1 refusal.
    let mut misaligned = canonical();
    container_u32(&mut misaligned, RAW_POINTER, 0x610);
    let mut overlapping = canonical();
    container_u32(&mut overlapping, RAW_POINTER, 0x400);
    let mut address = canonical();
    container_u32(&mut address, VIRTUAL_ADDRESS, 0x4000);
    let mut size_of_image = canonical();
    let at = optional_of(&size_of_image) + SIZE_OF_IMAGE;
    put_u32(&mut size_of_image, at, 0x5000);
    let mut truncated = canonical();
    truncated.pop();
    // Only the container's raw data lies in the header area (before SizeOfHeaders 0x400).
    let inside_headers = build(&Spec {
        headers: 0x400,
        sections: vec![
            Section {
                raw_pointer: 0x400,
                ..text()
            },
            Section {
                raw_pointer: 0x600,
                ..data()
            },
            container_section(*b".keldeai", 0x3000, 0x200),
        ],
        ..two_section_spec()
    });
    for (image, detail) in [
        (misaligned, "section raw data is not FileAlignment-aligned"),
        (
            inside_headers,
            "section raw data starts before SizeOfHeaders",
        ),
        (overlapping, "section raw ranges overlap"),
        (
            address,
            "section virtual addresses are not ascending and adjacent",
        ),
        (
            size_of_image,
            "SizeOfImage is not the last section's virtual end rounded up to SectionAlignment",
        ),
        (
            truncated,
            "section raw data lies beyond the end of the image",
        ),
    ] {
        assert_eq!(
            refusal(read_host_identity_bytes(&image)),
            ("KELD-PACK-006", detail)
        );
    }
}

#[test]
fn ac9_a_canonical_container_with_a_non_canonical_payload_refuses_with_005() {
    // Truncated payload: L - 1 bytes, the dropped key byte zeroed into the padding.
    let mut truncated = canonical();
    container_u32(&mut truncated, VIRTUAL_SIZE, 96);
    truncated[0x600 + 96] = 0;
    let mut domain = canonical();
    domain[0x600] ^= 0x01;
    for (image, detail) in [
        (truncated, "public key length or trailing bytes"),
        (domain, "domain tag"),
    ] {
        assert_eq!(
            refusal(read_host_identity_bytes(&image)),
            ("KELD-PACK-005", detail)
        );
    }
}

#[test]
fn the_payload_length_bounds_and_a_signed_shape_are_accepted() {
    let shortest = ExpectedAppIdentityPayload::new("a", "c", "t", golden_key()).expect("valid");
    let longest = ExpectedAppIdentityPayload::new(
        &"a".repeat(255),
        &"c".repeat(16),
        &"t".repeat(64),
        golden_key(),
    )
    .expect("valid");
    for (payload, length) in [(shortest, 68), (longest, 400)] {
        let output = embed_host_identity(&two_sections(), &payload).expect("embed");
        assert_eq!(
            get_u32(&output, header_at(&output, 2) + VIRTUAL_SIZE),
            length
        );
        assert_eq!(read_host_identity_bytes(&output).expect("read"), payload);
    }
    // After signing: the certificate table starts at the container's raw end, ends at
    // end of file, and the signer may set the CheckSum.
    let mut signed = with_certificate(canonical(), 0x10, 0x800, 0x10);
    let at = optional_of(&signed) + CHECKSUM;
    put_u32(&mut signed, at, 0x0001_2345);
    assert_eq!(
        read_host_identity_bytes(&signed).expect("signed shape"),
        golden_payload()
    );
}

/// A source with a declared length that serves `prefix` and zeros beyond it, counting
/// every byte the reader requests.
struct Sparse {
    prefix: Vec<u8>,
    len: u64,
    requested: Cell<u64>,
}

impl Sparse {
    fn new(prefix: Vec<u8>, len: u64) -> Self {
        Self {
            prefix,
            len,
            requested: Cell::new(0),
        }
    }
}

impl ImageSource for Sparse {
    fn image_len(&self) -> Result<u64, PackError> {
        Ok(self.len)
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<bool, PackError> {
        let count = buf.len() as u64;
        self.requested.set(self.requested.get() + count);
        // Fail at the crossing, so an unbounded reader fails fast instead of scanning.
        assert!(
            self.requested.get() <= READ_CEILING,
            "the reader requested {} bytes, above the fixed ceiling",
            self.requested.get()
        );
        if offset.checked_add(count).is_none_or(|end| end > self.len) {
            return Ok(false);
        }
        for (index, slot) in buf.iter_mut().enumerate() {
            let at = usize::try_from(offset)
                .ok()
                .and_then(|start| start.checked_add(index));
            *slot = at.and_then(|at| self.prefix.get(at)).copied().unwrap_or(0);
        }
        Ok(true)
    }
}

/// DOS header, PE signature and COFF header, optional header, 96 section headers, the
/// largest payload and the largest padding (`FileAlignment` 65536 minus one byte).
const READ_CEILING: u64 = 64 + 24 + 240 + 96 * 40 + 400 + 65_535;

#[test]
fn the_reader_requests_a_fixed_byte_count_whatever_the_file_length() {
    let output = canonical();
    // DOS 64 + COFF 24 + optional 240 + three headers 120 + payload 97 + padding 415.
    for len in [
        output.len() as u64,
        output.len() as u64 + (1 << 20),
        1 << 40,
    ] {
        let source = Sparse::new(output.clone(), len);
        assert_eq!(read_container(&source).expect("read"), golden_payload());
        assert_eq!(source.requested.get(), 960, "file length {len}");
    }
    // A declared 4 GiB certificate table after the container is never read.
    let signed = with_certificate(output, 0, 0x800, u32::MAX);
    let source = Sparse::new(signed, 0x800 + u64::from(u32::MAX));
    assert_eq!(read_container(&source).expect("read"), golden_payload());
    assert_eq!(source.requested.get(), 960);
}

#[test]
fn every_refusal_requests_at_most_the_fixed_ceiling() {
    let output = canonical();
    for (name, mutate, _) in cases() {
        let mut image = output.clone();
        mutate(&mut image);
        let source = Sparse::new(image, 1 << 40);
        assert!(read_container(&source).is_err(), "{name}");
        assert!(source.requested.get() <= READ_CEILING, "{name}");
    }
}
