//! Test-only ownership of the fixed NSIS diagnostic fixture inventory.
//! No production API, signer, or arbitrary path cleanup is exposed.

use super::{Fixture, OwnedFixtureEntry, entry_kind, metadata, retained_directory};
use crate::windows_fs::tests::support::{
    clear_mount_point_on_open_directory, set_mount_point_on_open_directory,
};
use crate::windows_fs::{EntryKind, create_directory_relative, create_file_relative_with_profile};
use cap_fs_ext::MetadataExt as _;
use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
};

const DIRECTORIES: &[&str] = &[
    "stage",
    "stage/bin",
    "stage/recovery",
    "stage/bindings",
    "tampered-nsis",
    "outside",
    "alias",
    "shadow-hash",
    "shadow-signer",
    "shadow-modules",
    "shadow-modules/Microsoft.PowerShell.Security",
    "mutant-signer",
    "compiler-parent",
    "compiler-parent/pre-publication",
    "compiler-parent/alias-target",
    "compiler-parent/published-alias",
];

const FILES: &[&str] = &[
    "stage/bin/keld-host.exe",
    "stage/recovery/keld-host.exe",
    "stage/bin/keld.exe",
    "stage/bin/keld-role-launcher.exe",
    "stage/bin/keld-updater-helper.exe",
    "stage/bindings/keld-mcp-stdio.json",
    "UNCOMMISSIONED-CATALOGUE.bin",
    "stage/manifest.json",
    "tampered-nsis/windows_v2_nsis_build.ps1",
    "tampered-nsis/windows_v2_nsis_installer.nsi",
    "shadow-hash/Get-FileHash.ps1",
    "shadow-signer/Get-AuthenticodeSignature.ps1",
    "shadow-modules/Microsoft.PowerShell.Security/Microsoft.PowerShell.Security.psm1",
    "mutant-signer/windows_v2_nsis_installer.nsi",
    "mutant-signer/mutant-builder.ps1",
    "compiler-parent/pre-publication/keld-v2-peruserdirect-UNSIGNED.exe",
    "compiler-parent/alias-target/keld-v2-peruserdirect-UNSIGNED.exe",
];

struct OwnedDirectory {
    relative: &'static str,
    parent: Option<usize>,
    name: &'static str,
    owner: OwnedFixtureEntry,
    directory: Option<File>,
    junction: Option<File>,
}

struct OwnedFile {
    parent: Option<usize>,
    name: &'static str,
    owner: OwnedFixtureEntry,
    // Pin fixed ordinary files against deletion. No child write or directory
    // move authority is conveyed by this test-only inventory.
    pin: File,
}

struct DiagnosticOwner {
    fixture: Fixture,
    directories: Vec<OwnedDirectory>,
    files: Vec<OwnedFile>,
}

fn split_relative(relative: &'static str) -> (Option<&'static str>, &'static str) {
    relative
        .rsplit_once('/')
        .map_or((None, relative), |(parent, name)| (Some(parent), name))
}

impl DiagnosticOwner {
    fn directory_index(&self, relative: &str) -> usize {
        self.directories
            .iter()
            .position(|directory| directory.relative == relative)
            .expect("fixed diagnostic parent")
    }

    fn parent(&self, index: Option<usize>) -> &File {
        index.map_or(&self.fixture.root, |index| {
            self.directories[index]
                .directory
                .as_ref()
                .expect("ordinary retained diagnostic directory")
        })
    }

    fn new() -> Self {
        Self::new_with_alias_payload(None)
    }

    fn new_with_alias_payload(alias_payload: Option<&[u8]>) -> Self {
        let mut tree = Self {
            fixture: Fixture::new(),
            directories: Vec::new(),
            files: Vec::new(),
        };

        for &relative in DIRECTORIES {
            let (parent_name, name) = split_relative(relative);
            let parent = parent_name.map(|value| tree.directory_index(value));
            let created = create_directory_relative(tree.parent(parent), name)
                .expect("exclusive diagnostic directory creation");
            let path = tree.fixture.path.join(relative);
            let owner = OwnedFixtureEntry::capture(&created, &path, EntryKind::Directory)
                .expect("pin diagnostic directory at creation");
            tree.directories.push(OwnedDirectory {
                relative,
                parent,
                name,
                owner,
                directory: Some(created),
                junction: None,
            });
        }

        for &relative in FILES {
            let (parent_name, name) = split_relative(relative);
            let parent = parent_name.map(|value| tree.directory_index(value));
            let mut created = create_file_relative_with_profile(
                tree.parent(parent),
                name,
                keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
            )
            .expect("exclusive diagnostic file creation");
            let path = tree.fixture.path.join(relative);
            let owner = OwnedFixtureEntry::capture(&created, &path, EntryKind::File)
                .expect("pin diagnostic file at creation");
            if let Some(alias_payload) = alias_payload
                && relative == "compiler-parent/alias-target/keld-v2-peruserdirect-UNSIGNED.exe"
            {
                created
                    .write_all(alias_payload)
                    .expect("write alias control through original creation handle");
                created.sync_all().expect("flush alias control");
            }
            let pin = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path)
                .expect("pin fixed diagnostic file without delete sharing");
            owner.require_same(&pin).expect("same diagnostic file");
            drop(created);
            tree.files.push(OwnedFile {
                parent,
                name,
                owner,
                pin,
            });
        }

        tree.make_junction("alias", "outside");
        tree.make_junction(
            "compiler-parent/published-alias",
            "compiler-parent/alias-target",
        );
        tree
    }

    fn make_junction(&mut self, relative: &str, target: &str) {
        let path = self.fixture.path.join(relative);
        let target = self.fixture.path.join(target);
        let index = self.directory_index(relative);
        let directory = &mut self.directories[index];
        let mutation = OpenOptions::new()
            .access_mode(FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .expect("open original empty diagnostic link slot");
        directory
            .owner
            .require_same(&mutation)
            .expect("original diagnostic link slot");
        set_mount_point_on_open_directory(&mutation, &target);
        assert_eq!(
            entry_kind(&mutation).expect("junction kind"),
            EntryKind::Link
        );
        drop(directory.directory.take());
        directory.junction = Some(mutation);
    }

    fn path(&self) -> &Path {
        &self.fixture.path
    }

    // This test-only census never conveys ownership. A name is expected
    // only because the parent already holds its create-new identity token.
    // The nonrecursive delete still refuses an unobserved child at effect
    // time, including a name not representable by the UTF-8 listing helper.
    fn require_fixed_inventory(&self) -> io::Result<()> {
        for parent in std::iter::once(None).chain((0..self.directories.len()).map(Some)) {
            if parent.is_some_and(|index| self.directories[index].directory.is_none()) {
                continue;
            }
            let mut actual = crate::windows_fs::child_names(self.parent(parent))?;
            let mut expected: Vec<_> = self
                .directories
                .iter()
                .filter(|directory| directory.parent == parent)
                .map(|directory| directory.name.to_owned())
                .chain(
                    self.files
                        .iter()
                        .filter(|file| file.parent == parent)
                        .map(|file| file.name.to_owned()),
                )
                .collect();
            actual.sort_unstable();
            expected.sort_unstable();
            if actual != expected {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "diagnostic inventory differs; retain all objects",
                ));
            }
        }
        Ok(())
    }

    fn retire_members(&mut self) -> io::Result<()> {
        self.require_fixed_inventory()?;

        for directory in &mut self.directories {
            if let Some(junction) = directory.junction.take() {
                let expected = metadata(&directory.owner.retained)?;
                let actual = metadata(&junction)?;
                if entry_kind(&junction)? != EntryKind::Link
                    || entry_kind(&directory.owner.retained)? != EntryKind::Link
                    || (expected.dev(), expected.ino()) != (actual.dev(), actual.ino())
                {
                    return Err(io::Error::other(
                        "diagnostic junction identity changed; retain",
                    ));
                }
                // Clear the deliberately created reparse data on the exact
                // retained object, never traverse the target or unlink by path.
                clear_mount_point_on_open_directory(&junction);
                directory.owner.require_same(&junction)?;
                drop(junction);
            }
        }

        while let Some(file) = self.files.pop() {
            drop(file.pin);
            let parent = self.parent(file.parent);
            file.owner.remove(parent, file.name)?;
            drop(file.owner);
            super::assert_absent(parent, file.name);
        }

        while let Some(directory) = self.directories.pop() {
            let parent = self.parent(directory.parent);
            drop(directory.directory);
            directory.owner.remove(parent, directory.name)?;
            drop(directory.owner);
            super::assert_absent(parent, directory.name);
        }

        Ok(())
    }

    fn retire(mut self) -> io::Result<()> {
        self.retire_members()?;
        // Unknown child entries are never adopted; the nonrecursive root
        // removal refuses a nonempty directory and leaves the fixture intact.
        self.fixture.finish();
        Ok(())
    }
}

#[test]
fn owned_diagnostic_inventory_retires_original_fixed_objects() {
    let tree = DiagnosticOwner::new();
    let root = tree.path().to_owned();
    tree.retire()
        .expect("retire exact owned diagnostic inventory");
    assert!(!root.exists(), "owned diagnostic root remains");
}

#[test]
fn owned_diagnostic_alias_child_reads_then_retires_native_inventory() {
    let bytes = [0x4d, 0x5a, 0x01];
    let tree = DiagnosticOwner::new_with_alias_payload(Some(&bytes));
    let root = tree.path().to_owned();
    let target_parent = tree.directory_index("compiler-parent/alias-target");
    let target = tree
        .files
        .iter()
        .find(|file| file.parent == Some(target_parent))
        .expect("owned alias target file");
    let target_path = root.join("compiler-parent/alias-target").join(target.name);
    // The creation token and no-delete pin remain live. A second same-object
    // read pin also excludes data writers during the read-only child.
    let read_pin = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&target_path)
        .expect("pin original alias target against data writes");
    target.owner.require_same(&read_pin).expect("same target");
    // Independent positive control: the intentional alias really resolves to
    // the same fixed bytes before the child must reject it as publication.
    let alias_file = root
        .join("compiler-parent/published-alias")
        .join(target.name);
    assert_eq!(
        std::fs::read(alias_file).expect("alias control bytes"),
        bytes
    );
    let runner =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/windows_v2_nsis_build_test.ps1");
    let mut command =
        std::process::Command::new(super::windows_powershell().expect("Windows PowerShell path"));
    command
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(runner)
        .arg("-OwnedDiagnosticAliasRoot")
        .arg(&root);
    // No retirement on a runner error: child quiescence may be unproven.
    let output = super::run_owned_smoke_child(&mut command, std::time::Duration::from_secs(15))
        .expect("bounded read-only diagnostic child completed");
    target
        .owner
        .require_same(&read_pin)
        .expect("same target after child");
    let mut retained = &read_pin;
    let mut actual = Vec::new();
    std::io::Read::read_to_end(&mut retained, &mut actual).expect("read retained alias target");
    assert_eq!(actual, bytes);
    drop(read_pin);
    tree.retire().expect("retire original diagnostic inventory");
    assert!(!root.exists(), "owned diagnostic root remains");
    let marker: &[u8] = b"KEL270_NSIS_OWNED_DIAGNOSTIC_ALIAS_REFUSED read_only=true";
    assert!(
        output.status.success()
            && output.stderr.is_empty()
            && output
                .stdout
                .strip_suffix(b"\r\n")
                .or_else(|| output.stdout.strip_suffix(b"\n"))
                == Some(marker),
        "read-only diagnostic child outcome: {output:?}"
    );
}

#[test]
fn owned_diagnostic_publication_refuses_open_descendant() {
    // The default diagnostic suite publishes a nonempty directory. NTFS
    // refuses that move even with a metadata-only retained descendant that
    // shares deletion. Do not drop that token and re-adopt an observed name.
    let fixture = Fixture::new();
    let name = "pre-publication";
    let destination = "published-output";
    let path = fixture.path.join(name);
    let created = create_directory_relative(&fixture.root, name).expect("create publication slot");
    let owner = OwnedFixtureEntry::capture(&created, &path, EntryKind::Directory)
        .expect("retain original directory");
    let directory = retained_directory(&path);
    owner.require_same(&directory).expect("same directory");
    drop(created);
    let child = super::create_file(&directory, &path, "payload", b"original bytes");
    let moved_path = fixture.path.join(destination);
    let error = std::fs::rename(&path, &moved_path)
        .expect_err("pathname move with retained descendant must refuse");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    let error = crate::windows_fs::publish_new(&fixture.root, name, destination)
        .expect_err("native move with retained descendant must refuse");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    super::assert_absent(&fixture.root, destination);
    owner
        .require_same(&directory)
        .expect("original directory retained");
    assert_eq!(
        std::fs::read(path.join("payload")).expect("preserved payload"),
        b"original bytes"
    );
    child
        .remove(&directory, "payload")
        .expect("retire original child");
    drop(child);
    super::assert_absent(&directory, "payload");
    drop(directory);
    // Positive control: once the original child is retired, the same native
    // move can publish the empty original directory with its own token live.
    crate::windows_fs::publish_new(&fixture.root, name, destination)
        .expect("publish original empty directory");
    super::assert_absent(&fixture.root, name);
    owner
        .remove(&fixture.root, destination)
        .expect("retire original moved directory");
    drop(owner);
    super::assert_absent(&fixture.root, destination);
    fixture.finish();
}

#[test]
fn owned_diagnostic_inventory_refuses_unclaimed_entry_before_retirement() {
    let mut tree = DiagnosticOwner::new();
    let root = tree.path().to_owned();
    let independent = super::create_file(
        &tree.fixture.root,
        &root,
        "independent-sentinel",
        b"independently owned bytes",
    );
    let error = tree.retire_members().expect_err("extra entry must refuse");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(tree.directories.len(), DIRECTORIES.len());
    assert_eq!(tree.files.len(), FILES.len());
    for directory in &tree.directories {
        if let Some(junction) = &directory.junction {
            assert_eq!(
                entry_kind(junction).expect("retained junction"),
                EntryKind::Link
            );
        }
    }
    assert_eq!(
        std::fs::read(root.join("independent-sentinel")).expect("preserved independent sentinel"),
        b"independently owned bytes"
    );
    // These are separate creation tokens; the diagnostic owner never
    // receives ownership of the independent sentinel.
    independent
        .remove(&tree.fixture.root, "independent-sentinel")
        .expect("retire independently owned sentinel");
    drop(independent);
    super::assert_absent(&tree.fixture.root, "independent-sentinel");
    tree.require_fixed_inventory()
        .expect("all originally owned members remain after refusal");
    tree.retire().expect("retire restored fixed inventory");
    assert!(!root.exists(), "owned diagnostic root remains");
}
