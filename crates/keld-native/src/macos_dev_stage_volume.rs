//! Native macOS filesystem flags required for owner-private dev-stage paths.
#![allow(unsafe_code)] // GH-1563: exact descriptor ACL inspection sanctioned by crate AGENTS.md.
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::{c_int, c_void};
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use rustix::fs::{Mode, OFlags, fstatfs, open};

// Darwin 26.5 SDK sys/mount.h: MNT_LOCAL, MNT_REMOVABLE and
// MNT_IGNORE_OWNERSHIP. The last flag disables OS UID ownership enforcement.
const MNT_LOCAL: u32 = 0x0000_1000;
const MNT_REMOVABLE: u32 = 0x0000_0200;
const MNT_IGNORE_OWNERSHIP: u32 = 0x0020_0000;

// macOS SDK sys/acl.h. These calls only inspect an already opened directory.
// acl_get_fd_np returns a null ACL with ENOENT when no extended ACL exists.
const ACL_TYPE_EXTENDED: c_int = 0x100;
const ACL_FIRST_ENTRY: c_int = 0;
const ACL_NEXT_ENTRY: c_int = -1;
const ACL_EXTENDED_ALLOW: c_int = 1;
const ACL_EXTENDED_DENY: c_int = 2;
const ACL_FLAG_DEFER_INHERIT: c_int = 1;
const ACL_ENTRY_FILE_INHERIT: c_int = 1 << 5;
const ACL_ENTRY_DIRECTORY_INHERIT: c_int = 1 << 6;
const ACL_ENTRY_ONLY_INHERIT: c_int = 1 << 8;
const ENOENT: i32 = 2;
const EINVAL: i32 = 22;

// Read/list, search, read attributes, read extended attributes, read security
// and synchronize cannot mutate a directory entry or its ownership. Unknown
// permission bits fail closed. These masks follow Darwin sys/acl.h.
const ACL_READ_ONLY_PERMISSIONS: u64 =
    (1 << 1) | (1 << 3) | (1 << 7) | (1 << 9) | (1 << 11) | (1 << 20);

unsafe extern "C" {
    fn acl_get_fd_np(fd: c_int, acl_type: c_int) -> *mut c_void;
    fn acl_free(acl: *mut c_void) -> c_int;
    fn acl_valid(acl: *mut c_void) -> c_int;
    fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
    fn acl_get_tag_type(entry: *mut c_void, tag_type: *mut c_int) -> c_int;
    fn acl_get_permset_mask_np(entry: *mut c_void, mask: *mut u64) -> c_int;
    fn acl_get_flagset_np(object: *mut c_void, flags: *mut *mut c_void) -> c_int;
    fn acl_get_flag_np(flags: *mut c_void, flag: c_int) -> c_int;
}

struct OwnedAcl(*mut c_void);

impl Drop for OwnedAcl {
    fn drop(&mut self) {
        // SAFETY: acl_get_fd_np returned this nonnull ACL pointer and this is its
        // only owner. Entries are only borrowed while this object is alive.
        let _ = unsafe { acl_free(self.0) };
    }
}

fn validate_open_directory_acl(directory: &std::fs::File) -> io::Result<()> {
    // SAFETY: directory is an open, live O_DIRECTORY fd. The library borrows it
    // only for this call; a returned ACL is separately freed by OwnedAcl.
    let acl = unsafe { acl_get_fd_np(directory.as_raw_fd(), ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ENOENT) {
            Ok(())
        } else {
            Err(error)
        };
    }
    let acl = OwnedAcl(acl);
    // SAFETY: the ACL is a valid owned pointer for the duration of this call.
    if unsafe { acl_valid(acl.0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // DEFER_INHERIT belongs to the ACL object, not an individual entry.
    // Refuse any deferred propagation state before examining its ACEs.
    let mut acl_flags = std::ptr::null_mut();
    // SAFETY: acl remains owned for the call and the flag set is borrowed from it.
    if unsafe { acl_get_flagset_np(acl.0, &raw mut acl_flags) } != 0 || acl_flags.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "macOS ACL object flags are unavailable",
        ));
    }
    // SAFETY: acl_flags borrows from the live ACL; this is a Darwin sys/acl.h bit.
    match unsafe { acl_get_flag_np(acl_flags, ACL_FLAG_DEFER_INHERIT) } {
        0 => {}
        1 => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "macOS ACL has deferred inheritance state",
            ));
        }
        _ => return Err(io::Error::last_os_error()),
    }
    let mut next = ACL_FIRST_ENTRY;
    loop {
        let mut entry = std::ptr::null_mut();
        // SAFETY: the ACL remains owned throughout the loop, and the entry
        // out-pointer is valid. Darwin returns 0 for an entry; after the last
        // entry ACL_NEXT_ENTRY returns -1/EINVAL (acl_get_entry(3)).
        match unsafe { acl_get_entry(acl.0, next, &raw mut entry) } {
            0 if !entry.is_null() => {}
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "null macOS ACL entry",
                ));
            }
            _ if next == ACL_NEXT_ENTRY
                && io::Error::last_os_error().raw_os_error() == Some(EINVAL) =>
            {
                return Ok(());
            }
            _ => return Err(io::Error::last_os_error()),
        }
        next = ACL_NEXT_ENTRY;
        let mut tag = 0;
        // SAFETY: entry borrows from the live ACL, and tag is writable.
        if unsafe { acl_get_tag_type(entry, &raw mut tag) } != 0 {
            return Err(io::Error::last_os_error());
        }
        match tag {
            ACL_EXTENDED_DENY => {}
            ACL_EXTENDED_ALLOW => {
                let mut mask = 0_u64;
                // SAFETY: entry borrows from the live ACL; mask is writable.
                if unsafe { acl_get_permset_mask_np(entry, &raw mut mask) } != 0 {
                    return Err(io::Error::last_os_error());
                }
                if mask & !ACL_READ_ONLY_PERMISSIONS != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "macOS ACL grants directory mutation or unsupported rights",
                    ));
                }
                // Directory-only read grants can be safe on an ancestor,
                // but file_inherit and directory_inherit propagate them to
                // new 0400 staged files and 0700 children. On a file, LIST
                // means READ and SEARCH means EXECUTE. Reject propagation.
                let mut flags = std::ptr::null_mut();
                // SAFETY: entry borrows from the live ACL; flags is writable,
                // and the returned flag set remains borrowed from that ACL.
                if unsafe { acl_get_flagset_np(entry, &raw mut flags) } != 0 || flags.is_null() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "macOS ACL flag set is unavailable",
                    ));
                }
                for flag in [
                    ACL_ENTRY_FILE_INHERIT,
                    ACL_ENTRY_DIRECTORY_INHERIT,
                    ACL_ENTRY_ONLY_INHERIT,
                ] {
                    // SAFETY: flags borrows from the live ACL and flag names a
                    // Darwin sys/acl.h bit. The API returns 0, 1, or -1.
                    match unsafe { acl_get_flag_np(flags, flag) } {
                        0 => {}
                        1 => {
                            return Err(io::Error::new(
                                io::ErrorKind::PermissionDenied,
                                "macOS ACL can propagate read access into private stage files",
                            ));
                        }
                        _ => return Err(io::Error::last_os_error()),
                    }
                }
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unknown macOS ACL tag",
                ));
            }
        }
    }
}

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
    // ACLs can grant namespace mutation even with POSIX mode 0700. Inspect the
    // same opened inode without launching a child during boot preflight.
    validate_open_directory_acl(&opened)?;
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
