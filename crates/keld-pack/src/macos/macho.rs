//! Shared fail-closed Mach-O shape validator for signed macOS packaging and
//! installed Host admission. It reads only already-open executable handles.
//! This is not a signature authority: the producer and Core independently
//! retain their existing code-signature, provenance, and digest checks.

use std::fmt;
use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt as _;

use super::capture::Identity;
use crate::MacosBundleMetadata;

/// A bounded Mach-O shape validation failure, not publisher authorization.
#[derive(Debug)]
pub struct MacosMachoError {
    kind: &'static str,
    detail: String,
}

impl fmt::Display for MacosMachoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for MacosMachoError {}

fn macho_error(kind: &'static str, detail: impl fmt::Display) -> MacosMachoError {
    MacosMachoError {
        kind,
        detail: detail.to_string(),
    }
}

fn read_identity(file: &File, kind: &'static str) -> Result<(Identity, u64), MacosMachoError> {
    let metadata = file.metadata().map_err(|error| macho_error(kind, error))?;
    if !metadata.is_file() {
        return Err(macho_error(
            kind,
            "Mach-O input must be a retained regular file",
        ));
    }
    Ok((Identity::of(&metadata), metadata.len()))
}

const MACHO_HEADER_64: usize = 32;
const FAT_HEADER: u64 = 8;
const FAT_ARCH_32: usize = 20;
const FAT_ARCH_64: usize = 32;
const MAX_SLICES: usize = 2;
const CPU_TYPE_ARM64: u32 = 0x0100_000c;
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const CPU_SUBTYPE_LIB64: u32 = 0x8000_0000;
const MAX_LOAD_COMMANDS: u32 = 4096;
const MAX_LOAD_COMMAND_BYTES: u32 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Architecture {
    Arm64,
    X86_64,
}

impl Architecture {
    const fn name(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::X86_64 => "x86_64",
        }
    }

    fn from_macho(cpu: u32, subtype: u32) -> Option<Self> {
        let capability = subtype & 0xff00_0000;
        if capability != 0 && capability != CPU_SUBTYPE_LIB64 {
            return None;
        }
        match (cpu, subtype & 0x00ff_ffff) {
            (CPU_TYPE_ARM64, 0) => Some(Self::Arm64),
            (CPU_TYPE_X86_64, 3) => Some(Self::X86_64),
            _ => None, // Reject arm64e, x86_64h, 32-bit and unknown CPU subtypes.
        }
    }
}

#[derive(Clone, Copy)]
enum Endian {
    Big,
    Little,
}

fn u32_at(bytes: &[u8], offset: usize, endian: Endian) -> Option<u32> {
    let chunk: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(match endian {
        Endian::Big => u32::from_be_bytes(chunk),
        Endian::Little => u32::from_le_bytes(chunk),
    })
}

fn u64_at(bytes: &[u8], offset: usize, endian: Endian) -> Option<u64> {
    let chunk: [u8; 8] = bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?;
    Some(match endian {
        Endian::Big => u64::from_be_bytes(chunk),
        Endian::Little => u64::from_le_bytes(chunk),
    })
}

fn read_exact_at(
    file: &File,
    file_len: u64,
    offset: u64,
    output: &mut [u8],
    kind: &'static str,
) -> Result<(), MacosMachoError> {
    let length = u64::try_from(output.len()).map_err(|error| macho_error(kind, error))?;
    let end = offset
        .checked_add(length)
        .filter(|end| *end <= file_len)
        .ok_or_else(|| macho_error(kind, "Mach-O header or slice exceeds the retained file"))?;
    let mut cursor = offset;
    while cursor < end {
        let written = usize::try_from(cursor - offset).map_err(|error| macho_error(kind, error))?;
        match file.read_at(&mut output[written..], cursor) {
            Ok(0) => return Err(macho_error(kind, "Mach-O file shrank while reading")),
            Ok(bytes) => {
                cursor += u64::try_from(bytes).map_err(|error| macho_error(kind, error))?;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(macho_error(kind, error)),
        }
    }
    Ok(())
}

/// Check every generic load-command header through the retained executable.
/// The command table has one bounded byte range and no unclaimed padding.
/// Specific command payload semantics remain under macOS code-signature and
/// loader validation; this owner checks structural offsets and command sizes.
fn validate_load_command_bounds(
    file: &File,
    file_len: u64,
    slice_offset: u64,
    slice_size: u64,
    command_count: u32,
    command_bytes: u32,
    kind: &'static str,
) -> Result<(), MacosMachoError> {
    let commands_start = slice_offset
        .checked_add(MACHO_HEADER_64 as u64)
        .ok_or_else(|| macho_error(kind, "Mach-O load command offset overflow"))?;
    let commands_end = commands_start
        .checked_add(u64::from(command_bytes))
        .filter(|end| *end <= file_len)
        .filter(|end| *end - slice_offset <= slice_size)
        .ok_or_else(|| macho_error(kind, "Mach-O load commands escape their slice"))?;
    let mut cursor = commands_start;
    for _ in 0..command_count {
        let mut header = [0_u8; 8];
        if cursor.checked_add(8).is_none_or(|end| end > commands_end) {
            return Err(macho_error(
                kind,
                "Mach-O load-command header escapes its declared table",
            ));
        }
        read_exact_at(file, file_len, cursor, &mut header, kind)?;
        let code = u32_at(&header, 0, Endian::Little)
            .ok_or_else(|| macho_error(kind, "malformed Mach-O load-command identifier"))?;
        let size = u32_at(&header, 4, Endian::Little)
            .ok_or_else(|| macho_error(kind, "malformed Mach-O load-command size"))?;
        if code == 0 || size < 8 || size % 8 != 0 {
            return Err(macho_error(
                kind,
                "invalid Mach-O load-command code or byte size",
            ));
        }
        cursor = cursor
            .checked_add(u64::from(size))
            .filter(|end| *end <= commands_end)
            .ok_or_else(|| macho_error(kind, "Mach-O load-command escapes its table"))?;
    }
    if cursor != commands_end {
        return Err(macho_error(
            kind,
            "Mach-O load-command lengths do not sum to sizeofcmds",
        ));
    }
    Ok(())
}

/// The 32-byte 64-bit inner Mach-O header must identify executable code,
/// with load command bytes contained within this exact slice.
fn thin_architecture(
    file: &File,
    file_len: u64,
    offset: u64,
    slice_size: u64,
    kind: &'static str,
) -> Result<(Architecture, u32, u32), MacosMachoError> {
    if slice_size < MACHO_HEADER_64 as u64 {
        return Err(macho_error(kind, "Mach-O 64-bit header is truncated"));
    }
    let mut header = [0_u8; MACHO_HEADER_64];
    read_exact_at(file, file_len, offset, &mut header, kind)?;
    if header[..4] != [0xcf, 0xfa, 0xed, 0xfe] {
        return Err(macho_error(
            kind,
            "expected a little-endian 64-bit Mach-O slice",
        ));
    }
    let read = |position| {
        u32_at(&header, position, Endian::Little)
            .ok_or_else(|| macho_error(kind, "invalid Mach-O header field"))
    };
    let cpu = read(4)?;
    let subtype = read(8)?;
    let arch = Architecture::from_macho(cpu, subtype)
        .ok_or_else(|| macho_error(kind, "unsupported Mach-O CPU type or subtype"))?;
    let file_type = read(12)?;
    let command_count = read(16)?;
    let command_bytes = read(20)?;
    if file_type != 2
        || command_count == 0
        || command_count > MAX_LOAD_COMMANDS
        || command_bytes == 0
        || command_bytes > MAX_LOAD_COMMAND_BYTES
        || command_bytes % 8 != 0
        || u64::from(command_bytes) < u64::from(command_count) * 8
        || u64::from(command_bytes) > slice_size - MACHO_HEADER_64 as u64
        || read(28)? != 0
    {
        return Err(macho_error(
            kind,
            "Mach-O slice is not a bounded 64-bit executable with valid load-command bounds",
        ));
    }
    validate_load_command_bounds(
        file,
        file_len,
        offset,
        slice_size,
        command_count,
        command_bytes,
        kind,
    )?;
    Ok((arch, cpu, subtype))
}

#[derive(Clone, Copy)]
struct FatEntry {
    arch: Architecture,
    cpu: u32,
    subtype: u32,
    start: u64,
    size: u64,
    end: u64,
}

fn fat_architectures(
    file: &File,
    file_len: u64,
    prefix: [u8; 8],
    is_64: bool,
    endian: Endian,
    kind: &'static str,
) -> Result<Vec<Architecture>, MacosMachoError> {
    let count = u32_at(&prefix, 4, endian)
        .ok_or_else(|| macho_error(kind, "truncated fat Mach-O header"))?;
    if usize::try_from(count).ok() != Some(MAX_SLICES) {
        return Err(macho_error(
            kind,
            "fat Mach-O must contain exactly two independently named slices",
        ));
    }
    let entry_size = if is_64 { FAT_ARCH_64 } else { FAT_ARCH_32 };
    let table_end = FAT_HEADER
        + u64::try_from(MAX_SLICES * entry_size).map_err(|error| macho_error(kind, error))?;
    if table_end > file_len {
        return Err(macho_error(
            kind,
            "fat Mach-O architecture table is truncated",
        ));
    }
    let mut entries: Vec<FatEntry> = Vec::with_capacity(MAX_SLICES);
    for index in 0..MAX_SLICES {
        let mut bytes = [0_u8; FAT_ARCH_64];
        let entry_start = FAT_HEADER
            + u64::try_from(index * entry_size).map_err(|error| macho_error(kind, error))?;
        read_exact_at(file, file_len, entry_start, &mut bytes[..entry_size], kind)?;
        let field = |position| {
            u32_at(&bytes, position, endian)
                .ok_or_else(|| macho_error(kind, "invalid fat Mach-O entry field"))
        };
        let cpu = field(0)?;
        let subtype = field(4)?;
        let arch = Architecture::from_macho(cpu, subtype)
            .ok_or_else(|| macho_error(kind, "unsupported fat Mach-O CPU subtype"))?;
        if entries.iter().any(|entry| entry.arch == arch) {
            return Err(macho_error(kind, "duplicate fat Mach-O architecture slice"));
        }
        let (start, size, alignment) = if is_64 {
            if field(28)? != 0 {
                return Err(macho_error(
                    kind,
                    "fat Mach-O 64-bit reserved field is nonzero",
                ));
            }
            (
                u64_at(&bytes, 8, endian)
                    .ok_or_else(|| macho_error(kind, "invalid fat Mach-O offset"))?,
                u64_at(&bytes, 16, endian)
                    .ok_or_else(|| macho_error(kind, "invalid fat Mach-O size"))?,
                field(24)?,
            )
        } else {
            (u64::from(field(8)?), u64::from(field(12)?), field(16)?)
        };
        if alignment > 31 {
            return Err(macho_error(
                kind,
                "fat Mach-O slice alignment exponent exceeds 31",
            ));
        }
        let end = start
            .checked_add(size)
            .filter(|end| *end <= file_len)
            .ok_or_else(|| macho_error(kind, "fat Mach-O slice escapes the retained file"))?;
        if start < table_end || size < MACHO_HEADER_64 as u64 || start % (1_u64 << alignment) != 0 {
            return Err(macho_error(
                kind,
                "fat Mach-O slice overlaps its table or violates length/alignment",
            ));
        }
        entries.push(FatEntry {
            arch,
            cpu,
            subtype,
            start,
            size,
            end,
        });
    }
    if entries[0].start < entries[1].end && entries[1].start < entries[0].end {
        return Err(macho_error(kind, "fat Mach-O executable slices overlap"));
    }
    for entry in &entries {
        let (inner_arch, inner_cpu, inner_subtype) =
            thin_architecture(file, file_len, entry.start, entry.size, kind)?;
        if inner_arch != entry.arch || inner_cpu != entry.cpu || inner_subtype != entry.subtype {
            return Err(macho_error(
                kind,
                "fat Mach-O table architecture disagrees with the inner executable header",
            ));
        }
    }
    Ok(entries.into_iter().map(|entry| entry.arch).collect())
}

fn read_architectures(
    file: &File,
    kind: &'static str,
) -> Result<Vec<Architecture>, MacosMachoError> {
    let (identity, file_length) = read_identity(file, kind)?;
    if file_length < MACHO_HEADER_64 as u64 {
        return Err(macho_error(kind, "retained Mach-O executable is truncated"));
    }
    let mut prefix = [0_u8; 8];
    read_exact_at(file, file_length, 0, &mut prefix, kind)?;
    let arches = match prefix[..4] {
        [0xcf, 0xfa, 0xed, 0xfe] => {
            let (arch, _, _) = thin_architecture(file, file_length, 0, file_length, kind)?;
            vec![arch]
        }
        [0xca, 0xfe, 0xba, 0xbe] => {
            fat_architectures(file, file_length, prefix, false, Endian::Big, kind)?
        }
        [0xbe, 0xba, 0xfe, 0xca] => {
            fat_architectures(file, file_length, prefix, false, Endian::Little, kind)?
        }
        [0xca, 0xfe, 0xba, 0xbf] => {
            fat_architectures(file, file_length, prefix, true, Endian::Big, kind)?
        }
        [0xbf, 0xba, 0xfe, 0xca] => {
            fat_architectures(file, file_length, prefix, true, Endian::Little, kind)?
        }
        _ => {
            return Err(macho_error(
                kind,
                "unsupported Mach-O header magic or endianness",
            ));
        }
    };
    if read_identity(file, kind)?.0 != identity {
        return Err(macho_error(
            kind,
            "retained Mach-O executable changed during admission",
        ));
    }
    Ok(arches)
}

/// Read the exact bounded Mach-O slice names from a retained executable.
/// The same structural scanner is used by Pack and by installed Core.
///
/// # Errors
/// Refuses invalid, oversized, unsupported or mutated executable shapes.
pub fn read_macos_macho_architectures(file: &File) -> Result<Vec<&'static str>, MacosMachoError> {
    let arches = read_architectures(file, "retained Mach-O")?;
    Ok(arches.into_iter().map(Architecture::name).collect())
}

/// Verify that one retained executable contains exactly the slices named by
/// its closed, sealed bundle target. No ambient path or platform tool is used.
///
/// # Errors
/// Refuses unknown target, any missing/extra slice or malformed Mach-O bytes.
pub fn validate_macos_macho_target(
    file: &File,
    metadata: &MacosBundleMetadata,
) -> Result<(), MacosMachoError> {
    let required = metadata
        .target_architectures()
        .map_err(|error| macho_error("closed target", error))?;
    let present = read_macos_macho_architectures(file)?;
    if present.len() != required.len()
        || required.iter().any(|expected| !present.contains(expected))
    {
        return Err(macho_error(
            "closed target",
            "retained executable Mach-O slices differ from the sealed target",
        ));
    }
    Ok(())
}
