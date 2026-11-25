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
    run_ordinary_cleanup(b"owned test bytes", false).expect("clean up the ordinary fixture");
}

#[test]
fn nsis_fixture_ordinary_cleanup_after_powershell_smoke() {
    run_ordinary_cleanup(b"owned test bytes", true)
        .expect("PowerShell read-only owned fixture smoke");
}

#[test]
fn nsis_fixture_ordinary_cleanup_after_powershell_payload_refusal() {
    let error = run_ordinary_cleanup(b"ordinary fixture mismatch", true)
        .expect_err("the child must refuse different ordinary bytes");
    assert!(
        error
            .get_ref()
            .is_some_and(|cause| cause.is::<OwnedFixturePayloadRefused>()),
        "{error}"
    );
}

#[test]
fn nsis_fixture_ordinary_cleanup_retains_independent_extra_file() {
    let fixture = Fixture::new();
    let extra = create_file(
        &fixture.root,
        &fixture.path,
        "extra",
        b"independent fixture data",
    );
    clean_owned_stage(&fixture, b"owned test bytes", false).expect("clean only our stage");

    let Fixture {
        temp_parent,
        root,
        root_token,
        root_name,
        path,
    } = fixture;
    drop(root);
    let error = root_token
        .remove(&temp_parent, &root_name)
        .expect_err("must not discard a non-empty fixture");
    assert_eq!(error.kind(), io::ErrorKind::DirectoryNotEmpty);
    assert_eq!(
        fs::read(path.join("extra")).expect("independent fixture entry retained"),
        b"independent fixture data"
    );

    // This is the independent test author's own token, never available to
    // the cleanup path under test. Only it may dispose of the extra file.
    let root = retained_directory(&path);
    root_token
        .require_same(&root)
        .expect("fixture root unchanged");
    extra
        .remove(&root, "extra")
        .expect("release independently owned extra");
    drop(extra);
    assert_absent(&root, "extra");
    drop(root);
    root_token
        .remove(&temp_parent, &root_name)
        .expect("dispose of our empty test root");
    drop(root_token);
    assert_absent(&temp_parent, &root_name);
}

fn run_ordinary_cleanup(bytes: &[u8], run_smoke: bool) -> io::Result<()> {
    let fixture = Fixture::new();
    let result = clean_owned_stage(&fixture, bytes, run_smoke);
    fixture.finish();
    result
}

fn clean_owned_stage(fixture: &Fixture, bytes: &[u8], run_smoke: bool) -> io::Result<()> {
    let nested = create_directory_relative(&fixture.root, "stage").expect("create-new stage");
    let nested_path = fixture.path.join("stage");
    let nested_token =
        OwnedFixtureEntry::capture(&nested, &nested_path, EntryKind::Directory).expect("pin stage");
    let payload = create_file(&nested, &nested_path, "payload", bytes);
    assert_eq!(
        fs::read(nested_path.join("payload")).expect("ordinary payload"),
        bytes
    );
    // The child reports only whether its ordinary read worked; it is not
    // an identity witness. Never give the child a deletion token, and never
    // skip cleanup of our original entries because it refused the fixture.
    let result = if run_smoke {
        read_fixture_through_runner(&fixture.path)
    } else {
        Ok(())
    };
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
    result
}

fn read_fixture_through_runner(root: &Path) -> io::Result<()> {
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| io::Error::other("Windows SystemRoot is unavailable"))?;
    let powershell = system_root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let runner =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/windows_v2_nsis_build_test.ps1");
    let output = std::process::Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(runner)
        .arg("-OwnedFixtureSmokeRoot")
        .arg(root)
        .output()?;
    classify_owned_fixture_smoke(output.status.code(), &output.stdout, &output.stderr)
}

#[derive(Debug)]
struct OwnedFixturePayloadRefused;

impl std::fmt::Display for OwnedFixturePayloadRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("owned fixture payload refused")
    }
}

impl std::error::Error for OwnedFixturePayloadRefused {}

fn classify_owned_fixture_smoke(
    exit_code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> io::Result<()> {
    const PASS: &[u8] = b"KEL270_NSIS_OWNED_FIXTURE_SMOKE_PASS read_only=true";
    const REFUSAL: &[u8] = b"KEL270_NSIS_OWNED_FIXTURE_SMOKE_REFUSED: fixture content differs";
    fn is_marker(line: &[u8], marker: &[u8]) -> bool {
        line.strip_suffix(b"\r\n")
            .or_else(|| line.strip_suffix(b"\n"))
            .unwrap_or(line)
            == marker
    }
    if exit_code == Some(0) && is_marker(stdout, PASS) && stderr.is_empty() {
        return Ok(());
    }
    if exit_code == Some(4) && stdout.is_empty() && is_marker(stderr, REFUSAL) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            OwnedFixturePayloadRefused,
        ));
    }
    Err(io::Error::other(format!(
        "owned fixture smoke had unexpected exit or markers (exit code {exit_code:?})"
    )))
}

#[test]
fn owned_fixture_smoke_classifier_requires_exact_exit_and_single_marker() {
    let accepted = b"KEL270_NSIS_OWNED_FIXTURE_SMOKE_PASS read_only=true\r\n";
    let refused = b"KEL270_NSIS_OWNED_FIXTURE_SMOKE_REFUSED: fixture content differs\r\n";
    assert!(classify_owned_fixture_smoke(Some(0), accepted, b"").is_ok());
    let is_refusal = |exit, stdout: &[u8], stderr: &[u8]| {
        classify_owned_fixture_smoke(exit, stdout, stderr)
            .expect_err("input must not be accepted")
            .get_ref()
            .is_some_and(|cause| cause.is::<OwnedFixturePayloadRefused>())
    };
    assert!(is_refusal(Some(4), b"", refused));
    assert!(!is_refusal(Some(0), b"", refused));
    assert!(!is_refusal(Some(0), accepted, refused));
    assert!(!is_refusal(Some(4), accepted, refused));
    assert!(!is_refusal(Some(1), b"", refused));
    assert!(!is_refusal(None, b"", refused));
    assert!(!is_refusal(
        Some(4),
        b"",
        b"something else: fixture content differs"
    ));
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

#[test]
fn nsis_fixture_ordinary_lifecycle_exclusive_creation_and_in_place_rewrite() {
    use std::io::{Seek as _, SeekFrom};

    let fixture = Fixture::new();
    let name = "sample";
    let profile = keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;
    let mut created = create_file_relative_with_profile(&fixture.root, name, profile)
        .expect("create new ordinary sample");
    created
        .write_all(b"initial fixture bytes")
        .expect("write sample");
    let owner = OwnedFixtureEntry::capture(&created, &fixture.path.join(name), EntryKind::File)
        .expect("retain the created sample");

    // Keep the original writer open: rewrite does not reopen the name or replace the object.
    created
        .seek(SeekFrom::Start(0))
        .expect("rewind retained sample");
    created.write_all(b"new").expect("rewrite retained sample");
    created.set_len(3).expect("truncate original sample");
    created.sync_all().expect("flush the ordinary sample");
    owner
        .require_same(&created)
        .expect("same sample after rewrite");
    drop(created);

    let duplicate = create_file_relative_with_profile(&fixture.root, name, profile)
        .expect_err("exclusive creation must not replace a present sample");
    assert_eq!(duplicate.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(
        fs::read(fixture.path.join(name)).expect("check rewritten sample"),
        b"new"
    );
    owner
        .remove(&fixture.root, name)
        .expect("retire rewritten sample");
    drop(owner);
    assert_absent(&fixture.root, name);
    fixture.finish();
}

#[test]
fn nsis_fixture_ordinary_lifecycle_move_then_retire_child_before_parent() {
    use super::super::publish_new_record_file;

    let fixture = Fixture::new();
    let stage_name = "ordinary-stage";
    let stage =
        create_directory_relative(&fixture.root, stage_name).expect("create ordinary stage");
    let stage_path = fixture.path.join(stage_name);
    let stage_owner = OwnedFixtureEntry::capture(&stage, &stage_path, EntryKind::Directory)
        .expect("retain created stage");
    // This private ordinary test owns both leaf names; the move is not a public
    // authorization primitive. Keep the created object's identity handle live
    // and require that exact object before using the new name for retirement.
    let owner = create_file(&stage, &stage_path, "pending-sample", b"ordinary sample");
    publish_new_record_file(&stage, "pending-sample", "published-sample")
        .expect("move ordinary file to absent sibling");
    assert_absent(&stage, "pending-sample");
    let observed = open_for_delete(&stage, "published-sample", DeletePurpose::Remove)
        .expect("open ordinary moved sample relative to retained stage");
    owner
        .require_same(observed.file())
        .expect("moved object identity");
    drop(observed);
    assert_eq!(
        fs::read(stage_path.join("published-sample")).expect("read ordinary moved sample"),
        b"ordinary sample"
    );

    owner
        .remove(&stage, "published-sample")
        .expect("retire owned child");
    drop(owner);
    assert_absent(&stage, "published-sample");
    drop(stage);
    stage_owner
        .remove(&fixture.root, stage_name)
        .expect("retire now-empty owned stage");
    drop(stage_owner);
    assert_absent(&fixture.root, stage_name);
    fixture.finish();
}
