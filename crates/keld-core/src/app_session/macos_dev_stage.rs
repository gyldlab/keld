//! macOS owner-private developer-stage ancestry, shared by the CLI producer
//! and the no-flag Host/Guardian consumers.

use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::Path;

fn unsafe_namespace(path: &Path, detail: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{}: {detail}", path.display()),
    )
}

/// Refuse a directory that another local account could replace or control.
///
/// This checks POSIX owner/mode as well as macOS extended ACL entries, which
/// can grant rights independently of mode bits. The caller must also validate
/// every ancestor: a safe leaf beneath a replaceable parent is not safe.
///
/// # Errors
///
/// Returns an error for a symlink, foreign owner, replaceable directory, ACL
/// grant or unreadable metadata. Does not modify an existing directory.
pub fn validate_macos_dev_stage_directory(path: &Path) -> io::Result<()> {
    let uid = nix::unistd::geteuid().as_raw();
    // A mode/UID check is not an authority check on noowners volumes. Inspect
    // the actual opened mount before relying on the directory's metadata.
    keld_native::macos_dev_stage_volume::validate_macos_dev_stage_volume(path, uid)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || (metadata.uid() != uid && metadata.uid() != 0) {
        return Err(unsafe_namespace(
            path,
            "not an invoking-user or root-owned non-symlink directory",
        ));
    }
    let mode = metadata.permissions().mode();
    // A sticky directory prevents some renames, but other UIDs may create
    // node_modules there. Bun searches upward for packages when resolving a
    // staged app's bare imports, so even a sticky ancestor is unsafe.
    if mode & 0o022 != 0 {
        return Err(unsafe_namespace(
            path,
            "grants another principal namespace write access",
        ));
    }

    // The native mount validator also inspects the ACL on its retained
    // O_NOFOLLOW directory descriptor. This remains process-free before boot
    // and guard preflight. Read-only allow entries are safe; write grants fail.
    let after = fs::symlink_metadata(path)?;
    if after.dev() != metadata.dev() || after.ino() != metadata.ino() {
        return Err(unsafe_namespace(
            path,
            "directory identity changed while validating its namespace",
        ));
    }
    Ok(())
}

/// Verify that no other principal can replace an accepted developer-stage path.
///
/// Inspect the complete ancestry from the filesystem root through the selected
/// directory; each accepted parent makes the next child's pathname stable
/// against other local UIDs. Call this before stage creation, before Host boot
/// admission, and again in Guardian before each Bun generation. This is a
/// non-release cross-user property, not same-user anti-tamper or authentication.
///
/// # Errors
///
/// Returns an error if any ancestor cannot be proved safe.
pub fn validate_macos_dev_stage_ancestry(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        validate_macos_dev_stage_directory(ancestor)?;
    }
    Ok(())
}
