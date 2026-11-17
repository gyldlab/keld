//! Native macOS filesystem flags required for owner-private dev-stage paths.

use std::io;
use std::path::Path;

use rustix::fs::{Mode, OFlags, fstatfs, open};

// Darwin 26.5 SDK sys/mount.h: MNT_LOCAL, MNT_REMOVABLE and
// MNT_IGNORE_OWNERSHIP. The last flag disables OS UID ownership enforcement.
const MNT_LOCAL: u32 = 0x0000_1000;
const MNT_REMOVABLE: u32 = 0x0000_0200;
const MNT_IGNORE_OWNERSHIP: u32 = 0x0020_0000;

fn owner_enforcing_local_volume(filesystem: &[u8], flags: u32) -> bool {
    matches!(filesystem, b"apfs" | b"hfs")
        && flags & MNT_LOCAL != 0
        && flags & (MNT_IGNORE_OWNERSHIP | MNT_REMOVABLE) == 0
}

/// Validates an opened real directory's macOS mount ownership contract.
///
/// A volume mounted with Ignore Ownership can report private Unix mode and
/// the current user as owner while other users can change its entries.
/// Reject unsupported, non-local and removable volumes as well: their
/// filesystem or mount custody is not established by the developer-stage
/// path/UID checks. This is a fail-closed *dev* admission, not a release seal.
///
/// # Errors
///
/// Returns an error for a non-directory or symlink, an unreadable mount
/// status, or a mount that cannot prove local ownership enforcement.
pub fn validate_macos_dev_stage_volume(path: &Path) -> io::Result<()> {
    let directory = open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let stat = fstatfs(&directory).map_err(io::Error::from)?;
    let filesystem = stat
        .f_fstypename
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .map(i8::cast_unsigned)
        .collect::<Vec<_>>();
    if !owner_enforcing_local_volume(&filesystem, stat.f_flags) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{}: the mount does not prove local owner-enforced, non-removable APFS/HFS+ semantics",
                path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_flag_classification_rejects_ignore_ownership_and_unsupported_mounts() {
        assert!(owner_enforcing_local_volume(b"apfs", MNT_LOCAL));
        assert!(owner_enforcing_local_volume(b"hfs", MNT_LOCAL));
        for (type_name, flags) in [
            (b"apfs".as_slice(), MNT_LOCAL | MNT_IGNORE_OWNERSHIP),
            (b"apfs".as_slice(), MNT_LOCAL | MNT_REMOVABLE),
            (b"apfs".as_slice(), 0),
            (b"nfs".as_slice(), 0),
            (b"osxfuse".as_slice(), MNT_LOCAL),
            (b"exfat".as_slice(), MNT_LOCAL),
        ] {
            assert!(
                !owner_enforcing_local_volume(type_name, flags),
                "{type_name:?} {flags:#x} cannot protect a staged pathname"
            );
        }
    }

    #[test]
    fn real_native_owner_enforcing_mount_accepts_canonical_temporary_directory() {
        let root = std::env::temp_dir()
            .canonicalize()
            .expect("canonical native macOS temporary root");
        validate_macos_dev_stage_volume(&root).expect("owned local macOS volume");
    }
}
