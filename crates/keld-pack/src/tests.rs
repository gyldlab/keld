//! Contract tests for architecture 06 §4's Windows v0 full-package cell.

use super::*;
#[cfg(windows)]
use std::io::Cursor;
use std::io::{self, Read, Write};

// Generated with Python 3 stdlib tarfile's USTAR_FORMAT, then adjusted only where
// its defaults differ from the specified profile: directory trailing slashes were
// removed, unused device fields set to seven octal zeros plus NUL, and checksums
// recalculated. Every header field and padding block was inspected before check-in.
// KEL-270 T4d S9a regenerated it with the same procedure, adding the root regular file
// `keld-updater-helper.exe` (27 bytes); the same script reproduces the previous fixture
// (SHA-256 02c6d279be74c9a82b0f90fa1214b4e8147d09f5747d3f050a3c97dfa8f1961b) byte for
// byte from the previous entry list. SHA-256 of the resulting independent fixture:
// 60ea733b9f0ca5efc5b85f9e2a0691a9b35a0fca95e243fc4ba21fa9eab3c05e
#[cfg(windows)]
const GOLDEN_TAR: &[u8] = include_bytes!("../tests/fixtures/windows-v0-content.tar");

/// The golden's updater-helper bytes; any regular file satisfies the presence rule.
#[cfg(windows)]
const HELPER: &[u8] = b"keld-updater-helper fixture";

#[derive(Default)]
struct CountReader {
    reads: usize,
}

impl Read for CountReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        Ok(0)
    }
}

#[derive(Default)]
struct CountWriter {
    writes: usize,
}

impl Write for CountWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
fn golden_entries<'a>(
    one: &'a mut dyn Read,
    multi: &'a mut dyn Read,
    helper: &'a mut dyn Read,
) -> [PackageEntry<'a>; 5] {
    [
        PackageEntry::Directory { name: "empty" },
        PackageEntry::Directory { name: "nest" },
        PackageEntry::File {
            name: "nest/one",
            size: 1,
            input: one,
        },
        PackageEntry::File {
            name: "nest/multi",
            size: 1025,
            input: multi,
        },
        PackageEntry::File {
            name: "keld-updater-helper.exe",
            size: HELPER.len() as u64,
            input: helper,
        },
    ]
}

#[cfg(windows)]
fn produce_golden() -> (Vec<u8>, ProducedFull) {
    let mut one = Cursor::new(b"!");
    let multi_bytes: Vec<u8> = (0u8..=255).cycle().take(1024).chain(*b"Z").collect();
    let mut multi = Cursor::new(multi_bytes);
    let mut helper = Cursor::new(HELPER);
    let mut entries = golden_entries(&mut one, &mut multi, &mut helper);
    let mut compressed = Vec::new();
    let receipt = produce_windows_v0(&mut entries, &mut compressed).expect("golden package");
    (compressed, receipt)
}

/// A zero-length updater helper that counts its source reads, so a fixture that
/// isolates another metadata rule is otherwise canonical.
#[cfg(windows)]
fn idle_helper(source: &mut CountReader) -> PackageEntry<'_> {
    PackageEntry::File {
        name: "keld-updater-helper.exe",
        size: 0,
        input: source,
    }
}

#[cfg(windows)]
#[test]
fn canonical_tar_matches_independent_ustar_golden_and_receipt() {
    let (compressed, receipt) = produce_golden();
    let tar = zstd::stream::decode_all(compressed.as_slice()).expect("valid zstd");
    assert_eq!(
        tar.as_slice(),
        GOLDEN_TAR,
        "every header, payload, pad and trailer byte"
    );
    assert_eq!(receipt.content_size(), 7680);
    assert_eq!(
        receipt.content_blake3(),
        blake3::hash(GOLDEN_TAR).as_bytes()
    );
    assert_eq!(receipt.compressed_size(), compressed.len() as u64);
    assert_eq!(
        receipt.compressed_blake3(),
        blake3::hash(&compressed).as_bytes()
    );

    let (again, again_receipt) = produce_golden();
    assert_eq!(compressed, again, "zstd output must be reproducible");
    assert_eq!(
        receipt.compressed_blake3(),
        again_receipt.compressed_blake3()
    );
}

#[cfg(windows)]
struct FragmentedReader {
    data: Cursor<Vec<u8>>,
    interrupted_once: bool,
    successful_reads: usize,
}

#[cfg(windows)]
impl FragmentedReader {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data: Cursor::new(data),
            interrupted_once: false,
            successful_reads: 0,
        }
    }
}

#[cfg(windows)]
impl Read for FragmentedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted_once {
            self.interrupted_once = true;
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "injected interruption",
            ));
        }
        self.successful_reads += 1;
        let limit = buf.len().min(3);
        self.data.read(&mut buf[..limit])
    }
}

#[cfg(windows)]
#[derive(Default)]
struct ShortWriter {
    bytes: Vec<u8>,
    writes: usize,
}

#[cfg(windows)]
impl Write for ShortWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let count = buf.len().min(7);
        self.bytes.extend_from_slice(&buf[..count]);
        self.writes += 1;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
#[test]
fn fragmented_sources_and_partial_sink_writes_preserve_bytes_and_receipt() {
    let mut one = FragmentedReader::new(b"!".to_vec());
    let multi_bytes: Vec<u8> = (0u8..=255).cycle().take(1024).chain(*b"Z").collect();
    let mut multi = FragmentedReader::new(multi_bytes);
    let mut helper = FragmentedReader::new(HELPER.to_vec());
    let mut entries = golden_entries(&mut one, &mut multi, &mut helper);
    let mut sink = ShortWriter::default();
    let receipt = produce_windows_v0(&mut entries, &mut sink).expect("short writes are retried");

    assert!(one.interrupted_once && multi.interrupted_once && helper.interrupted_once);
    assert!(
        multi.successful_reads > 342,
        "fragmented reads include an EOF probe"
    );
    assert!(sink.writes > 1, "compressed writes were fragmented");
    let tar = zstd::stream::decode_all(sink.bytes.as_slice()).expect("complete compressed stream");
    assert_eq!(tar.as_slice(), GOLDEN_TAR);
    assert_eq!(receipt.content_size(), GOLDEN_TAR.len() as u64);
    assert_eq!(
        receipt.content_blake3(),
        blake3::hash(GOLDEN_TAR).as_bytes()
    );
    assert_eq!(receipt.compressed_size(), sink.bytes.len() as u64);
    assert_eq!(
        receipt.compressed_blake3(),
        blake3::hash(&sink.bytes).as_bytes()
    );
}

#[test]
fn portable_policy_constant_is_pinned() {
    assert_eq!(UPDATE_POLICY_PATH, ".keld/update-policy.v1");
    assert_eq!(
        NO_MIGRATION_POLICY,
        b"{\"schema\":1,\"dataMigration\":\"none\"}\n"
    );
}

#[test]
fn portable_updater_helper_path_is_pinned() {
    assert_eq!(UPDATER_HELPER_PATH, "keld-updater-helper.exe");
}

fn helper_detail(members: &[ArchiveMember<'_>]) -> Option<&'static str> {
    match require_updater_helper(members) {
        Ok(()) => None,
        Err(PackError::InvalidMetadata { detail }) => Some(detail),
        Err(other) => panic!("the helper rule refuses only as KELD-PACK-002: {other:?}"),
    }
}

const fn member(name: &str, kind: ArchiveEntryKind) -> ArchiveMember<'_> {
    ArchiveMember {
        name,
        kind,
        size: 0,
    }
}

#[test]
fn shared_member_rule_requires_exactly_one_root_helper_file() {
    use ArchiveEntryKind::{Directory, File};
    assert_eq!(
        helper_detail(&[member("keld-updater-helper.exe", File)]),
        None
    );
    assert_eq!(
        helper_detail(&[
            member("a", Directory),
            member("a/b", File),
            member("keld-updater-helper.exe", File),
        ]),
        None,
        "the helper is admitted beside other members"
    );
    for (why, members, detail) in [
        (
            "empty",
            vec![],
            "required keld-updater-helper.exe is missing",
        ),
        (
            "absent",
            vec![member("keld-host.exe", File)],
            "required keld-updater-helper.exe is missing",
        ),
        (
            "case variant only",
            vec![member("KELD-UPDATER-HELPER.EXE", File)],
            "required keld-updater-helper.exe is missing",
        ),
        (
            "nested only",
            vec![
                member("nested", Directory),
                member("nested/keld-updater-helper.exe", File),
            ],
            "required keld-updater-helper.exe is missing",
        ),
        (
            "directory",
            vec![member("keld-updater-helper.exe", Directory)],
            "keld-updater-helper.exe is not a regular file",
        ),
        (
            "duplicated",
            vec![
                member("keld-updater-helper.exe", File),
                member("keld-updater-helper.exe", File),
            ],
            "keld-updater-helper.exe appears more than once",
        ),
    ] {
        assert_eq!(helper_detail(&members), Some(detail), "{why}");
    }
}

#[cfg(windows)]
#[test]
fn producer_requires_the_updater_helper_before_any_source_or_sink_io() {
    // Each tree is otherwise canonical; the helper rule alone refuses it, in metadata
    // pass 1, before any source is read or any compressed byte is written.
    for (why, helper_name, kind, detail) in [
        (
            "absent",
            None,
            ArchiveEntryKind::File,
            "required keld-updater-helper.exe is missing",
        ),
        (
            "case variant only",
            Some("KELD-UPDATER-HELPER.EXE"),
            ArchiveEntryKind::File,
            "required keld-updater-helper.exe is missing",
        ),
        (
            "nested only",
            Some("nested/keld-updater-helper.exe"),
            ArchiveEntryKind::File,
            "required keld-updater-helper.exe is missing",
        ),
        (
            "directory",
            Some("keld-updater-helper.exe"),
            ArchiveEntryKind::Directory,
            "keld-updater-helper.exe is not a regular file",
        ),
    ] {
        let mut app = CountReader::default();
        let mut helper = CountReader::default();
        let mut sink = CountWriter::default();
        let mut entries = vec![
            PackageEntry::Directory { name: "nested" },
            PackageEntry::File {
                name: "app.bin",
                size: 1,
                input: &mut app,
            },
        ];
        match (helper_name, kind) {
            (None, _) => {}
            (Some(name), ArchiveEntryKind::Directory) => {
                entries.push(PackageEntry::Directory { name });
            }
            (Some(name), ArchiveEntryKind::File) => entries.push(PackageEntry::File {
                name,
                size: 1,
                input: &mut helper,
            }),
        }
        let error = produce_windows_v0(&mut entries, &mut sink).expect_err(why);
        assert!(
            matches!(error, PackError::InvalidMetadata { detail: found } if found == detail),
            "{why}: {error}"
        );
        assert_eq!(error.code(), "KELD-PACK-002", "{why}");
        assert_eq!((app.reads, helper.reads, sink.writes), (0, 0, 0), "{why}");
    }
}

#[cfg(windows)]
#[test]
fn native_namespace_rejects_before_any_source_or_sink_io() {
    // Each row changes one v0 admission rule; CountReader and CountWriter prove
    // that rejection happens in metadata pass 1, before compression begins.
    for name in [
        "a\\b",
        "a:b",
        "a<bad",
        "CON",
        "aux.txt",
        "x.",
        "x ",
        "x~1",
        "a//b",
        "a/../b",
        "/absolute",
        "a/./b",
    ] {
        let mut source = CountReader::default();
        let mut helper = CountReader::default();
        let mut sink = CountWriter::default();
        let mut entries = [
            PackageEntry::File {
                name,
                size: 0,
                input: &mut source,
            },
            idle_helper(&mut helper),
        ];
        let error = produce_windows_v0(&mut entries, &mut sink).expect_err(name);
        assert_eq!(error.code(), "KELD-PACK-002", "{name}");
        assert_eq!(source.reads + helper.reads, 0, "{name}");
        assert_eq!(sink.writes, 0, "{name}");
    }
}

#[cfg(windows)]
#[test]
fn policy_namespace_and_aliases_are_reserved_before_io() {
    for name in [
        ".keld",
        ".keld/update-policy.v1",
        ".KELD",
        ".keld/UPDATE-POLICY.V1",
    ] {
        let mut source = CountReader::default();
        let mut helper = CountReader::default();
        let mut sink = CountWriter::default();
        let mut entries = [
            PackageEntry::File {
                name,
                size: 0,
                input: &mut source,
            },
            idle_helper(&mut helper),
        ];
        let error = produce_windows_v0(&mut entries, &mut sink).expect_err(name);
        assert_eq!(error.code(), "KELD-PACK-002", "{name}");
        assert_eq!((source.reads + helper.reads, sink.writes), (0, 0), "{name}");
    }

    let mut source = CountReader::default();
    let mut helper = CountReader::default();
    let mut sink = CountWriter::default();
    let mut entries = [
        PackageEntry::Directory { name: ".keld" },
        PackageEntry::File {
            name: ".keld/update-policy.v1",
            size: 0,
            input: &mut source,
        },
        idle_helper(&mut helper),
    ];
    assert_eq!(
        produce_windows_v0(&mut entries, &mut sink)
            .unwrap_err()
            .code(),
        "KELD-PACK-002"
    );
    assert_eq!((source.reads + helper.reads, sink.writes), (0, 0));
}

#[cfg(windows)]
#[test]
fn producer_rejects_missing_parent_file_ancestor_and_case_alias_before_io() {
    for (first, second) in [
        ("parent/child", None),
        ("parent", Some("parent/child")),
        ("readme", Some("README")),
    ] {
        let mut source = CountReader::default();
        let mut helper = CountReader::default();
        let mut sink = CountWriter::default();
        let mut entries = vec![
            PackageEntry::File {
                name: first,
                size: 0,
                input: &mut source,
            },
            idle_helper(&mut helper),
        ];
        if let Some(second) = second {
            entries.push(PackageEntry::Directory { name: second });
        }
        let error = produce_windows_v0(&mut entries, &mut sink).unwrap_err();
        assert_eq!(error.code(), "KELD-PACK-002", "{first}");
        assert_eq!(
            (source.reads + helper.reads, sink.writes),
            (0, 0),
            "{first}"
        );
    }
}

#[cfg(windows)]
#[test]
fn explicit_exact_keld_directory_is_accepted_once() {
    let mut helper = CountReader::default();
    let mut entries = [
        PackageEntry::Directory { name: ".keld" },
        idle_helper(&mut helper),
    ];
    let mut output = Vec::new();
    produce_windows_v0(&mut entries, &mut output).expect("exact reserved parent is allowed");
    let tar = zstd::stream::decode_all(output.as_slice()).expect("valid zstd");
    assert_eq!(&tar[..6], b".keld\0");
    assert_eq!(
        tar.windows(UPDATE_POLICY_PATH.len())
            .filter(|window| *window == UPDATE_POLICY_PATH.as_bytes())
            .count(),
        1
    );
}

#[test]
fn shared_member_validator_rejects_missing_parent_and_file_ancestor() {
    let invalid = [
        vec![ArchiveMember {
            name: "a/b",
            kind: ArchiveEntryKind::File,
            size: 0,
        }],
        vec![
            ArchiveMember {
                name: "a",
                kind: ArchiveEntryKind::File,
                size: 0,
            },
            ArchiveMember {
                name: "a/b",
                kind: ArchiveEntryKind::File,
                size: 0,
            },
        ],
        vec![
            ArchiveMember {
                name: "a",
                kind: ArchiveEntryKind::Directory,
                size: 0,
            },
            ArchiveMember {
                name: "a",
                kind: ArchiveEntryKind::Directory,
                size: 0,
            },
        ],
        vec![ArchiveMember {
            name: "a",
            kind: ArchiveEntryKind::Directory,
            size: 1,
        }],
    ];
    for members in invalid {
        assert_eq!(
            validate_v0_members(&members).unwrap_err().code(),
            "KELD-PACK-002"
        );
    }
    validate_v0_members(&[
        ArchiveMember {
            name: "a",
            kind: ArchiveEntryKind::Directory,
            size: 0,
        },
        ArchiveMember {
            name: "a/b",
            kind: ArchiveEntryKind::File,
            size: 0,
        },
    ])
    .expect("complete tree");
}

#[test]
fn portable_member_validator_pins_ustar_name_and_size_limits() {
    let max_name = "x".repeat(100);
    validate_v0_members(&[ArchiveMember {
        name: &max_name,
        kind: ArchiveEntryKind::File,
        size: 0o77_777_777_777,
    }])
    .expect("100-byte name and eleven-digit octal size fit");

    let too_long = "x".repeat(101);
    for (name, size) in [(&too_long[..], 0), (&max_name[..], 0o100_000_000_000)] {
        assert_eq!(
            validate_v0_members(&[ArchiveMember {
                name,
                kind: ArchiveEntryKind::File,
                size,
            }])
            .unwrap_err()
            .code(),
            "KELD-PACK-002"
        );
    }
}

#[cfg(windows)]
#[test]
fn source_length_must_equal_declared_size() {
    for (bytes, declared) in [(b"".as_slice(), 1), (b"ab".as_slice(), 1)] {
        let mut source = Cursor::new(bytes);
        let mut helper = CountReader::default();
        let mut entries = [
            PackageEntry::File {
                name: "x",
                size: declared,
                input: &mut source,
            },
            idle_helper(&mut helper),
        ];
        let mut output = Vec::new();
        assert_eq!(
            produce_windows_v0(&mut entries, &mut output)
                .unwrap_err()
                .code(),
            "KELD-PACK-003"
        );
    }
}

#[cfg(windows)]
struct BrokenReader;

#[cfg(windows)]
impl Read for BrokenReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("injected source failure"))
    }
}

#[cfg(windows)]
struct BrokenWriter;

#[cfg(windows)]
impl Write for BrokenWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("injected sink failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("injected finalization failure"))
    }
}

#[cfg(windows)]
struct WriteZeroSink;

#[cfg(windows)]
impl Write for WriteZeroSink {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Ok(0)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
#[test]
fn stream_failures_return_no_receipt() {
    let mut source = BrokenReader;
    let mut helper = CountReader::default();
    let mut entries = [
        PackageEntry::File {
            name: "x",
            size: 1,
            input: &mut source,
        },
        idle_helper(&mut helper),
    ];
    assert_eq!(
        produce_windows_v0(&mut entries, &mut Vec::new())
            .unwrap_err()
            .code(),
        "KELD-PACK-004"
    );

    let mut source = Cursor::new(b"x");
    let mut helper = CountReader::default();
    let mut entries = [
        PackageEntry::File {
            name: "x",
            size: 1,
            input: &mut source,
        },
        idle_helper(&mut helper),
    ];
    let error = produce_windows_v0(&mut entries, &mut BrokenWriter).unwrap_err();
    assert_eq!(error.code(), "KELD-PACK-004");
    assert!(
        matches!(
            error,
            PackError::Processing {
                stage: "zstd finalization",
                ..
            }
        ),
        "small archive must report the sink failure during encoder.finish()"
    );

    let mut source = Cursor::new(b"x");
    let mut helper = CountReader::default();
    let mut entries = [
        PackageEntry::File {
            name: "x",
            size: 1,
            input: &mut source,
        },
        idle_helper(&mut helper),
    ];
    let error = produce_windows_v0(&mut entries, &mut WriteZeroSink).unwrap_err();
    assert_eq!(error.code(), "KELD-PACK-004");
}

#[cfg(not(windows))]
#[test]
fn foreign_host_refuses_before_source_or_sink_io() {
    let mut source = CountReader::default();
    let mut sink = CountWriter::default();
    let mut entries = [PackageEntry::File {
        name: "x",
        size: 1,
        input: &mut source,
    }];
    assert_eq!(
        produce_windows_v0(&mut entries, &mut sink)
            .unwrap_err()
            .code(),
        "KELD-PACK-001"
    );
    assert_eq!((source.reads, sink.writes), (0, 0));
}
