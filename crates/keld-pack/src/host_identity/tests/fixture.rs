//! Synthetic PE32+ AMD64 images built field by field from the PE Format layout.
//!
//! Every offset here is restated from the PE Format page, never imported from the
//! production module, so the fixtures and the byte-diff oracle stay independent of the
//! writer and reader under test.

use crate::{EXPECTED_APP_IDENTITY_KEY_BYTES, ExpectedAppIdentityPayload};

/// `e_lfanew` of the default fixtures: the PE signature follows the 64-byte DOS header.
pub(super) const NT: usize = 0x40;
pub(super) const FILE_ALIGNMENT: u32 = 0x200;
pub(super) const SECTION_ALIGNMENT: u32 = 0x1000;
pub(super) const HEADERS: u32 = 0x200;
/// Non-zero, so the writer's zeroing is observable.
pub(super) const CHECKSUM_VALUE: u32 = 0x0002_A5C3;

// Offsets relative to the PE signature.
pub(super) const MACHINE: usize = 4;
pub(super) const NUMBER_OF_SECTIONS: usize = 6;
pub(super) const SIZE_OF_OPTIONAL_HEADER: usize = 20;
pub(super) const CHARACTERISTICS: usize = 22;
pub(super) const OPTIONAL: usize = 24;
// Offsets relative to the optional header.
pub(super) const MAGIC: usize = 0;
pub(super) const SIZE_OF_INITIALIZED_DATA: usize = 8;
pub(super) const SECTION_ALIGNMENT_FIELD: usize = 32;
pub(super) const FILE_ALIGNMENT_FIELD: usize = 36;
pub(super) const SIZE_OF_IMAGE: usize = 56;
pub(super) const SIZE_OF_HEADERS: usize = 60;
pub(super) const CHECKSUM: usize = 64;
pub(super) const NUMBER_OF_RVA_AND_SIZES: usize = 108;
pub(super) const CERTIFICATE_TABLE: usize = 144;
pub(super) const DEBUG_DIRECTORY: usize = 160;
pub(super) const BOUND_IMPORT: usize = 200;
pub(super) const OPTIONAL_HEADER_BYTES: usize = 240;
// Offsets inside one 40-byte section header.
pub(super) const VIRTUAL_SIZE: usize = 8;
pub(super) const VIRTUAL_ADDRESS: usize = 12;
pub(super) const RAW_SIZE: usize = 16;
pub(super) const RAW_POINTER: usize = 20;
pub(super) const RELOCATIONS_POINTER: usize = 24;
pub(super) const LINE_NUMBERS_POINTER: usize = 28;
pub(super) const RELOCATION_COUNT: usize = 32;
pub(super) const LINE_NUMBER_COUNT: usize = 34;
pub(super) const SECTION_CHARACTERISTICS: usize = 36;
pub(super) const SECTION_HEADER_BYTES: usize = 40;

/// One section header plus the bytes written at its raw pointer.
#[derive(Debug, Clone)]
pub(super) struct Section {
    pub(super) name: [u8; 8],
    pub(super) virtual_size: u32,
    pub(super) virtual_address: u32,
    pub(super) raw_size: u32,
    pub(super) raw_pointer: u32,
    pub(super) characteristics: u32,
    pub(super) content: Vec<u8>,
}

/// A whole image: header placement, sections, and bytes after the last raw data.
#[derive(Debug, Clone)]
pub(super) struct Spec {
    pub(super) nt: usize,
    pub(super) headers: u32,
    pub(super) file_alignment: u32,
    pub(super) section_alignment: u32,
    pub(super) sections: Vec<Section>,
    pub(super) trailing: Vec<u8>,
}

pub(super) fn text() -> Section {
    Section {
        name: *b".text\0\0\0",
        virtual_size: 0x1F0,
        virtual_address: 0x1000,
        raw_size: 0x200,
        raw_pointer: 0x200,
        characteristics: 0x6000_0020,
        content: vec![0xCC; 0x200],
    }
}

pub(super) fn data() -> Section {
    Section {
        name: *b".data\0\0\0",
        virtual_size: 0x80,
        virtual_address: 0x2000,
        raw_size: 0x200,
        raw_pointer: 0x400,
        characteristics: 0xC000_0040,
        content: vec![0x5A; 0x200],
    }
}

/// Uninitialized data only: `SizeOfRawData` and `PointerToRawData` both zero.
pub(super) fn bss(virtual_address: u32) -> Section {
    Section {
        name: *b".bss\0\0\0\0",
        virtual_size: 0x400,
        virtual_address,
        raw_size: 0,
        raw_pointer: 0,
        characteristics: 0xC000_0080,
        content: Vec::new(),
    }
}

/// A section that already carries container-shaped bytes, built without the writer.
pub(super) fn container_section(name: [u8; 8], virtual_address: u32, raw_pointer: u32) -> Section {
    let payload = golden_payload().encode();
    Section {
        name,
        virtual_size: u32::try_from(payload.len()).expect("payload length fits"),
        virtual_address,
        raw_size: 0x200,
        raw_pointer,
        characteristics: 0x4000_0040,
        content: payload,
    }
}

pub(super) fn two_section_spec() -> Spec {
    Spec {
        nt: NT,
        headers: HEADERS,
        file_alignment: FILE_ALIGNMENT,
        section_alignment: SECTION_ALIGNMENT,
        sections: vec![text(), data()],
        trailing: Vec::new(),
    }
}

/// `.text`, `.data`, then a trailing `.bss` with both raw fields zero.
pub(super) fn uninitialized_spec() -> Spec {
    Spec {
        nt: NT,
        headers: HEADERS,
        file_alignment: FILE_ALIGNMENT,
        section_alignment: SECTION_ALIGNMENT,
        sections: vec![text(), data(), bss(0x3000)],
        trailing: Vec::new(),
    }
}

pub(super) fn two_sections() -> Vec<u8> {
    build(&two_section_spec())
}

pub(super) fn with_uninitialized() -> Vec<u8> {
    build(&uninitialized_spec())
}

pub(super) fn round_up(value: u64, alignment: u32) -> u64 {
    value.div_ceil(u64::from(alignment)) * u64::from(alignment)
}

/// Builds the image. `SizeOfImage` and `SizeOfInitializedData` are computed from the
/// sections, so every default fixture is admissible.
pub(super) fn build(spec: &Spec) -> Vec<u8> {
    let raw_end = spec
        .sections
        .iter()
        .map(|section| u64::from(section.raw_pointer) + u64::from(section.raw_size))
        .fold(u64::from(spec.headers), u64::max);
    let mut image = vec![0_u8; usize::try_from(raw_end).expect("fixture fits memory")];
    image[..2].copy_from_slice(b"MZ");
    put_u32(
        &mut image,
        0x3C,
        u32::try_from(spec.nt).expect("e_lfanew fits"),
    );
    image[spec.nt..spec.nt + 4].copy_from_slice(b"PE\0\0");
    let nt = spec.nt;
    put_u16(&mut image, nt + MACHINE, 0x8664);
    put_u16(
        &mut image,
        nt + NUMBER_OF_SECTIONS,
        u16::try_from(spec.sections.len()).expect("section count fits"),
    );
    put_u32(&mut image, nt + 8, 0x5F5E_1000); // TimeDateStamp
    put_u16(&mut image, nt + SIZE_OF_OPTIONAL_HEADER, 240);
    put_u16(&mut image, nt + CHARACTERISTICS, 0x0022); // EXECUTABLE | LARGE_ADDRESS_AWARE

    let optional = nt + OPTIONAL;
    let initialized: u32 = spec
        .sections
        .iter()
        .filter(|section| section.characteristics & 0x40 != 0)
        .map(|section| section.raw_size)
        .sum();
    let size_of_image = spec.sections.last().map_or(0, |last| {
        round_up(
            u64::from(last.virtual_address) + u64::from(last.virtual_size),
            spec.section_alignment,
        )
    });
    put_u16(&mut image, optional + MAGIC, 0x20B);
    image[optional + 2] = 14; // linker major
    put_u32(&mut image, optional + 4, 0x200); // SizeOfCode
    put_u32(&mut image, optional + SIZE_OF_INITIALIZED_DATA, initialized);
    put_u32(&mut image, optional + 16, 0x1000); // AddressOfEntryPoint
    put_u32(&mut image, optional + 20, 0x1000); // BaseOfCode
    put_u64(&mut image, optional + 24, 0x1_4000_0000); // ImageBase
    put_u32(
        &mut image,
        optional + SECTION_ALIGNMENT_FIELD,
        spec.section_alignment,
    );
    put_u32(
        &mut image,
        optional + FILE_ALIGNMENT_FIELD,
        spec.file_alignment,
    );
    put_u16(&mut image, optional + 40, 6); // MajorOperatingSystemVersion
    put_u16(&mut image, optional + 48, 6); // MajorSubsystemVersion
    put_u32(
        &mut image,
        optional + SIZE_OF_IMAGE,
        u32::try_from(size_of_image).expect("fixture SizeOfImage fits"),
    );
    put_u32(&mut image, optional + SIZE_OF_HEADERS, spec.headers);
    put_u32(&mut image, optional + CHECKSUM, CHECKSUM_VALUE);
    put_u16(&mut image, optional + 68, 3); // Subsystem: console
    put_u16(&mut image, optional + 70, 0x8160); // DllCharacteristics
    put_u64(&mut image, optional + 72, 0x10_0000); // SizeOfStackReserve
    put_u64(&mut image, optional + 80, 0x1000); // SizeOfStackCommit
    put_u64(&mut image, optional + 88, 0x10_0000); // SizeOfHeapReserve
    put_u64(&mut image, optional + 96, 0x1000); // SizeOfHeapCommit
    put_u32(&mut image, optional + NUMBER_OF_RVA_AND_SIZES, 16);
    // A debug directory the writer must leave untouched.
    put_u32(&mut image, optional + DEBUG_DIRECTORY, 0x2010);
    put_u32(&mut image, optional + DEBUG_DIRECTORY + 4, 0x1C);

    for (index, section) in spec.sections.iter().enumerate() {
        let header = section_header(spec, index);
        image[header..header + 8].copy_from_slice(&section.name);
        put_u32(&mut image, header + VIRTUAL_SIZE, section.virtual_size);
        put_u32(
            &mut image,
            header + VIRTUAL_ADDRESS,
            section.virtual_address,
        );
        put_u32(&mut image, header + RAW_SIZE, section.raw_size);
        put_u32(&mut image, header + RAW_POINTER, section.raw_pointer);
        put_u32(
            &mut image,
            header + SECTION_CHARACTERISTICS,
            section.characteristics,
        );
        let start = usize::try_from(section.raw_pointer).expect("raw pointer fits");
        image[start..start + section.content.len()].copy_from_slice(&section.content);
    }
    image.extend_from_slice(&spec.trailing);
    image
}

/// Offset of section header `index` for a fixture whose PE signature is at `spec.nt`.
pub(super) fn section_header(spec: &Spec, index: usize) -> usize {
    spec.nt + OPTIONAL + OPTIONAL_HEADER_BYTES + index * SECTION_HEADER_BYTES
}

/// Offset of section header `index` in any image, from its own `e_lfanew`.
pub(super) fn header_at(image: &[u8], index: usize) -> usize {
    nt_of(image) + OPTIONAL + OPTIONAL_HEADER_BYTES + index * SECTION_HEADER_BYTES
}

pub(super) fn nt_of(image: &[u8]) -> usize {
    usize::try_from(get_u32(image, 0x3C)).expect("e_lfanew fits")
}

pub(super) fn optional_of(image: &[u8]) -> usize {
    nt_of(image) + OPTIONAL
}

pub(super) fn section_count(image: &[u8]) -> usize {
    usize::from(get_u16(image, nt_of(image) + NUMBER_OF_SECTIONS))
}

pub(super) fn get_u16(image: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(image[at..at + 2].try_into().expect("two bytes"))
}

pub(super) fn get_u32(image: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(image[at..at + 4].try_into().expect("four bytes"))
}

pub(super) fn put_u16(image: &mut [u8], at: usize, value: u16) {
    image[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn put_u32(image: &mut [u8], at: usize, value: u32) {
    image[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn put_u64(image: &mut [u8], at: usize, value: u64) {
    image[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn golden_key() -> [u8; EXPECTED_APP_IDENTITY_KEY_BYTES] {
    let mut key = [0_u8; EXPECTED_APP_IDENTITY_KEY_BYTES];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::try_from(index).expect("key index fits");
    }
    key
}

/// The codec's own golden fields (97 encoded bytes).
pub(super) fn golden_payload() -> ExpectedAppIdentityPayload {
    ExpectedAppIdentityPayload::new("com.example.app", "stable", "windows-x64", golden_key())
        .expect("golden fields are canonical")
}
