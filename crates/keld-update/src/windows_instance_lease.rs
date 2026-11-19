//! Retained filesystem selection for the cooperative Windows Host election.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _, OpenOptionsMaybeDirExt as _};
use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt as _};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ,
    FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use crate::{retained_graph_metadata, windows_extraction::open_ancestors, windows_fs};

#[derive(Debug)]
struct LeaseIoError {
    operation: &'static str,
    component: &'static str,
    source: io::Error,
}

impl std::fmt::Display for LeaseIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} {}: {}",
            self.operation, self.component, self.source
        )
    }
}

impl std::error::Error for LeaseIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

fn context(
    operation: &'static str,
    component: &'static str,
) -> impl FnOnce(io::Error) -> io::Error {
    move |source| {
        io::Error::new(
            source.kind(),
            LeaseIoError {
                operation,
                component,
                source,
            },
        )
    }
}

fn service_component(name: &str) -> &'static str {
    match name {
        "Keld" => "Keld",
        "InstanceLocks" => "InstanceLocks",
        "v1" => "v1",
        "lease" => "lease",
        _ => "declared ID directory",
    }
}

/// A single-link lease file and the retained directories selecting it.
#[derive(Debug)]
pub struct WindowsInstanceLeaseFile {
    file: File,
    ancestors: Vec<File>,
    local_index: usize,
    path: PathBuf,
}

impl WindowsInstanceLeaseFile {
    /// Selects an owner-private lease under the native current user's data directory.
    /// The trusted Host supplies its token's known folder and validated cooperative ID.
    ///
    /// # Errors
    /// Refuses reparse points, foreign mutation rights, invalid components, and nonlocal volumes.
    pub fn open(local_app_data: &Path, declared_id: &str) -> io::Result<Self> {
        let id = format!("id-{declared_id}");
        keld_guard::validate_fs_component(&id)
            .map_err(io::Error::other)
            .map_err(context("validate spelling of", "declared ID directory"))?;
        let mut ancestors: Vec<_> = open_ancestors(local_app_data)
            .map_err(context("retain ancestry of", "LocalAppData"))?
            .into_iter()
            .map(Dir::into_std_file)
            .collect();
        let local_index = ancestors
            .len()
            .checked_sub(1)
            .ok_or_else(|| io::Error::other("missing native local data root"))?;
        let local = &ancestors[local_index];
        let mut path = local_app_data.to_path_buf();
        windows_fs::qualify_volume(local).map_err(context("qualify volume of", "LocalAppData"))?;
        keld_guard::validate_windows_user_service_ancestor(local)
            .map_err(context("verify ACL of", "LocalAppData"))?;
        for (index, name) in ["Keld", "InstanceLocks", "v1", id.as_str()]
            .into_iter()
            .enumerate()
        {
            let parent = ancestors
                .last()
                .ok_or_else(|| io::Error::other("missing lease ancestor"))?;
            let child = open_or_create(parent, name, true, || {
                windows_fs::create_instance_directory(parent, name)
            })?;
            if index == 0 {
                keld_guard::validate_windows_user_service_ancestor(&child)
                    .map_err(context("verify ACL of", service_component(name)))?;
            } else {
                keld_guard::validate_windows_owner_private_directory(&child)
                    .map_err(context("verify ACL of", service_component(name)))?;
            }
            ancestors.push(child);
            path.push(name);
        }
        let parent = ancestors
            .last()
            .ok_or_else(|| io::Error::other("missing instance directory"))?;
        let file = open_or_create(parent, "lease", false, || {
            windows_fs::create_instance_lease_file(parent)
        })?;
        let selection = Self {
            file,
            ancestors,
            local_index,
            path: path.join("lease"),
        };
        selection.validate()?;
        Ok(selection)
    }

    /// The retained file whose kernel lock determines the election.
    #[must_use]
    pub const fn file(&self) -> &File {
        &self.file
    }

    /// Rechecks type, volume, link count, owner and DACL through the retained handles.
    ///
    /// # Errors
    /// Returns an error when the lease or an ancestor no longer meets its contract.
    pub fn validate(&self) -> io::Result<()> {
        let volume = retained_graph_metadata(&self.ancestors[self.local_index])
            .map_err(context("read volume of", "LocalAppData"))?
            .dev();
        let mut names = self.path.ancestors();
        let leaf = selected_identity(&self.file, names.next())
            .map_err(context("verify identity of", "lease"))?;
        for (index, directory) in self.ancestors.iter().enumerate().rev() {
            let name = names.next();
            let component = match index.checked_sub(self.local_index) {
                None => "native ancestor",
                Some(0) => "LocalAppData",
                Some(_) => service_component(
                    name.and_then(Path::file_name)
                        .and_then(|part| part.to_str())
                        .unwrap_or(""),
                ),
            };
            let meta = selected_identity(directory, name)
                .map_err(context("verify identity of", component))?;
            if !meta.is_dir() || meta.dev() != volume {
                return Err(context("verify type and volume of", component)(
                    io::Error::other("invalid directory or volume in instance ancestry"),
                ));
            }
            if index == self.local_index || index == self.local_index + 1 {
                keld_guard::validate_windows_user_service_ancestor(directory)
                    .map_err(context("recheck ACL of", component))?;
            } else if index > self.local_index {
                keld_guard::validate_windows_owner_private_directory(directory)
                    .map_err(context("recheck ACL of", component))?;
            }
        }
        if names.next().is_some() || !leaf.is_file() || leaf.dev() != volume {
            return Err(io::Error::other(
                "lease is not a single-link file on the selected volume",
            ));
        }
        keld_guard::validate_windows_owner_private_file(&self.file)
            .map_err(context("recheck ACL of", "lease"))
    }
}

fn selected_identity(file: &File, path: Option<&Path>) -> io::Result<crate::WindowsGraphMetadata> {
    let name = path.ok_or_else(|| io::Error::other("missing selected instance name"))?;
    let held = retained_graph_metadata(file)?;
    let named = crate::graph_locator_metadata(name)?;
    if (held.dev(), held.ino()) != (named.dev(), named.ino()) {
        return Err(io::Error::other("selected instance object changed"));
    }
    Ok(held)
}

fn open_or_create(
    parent: &File,
    name: &str,
    directory: bool,
    create: impl FnOnce() -> io::Result<File>,
) -> io::Result<File> {
    let label = service_component(name);
    match create() {
        Ok(file) => return Ok(file),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(context("create", label)(error)),
    }
    let parent = Dir::from_std_file(
        parent
            .try_clone()
            .map_err(context("retain parent of", label))?,
    );
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .follow(FollowSymlinks::No);
    if directory {
        options
            .maybe_dir(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .access_mode(FILE_GENERIC_READ | FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY);
    } else {
        options
            .write(true)
            .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE);
    }
    Ok(parent
        .open_with(name, &options)
        .map_err(context("reopen", label))?
        .into_std())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, TryLockError};
    use std::os::windows::fs::OpenOptionsExt as _;
    use std::process::{Command, Stdio};

    fn local_root(fixture: &tempfile::TempDir) -> std::path::PathBuf {
        let parent = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(fixture.path())
            .expect("fixture parent");
        windows_fs::create_directory_relative(&parent, "local")
            .expect("isolated user-only local root");
        fixture.path().join("local")
    }

    #[test]
    fn election_retains_same_leaf_and_distinct_app_names() {
        let fixture = tempfile::tempdir().expect("owned test root");
        let root = local_root(&fixture);
        let one = WindowsInstanceLeaseFile::open(&root, "con.example").expect("first selection");
        one.file().try_lock().expect("holder");
        one.validate().expect("post-election identity and ACL");
        assert_child_election(&root, "contended");
        let two = WindowsInstanceLeaseFile::open(&root, "con.example").expect("second selection");
        assert!(matches!(
            two.file().try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        let other = WindowsInstanceLeaseFile::open(&root, &"a".repeat(128))
            .expect("full-length different ID");
        other.file().try_lock().expect("different ID acquires");
        let leaf = root.join("Keld/InstanceLocks/v1/id-con.example/lease");
        assert!(leaf.is_file());
        assert_eq!(
            fs::rename(&leaf, leaf.with_extension("replaced"))
                .expect_err("deny delete sharing")
                .raw_os_error(),
            Some(32)
        );
        drop(one);
        assert_child_election(&root, "acquired");
        two.file()
            .try_lock()
            .expect("acquires after original handle release");
    }

    const CHILD_ROOT: &str = "KELD_TEST_INSTANCE_LEASE_ROOT";
    const CHILD_OUTCOME: &str = "KELD_TEST_INSTANCE_LEASE_OUTCOME";

    fn assert_child_election(root: &Path, outcome: &str) {
        use std::io::Read as _;
        let mut child = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "windows_instance_lease::tests::instance_lease_child",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD_ROOT, root)
            .env(CHILD_OUTCOME, outcome)
            .stdout(Stdio::piped())
            .spawn()
            .expect("independent election process");
        let mut stdout = child.stdout.take().expect("child stdout");
        let (done, result) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut evidence = String::new();
            stdout
                .read_to_string(&mut evidence)
                .expect("child evidence");
            let _ = done.send(evidence);
        });
        let received = result.recv_timeout(std::time::Duration::from_secs(20));
        if received.is_err() {
            let _ = child.kill();
        }
        let exit = child.wait().expect("reap exact election process");
        reader.join().expect("drain child stdout");
        let stdout = received.expect("election completed before outer deadline");
        assert!(
            exit.success() && stdout.contains("INSTANCE_ELECTION_VERIFIED"),
            "{exit}: {stdout}"
        );
    }

    #[test]
    #[ignore = "subprocess actor for the ordinary independent election test"]
    fn instance_lease_child() {
        let root = std::env::var_os(CHILD_ROOT).expect("explicit fixture root");
        let selected = WindowsInstanceLeaseFile::open(Path::new(&root), "con.example")
            .expect("child selects same leaf");
        let outcome = match selected.file().try_lock() {
            Ok(()) => "acquired",
            Err(TryLockError::WouldBlock) => "contended",
            Err(error) => panic!("kernel election failed: {error}"),
        };
        selected
            .validate()
            .expect("revalidates actual child selection");
        assert_eq!(
            outcome,
            std::env::var(CHILD_OUTCOME).expect("expected case")
        );
        println!("INSTANCE_ELECTION_VERIFIED");
    }

    #[test]
    fn linked_or_incompatibly_opened_leaf_refuses_before_election() {
        let fixture = tempfile::tempdir().expect("owned test root");
        let root = local_root(&fixture);
        drop(WindowsInstanceLeaseFile::open(&root, "test.app").expect("create lease"));
        let leaf = root.join("Keld/InstanceLocks/v1/id-test.app/lease");
        let alias = root.join("alias");
        fs::hard_link(&leaf, &alias).expect("fixture second name");
        let linked =
            WindowsInstanceLeaseFile::open(&root, "test.app").expect_err("multiply linked lease");
        assert!(
            linked.to_string().contains("verify identity of lease"),
            "{linked}"
        );
        fs::remove_file(&alias).expect("remove test alias");
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&leaf)
            .expect("incompatible handle");
        let incompatible =
            WindowsInstanceLeaseFile::open(&root, "test.app").expect_err("share-zero lease");
        assert!(
            incompatible.to_string().starts_with("reopen lease: "),
            "{incompatible}"
        );
        let context = incompatible
            .get_ref()
            .and_then(|error| error.downcast_ref::<LeaseIoError>())
            .expect("named operation retains its Windows cause");
        assert_eq!(context.source.raw_os_error(), Some(32));
        assert_eq!(incompatible.kind(), context.source.kind());
        drop(held);
        WindowsInstanceLeaseFile::open(&root, "test.app")
            .expect("valid same leaf after incompatible handle exits");
    }
}
