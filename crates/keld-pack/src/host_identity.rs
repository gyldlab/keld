//! `ExpectedAppIdentity` Windows host container v1 (KEL-19 container spec §4).
//!
//! The container is one dedicated PE section named `.keldeai`, appended to the unsigned
//! prebuilt Windows x64 host after link and before its final Authenticode signature, so
//! the image hash covers both its 40-byte header and its raw data. The raw data is the
//! canonical [`ExpectedAppIdentityPayload`] followed by zero padding up to
//! `FileAlignment`.
//!
//! The writer is a pure byte transformation. Writer and readers share one structure
//! parser over a crate-private positioned-read source with two implementations: byte
//! slices on every target and, on Windows, an open `File` read with `seek_read` and its
//! handle-derived length. The parser keeps every buffer on the stack with a size fixed by
//! the format, never by the file. It authenticates nothing: a result is authentic only
//! when it was read from the handle KEL-135 has just verified.

use crate::PackError;
use crate::expected_identity::{ExpectedAppIdentityPayload, MAX_PAYLOAD_BYTES, MIN_PAYLOAD_BYTES};

/// Exact container section name. Crate-private like the payload domain tag, so no second
/// writer or reader can exist outside keld-pack.
const SECTION_NAME: [u8; 8] = *b".keldeai";
/// `IMAGE_SCN_CNT_INITIALIZED_DATA` (`0x40`) plus `IMAGE_SCN_MEM_READ` (`0x4000_0000`).
const CONTAINER_CHARACTERISTICS: u32 = 0x4000_0040;

const DOS_HEADER_BYTES: usize = 64;
const E_LFANEW: usize = 0x3C;
/// PE signature (4 bytes) plus COFF file header (20 bytes).
const COFF_END: usize = 24;
/// PE32+ optional header with 16 data directories.
const OPTIONAL_HEADER_BYTES: usize = 240;
const SECTION_HEADER_BYTES: usize = 40;
/// "the Windows loader limits the number of sections to 96" (PE Format).
const LOADER_MAX_SECTIONS: usize = 96;
const SECTION_TABLE_MAX_BYTES: usize = LOADER_MAX_SECTIONS * SECTION_HEADER_BYTES;

const MACHINE_AMD64: u16 = 0x8664;
const FILE_EXECUTABLE_IMAGE: u16 = 0x0002;
const FILE_DLL: u16 = 0x2000;
const PE32_PLUS_MAGIC: u16 = 0x20B;
const DATA_DIRECTORY_COUNT: u32 = 16;
const MIN_FILE_ALIGNMENT: u32 = 512;
const MAX_FILE_ALIGNMENT: u32 = 65_536;
const MIN_SECTION_ALIGNMENT: u32 = 4096;

// Offsets inside the 24 bytes that start at the PE signature.
const MACHINE: usize = 4;
const NUMBER_OF_SECTIONS: usize = 6;
const SIZE_OF_OPTIONAL_HEADER: usize = 20;
const CHARACTERISTICS: usize = 22;

// Offsets inside the PE32+ optional header.
const MAGIC: usize = 0;
const SIZE_OF_INITIALIZED_DATA: usize = 8;
const SECTION_ALIGNMENT: usize = 32;
const FILE_ALIGNMENT: usize = 36;
const SIZE_OF_IMAGE: usize = 56;
const SIZE_OF_HEADERS: usize = 60;
const CHECKSUM: usize = 64;
const NUMBER_OF_RVA_AND_SIZES: usize = 108;
/// Data-directory index 4: file offset and size of the attribute certificate table.
const CERTIFICATE_TABLE: usize = 144;
/// Data-directory index 11, which would live in the header area the container uses.
const BOUND_IMPORT: usize = 200;

/// Container padding is checked through a fixed chunk, so no buffer depends on the file.
const PADDING_CHUNK_BYTES: usize = 4096;

/// Embeds `payload` exactly once into an unsigned, unmodified prebuilt Windows x64
/// host image and returns the new image bytes. Pure and deterministic.
///
/// The input is admitted in order and refused at the first failure, before the output
/// is allocated. The output equals the input except for `NumberOfSections`,
/// `SizeOfInitializedData`, `SizeOfImage`, a zeroed `CheckSum`, the new 40-byte section
/// header after the existing table, and the appended raw data. Before returning, the
/// writer reads its own output back and requires the embedded payload.
///
/// # Errors
/// - `KELD-PACK-006` when the input is not an admissible PE32+ AMD64 executable, or a
///   written field would overflow 32 bits;
/// - `KELD-PACK-008` when it already carries a `.keldeai` section;
/// - `KELD-PACK-009` when its Certificate Table entry is non-zero or bytes follow its
///   last section's raw data;
/// - `KELD-PACK-010` when it has 96 sections, fewer than 40 zero bytes after its
///   section table, or a bound-import directory;
/// - `KELD-PACK-011` when the read-back of the output does not return `payload`, which
///   is a writer defect.
pub fn embed_host_identity(
    host: &[u8],
    payload: &ExpectedAppIdentityPayload,
) -> Result<Vec<u8>, PackError> {
    // Step 1: structure, then every value the writer will store must fit its field.
    let layout = admit_structure(&Bytes(host))?;
    let payload_bytes = payload.encode();
    let length = u64::try_from(payload_bytes.len()).unwrap_or(u64::MAX);
    let raw_size = round_up(length, layout.file_alignment);
    let size_of_image = round_up(
        u64::from(layout.size_of_image).saturating_add(length),
        layout.section_alignment,
    );
    let initialized_data = u64::from(layout.size_of_initialized_data).saturating_add(raw_size);
    let (
        Ok(virtual_size),
        Ok(raw_size_field),
        Ok(raw_pointer),
        Ok(size_of_image),
        Ok(initialized_data),
    ) = (
        u32::try_from(length),
        u32::try_from(raw_size),
        u32::try_from(layout.raw_end),
        u32::try_from(size_of_image),
        u32::try_from(initialized_data),
    )
    else {
        return Err(invalid("a written header field would overflow 32 bits"));
    };

    // Step 2: exactly once.
    if layout
        .sections()
        .iter()
        .any(|section| section.name == SECTION_NAME)
    {
        return Err(PackError::IdentityContainerDuplicate);
    }

    // Step 3: before signing. The writer never strips or repairs a signature.
    if layout.certificate != (0, 0) {
        return Err(not_pristine("the Certificate Table entry is present"));
    }
    if layout.len != layout.raw_end {
        return Err(not_pristine("bytes follow the last section's raw data"));
    }

    // Step 4: room. Step 1 already holds every non-empty raw range at or after
    // SizeOfHeaders, so slack that ends by SizeOfHeaders also ends before the lowest raw
    // data. SizeOfHeaders never grows: that would move every section's raw data.
    if layout.count >= LOADER_MAX_SECTIONS {
        return Err(no_room(
            "the section table already holds the loader maximum of 96 sections",
        ));
    }
    let slack_start = layout.table_end();
    let slack_end = slack_start + SECTION_HEADER_BYTES as u64;
    if slack_end > u64::from(layout.size_of_headers) {
        return Err(no_room(
            "fewer than 40 bytes lie between the section table and SizeOfHeaders",
        ));
    }
    let slack = host
        .get(to_index(slack_start)..to_index(slack_end))
        .unwrap_or_default();
    if slack.len() != SECTION_HEADER_BYTES || slack.iter().any(|&byte| byte != 0) {
        return Err(no_room("the 40 bytes after the section table are not zero"));
    }
    if layout.bound_import {
        return Err(no_room("the bound-import directory is present"));
    }

    // Output: the admitted input, six changed ranges, then the appended raw data.
    let sections = u16::try_from(layout.count + 1).unwrap_or(u16::MAX);
    let optional = layout.nt + COFF_END as u64;
    let header = section_header(
        virtual_size,
        layout.size_of_image,
        raw_size_field,
        raw_pointer,
    );
    let total = host.len().saturating_add(to_index(raw_size));
    let mut image = Vec::with_capacity(total);
    image.extend_from_slice(host);
    patch(
        &mut image,
        layout.nt + NUMBER_OF_SECTIONS as u64,
        &sections.to_le_bytes(),
    );
    patch(
        &mut image,
        optional + SIZE_OF_INITIALIZED_DATA as u64,
        &initialized_data.to_le_bytes(),
    );
    patch(
        &mut image,
        optional + SIZE_OF_IMAGE as u64,
        &size_of_image.to_le_bytes(),
    );
    patch(&mut image, optional + CHECKSUM as u64, &0_u32.to_le_bytes());
    patch(&mut image, slack_start, &header);
    image.extend_from_slice(&payload_bytes);
    image.resize(total, 0);

    match read_container(&Bytes(&image)) {
        Ok(read_back) if read_back == *payload => Ok(image),
        _ => Err(container_invalid(
            "the writer's read-back did not return the embedded payload",
        )),
    }
}

/// Reads the single expected-identity container from an open image using only
/// positioned reads on `image` and its handle-derived length.
///
/// The reader neither depends on nor restores the handle's cursor; later users of the
/// same handle must not rely on it. It opens no path and calls no loader or resource
/// API.
///
/// # Errors
/// `KELD-PACK-006` for an inadmissible image, `KELD-PACK-007` when no `.keldeai`
/// section exists, `KELD-PACK-008` when more than one exists, `KELD-PACK-011` when the
/// one container is not canonical, `KELD-PACK-005` when its payload is not canonical,
/// and `KELD-PACK-012` when the handle's length or a positioned read fails.
#[cfg(windows)]
pub fn read_host_identity(image: &std::fs::File) -> Result<ExpectedAppIdentityPayload, PackError> {
    read_container(&Handle(image))
}

/// The same parser over in-memory bytes: the writer's read-back, the `keld build`
/// post-sign check, the fuzz target and tests. Compiled on every target.
///
/// # Errors
/// As `read_host_identity`, except that in-memory reads cannot fail with
/// `KELD-PACK-012`.
pub fn read_host_identity_bytes(image: &[u8]) -> Result<ExpectedAppIdentityPayload, PackError> {
    read_container(&Bytes(image))
}

/// Positioned, cursor-independent reads over one image.
trait ImageSource {
    /// Image length in bytes.
    fn image_len(&self) -> Result<u64, PackError>;
    /// Fills all of `buf` from `offset`; `Ok(false)` when the range passes the end.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<bool, PackError>;
}

/// An in-memory image.
struct Bytes<'a>(&'a [u8]);

impl ImageSource for Bytes<'_> {
    fn image_len(&self) -> Result<u64, PackError> {
        Ok(u64::try_from(self.0.len()).unwrap_or(u64::MAX))
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<bool, PackError> {
        let Ok(start) = usize::try_from(offset) else {
            return Ok(false);
        };
        let Some(source) = start
            .checked_add(buf.len())
            .and_then(|end| self.0.get(start..end))
        else {
            return Ok(false);
        };
        buf.copy_from_slice(source);
        Ok(true)
    }
}

/// An open image handle on Windows.
#[cfg(windows)]
struct Handle<'a>(&'a std::fs::File);

#[cfg(windows)]
impl ImageSource for Handle<'_> {
    fn image_len(&self) -> Result<u64, PackError> {
        self.0
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|source| PackError::IdentityContainerRead { source })
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<bool, PackError> {
        use std::os::windows::fs::FileExt;
        // `seek_read` may return a short read, so loop; zero bytes is end of file, which
        // is a range beyond the file.
        let mut filled = 0;
        while let Some(rest) = buf.get_mut(filled..).filter(|rest| !rest.is_empty()) {
            let Some(at) = u64::try_from(filled)
                .ok()
                .and_then(|done| offset.checked_add(done))
            else {
                return Ok(false);
            };
            match self.0.seek_read(rest, at) {
                Ok(0) => return Ok(false),
                Ok(read) => filled += read,
                Err(source) => return Err(PackError::IdentityContainerRead { source }),
            }
        }
        Ok(true)
    }
}

/// One decoded section header.
#[derive(Debug, Clone, Copy)]
struct Section {
    name: [u8; 8],
    virtual_size: u32,
    virtual_address: u32,
    raw_size: u32,
    raw_pointer: u32,
    relocations_pointer: u32,
    line_numbers_pointer: u32,
    relocation_count: u16,
    line_number_count: u16,
    characteristics: u32,
}

impl Section {
    const ZERO: Self = Self {
        name: [0; 8],
        virtual_size: 0,
        virtual_address: 0,
        raw_size: 0,
        raw_pointer: 0,
        relocations_pointer: 0,
        line_numbers_pointer: 0,
        relocation_count: 0,
        line_number_count: 0,
        characteristics: 0,
    };

    fn parse(header: &[u8]) -> Self {
        let mut name = [0; 8];
        for (slot, byte) in name.iter_mut().zip(header) {
            *slot = *byte;
        }
        Self {
            name,
            virtual_size: le32(header, 8),
            virtual_address: le32(header, 12),
            raw_size: le32(header, 16),
            raw_pointer: le32(header, 20),
            relocations_pointer: le32(header, 24),
            line_numbers_pointer: le32(header, 28),
            relocation_count: le16(header, 32),
            line_number_count: le16(header, 34),
            characteristics: le32(header, 36),
        }
    }

    /// A section whose `SizeOfRawData` and `PointerToRawData` are both zero holds only
    /// uninitialized data: it has no raw range and the image hash skips it.
    const fn has_raw_range(&self) -> bool {
        self.raw_size != 0 || self.raw_pointer != 0
    }

    fn raw_end(&self) -> u64 {
        u64::from(self.raw_pointer) + u64::from(self.raw_size)
    }
}

/// The admitted structure shared by the writer and both readers.
#[derive(Debug)]
struct Layout {
    len: u64,
    /// Offset of the PE signature (`e_lfanew`).
    nt: u64,
    count: usize,
    sections: [Section; LOADER_MAX_SECTIONS],
    file_alignment: u32,
    section_alignment: u32,
    size_of_headers: u32,
    size_of_image: u32,
    size_of_initialized_data: u32,
    /// Certificate Table entry: file offset and size.
    certificate: (u32, u32),
    bound_import: bool,
    /// Raw end E: the larger of `SizeOfHeaders` and every non-empty raw range's end.
    raw_end: u64,
}

impl Layout {
    fn sections(&self) -> &[Section] {
        self.sections.get(..self.count).unwrap_or_default()
    }

    fn table_end(&self) -> u64 {
        self.nt
            + (COFF_END + OPTIONAL_HEADER_BYTES) as u64
            + (self.count * SECTION_HEADER_BYTES) as u64
    }
}

/// Writer step 1 and reader step 1: admits the PE32+ AMD64 executable structure
/// (`KELD-PACK-006`). It reads at most the DOS header, the 264 bytes of PE signature,
/// COFF and optional headers, and a section table of at most 96 entries. All arithmetic
/// is in `u64`.
fn admit_structure(source: &impl ImageSource) -> Result<Layout, PackError> {
    let len = source.image_len()?;
    let (nt, count, optional) = admit_headers(source, len)?;
    let file_alignment = le32(&optional, FILE_ALIGNMENT);
    let section_alignment = le32(&optional, SECTION_ALIGNMENT);
    let size_of_headers = le32(&optional, SIZE_OF_HEADERS);
    let size_of_image = le32(&optional, SIZE_OF_IMAGE);

    let table = nt + (COFF_END + OPTIONAL_HEADER_BYTES) as u64;
    let table_bytes = count * SECTION_HEADER_BYTES;
    if table + table_bytes as u64 > u64::from(size_of_headers) {
        return Err(invalid("section table ends after SizeOfHeaders"));
    }
    let mut raw_table = [0_u8; SECTION_TABLE_MAX_BYTES];
    let (raw_table, _) = raw_table.split_at_mut(table_bytes);
    read_exact(
        source,
        table,
        raw_table,
        invalid("section table lies beyond the end of the image"),
    )?;
    let mut sections = [Section::ZERO; LOADER_MAX_SECTIONS];
    for (slot, header) in sections
        .iter_mut()
        .zip(raw_table.as_chunks::<SECTION_HEADER_BYTES>().0.iter())
    {
        *slot = Section::parse(header);
    }
    let admitted = sections.get(..count).unwrap_or_default();
    let raw_end = admit_raw_ranges(admitted, size_of_headers, file_alignment, len)?;
    admit_virtual_layout(admitted, section_alignment, size_of_image)?;

    Ok(Layout {
        len,
        nt,
        count,
        sections,
        file_alignment,
        section_alignment,
        size_of_headers,
        size_of_image,
        size_of_initialized_data: le32(&optional, SIZE_OF_INITIALIZED_DATA),
        certificate: (
            le32(&optional, CERTIFICATE_TABLE),
            le32(&optional, CERTIFICATE_TABLE + 4),
        ),
        bound_import: optional
            .get(BOUND_IMPORT..BOUND_IMPORT + 8)
            .is_some_and(|entry| entry.iter().any(|&byte| byte != 0)),
        raw_end,
    })
}

/// DOS header, PE signature, COFF header and PE32+ optional header. Returns the PE
/// signature offset, the admitted section count and the optional header.
fn admit_headers(
    source: &impl ImageSource,
    len: u64,
) -> Result<(u64, usize, [u8; OPTIONAL_HEADER_BYTES]), PackError> {
    let mut dos = [0_u8; DOS_HEADER_BYTES];
    read_exact(
        source,
        0,
        &mut dos,
        invalid("image is shorter than the DOS header"),
    )?;
    if dos.get(..2) != Some(b"MZ".as_slice()) {
        return Err(invalid("missing MZ signature"));
    }
    let nt = u64::from(le32(&dos, E_LFANEW));
    if nt < DOS_HEADER_BYTES as u64 {
        return Err(invalid("e_lfanew points inside the DOS header"));
    }

    let mut coff = [0_u8; COFF_END];
    read_exact(
        source,
        nt,
        &mut coff,
        invalid("PE signature or COFF header lies beyond the end of the image"),
    )?;
    if coff.get(..4) != Some(b"PE\0\0".as_slice()) {
        return Err(invalid("missing PE signature"));
    }
    if le16(&coff, MACHINE) != MACHINE_AMD64 {
        return Err(invalid("machine is not AMD64"));
    }
    let characteristics = le16(&coff, CHARACTERISTICS);
    if characteristics & FILE_EXECUTABLE_IMAGE == 0 || characteristics & FILE_DLL != 0 {
        return Err(invalid("image is not an executable image or is a DLL"));
    }
    if usize::from(le16(&coff, SIZE_OF_OPTIONAL_HEADER)) != OPTIONAL_HEADER_BYTES {
        return Err(invalid("SizeOfOptionalHeader is not 240"));
    }

    let mut optional = [0_u8; OPTIONAL_HEADER_BYTES];
    read_exact(
        source,
        nt + COFF_END as u64,
        &mut optional,
        invalid("optional header lies beyond the end of the image"),
    )?;
    if le16(&optional, MAGIC) != PE32_PLUS_MAGIC {
        return Err(invalid("optional header is not PE32+"));
    }
    if le32(&optional, NUMBER_OF_RVA_AND_SIZES) != DATA_DIRECTORY_COUNT {
        return Err(invalid("NumberOfRvaAndSizes is not 16"));
    }
    let file_alignment = le32(&optional, FILE_ALIGNMENT);
    if !file_alignment.is_power_of_two()
        || !(MIN_FILE_ALIGNMENT..=MAX_FILE_ALIGNMENT).contains(&file_alignment)
    {
        return Err(invalid(
            "FileAlignment is not a power of two from 512 to 65536",
        ));
    }
    let section_alignment = le32(&optional, SECTION_ALIGNMENT);
    if !section_alignment.is_power_of_two()
        || section_alignment < MIN_SECTION_ALIGNMENT
        || section_alignment < file_alignment
    {
        return Err(invalid(
            "SectionAlignment is not a power of two of at least 4096 and FileAlignment",
        ));
    }
    let size_of_headers = le32(&optional, SIZE_OF_HEADERS);
    if !size_of_headers.is_multiple_of(file_alignment) || u64::from(size_of_headers) > len {
        return Err(invalid(
            "SizeOfHeaders is not FileAlignment-aligned inside the image",
        ));
    }
    let count = usize::from(le16(&coff, NUMBER_OF_SECTIONS));
    if count == 0 || count > LOADER_MAX_SECTIONS {
        return Err(invalid("NumberOfSections is not 1 to 96"));
    }
    Ok((nt, count, optional))
}

/// Raw-range rules for every non-empty section; returns the raw end E. A section with
/// both raw fields zero holds only uninitialized data, has no raw range, never
/// contributes to E and is never read.
fn admit_raw_ranges(
    sections: &[Section],
    size_of_headers: u32,
    file_alignment: u32,
    len: u64,
) -> Result<u64, PackError> {
    let mut raw_end = u64::from(size_of_headers);
    for (index, section) in sections.iter().enumerate() {
        if !section.has_raw_range() {
            continue;
        }
        if section.raw_size == 0 || section.raw_pointer == 0 {
            return Err(invalid(
                "section has exactly one of SizeOfRawData and PointerToRawData zero",
            ));
        }
        if !section.raw_pointer.is_multiple_of(file_alignment)
            || !section.raw_size.is_multiple_of(file_alignment)
        {
            return Err(invalid("section raw data is not FileAlignment-aligned"));
        }
        if section.raw_pointer < size_of_headers {
            return Err(invalid("section raw data starts before SizeOfHeaders"));
        }
        if section.raw_end() > len {
            return Err(invalid("section raw data lies beyond the end of the image"));
        }
        let start = u64::from(section.raw_pointer);
        let overlaps = sections
            .get(..index)
            .unwrap_or_default()
            .iter()
            .filter(|earlier| earlier.has_raw_range())
            .any(|earlier| {
                start < earlier.raw_end() && u64::from(earlier.raw_pointer) < section.raw_end()
            });
        if overlaps {
            return Err(invalid("section raw ranges overlap"));
        }
        raw_end = raw_end.max(section.raw_end());
    }
    Ok(raw_end)
}

/// "the VAs for sections must be assigned by the linker so that they are in ascending
/// order and adjacent, and they must be a multiple of the `SectionAlignment` value", and
/// `SizeOfImage` is the last section's end rounded up. A zero `VirtualSize` occupies no
/// address range, so it cannot be ascending.
fn admit_virtual_layout(
    sections: &[Section],
    section_alignment: u32,
    size_of_image: u32,
) -> Result<(), PackError> {
    let mut virtual_end = None;
    for section in sections {
        if section.virtual_size == 0 {
            return Err(invalid("section VirtualSize is zero"));
        }
        if !section.virtual_address.is_multiple_of(section_alignment) {
            return Err(invalid(
                "section VirtualAddress is not SectionAlignment-aligned",
            ));
        }
        if virtual_end.is_some_and(|end| end != u64::from(section.virtual_address)) {
            return Err(invalid(
                "section virtual addresses are not ascending and adjacent",
            ));
        }
        virtual_end = Some(round_up(
            u64::from(section.virtual_address) + u64::from(section.virtual_size),
            section_alignment,
        ));
    }
    if virtual_end != Some(u64::from(size_of_image)) {
        return Err(invalid(
            "SizeOfImage is not the last section's virtual end rounded up to SectionAlignment",
        ));
    }
    Ok(())
}

/// Reader steps 1 to 5 over any positioned-read source.
fn read_container(source: &impl ImageSource) -> Result<ExpectedAppIdentityPayload, PackError> {
    // Step 1.
    let layout = admit_structure(source)?;
    let sections = layout.sections();

    // Step 2: count by the exact 8 name bytes.
    let mut named = sections
        .iter()
        .enumerate()
        .filter(|(_, section)| section.name == SECTION_NAME);
    let Some((index, container)) = named.next() else {
        return Err(PackError::IdentityContainerMissing);
    };
    if named.next().is_some() {
        return Err(PackError::IdentityContainerDuplicate);
    }

    // Step 3: the canonical v1 form. Step 1 already holds the container's raw range
    // FileAlignment-aligned, at or after SizeOfHeaders and inside the file, its
    // VirtualAddress adjacent to the preceding section's rounded end, and SizeOfImage
    // equal to its own rounded end.
    if index + 1 != sections.len() {
        return Err(container_invalid(
            "the container is not the last section header",
        ));
    }
    if index == 0 {
        return Err(container_invalid("the container is the only section"));
    }
    if container.characteristics != CONTAINER_CHARACTERISTICS {
        return Err(container_invalid(
            "the container Characteristics are not 0x40000040",
        ));
    }
    if container.relocations_pointer != 0
        || container.line_numbers_pointer != 0
        || container.relocation_count != 0
        || container.line_number_count != 0
    {
        return Err(container_invalid(
            "the container relocation or line-number fields are not zero",
        ));
    }
    let length = usize::try_from(container.virtual_size).unwrap_or(usize::MAX);
    if !(MIN_PAYLOAD_BYTES..=MAX_PAYLOAD_BYTES).contains(&length) {
        return Err(container_invalid(
            "the container VirtualSize is outside the payload length bounds",
        ));
    }
    if u64::from(container.raw_size)
        != round_up(u64::from(container.virtual_size), layout.file_alignment)
    {
        return Err(container_invalid(
            "the container SizeOfRawData is not VirtualSize rounded up to FileAlignment",
        ));
    }
    let raw_start = u64::from(container.raw_pointer);
    let raw_end = container.raw_end();
    if sections
        .get(..index)
        .unwrap_or_default()
        .iter()
        .any(|earlier| earlier.raw_end() > raw_start)
    {
        return Err(container_invalid(
            "the container raw data is not the last raw data in the image",
        ));
    }
    let (certificate_start, certificate_size) = layout.certificate;
    if (certificate_start, certificate_size) != (0, 0)
        && (u64::from(certificate_start) < raw_end
            || u64::from(certificate_start) + u64::from(certificate_size) > layout.len)
    {
        return Err(container_invalid(
            "the certificate table overlaps the container or ends beyond the image",
        ));
    }

    // Step 4: the payload, then the padding (fewer than 65536 bytes) in fixed chunks.
    let mut payload = [0_u8; MAX_PAYLOAD_BYTES];
    let (payload, _) = payload.split_at_mut(length);
    read_exact(
        source,
        raw_start,
        payload,
        container_invalid("the container raw data ends beyond the image"),
    )?;
    let mut chunk = [0_u8; PADDING_CHUNK_BYTES];
    let mut offset = raw_start + length as u64;
    while offset < raw_end {
        let step = usize::try_from(raw_end - offset)
            .unwrap_or(PADDING_CHUNK_BYTES)
            .min(PADDING_CHUNK_BYTES);
        let (padding, _) = chunk.split_at_mut(step);
        read_exact(
            source,
            offset,
            padding,
            container_invalid("the container raw data ends beyond the image"),
        )?;
        if padding.iter().any(|&byte| byte != 0) {
            return Err(container_invalid("the container padding is not zero"));
        }
        offset += step as u64;
    }

    // Step 5: the codec owns every payload byte.
    ExpectedAppIdentityPayload::decode(payload)
}

fn read_exact(
    source: &impl ImageSource,
    offset: u64,
    buf: &mut [u8],
    beyond: PackError,
) -> Result<(), PackError> {
    if source.read_at(offset, buf)? {
        Ok(())
    } else {
        Err(beyond)
    }
}

/// The container's 40-byte section header (container format v1).
fn section_header(
    virtual_size: u32,
    virtual_address: u32,
    raw_size: u32,
    raw_pointer: u32,
) -> [u8; SECTION_HEADER_BYTES] {
    let mut header = [0_u8; SECTION_HEADER_BYTES];
    patch(&mut header, 0, &SECTION_NAME);
    patch(&mut header, 8, &virtual_size.to_le_bytes());
    patch(&mut header, 12, &virtual_address.to_le_bytes());
    patch(&mut header, 16, &raw_size.to_le_bytes());
    patch(&mut header, 20, &raw_pointer.to_le_bytes());
    patch(&mut header, 36, &CONTAINER_CHARACTERISTICS.to_le_bytes());
    header
}

/// Overwrites bytes at `offset`. A range outside `image` changes nothing, and the
/// writer's read-back then refuses the output.
fn patch(image: &mut [u8], offset: u64, bytes: &[u8]) {
    for (slot, byte) in image.iter_mut().skip(to_index(offset)).zip(bytes) {
        *slot = *byte;
    }
}

fn to_index(offset: u64) -> usize {
    usize::try_from(offset).unwrap_or(usize::MAX)
}

fn round_up(value: u64, alignment: u32) -> u64 {
    value
        .checked_next_multiple_of(u64::from(alignment))
        .unwrap_or(u64::MAX)
}

fn le16(bytes: &[u8], offset: usize) -> u16 {
    let mut word = [0; 2];
    for (slot, byte) in word.iter_mut().zip(bytes.iter().skip(offset)) {
        *slot = *byte;
    }
    u16::from_le_bytes(word)
}

fn le32(bytes: &[u8], offset: usize) -> u32 {
    let mut word = [0; 4];
    for (slot, byte) in word.iter_mut().zip(bytes.iter().skip(offset)) {
        *slot = *byte;
    }
    u32::from_le_bytes(word)
}

const fn invalid(detail: &'static str) -> PackError {
    PackError::HostImageInvalid { detail }
}

const fn not_pristine(detail: &'static str) -> PackError {
    PackError::HostImageNotPristine { detail }
}

const fn no_room(detail: &'static str) -> PackError {
    PackError::HostImageNoRoom { detail }
}

const fn container_invalid(detail: &'static str) -> PackError {
    PackError::IdentityContainerInvalid { detail }
}

#[cfg(test)]
mod tests;
