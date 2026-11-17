//! Native macOS filesystem flags required for owner-private dev-stage paths.

use std::io;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use rustix::fs::{Mode, OFlags, fstatfs, open};

// Darwin 26.5 SDK sys/mount.h: MNT_LOCAL, MNT_REMOVABLE and
// MNT_IGNORE_OWNERSHIP. The last flag disables OS UID ownership enforcement.
const MNT_LOCAL: u32 = 0x0000_1000;
const MNT_REMOVABLE: u32 = 0x0000_0200;
const MNT_IGNORE_OWNERSHIP: u32 = 0x0020_0000;

fn owner_enforcing_local_volume(filesystem: &[u8], flags: u32, mount_owner: u32, uid: u32) -> bool {
    matches!(filesystem, b"apfs" | b"hfs")
        && flags & MNT_LOCAL != 0
        && flags & (MNT_IGNORE_OWNERSHIP | MNT_REMOVABLE) == 0
        && (mount_owner == 0 || mount_owner == uid)
}

/// Validates an opened real directory's macOS mount ownership contract.
///
/// A volume mounted with Ignore Ownership can report private Unix mode and
/// the current user as owner while other users can change its entries.
/// Only the boot/data system volumes are accepted: a different account can
/// mount an arbitrary local APFS disk image with genuine owner mode bits, then
/// replace it after check. Reject foreign mount owners and unsupported mount
/// types as well. This is a fail-closed *dev* admission, not a release seal.
///
/// # Errors
///
/// Returns an error for a non-directory or symlink, an unreadable mount
/// status, or a mount that cannot prove local ownership enforcement.
pub fn validate_macos_dev_stage_volume(path: &Path, uid: u32) -> io::Result<()> {
    let directory = open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let stat = fstatfs(&directory).map_err(io::Error::from)?;
    let opened = std::fs::File::from(directory);
    let selected_dev = opened.metadata()?.dev();
    // Mac's system and user-data volumes are OS-owned mount anchors. External
    // disk images, FUSE and removable filesystems cannot inherit their dev_t.
    let system_dev = std::fs::metadata("/")?.dev();
    let data_dev = std::fs::metadata("/System/Volumes/Data")?.dev();
    let filesystem = stat
        .f_fstypename
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .map(i8::cast_unsigned)
        .collect::<Vec<_>>();
    if !owner_enforcing_local_volume(&filesystem, stat.f_flags, stat.f_owner, uid)
        || (selected_dev != system_dev && selected_dev != data_dev)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{}: the mount does not prove system-owned, local owner-enforced APFS/HFS+ semantics",
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
        const UID: u32 = 501;
        assert!(owner_enforcing_local_volume(b"apfs", MNT_LOCAL, 0, UID));
        assert!(owner_enforcing_local_volume(b"hfs", MNT_LOCAL, UID, UID));
        for (type_name, flags) in [
            (b"apfs".as_slice(), MNT_LOCAL | MNT_IGNORE_OWNERSHIP),
            (b"apfs".as_slice(), MNT_LOCAL | MNT_REMOVABLE),
            (b"apfs".as_slice(), 0),
            (b"nfs".as_slice(), 0),
            (b"osxfuse".as_slice(), MNT_LOCAL),
            (b"exfat".as_slice(), MNT_LOCAL),
        ] {
            assert!(
                !owner_enforcing_local_volume(type_name, flags, 0, UID),
                "{type_name:?} {flags:#x} cannot protect a staged pathname"
            );
        }
        assert!(
            !owner_enforcing_local_volume(b"apfs", MNT_LOCAL, UID + 1, UID),
            "a foreign-owned APFS mount is not a trusted dev-stage ancestor"
        );
    }

    #[test]
    fn real_native_owner_enforcing_mount_accepts_canonical_temporary_directory() {
        let root = std::env::temp_dir()
            .canonicalize()
            .expect("canonical native macOS temporary root");
        let uid = std::fs::metadata(&root)
            .expect("owner-private test root metadata")
            .uid();
        validate_macos_dev_stage_volume(&root, uid).expect("owned system/data macOS volume");
    }
}
