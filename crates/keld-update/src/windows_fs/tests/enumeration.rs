// Responsibility-local Windows native contract tests: enumeration.

use super::super::parse_directory_entries;
use super::support::{chain, directory_record};
use windows_sys::Win32::Storage::FileSystem::FILE_ID_BOTH_DIR_INFO;

#[test]
fn directory_entry_parser_checks_every_offset_and_skips_dot_entries() {
    let names = parse_directory_entries(&chain(&[".", "..", "a", "b\u{e9}"]))
        .expect("well-formed chain")
        .into_iter()
        .map(|child| child.name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "a".encode_utf16().collect::<Vec<_>>(),
            "b\u{e9}".encode_utf16().collect::<Vec<_>>()
        ]
    );
    let name = "a".encode_utf16().collect::<Vec<_>>();
    // A name that claims more bytes than the buffer holds.
    assert!(parse_directory_entries(&directory_record(0, 4096, &name)).is_err());
    // An odd byte length cannot be UTF-16.
    assert!(parse_directory_entries(&directory_record(0, 1, &name)).is_err());
    // A successor offset inside the fixed header overlaps the current record.
    // Plant a record that would parse cleanly at that overlapping offset, so only
    // the successor-offset check can reject it.
    let overlap = 16_usize;
    let mut overlapping = directory_record(u32::try_from(overlap).expect("small offset"), 2, &name);
    let name_offset = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
    let length_at = overlap + std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength);
    overlapping[length_at..length_at + 4].copy_from_slice(&2_u32.to_ne_bytes());
    overlapping.resize(overlap + name_offset + 2, 0);
    overlapping[overlap + name_offset..].copy_from_slice(&u16::from(b'b').to_ne_bytes());
    let error = parse_directory_entries(&overlapping).expect_err("overlapping successor");
    assert_eq!(error.to_string(), "malformed directory enumeration record");
    // A successor past the end of the buffer.
    assert!(parse_directory_entries(&directory_record(4096, 2, &name)).is_err());
    // A truncated record.
    assert!(parse_directory_entries(&directory_record(0, 2, &name)[..16]).is_err());
    // Separators or stream syntax are never one component.
    for bad in ["a\\b", "a/b", "a:b", "a\u{0}b"] {
        assert!(parse_directory_entries(&chain(&[bad])).is_err(), "{bad}");
    }
}

/// The returned kernel attributes control whether deletion treats a child
/// as a real directory or a reparse link. Literal bytes are the independent
/// oracle: even a name that decodes cleanly must retain both flags.
#[test]
fn directory_entry_parser_keeps_reparse_and_directory_attribute_bits() {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };

    let expected = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
    let mut record = directory_record(0, 2, &[u16::from(b'x')]);
    let at = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileAttributes);
    record[at..at + 4].copy_from_slice(&expected.to_ne_bytes());
    let entries = parse_directory_entries(&record).expect("valid directory record");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, vec![u16::from(b'x')]);
    assert_eq!(
        entries[0].attributes, expected,
        "preserve directory and reparse flags"
    );
}
