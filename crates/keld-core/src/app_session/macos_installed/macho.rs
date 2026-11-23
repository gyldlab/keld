//! Installed Mach-O target validation on the already-retained Host and Bun files.
//! Apple Security.framework independently checks all code signatures and nested
//! code before this closed target-vs-slice consistency check.

use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt as _;

use super::{FileIdentity, HostAppError, MacosBundleMetadata, admission_error};

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
) -> Result<(), HostAppError> {
    let length = u64::try_from(output.len()).map_err(|error| admission_error(kind, error))?;
    let end = offset
        .checked_add(length)
        .filter(|end| *end <= file_len)
        .ok_or_else(|| admission_error(kind, "Mach-O header or slice exceeds the retained file"))?;
    let mut cursor = offset;
    while cursor < end {
        let written =
            usize::try_from(cursor - offset).map_err(|error| admission_error(kind, error))?;
        match file.read_at(&mut output[written..], cursor) {
            Ok(0) => return Err(admission_error(kind, "Mach-O file shrank while reading")),
            Ok(bytes) => {
                cursor += u64::try_from(bytes).map_err(|error| admission_error(kind, error))?;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(admission_error(kind, error)),
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
) -> Result<(), HostAppError> {
    let commands_start = slice_offset
        .checked_add(MACHO_HEADER_64 as u64)
        .ok_or_else(|| admission_error(kind, "Mach-O load command offset overflow"))?;
    let commands_end = commands_start
        .checked_add(u64::from(command_bytes))
        .filter(|end| *end <= file_len)
        .filter(|end| *end - slice_offset <= slice_size)
        .ok_or_else(|| admission_error(kind, "Mach-O load commands escape their slice"))?;
    let mut cursor = commands_start;
    for _ in 0..command_count {
        let mut header = [0_u8; 8];
        if cursor.checked_add(8).is_none_or(|end| end > commands_end) {
            return Err(admission_error(
                kind,
                "Mach-O load-command header escapes its declared table",
            ));
        }
        read_exact_at(file, file_len, cursor, &mut header, kind)?;
        let code = u32_at(&header, 0, Endian::Little)
            .ok_or_else(|| admission_error(kind, "malformed Mach-O load-command identifier"))?;
        let size = u32_at(&header, 4, Endian::Little)
            .ok_or_else(|| admission_error(kind, "malformed Mach-O load-command size"))?;
        if code == 0 || size < 8 || size % 8 != 0 {
            return Err(admission_error(
                kind,
                "invalid Mach-O load-command code or byte size",
            ));
        }
        cursor = cursor
            .checked_add(u64::from(size))
            .filter(|end| *end <= commands_end)
            .ok_or_else(|| admission_error(kind, "Mach-O load-command escapes its table"))?;
    }
    if cursor != commands_end {
        return Err(admission_error(
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
) -> Result<(Architecture, u32, u32), HostAppError> {
    if slice_size < MACHO_HEADER_64 as u64 {
        return Err(admission_error(kind, "Mach-O 64-bit header is truncated"));
    }
    let mut header = [0_u8; MACHO_HEADER_64];
    read_exact_at(file, file_len, offset, &mut header, kind)?;
    if header[..4] != [0xcf, 0xfa, 0xed, 0xfe] {
        return Err(admission_error(
            kind,
            "expected a little-endian 64-bit Mach-O slice",
        ));
    }
    let read = |position| {
        u32_at(&header, position, Endian::Little)
            .ok_or_else(|| admission_error(kind, "invalid Mach-O header field"))
    };
    let cpu = read(4)?;
    let subtype = read(8)?;
    let arch = Architecture::from_macho(cpu, subtype)
        .ok_or_else(|| admission_error(kind, "unsupported Mach-O CPU type or subtype"))?;
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
        return Err(admission_error(
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
) -> Result<Vec<Architecture>, HostAppError> {
    let count = u32_at(&prefix, 4, endian)
        .ok_or_else(|| admission_error(kind, "truncated fat Mach-O header"))?;
    if usize::try_from(count).ok() != Some(MAX_SLICES) {
        return Err(admission_error(
            kind,
            "fat Mach-O must contain exactly two independently named slices",
        ));
    }
    let entry_size = if is_64 { FAT_ARCH_64 } else { FAT_ARCH_32 };
    let table_end = FAT_HEADER
        + u64::try_from(MAX_SLICES * entry_size).map_err(|error| admission_error(kind, error))?;
    if table_end > file_len {
        return Err(admission_error(
            kind,
            "fat Mach-O architecture table is truncated",
        ));
    }
    let mut entries: Vec<FatEntry> = Vec::with_capacity(MAX_SLICES);
    for index in 0..MAX_SLICES {
        let mut bytes = [0_u8; FAT_ARCH_64];
        let entry_start = FAT_HEADER
            + u64::try_from(index * entry_size).map_err(|error| admission_error(kind, error))?;
        read_exact_at(file, file_len, entry_start, &mut bytes[..entry_size], kind)?;
        let field = |position| {
            u32_at(&bytes, position, endian)
                .ok_or_else(|| admission_error(kind, "invalid fat Mach-O entry field"))
        };
        let cpu = field(0)?;
        let subtype = field(4)?;
        let arch = Architecture::from_macho(cpu, subtype)
            .ok_or_else(|| admission_error(kind, "unsupported fat Mach-O CPU subtype"))?;
        if entries.iter().any(|entry| entry.arch == arch) {
            return Err(admission_error(
                kind,
                "duplicate fat Mach-O architecture slice",
            ));
        }
        let (start, size, alignment) = if is_64 {
            if field(28)? != 0 {
                return Err(admission_error(
                    kind,
                    "fat Mach-O 64-bit reserved field is nonzero",
                ));
            }
            (
                u64_at(&bytes, 8, endian)
                    .ok_or_else(|| admission_error(kind, "invalid fat Mach-O offset"))?,
                u64_at(&bytes, 16, endian)
                    .ok_or_else(|| admission_error(kind, "invalid fat Mach-O size"))?,
                field(24)?,
            )
        } else {
            (u64::from(field(8)?), u64::from(field(12)?), field(16)?)
        };
        if alignment > 31 {
            return Err(admission_error(
                kind,
                "fat Mach-O slice alignment exponent exceeds 31",
            ));
        }
        let end = start
            .checked_add(size)
            .filter(|end| *end <= file_len)
            .ok_or_else(|| admission_error(kind, "fat Mach-O slice escapes the retained file"))?;
        if start < table_end || size < MACHO_HEADER_64 as u64 || start % (1_u64 << alignment) != 0 {
            return Err(admission_error(
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
        return Err(admission_error(
            kind,
            "fat Mach-O executable slices overlap",
        ));
    }
    for entry in &entries {
        let (inner_arch, inner_cpu, inner_subtype) =
            thin_architecture(file, file_len, entry.start, entry.size, kind)?;
        if inner_arch != entry.arch || inner_cpu != entry.cpu || inner_subtype != entry.subtype {
            return Err(admission_error(
                kind,
                "fat Mach-O table architecture disagrees with the inner executable header",
            ));
        }
    }
    Ok(entries.into_iter().map(|entry| entry.arch).collect())
}

fn read_architectures(file: &File, kind: &'static str) -> Result<Vec<Architecture>, HostAppError> {
    let identity = FileIdentity::of(file, kind)?;
    if identity.size < MACHO_HEADER_64 as u64 {
        return Err(admission_error(
            kind,
            "retained Mach-O executable is truncated",
        ));
    }
    let mut prefix = [0_u8; 8];
    read_exact_at(file, identity.size, 0, &mut prefix, kind)?;
    let arches = match prefix[..4] {
        [0xcf, 0xfa, 0xed, 0xfe] => {
            let (arch, _, _) = thin_architecture(file, identity.size, 0, identity.size, kind)?;
            vec![arch]
        }
        [0xca, 0xfe, 0xba, 0xbe] => {
            fat_architectures(file, identity.size, prefix, false, Endian::Big, kind)?
        }
        [0xbe, 0xba, 0xfe, 0xca] => {
            fat_architectures(file, identity.size, prefix, false, Endian::Little, kind)?
        }
        [0xca, 0xfe, 0xba, 0xbf] => {
            fat_architectures(file, identity.size, prefix, true, Endian::Big, kind)?
        }
        [0xbf, 0xba, 0xfe, 0xca] => {
            fat_architectures(file, identity.size, prefix, true, Endian::Little, kind)?
        }
        _ => {
            return Err(admission_error(
                kind,
                "unsupported Mach-O header magic or endianness",
            ));
        }
    };
    if FileIdentity::of(file, kind)? != identity {
        return Err(admission_error(
            kind,
            "retained Mach-O executable changed during admission",
        ));
    }
    Ok(arches)
}

/// Reject a signed local installation unless the sealed target describes both
/// retained executable slice sets and the actual compiled Host architecture.
pub(super) fn admit_bundle_architectures(
    host: &File,
    bun: &File,
    metadata: &MacosBundleMetadata,
) -> Result<(), HostAppError> {
    let running = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        return Err(admission_error(
            "installed Mach-O target",
            "this Host process architecture is unsupported",
        ));
    };
    admit_bundle_architectures_for_process(host, bun, metadata, running)
}

fn admit_bundle_architectures_for_process(
    host: &File,
    bun: &File,
    metadata: &MacosBundleMetadata,
    running: &str,
) -> Result<(), HostAppError> {
    let required = metadata
        .target_architectures()
        .map_err(|error| admission_error("installed Mach-O target", error))?;
    if !required.contains(&running) {
        return Err(admission_error(
            "installed Mach-O target",
            "the running Host process architecture is absent from the sealed bundle target",
        ));
    }
    for (file, kind) in [(host, "retained Host Mach-O"), (bun, "retained Bun Mach-O")] {
        let present = read_architectures(file, kind)?;
        if present.len() != required.len()
            || required
                .iter()
                .any(|expected| !present.iter().any(|arch| arch.name() == *expected))
        {
            return Err(admission_error(
                kind,
                "retained executable architecture set differs from sealed Info.plist target",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{Seek as _, SeekFrom, Write as _};

    use super::{MacosBundleMetadata, admit_bundle_architectures_for_process, read_architectures};

    const X64: u32 = 0x0100_0007;
    const ARM64: u32 = 0x0100_000c;

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32, little: bool) {
        let encoded = if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        bytes[offset..offset + 4].copy_from_slice(&encoded);
    }

    fn write_u64(bytes: &mut [u8], offset: usize, value: u64, little: bool) {
        let encoded = if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        bytes[offset..offset + 8].copy_from_slice(&encoded);
    }

    fn thin(cpu: u32, subtype: u32) -> Vec<u8> {
        let mut bytes = vec![0_u8; 64];
        bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]); // MH_MAGIC_64
        write_u32(&mut bytes, 4, cpu, true);
        write_u32(&mut bytes, 8, subtype, true);
        write_u32(&mut bytes, 12, 2, true); // MH_EXECUTE
        write_u32(&mut bytes, 16, 1, true); // one load command
        write_u32(&mut bytes, 20, 24, true); // one LC_BUILD_VERSION
        write_u32(&mut bytes, 32, 0x32, true);
        write_u32(&mut bytes, 36, 24, true);
        write_u32(&mut bytes, 40, 1, true); // PLATFORM_MACOS
        bytes
    }

    /// Two 64-byte Mach-O slices with nonoverlapping, 256-byte-aligned offsets.
    fn fat(is_64: bool, little: bool) -> Vec<u8> {
        let mut bytes = vec![0_u8; 0x240];
        let magic: [u8; 4] = match (is_64, little) {
            (false, false) => [0xca, 0xfe, 0xba, 0xbe],
            (false, true) => [0xbe, 0xba, 0xfe, 0xca],
            (true, false) => [0xca, 0xfe, 0xba, 0xbf],
            (true, true) => [0xbf, 0xba, 0xfe, 0xca],
        };
        bytes[..4].copy_from_slice(&magic);
        write_u32(&mut bytes, 4, 2, little);
        for (index, (cpu, subtype)) in [(X64, 3_u32), (ARM64, 0_u32)].into_iter().enumerate() {
            let start = 8 + index * if is_64 { 32 } else { 20 };
            let slice_offset = (index as u64 + 1) * 0x100;
            write_u32(&mut bytes, start, cpu, little);
            write_u32(&mut bytes, start + 4, subtype, little);
            if is_64 {
                write_u64(&mut bytes, start + 8, slice_offset, little);
                write_u64(&mut bytes, start + 16, 64, little);
                write_u32(&mut bytes, start + 24, 4, little);
                write_u32(&mut bytes, start + 28, 0, little);
            } else {
                write_u32(&mut bytes, start + 8, slice_offset as u32, little);
                write_u32(&mut bytes, start + 12, 64, little);
                write_u32(&mut bytes, start + 16, 4, little);
            }
            let offset = slice_offset as usize;
            bytes[offset..offset + 64].copy_from_slice(&thin(cpu, subtype));
        }
        bytes
    }

    fn retain(bytes: &[u8]) -> File {
        let mut file = tempfile::tempfile().expect("private temporary executable");
        file.write_all(bytes).expect("write synthetic Mach-O");
        file
    }

    fn selected(target: &str) -> MacosBundleMetadata {
        MacosBundleMetadata {
            target: target.to_owned(),
            app_id: "org.example.keld".to_owned(),
            name: "Keld Fixture".to_owned(),
            version: "1.2.3".to_owned(),
            bundle_version: "1".to_owned(),
            minimum_system_version: "13.0".to_owned(),
            icon: false,
            bun_sha256: [0; 32],
        }
    }

    fn admitted(host: &[u8], bun: &[u8], target: &str, running: &str) -> bool {
        let host = retain(host);
        let bun = retain(bun);
        admit_bundle_architectures_for_process(&host, &bun, &selected(target), running).is_ok()
    }

    #[test]
    fn native_thin_host_and_bun_match_only_their_sealed_target_and_process_arch() {
        let arm = thin(ARM64, 0);
        let x64 = thin(X64, 3);
        assert!(admitted(&arm, &arm, "macos-arm64", "arm64"));
        assert!(admitted(&x64, &x64, "macos-x64", "x86_64"));
        assert!(!admitted(&arm, &arm, "macos-x64", "arm64"));
        assert!(!admitted(&x64, &x64, "macos-arm64", "x86_64"));
        assert!(!admitted(&arm, &x64, "macos-arm64", "arm64"));
        assert!(!admitted(&x64, &arm, "macos-x64", "x86_64"));
        assert!(!admitted(&arm, &arm, "macos-arm64", "x86_64"));
        assert!(!admitted(&x64, &x64, "macos-x64", "arm64"));
    }

    #[test]
    fn universal_fat32_and_fat64_allow_both_process_architectures() {
        for is_64 in [false, true] {
            for little in [false, true] {
                let binary = fat(is_64, little);
                assert!(admitted(&binary, &binary, "macos-universal", "arm64"));
                assert!(admitted(&binary, &binary, "macos-universal", "x86_64"));
                assert!(!admitted(&binary, &binary, "macos-arm64", "arm64"));
                assert!(!admitted(&binary, &binary, "macos-x64", "x86_64"));
            }
        }
        let mut reversed = fat(false, false);
        let first_entry = reversed[8..28].to_vec();
        let second_entry = reversed[28..48].to_vec();
        reversed[8..28].copy_from_slice(&second_entry);
        reversed[28..48].copy_from_slice(&first_entry);
        assert!(admitted(&reversed, &reversed, "macos-universal", "arm64"));
        assert!(admitted(&reversed, &reversed, "macos-universal", "x86_64"));
        assert!(!admitted(&reversed, &reversed, "macos-universal2", "arm64"));
    }

    #[test]
    fn fat_header_rejects_counts_overlap_alignment_and_truncation() {
        let original = fat(false, false);
        for mutator in [
            (4, 0u32),         // no slices
            (4, 1),            // fat single slice not canonical thin
            (4, 3),            // third slice cannot be admitted
            (16, 16),          // first slice starts inside fat table
            (16, 0),           // zero offset
            (16, 0x200),       // both slices overlap
            (16, 0xffff_ffff), // offset beyond file
            (20, 0xffff_ffff), // size overflows file
            (24, 40),          // alignment exponent out of range
            (24, 9),           // first offset does not satisfy alignment
            (28, X64),         // repeated CPU type: missing arm64
        ] {
            let mut malformed = original.clone();
            write_u32(&mut malformed, mutator.0, mutator.1, false);
            assert!(
                !admitted(&malformed, &malformed, "macos-universal", "arm64"),
                "malformed header at byte {} admitted",
                mutator.0
            );
        }
        let mut truncated = original.clone();
        truncated.truncate(0x210);
        assert!(!admitted(
            &truncated,
            &truncated,
            "macos-universal",
            "arm64"
        ));
        let mut fat64_reserved = fat(true, false);
        write_u32(&mut fat64_reserved, 8 + 28, 1, false);
        assert!(!admitted(
            &fat64_reserved,
            &fat64_reserved,
            "macos-universal",
            "arm64"
        ));
        let mut wrong_inner = original.clone();
        write_u32(&mut wrong_inner, 0x200 + 4, X64, true);
        write_u32(&mut wrong_inner, 0x200 + 8, 3, true);
        assert!(!admitted(
            &wrong_inner,
            &wrong_inner,
            "macos-universal",
            "arm64"
        ));
    }

    #[test]
    fn fat64_rejects_full_width_offset_overflow_and_duplicate_architectures() {
        let baseline = fat(true, false);
        for (offset, value) in [
            (8 + 8, u64::MAX),       // first offset cannot fit a slice
            (8 + 16, u64::MAX),      // first size overflows its offset
            (8 + 32 + 8, 0x100),     // second slice overlaps the first
            (8 + 32 + 16, u64::MAX), // second slice overflows its offset
        ] {
            let mut malformed = baseline.clone();
            write_u64(&mut malformed, offset, value, false);
            assert!(
                !admitted(&malformed, &malformed, "macos-universal", "x86_64"),
                "full-width hostile fat64 field at offset {offset} was admitted"
            );
        }
        let mut duplicate = baseline.clone();
        write_u32(&mut duplicate, 8 + 32, X64, false);
        write_u32(&mut duplicate, 8 + 32 + 4, 3, false);
        duplicate[0x200..0x240].copy_from_slice(&thin(X64, 3));
        assert!(!admitted(
            &duplicate,
            &duplicate,
            "macos-universal",
            "arm64"
        ));
        let mut little_endian_overlap = fat(true, true);
        write_u64(&mut little_endian_overlap, 8 + 32 + 8, 0x100, true);
        assert!(!admitted(
            &little_endian_overlap,
            &little_endian_overlap,
            "macos-universal",
            "x86_64"
        ));
        let mut zero_size = baseline.clone();
        write_u64(&mut zero_size, 8 + 32 + 16, 0, false);
        assert!(!admitted(
            &zero_size,
            &zero_size,
            "macos-universal",
            "arm64"
        ));
        let mut wrong_outer_subtype = baseline.clone();
        write_u32(&mut wrong_outer_subtype, 8 + 4, 0x8000_0003, false);
        assert!(!admitted(
            &wrong_outer_subtype,
            &wrong_outer_subtype,
            "macos-universal",
            "x86_64"
        ));
    }

    #[test]
    fn thin_macho_rejects_wrong_magic_subtype_filetype_and_load_bounds() {
        let original = thin(ARM64, 0);
        for (offset, value) in [
            (4, X64),
            (8, 1),            // unsupported arm64 v8 subtype
            (8, 2),            // arm64e is not a supported arm64 slice
            (12, 6),           // MH_DYLIB is not MH_EXECUTE
            (16, 0),           // zero load commands
            (20, 0),           // zero command bytes
            (20, 0xffff_ffff), // commands exceed this slice
        ] {
            let mut bad = original.clone();
            write_u32(&mut bad, offset, value, true);
            assert!(!admitted(&bad, &bad, "macos-arm64", "arm64"));
        }
        for magic in [
            [0xce, 0xfa, 0xed, 0xfe], // 32-bit Mach-O
            [0xfe, 0xed, 0xfa, 0xcf], // unsupported big-endian thin
            [0, 0, 0, 0],
        ] {
            let mut bad = original.clone();
            bad[..4].copy_from_slice(&magic);
            assert!(!admitted(&bad, &bad, "macos-arm64", "arm64"));
        }
        assert!(!admitted(
            &original[..31],
            &original,
            "macos-arm64",
            "arm64"
        ));
        assert!(!admitted(
            &original,
            &original[..31],
            "macos-arm64",
            "arm64"
        ));
        assert!(!admitted(&original, &original, "macos-arm64", "powerpc"));
        let x64h = thin(X64, 8);
        assert!(!admitted(&x64h, &x64h, "macos-x64", "x86_64"));
        let x64_lib64 = thin(X64, 0x8000_0003);
        assert!(admitted(&x64_lib64, &x64_lib64, "macos-x64", "x86_64"));
    }

    #[test]
    fn positioned_reads_preserve_the_existing_file_cursor() {
        let mut host = retain(&thin(ARM64, 0));
        let end = host
            .seek(SeekFrom::End(0))
            .expect("park file offset at EOF");
        assert_eq!(
            read_architectures(&host, "retained fixture").unwrap().len(),
            1
        );
        assert_eq!(
            host.stream_position().expect("retained current offset"),
            end,
            "architecture admission must use pread, not change the retained handle cursor"
        );
    }

    #[test]
    fn declared_load_commands_require_valid_records_and_exact_byte_sum() {
        let baseline = thin(ARM64, 0);
        for (offset, value) in [
            (32, 0),  // unknown zero command type
            (36, 0),  // zero command size
            (36, 7),  // short and unaligned
            (36, 16), // fewer command bytes than declared
            (36, 32), // command escapes the declared 24-byte region
            (16, 2),  // second declared command does not exist
            (20, 16), // command exceeds declared header-sizeofcmds
            (20, 32), // header contains unclaimed trailing command bytes
        ] {
            let mut malformed = baseline.clone();
            write_u32(&mut malformed, offset, value, true);
            assert!(
                !admitted(&malformed, &malformed, "macos-arm64", "arm64"),
                "invalid load-command field at byte {offset} with {value} admitted"
            );
        }
        let mut malformed_universal = fat(true, false);
        write_u32(&mut malformed_universal, 0x200 + 36, 0, true);
        assert!(!admitted(
            &malformed_universal,
            &malformed_universal,
            "macos-universal",
            "arm64"
        ));
    }
}
