//! Private draft: retained inventory -> canonical Windows archive bytes.
//!
//! This module does not attest inventory completeness, signature trust, source
//! immutability, installed provenance, durability, or Strict qualification.
//! Admission owners must supply retained immutable regular-file handles and hold
//! their namespace witnesses for the entire call. No source pathname is reopened.
//! The caller must provide exclusive reader ownership: Windows `seek_read` can
//! update the underlying shared cursor; cursor preservation is not promised.

use std::fs::File;
use std::io::{self, Cursor, Read, Write};
use std::os::windows::fs::FileExt as _;

use keld_pack::{PackError, PackageEntry, ProducedFull};

pub(super) enum Content {
    /// Already admitted, immutable regular file, with exact admitted length.
    Retained { file: File, size: u64 },
    /// Same-execution compiled boot/policy/etc. bytes, not an external receipt.
    Generated(Vec<u8>),
}

pub(super) struct Member {
    pub(super) name: String,
    /// None denotes an explicit directory, not an absent file.
    pub(super) content: Option<Content>,
}

enum Source<'a> {
    Retained { file: &'a File, offset: u64 },
    Generated(Cursor<&'a [u8]>),
}

impl Read for Source<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Retained { file, offset } => {
                let count = file.seek_read(buffer, *offset)?;
                *offset = offset
                    .checked_add(count as u64)
                    .ok_or_else(|| io::Error::other("archive input offset overflow"))?;
                Ok(count)
            }
            Self::Generated(reader) => reader.read(buffer),
        }
    }
}

/// Canonicalize ordering and delegate every namespace, helper, policy, size and
/// archive-format check to Pack. Explicit parent directories must be supplied.
///
/// On any error the caller must discard the entire sink. A successful receipt is
/// only an emitted-byte receipt; the output owner must separately flush/sync and
/// publish exclusively. This function never deletes or renames output paths.
pub(super) fn emit<W: Write>(
    inventory: &mut [Member],
    output: &mut W,
) -> Result<ProducedFull, PackError> {
    inventory.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    let mut readers: Vec<_> = inventory
        .iter()
        .map(|member| {
            member.content.as_ref().map(|content| match content {
                Content::Retained { file, .. } => Source::Retained { file, offset: 0 },
                Content::Generated(bytes) => Source::Generated(Cursor::new(bytes.as_slice())),
            })
        })
        .collect();
    let mut entries: Vec<_> = inventory
        .iter()
        .zip(readers.iter_mut())
        .map(|(member, reader)| match (&member.content, reader) {
            (None, None) => PackageEntry::Directory { name: &member.name },
            (Some(content), Some(reader)) => PackageEntry::File {
                name: &member.name,
                size: match content {
                    Content::Retained { size, .. } => *size,
                    Content::Generated(bytes) => bytes.len() as u64,
                },
                input: reader,
            },
            _ => unreachable!("readers are constructed one-to-one from this inventory"),
        })
        .collect();
    keld_pack::produce_windows_v0(&mut entries, output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated(name: &str, bytes: &[u8]) -> Member {
        Member {
            name: name.into(),
            content: Some(Content::Generated(bytes.to_vec())),
        }
    }

    #[test]
    fn retained_inventory_canonicalizes_order_without_changing_bytes() {
        let mut first = [
            generated("z", b"payload"),
            generated("keld-updater-helper.exe", b"helper"),
        ];
        let mut second = [
            generated("keld-updater-helper.exe", b"helper"),
            generated("z", b"payload"),
        ];
        let (mut a, mut b) = (Vec::new(), Vec::new());
        let receipt_a = emit(&mut first, &mut a).unwrap();
        let receipt_b = emit(&mut second, &mut b).unwrap();
        assert_eq!(receipt_a, receipt_b);
        assert_eq!(a, b);
    }

    #[test]
    fn invalid_inventory_never_touches_output() {
        for mut inventory in [
            vec![generated("app.exe", b"host")],
            vec![
                generated("keld-updater-helper.exe", b"a"),
                generated("keld-updater-helper.exe", b"b"),
            ],
            vec![
                generated("keld-updater-helper.exe", b"h"),
                generated("missing/child", b"x"),
            ],
            vec![
                generated("keld-updater-helper.exe", b"h"),
                generated("CON", b"x"),
            ],
        ] {
            let mut output = Vec::new();
            assert!(emit(&mut inventory, &mut output).is_err());
            assert_eq!(output, Vec::<u8>::new());
        }
    }

    struct FailingSink;
    impl Write for FailingSink {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("injected archive output failure"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn output_failure_cannot_return_a_receipt() {
        let mut inventory = [generated("keld-updater-helper.exe", b"helper")];
        assert!(emit(&mut inventory, &mut FailingSink).is_err());
    }
    #[test]
    fn retained_reader_starts_at_zero_regardless_of_shared_cursor() {
        use std::io::{Seek, SeekFrom};
        let expected: Vec<u8> = (0_u8..251).cycle().take(128 * 1024 + 31).collect();
        let mut fixture = RetainedFixture::new(&expected);
        fixture
            .file
            .as_mut()
            .unwrap()
            .seek(SeekFrom::Start(5))
            .unwrap();
        let mut source = Source::Retained {
            file: fixture.file.as_ref().unwrap(),
            offset: 0,
        };
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
        // Windows seek_read updates the shared cursor. Exclusive reader
        // ownership is required; only emitted offsets/bytes are guaranteed.
    }

    #[test]
    fn retained_archive_refuses_short_and_surplus_lengths() {
        let fixture = RetainedFixture::new(b"helper");
        for size in [5, 7] {
            let mut inventory = [Member {
                name: "keld-updater-helper.exe".into(),
                content: Some(Content::Retained {
                    file: fixture.file.as_ref().unwrap().try_clone().unwrap(),
                    size,
                }),
            }];
            assert!(matches!(
                emit(&mut inventory, &mut Vec::new()),
                Err(PackError::SourceSizeMismatch { .. })
            ));
        }
        let mut retained = [Member {
            name: "keld-updater-helper.exe".into(),
            content: Some(Content::Retained {
                file: fixture.file.as_ref().unwrap().try_clone().unwrap(),
                size: 6,
            }),
        }];
        let mut generated = [generated("keld-updater-helper.exe", b"helper")];
        let (mut actual, mut expected) = (Vec::new(), Vec::new());
        assert_eq!(
            emit(&mut retained, &mut actual).unwrap(),
            emit(&mut generated, &mut expected).unwrap()
        );
        assert_eq!(actual, expected);
    }

    struct RetainedFixture {
        file: Option<File>,
        path: std::path::PathBuf,
    }
    impl RetainedFixture {
        fn new(bytes: &[u8]) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "keld-archive-retained-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            file.write_all(bytes).unwrap();
            Self {
                file: Some(file),
                path,
            }
        }
    }
    impl Drop for RetainedFixture {
        fn drop(&mut self) {
            drop(self.file.take());
            std::fs::remove_file(&self.path).expect("remove exact owned fixture");
        }
    }
}
