//! AC10: the handle reader uses only positioned reads on the handle and its
//! handle-derived length, and keld-pack carries no loader or resource API (container
//! spec §7 row 10).

use std::path::{Path, PathBuf};

/// Every `.rs` file and manifest below `dir`.
fn source_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("read keld-pack directory");
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            source_files(&path, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "rs" || extension == "toml")
        {
            found.push(path);
        }
    }
}

#[test]
fn ac10_source_scan_finds_no_forbidden_reference_in_keld_pack() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("Cargo.toml")];
    source_files(&root.join("src"), &mut files);
    source_files(&root.join("tests"), &mut files);
    // Split literals so this file does not match its own scan.
    let forbidden = [
        concat!("un", "safe"),
        concat!("Load", "Library"),
        concat!("Find", "Resource"),
        concat!("Update", "Resource"),
    ];
    let mut scanned_container_module = false;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("read source file");
        for word in forbidden {
            assert!(
                !text.contains(word),
                "{} references `{word}`",
                file.display()
            );
        }
        scanned_container_module |= file.ends_with("src/host_identity.rs");
    }
    assert!(
        scanned_container_module,
        "the scan must cover the container module"
    );
}

#[cfg(windows)]
mod handle {
    use super::super::{embed, fixture::golden_payload, fixture::two_sections, refusal};
    use crate::{PackError, read_host_identity};
    use std::io::{Seek, SeekFrom, Write};

    /// `FILE_READ_ATTRIBUTES`: enough for the handle-derived length, not for reads.
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    /// `ERROR_ACCESS_DENIED`.
    const ACCESS_DENIED: i32 = 5;

    #[test]
    fn ac10_an_anonymous_file_reads_back_whatever_its_cursor() {
        let output = embed(&two_sections());
        let mut file = tempfile::tempfile().expect("anonymous temporary file");
        file.write_all(&output).expect("write image");
        let end = file.seek(SeekFrom::End(0)).expect("cursor to end of file");
        assert_eq!(end, output.len() as u64);
        assert_eq!(
            read_host_identity(&file).expect("handle read at end-of-file cursor"),
            golden_payload()
        );
        file.seek(SeekFrom::Start(3))
            .expect("cursor into the DOS header");
        assert_eq!(
            read_host_identity(&file).expect("handle read at another cursor"),
            golden_payload()
        );
        // Positioned reads only: no write, truncation or path reopen happened.
        file.seek(SeekFrom::Start(0)).expect("rewind");
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes).expect("read back");
        assert_eq!(bytes, output);
    }

    #[test]
    fn ac10_a_failed_positioned_read_is_keld_pack_012() {
        use std::os::windows::fs::OpenOptionsExt;
        let output = embed(&two_sections());
        let mut named = tempfile::NamedTempFile::new().expect("named temporary file");
        named.write_all(&output).expect("write image");
        named.flush().expect("flush image");
        let attributes_only = std::fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .open(named.path())
            .expect("attributes-only handle");
        // The handle-derived length succeeds, so the refusal comes from the read.
        assert_eq!(
            attributes_only.metadata().expect("length").len(),
            output.len() as u64
        );
        match read_host_identity(&attributes_only) {
            Err(PackError::IdentityContainerRead { source }) => {
                assert_eq!(source.raw_os_error(), Some(ACCESS_DENIED), "{source}");
            }
            other => panic!("expected KELD-PACK-012, got {other:?}"),
        }
        assert_eq!(
            refusal(read_host_identity(&attributes_only)).0,
            "KELD-PACK-012"
        );
        // Control: the same file through a readable handle reads back.
        let readable = std::fs::File::open(named.path()).expect("readable handle");
        assert_eq!(
            read_host_identity(&readable).expect("readable handle"),
            golden_payload()
        );
    }
}
