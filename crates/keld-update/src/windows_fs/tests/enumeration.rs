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

#[test]
fn strict_graph_names_refuse_invalid_utf16_and_enforce_entry_limit() {
    use super::super::{DirectoryChild, strict_graph_directory_names, strict_graph_names};
    let children = vec![DirectoryChild {
        name: vec![0xd800],
        attributes: 0,
    }];
    assert!(
        strict_graph_names(children, 100).is_err(),
        "unpaired surrogate must not disappear"
    );
    let temp = tempfile::tempdir().expect("directory fixture");
    std::fs::write(temp.path().join("one"), b"1").expect("first member");
    std::fs::write(temp.path().join("two"), b"2").expect("second member");
    let retained = super::support::retained_directory(temp.path());
    assert!(strict_graph_directory_names(&retained, 1, 100).is_err());
    assert!(strict_graph_directory_names(&retained, 2, 5).is_err());
    let regular = std::fs::File::open(temp.path().join("one")).expect("file handle");
    assert!(strict_graph_directory_names(&regular, 2, 100).is_err());
    let mut names = strict_graph_directory_names(&retained, 2, 100).expect("complete census");
    names.sort();
    assert_eq!(names, ["one", "two"]);
}

#[test]
fn retained_graph_child_opens_relative_and_refuses_hardlinks() {
    use crate::{WindowsGraphChildKind, open_retained_graph_child};
    let temp = tempfile::tempdir().expect("owned fixture");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("root");
    std::fs::create_dir(root.join("assets")).expect("assets");
    std::fs::write(root.join("assets").join("entry.html"), b"allowed").expect("entry");
    std::fs::write(temp.path().join("outside"), b"outside").expect("outside");
    std::fs::hard_link(
        temp.path().join("outside"),
        root.join("assets").join("alias"),
    )
    .expect("hardlink control");
    let parent = super::support::retained_directory(&root);
    let assets = open_retained_graph_child(&parent, "assets", WindowsGraphChildKind::Directory)
        .expect("retained child directory");
    let mut entry =
        open_retained_graph_child(&assets, "entry.html", WindowsGraphChildKind::RegularFile)
            .expect("retained file");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut bytes).expect("file bytes");
    assert_eq!(bytes, b"allowed");
    assert!(
        open_retained_graph_child(&assets, "alias", WindowsGraphChildKind::RegularFile).is_err()
    );
    assert!(
        open_retained_graph_child(&assets, "../outside", WindowsGraphChildKind::RegularFile)
            .is_err()
    );
    assert!(
        open_retained_graph_child(&assets, "entry.html", WindowsGraphChildKind::Directory).is_err()
    );
}

#[test]
fn retained_graph_directory_refuses_a_junction_to_an_outside_directory() {
    use crate::{WindowsGraphChildKind, open_retained_graph_child};
    let temp = tempfile::tempdir().expect("owned junction fixture");
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&root).expect("root");
    std::fs::create_dir(&outside).expect("outside");
    let sentinel = outside.join("sentinel");
    std::fs::write(&sentinel, b"outside").expect("sentinel");
    let link = root.join("junction");
    super::support::junction(&link, &outside);
    let parent = super::support::retained_directory(&root);
    let result = open_retained_graph_child(&parent, "junction", WindowsGraphChildKind::Directory);
    // The fixture removes only its own junction, never the outside target.
    std::fs::remove_dir(&link).expect("remove fixture junction itself");
    assert!(
        result.is_err(),
        "a junction must not become a retained graph directory"
    );
    assert_eq!(
        std::fs::read(&sentinel).expect("outside remains"),
        b"outside"
    );
}

#[test]
fn retained_graph_directory_pins_its_namespace_until_drop() {
    use crate::{WindowsGraphChildKind, open_retained_graph_child};
    let temp = tempfile::tempdir().expect("owned namespace fixture");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("root");
    let selected = root.join("assets");
    let moved = root.join("moved");
    std::fs::create_dir(&selected).expect("selected source");
    let parent = super::support::retained_directory(&root);
    let retained = open_retained_graph_child(&parent, "assets", WindowsGraphChildKind::Directory)
        .expect("retained selected directory");
    let result = std::fs::rename(&selected, &moved);
    assert!(
        result.is_err(),
        "an admitted directory must prevent namespace replacement while retained"
    );
    assert!(selected.is_dir());
    assert!(!moved.exists());
    drop(retained);
    std::fs::rename(&selected, &moved).expect("rename after releasing source custody");
    assert!(!selected.exists());
    assert!(moved.is_dir());
}

#[test]
fn graph_metadata_distinguishes_identical_bytes_and_observes_mutation() {
    use crate::{graph_locator_metadata, retained_graph_metadata};
    let temp = tempfile::tempdir().expect("owned metadata fixture");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::write(&first, b"same").expect("first file");
    std::fs::write(&second, b"same").expect("second file");
    let file = std::fs::File::open(&first).expect("retained first");
    let before = retained_graph_metadata(&file).expect("held identity");
    assert_eq!(
        before,
        graph_locator_metadata(&first).expect("same locator")
    );
    let other = graph_locator_metadata(&second).expect("other locator");
    assert_eq!(before.dev(), other.dev());
    assert_ne!(before.ino(), other.ino());
    assert_ne!(
        before, other,
        "equal bytes do not establish object identity"
    );
    std::fs::write(&first, b"changed").expect("mutate held ordinary file");
    assert_ne!(
        before,
        retained_graph_metadata(&file).expect("updated metadata")
    );
    let directory = super::support::retained_directory(temp.path());
    let metadata = retained_graph_metadata(&directory).expect("retained directory");
    assert!(metadata.is_dir());
    assert!(!metadata.is_file());
}

#[test]
fn retained_graph_descendant_pins_required_ancestors() {
    use crate::{WindowsGraphChildKind, open_retained_graph_child};
    let temp = tempfile::tempdir().expect("owned ancestry fixture");
    let root = temp.path().join("root");
    let parent_path = root.join("parent");
    let nested_path = parent_path.join("nested");
    std::fs::create_dir_all(&nested_path).expect("nested source");
    std::fs::write(nested_path.join("entry"), b"source").expect("entry");
    let root_handle = super::support::retained_directory(&root);
    let parent =
        open_retained_graph_child(&root_handle, "parent", WindowsGraphChildKind::Directory)
            .expect("retained parent");
    let nested = open_retained_graph_child(&parent, "nested", WindowsGraphChildKind::Directory)
        .expect("retained nested directory");
    let leaf = open_retained_graph_child(&nested, "entry", WindowsGraphChildKind::RegularFile)
        .expect("retained leaf");
    drop(parent);
    drop(nested);
    assert!(
        std::fs::rename(&parent_path, root.join("moved")).is_err(),
        "live descendant custody must pin required ancestors"
    );
    drop(leaf);
    std::fs::rename(&parent_path, root.join("moved")).expect("rename after custody released");
}

#[test]
fn graph_dos_locator_is_observed_from_the_retained_handle() {
    let temp = tempfile::tempdir().expect("owned locator fixture");
    let selected = temp.path().join("source with spaces");
    std::fs::create_dir(&selected).expect("fixture directory");
    let file = super::support::retained_directory(&selected);
    let observed = crate::retained_graph_dos_path(&file).expect("normalized handle locator");
    assert_eq!(
        observed,
        selected.canonicalize().expect("independent existing path")
    );
    let from_handle = crate::retained_graph_metadata(&file).expect("retained identity");
    let from_locator = crate::graph_locator_metadata(&observed).expect("named identity");
    assert_eq!(
        (from_handle.dev(), from_handle.ino()),
        (from_locator.dev(), from_locator.ino())
    );
}
