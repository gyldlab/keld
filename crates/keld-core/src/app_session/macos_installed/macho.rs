//! Installed macOS Mach-O admission via the Pack-owned bounded retained-handle
//! scanner. The running process architecture stays Core-owned; publisher and
//! signed bundle verification stays with existing Security.framework gates.

use std::fs::File;

#[cfg(test)]
use keld_pack::read_macos_macho_architectures;
use keld_pack::{MACOS_LOCAL_TARGET, MacosBundleMetadata, validate_macos_macho_target};

use super::{HostAppError, admission_error};

#[cfg(test)]
fn read_architectures(file: &File, kind: &'static str) -> Result<Vec<&'static str>, HostAppError> {
    read_macos_macho_architectures(file).map_err(|error| admission_error(kind, error))
}

/// Reject a signed local installation unless the sealed target describes both
/// retained executable slice sets and the actual compiled Host architecture.
pub(super) fn admit_bundle_architectures(
    host: &File,
    bun: &File,
    metadata: &MacosBundleMetadata,
) -> Result<(), HostAppError> {
    if !cfg!(target_arch = "aarch64") || metadata.target != MACOS_LOCAL_TARGET {
        return Err(admission_error(
            "installed Mach-O target",
            "the approved local macOS artifact requires an arm64 Host and macos-arm64 target",
        ));
    }
    admit_bundle_architectures_for_process(host, bun, metadata, "arm64")
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
        validate_macos_macho_target(file, metadata)
            .map_err(|error| admission_error(kind, error))?;
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
                write_u32(
                    &mut bytes,
                    start + 8,
                    u32::try_from(slice_offset).expect("fixture FAT32 slice offset"),
                    little,
                );
                write_u32(&mut bytes, start + 12, 64, little);
                write_u32(&mut bytes, start + 16, 4, little);
            }
            let offset = usize::try_from(slice_offset).expect("fixture slice offset");
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
    fn installed_admission_retains_the_approved_arm64_scope() {
        let arm = thin(ARM64, 0);
        let intel = thin(X64, 3);
        let universal = fat(false, false);
        let accepted = super::admit_bundle_architectures(
            &retain(&arm),
            &retain(&arm),
            &selected("macos-arm64"),
        );
        if cfg!(target_arch = "aarch64") {
            assert!(accepted.is_ok(), "approved arm64 thin inputs must pass");
        } else {
            let error = accepted.expect_err("unsupported process must refuse arm64 input");
            assert_eq!(error.code, "KELD-CORE-036");
            assert_eq!(error.phase, "installed Mach-O target");
            assert!(error.detail.contains("requires an arm64 Host"));
        }
        for (target, binary) in [
            ("macos-universal", universal.as_slice()),
            ("macos-x64", intel.as_slice()),
        ] {
            let error = super::admit_bundle_architectures(
                &retain(binary),
                &retain(binary),
                &selected(target),
            )
            .expect_err("unsupported target must refuse at installed admission");
            assert_eq!(error.code, "KELD-CORE-036");
            assert_eq!(error.phase, "installed Mach-O target");
            assert!(error.detail.contains("requires an arm64 Host"));
        }
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
