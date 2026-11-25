// Test-only NSIS fixture lifecycle regression. No PowerShell or production
// authority is exposed. Reuse the updater's native relative create/delete owner.
use super::super::{
    DeletePurpose, EntryKind, create_directory_relative, create_file_relative_with_profile,
    entry_kind, open_for_delete, qualify_volume,
};
use super::support::retained_directory;
use cap_fs_ext::MetadataExt as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

fn metadata(file: &File) -> io::Result<cap_std::fs::Metadata> {
    cap_std::fs::File::from_std(file.try_clone()?).metadata()
}

// This private token is minted only from a live create-new handle. An
// observation by name alone never becomes a deletion authorization.
struct OwnedFixtureEntry {
    retained: File,
    kind: EntryKind,
}

impl OwnedFixtureEntry {
    fn capture(created: &File, observation: &Path, kind: EntryKind) -> io::Result<Self> {
        // The creation handle is still live and excludes delete sharing.
        // The second handle is metadata-only, with delete sharing so cleanup
        // can later open DELETE access. An ambient observation may fail or
        // see a different object; only a same-ID observation can be retained.
        let retained = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(observation)?;
        let token = Self { retained, kind };
        token.require_same(created)?;
        Ok(token)
    }

    fn require_same(&self, candidate: &File) -> io::Result<()> {
        let expected = metadata(&self.retained)?;
        let actual = metadata(candidate)?;
        if entry_kind(&self.retained)? != self.kind
            || entry_kind(candidate)? != self.kind
            || (expected.dev(), expected.ino()) != (actual.dev(), actual.ino())
            || (self.kind == EntryKind::File && (expected.nlink() != 1 || actual.nlink() != 1))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture entry identity, kind, or link count changed; retain",
            ));
        }
        Ok(())
    }

    fn remove(&self, parent: &File, name: &str) -> io::Result<()> {
        if entry_kind(parent)? != EntryKind::Directory {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture parent is not a regular directory; retain",
            ));
        }
        let entry = open_for_delete(parent, name, DeletePurpose::Remove)?;
        if entry.kind() != self.kind {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture entry kind changed; retain",
            ));
        }
        self.require_same(entry.file())?;
        // Only this checked native handle is consumed. No recursive or
        // path-based deletion, retry, unknown child adoption or Drop cleanup.
        entry.delete()
    }
}

struct Fixture {
    temp_parent: File,
    root: File,
    root_token: OwnedFixtureEntry,
    root_name: String,
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        // The temp location selects where to create a new test object; it
        // never grants deletion of any preexisting entry or its ancestors.
        let temp = std::env::temp_dir();
        let temp_parent = retained_directory(&temp);
        assert_eq!(
            entry_kind(&temp_parent).expect("temp kind"),
            EntryKind::Directory
        );
        qualify_volume(&temp_parent).expect("qualified native fixture volume");
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).expect("unique fixture name");
        let root_name = format!("keld-nsis-fixture-{:032x}", u128::from_le_bytes(random));
        let root = create_directory_relative(&temp_parent, &root_name).expect("create-new root");
        let path = temp.join(&root_name);
        let root_token = OwnedFixtureEntry::capture(&root, &path, EntryKind::Directory)
            .expect("pin created root");
        Self {
            temp_parent,
            root,
            root_token,
            root_name,
            path,
        }
    }

    fn finish(self) {
        let Self {
            temp_parent,
            root,
            root_token,
            root_name,
            path,
        } = self;
        drop(root);
        root_token
            .remove(&temp_parent, &root_name)
            .expect("delete exact empty fixture root");
        drop(root_token);
        assert_absent(&temp_parent, &root_name);
        assert!(!path.exists(), "ordinary fixture root remains");
    }
}

fn assert_absent(parent: &File, name: &str) {
    let error = open_for_delete(parent, name, DeletePurpose::Remove)
        .err()
        .expect("entry absent after cleanup");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

fn create_file(parent: &File, path: &Path, name: &str, bytes: &[u8]) -> OwnedFixtureEntry {
    let mut created = create_file_relative_with_profile(
        parent,
        name,
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    )
    .expect("create-new fixture file");
    created.write_all(bytes).expect("write owned fixture");
    let owner = OwnedFixtureEntry::capture(&created, &path.join(name), EntryKind::File)
        .expect("retain original file identity");
    drop(created);
    owner
}

#[test]
fn nsis_fixture_ordinary_cleanup_uses_only_owned_handles() {
    let fixture = Fixture::new();
    let nested = create_directory_relative(&fixture.root, "stage").expect("create-new stage");
    let nested_path = fixture.path.join("stage");
    let nested_token =
        OwnedFixtureEntry::capture(&nested, &nested_path, EntryKind::Directory).expect("pin stage");
    let payload = create_file(&nested, &nested_path, "payload", b"owned test bytes");
    assert_eq!(
        fs::read(nested_path.join("payload")).expect("ordinary payload"),
        b"owned test bytes"
    );
    payload
        .remove(&nested, "payload")
        .expect("delete owned payload");
    drop(payload);
    assert_absent(&nested, "payload");
    drop(nested);
    nested_token
        .remove(&fixture.root, "stage")
        .expect("delete owned empty stage");
    drop(nested_token);
    assert_absent(&fixture.root, "stage");
    fixture.finish();
}

#[test]
fn nsis_fixture_cleanup_refuses_same_name_foreign_identity() {
    let fixture = Fixture::new();
    let original = create_file(
        &fixture.root,
        &fixture.path,
        "payload",
        b"identical test bytes",
    );
    // A test-controlled rename retires the original spelling. Its identity
    // handle remains alive, so inode reuse cannot impersonate the token.
    fs::rename(fixture.path.join("payload"), fixture.path.join("retired"))
        .expect("move owned original");
    let foreign = create_file(
        &fixture.root,
        &fixture.path,
        "payload",
        b"identical test bytes",
    );
    let error = original
        .remove(&fixture.root, "payload")
        .expect_err("foreign identity refused");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        fs::read(fixture.path.join("payload")).expect("foreign sentinel"),
        b"identical test bytes"
    );
    assert_eq!(
        fs::read(fixture.path.join("retired")).expect("original retained"),
        b"identical test bytes"
    );

    // The test itself created both objects and has a distinct token for each.
    // Retire each only with its actual token after the independent refusal.
    foreign
        .remove(&fixture.root, "payload")
        .expect("delete independently owned replacement");
    original
        .remove(&fixture.root, "retired")
        .expect("delete original at its new relative name");
    drop(foreign);
    drop(original);
    assert_absent(&fixture.root, "payload");
    assert_absent(&fixture.root, "retired");
    fixture.finish();
}

#[test]
fn nsis_fixture_cleanup_refuses_live_reparse_object() {
    use super::support::{clear_mount_point_on_open_directory, set_mount_point_on_open_directory};
    use windows_sys::Win32::Storage::FileSystem::{FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA};

    let fixture = Fixture::new();
    let outside = create_directory_relative(&fixture.root, "outside").expect("outside fixture");
    let outside_path = fixture.path.join("outside");
    let outside_token = OwnedFixtureEntry::capture(&outside, &outside_path, EntryKind::Directory)
        .expect("pin outside fixture");
    let sentinel = create_file(&outside, &outside_path, "sentinel", b"outside unchanged");
    let slot = create_directory_relative(&fixture.root, "slot").expect("create empty slot");
    let slot_path = fixture.path.join("slot");
    let slot_token = OwnedFixtureEntry::capture(&slot, &slot_path, EntryKind::Directory)
        .expect("pin slot before reparse");
    let mutation = OpenOptions::new()
        .access_mode(FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&slot_path)
        .expect("open owned empty slot for controlled native reparse");
    slot_token
        .require_same(&mutation)
        .expect("mutation owns same slot");
    set_mount_point_on_open_directory(&mutation, &outside_path);
    drop(slot);
    assert_eq!(
        fs::read(slot_path.join("sentinel")).expect("actual alias"),
        b"outside unchanged"
    );
    assert_eq!(
        entry_kind(&mutation).expect("live slot kind"),
        EntryKind::Link
    );
    assert!(
        slot_token.remove(&fixture.root, "slot").is_err(),
        "changed kind must be retained"
    );
    assert_eq!(
        fs::read(outside_path.join("sentinel")).expect("outside sentinel"),
        b"outside unchanged"
    );
    assert_eq!(
        entry_kind(&mutation).expect("retained link"),
        EntryKind::Link
    );

    // Clear only the test-created reparse object using the original verified
    // live mutation handle; no path-based unlink or traversal.
    clear_mount_point_on_open_directory(&mutation);
    drop(mutation);
    slot_token
        .remove(&fixture.root, "slot")
        .expect("delete original restored empty slot");
    drop(slot_token);
    sentinel
        .remove(&outside, "sentinel")
        .expect("delete owned outside test sentinel");
    drop(sentinel);
    drop(outside);
    outside_token
        .remove(&fixture.root, "outside")
        .expect("delete owned empty outside fixture");
    drop(outside_token);
    fixture.finish();
}
