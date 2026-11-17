//! macOS owner-private developer-stage ancestry, shared by the CLI producer
//! and the no-flag Host/Guardian consumers.

use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::Path;
use std::process::Command;

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
    // A mode/UID check is not an authority check on noowners volumes. Inspect
    // the actual opened mount before relying on the directory's metadata.
    keld_native::macos_dev_stage_volume::validate_macos_dev_stage_volume(path)?;
    let uid = nix::unistd::geteuid().as_raw();
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || (metadata.uid() != uid && metadata.uid() != 0) {
        return Err(unsafe_namespace(
            path,
            "not an invoking-user or root-owned non-symlink directory",
        ));
    }
    let mode = metadata.permissions().mode();
    // A sticky directory protects another user's entry from rename by a
    // third party, but only if that party does not own the directory.
    if mode & 0o022 != 0 && mode & 0o1000 == 0 {
        return Err(unsafe_namespace(
            path,
            "grants another principal namespace write access",
        ));
    }

    // A mode-0700 directory may still grant delete_child or add_file using a
    // macOS extended ACL. Use the installed read-only inspector because this
    // crate does not own permission to introduce unsafe ACL FFI. Output that
    // cannot be validated is refusal, not authority. No shell is involved.
    let output = Command::new("/bin/ls")
        .args(["-ldeq", "--"])
        .arg(path)
        .env("LC_ALL", "C")
        .output()?;
    let report = std::str::from_utf8(&output.stdout)
        .map_err(|error| unsafe_namespace(path, &format!("unreadable ACL: {error}")))?;
    let mut lines = report.lines();
    if !output.status.success() || lines.next().is_none() {
        return Err(unsafe_namespace(path, "unreadable macOS directory ACL"));
    }
    for entry in lines {
        let Some((index, text)) = entry.trim_start().split_once(':') else {
            return Err(unsafe_namespace(path, "unrecognized macOS ACL entry"));
        };
        let fields = text.split_ascii_whitespace().collect::<Vec<_>>();
        if index.is_empty()
            || !index.bytes().all(|byte| byte.is_ascii_digit())
            || !fields.contains(&"deny")
            || fields.contains(&"allow")
        {
            return Err(unsafe_namespace(path, "unsafe or unrecognized macOS ACL"));
        }
    }
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
