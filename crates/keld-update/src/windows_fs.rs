//! Narrow Windows filesystem operations for protected extraction.

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(unsafe_code)] // Exact volume, relative-directory and sibling-publication operations in AGENTS.md.

use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::ptr;

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_SYNCHRONOUS_IO_NONALERT,
    NtCreateFile,
};
use windows_sys::Win32::Foundation::{
    DuplicateHandle, GetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, RtlNtStatusToDosError, STATUS_SUCCESS, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ,
    FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, GetDriveTypeW, GetFinalPathNameByHandleW,
    GetVolumeInformationByHandleW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    READ_CONTROL, SYNCHRONIZE, VOLUME_NAME_GUID, WRITE_DAC,
};
use windows_sys::Win32::System::IO::{IO_STATUS_BLOCK, IO_STATUS_BLOCK_0};
use windows_sys::Win32::System::SystemServices::{FILE_PERSISTENT_ACLS, FILE_READ_ONLY_VOLUME};
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, FILE_CREATED};

const MAX_WINDOWS_PATH_UNITS: usize = 32_768;
const VOLUME_GUID_ROOT_UNITS: usize = 49;

/// Duplicates an exclusive activation-lock open into a non-writable keeper
/// retention handle. The duplicate references the exact same OS file object and
/// therefore preserves the original share-zero writer exclusion.
pub(crate) fn duplicate_activation_lease_for_keeper(lease: &File) -> io::Result<OwnedHandle> {
    let mut source_flags = 0_u32;
    // SAFETY: `lease` owns a live file handle and `source_flags` is writable.
    if unsafe { GetHandleInformation(lease.as_raw_handle().cast(), &raw mut source_flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if source_flags & HANDLE_FLAG_INHERIT != 0 {
        return Err(unsupported(
            "activation lock handle must be non-inheritable before keeper delegation",
        ));
    }
    let mut duplicate = ptr::null_mut();
    let rights = FILE_READ_ATTRIBUTES | SYNCHRONIZE;
    // SAFETY: source and target are the current process; `lease` owns the source
    // handle, output is writable HANDLE storage, and only read-attributes plus
    // synchronization rights are granted to the non-inheritable duplicate.
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            lease.as_raw_handle().cast(),
            GetCurrentProcess(),
            &raw mut duplicate,
            rights,
            0,
            0,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful DuplicateHandle returned one fresh owning handle.
    let duplicate = unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) };
    let mut duplicate_flags = 0_u32;
    // SAFETY: duplicate is live and duplicate_flags is writable output storage.
    if unsafe { GetHandleInformation(duplicate.as_raw_handle().cast(), &raw mut duplicate_flags) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    if duplicate_flags & HANDLE_FLAG_INHERIT != 0 {
        return Err(unsupported(
            "keeper activation-lock duplicate is inheritable",
        ));
    }
    Ok(duplicate)
}

/// Observes the retained object's fixed, writable NTFS volume with persistent ACLs.
pub(crate) fn qualify_volume(directory: &File) -> io::Result<()> {
    qualified_volume_root(directory).map(|_| ())
}

/// Qualifies the actual retained volume and returns its canonical GUID root.
pub(crate) fn qualified_volume_root(directory: &File) -> io::Result<String> {
    let mut filesystem = [0_u16; 261];
    let mut flags = 0;
    let filesystem_units = u32::try_from(filesystem.len()).map_err(io::Error::other)?;
    // SAFETY: `directory` owns the live handle throughout this synchronous query.
    // Both output pointers refer to initialized writable storage of the exact
    // advertised size; every unused optional output is null. No pointer escapes.
    let success = unsafe {
        GetVolumeInformationByHandleW(
            directory.as_raw_handle().cast(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut flags,
            filesystem.as_mut_ptr(),
            filesystem_units,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    let filesystem_end = filesystem
        .iter()
        .position(|unit| *unit == 0)
        .ok_or_else(|| unsupported("volume filesystem name is not terminated"))?;
    if !filesystem[..filesystem_end]
        .iter()
        .copied()
        .eq("NTFS".encode_utf16())
        || flags & FILE_PERSISTENT_ACLS == 0
        || flags & FILE_READ_ONLY_VOLUME != 0
    {
        return Err(unsupported(
            "extraction requires writable NTFS with persistent ACLs",
        ));
    }

    let path = normalized_guid_path(directory)?;
    let volume_root = volume_guid_root(&path)?;
    // SAFETY: `volume_root` is a checked volume-GUID root with a trailing slash
    // and exactly one terminating NUL, and it remains live for this read-only call.
    let drive_type = unsafe { GetDriveTypeW(volume_root.as_ptr()) };
    if drive_type != DRIVE_FIXED {
        return Err(unsupported("extraction requires a fixed local NTFS volume"));
    }
    String::from_utf16(&volume_root[..VOLUME_GUID_ROOT_UNITS]).map_err(io::Error::other)
}

fn normalized_guid_path(directory: &File) -> io::Result<Vec<u16>> {
    let mut path = Vec::new();
    path.try_reserve_exact(MAX_WINDOWS_PATH_UNITS)
        .map_err(io::Error::other)?;
    path.resize(MAX_WINDOWS_PATH_UNITS, 0_u16);
    let capacity = u32::try_from(path.len()).map_err(io::Error::other)?;
    // SAFETY: the retained handle stays live and `path` is writable for `capacity`
    // UTF-16 units. Only the returned in-bounds length is used below; no pointer
    // is retained. The flags request the object's normalized volume-GUID path.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            directory.as_raw_handle().cast(),
            path.as_mut_ptr(),
            capacity,
            FILE_NAME_NORMALIZED | VOLUME_NAME_GUID,
        )
    };
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    let length = usize::try_from(length).map_err(io::Error::other)?;
    if length >= path.len() || path[length] != 0 {
        return Err(unsupported(
            "volume-GUID path exceeds the supported Windows path bound",
        ));
    }
    path.truncate(length);
    volume_guid_root(&path)?;
    Ok(path)
}

pub(crate) fn validate_volume_locator(value: &str) -> io::Result<()> {
    let units: Vec<u16> = value.encode_utf16().collect();
    if units.len() != VOLUME_GUID_ROOT_UNITS {
        return Err(unsupported("expected one canonical volume-GUID root"));
    }
    volume_guid_root(&units).map(|_| ())
}

/// Rejects drive aliases whose anchor is a subdirectory of the expected volume.
pub(crate) fn require_volume_root_handle(directory: &File) -> io::Result<()> {
    if normalized_guid_path(directory)?.len() != VOLUME_GUID_ROOT_UNITS {
        return Err(unsupported("drive anchor is not the actual volume root"));
    }
    Ok(())
}

/// The closed set of mutable record slots the activation transaction may replace.
///
/// Version directories, provenance, `.complete` markers and the activation lease are
/// deliberately absent: they are absent-target publications or never replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecordSlot {
    Journal,
    Floor,
    Current,
    LastKnownGood,
    PreviousKnownGood,
}

impl RecordSlot {
    pub(crate) const fn leaf(self) -> &'static str {
        match self {
            Self::Journal => "activation-journal",
            Self::Floor => "version-floor",
            Self::Current => "current",
            Self::LastKnownGood => "last-known-good",
            Self::PreviousKnownGood => "previous-known-good",
        }
    }
}

/// Publishes to an absent sibling using the retained parent's actual volume path.
pub(crate) fn publish_new(parent: &File, source: &str, destination: &str) -> io::Result<()> {
    let (source, destination) = sibling_paths(parent, source, destination)?;
    // SAFETY: both checked UTF-16, NUL-terminated paths remain alive throughout
    // this synchronous call and are distinct single-component siblings under the
    // retained parent's observed volume-GUID path. The caller retains protected
    // ancestry and either exclusive bootstrap ownership or the share-zero activation
    // writer lease. The sole fixed flag requests write-through; replacement,
    // cross-volume copy and delayed work are absent.
    let success = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Replaces one fixed activation record slot with a prepared same-parent sibling.
///
/// The destination is typed to the closed slot set and the source must be a generated
/// `pending-*` sibling; anything else refuses before the system call. The caller holds
/// the stable share-zero activation lease and has already flushed and read back the
/// protected sibling.
pub(crate) fn replace_record_slot(parent: &File, source: &str, slot: RecordSlot) -> io::Result<()> {
    if !crate::windows_baseline::is_generated_leaf(source, "pending") {
        return Err(unsupported(
            "replacement source must be a generated pending record sibling",
        ));
    }
    let (source, destination) = sibling_paths(parent, source, slot.leaf())?;
    // SAFETY: both checked UTF-16, NUL-terminated paths remain alive throughout
    // this synchronous call and are distinct single-component siblings under the
    // retained parent's observed volume-GUID path. The destination is one fixed
    // record slot; the flags are exactly replace-existing plus write-through, so a
    // directory destination fails and copy, cross-volume and delayed work are absent.
    let success = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Builds checked NUL-terminated sibling paths under the retained parent's volume path.
fn sibling_paths(
    parent: &File,
    source: &str,
    destination: &str,
) -> io::Result<(Vec<u16>, Vec<u16>)> {
    if source.eq_ignore_ascii_case(destination) {
        return Err(unsupported(
            "publication source and destination must differ",
        ));
    }
    for component in [source, destination] {
        if component.contains('/') {
            return Err(unsupported("publication requires one component"));
        }
        keld_guard::validate_windows_package_paths(&[component]).map_err(io::Error::other)?;
    }
    let parent = String::from_utf16(&normalized_guid_path(parent)?).map_err(io::Error::other)?;
    let path = |component: &str| -> io::Result<Vec<u16>> {
        let mut path = PathBuf::from(&parent);
        path.push(component);
        let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
        if units.contains(&0) || units.len() >= MAX_WINDOWS_PATH_UNITS {
            return Err(unsupported("publication path is invalid or too long"));
        }
        units.push(0);
        Ok(units)
    };
    Ok((path(source)?, path(destination)?))
}

fn volume_guid_root(path: &[u16]) -> io::Result<[u16; VOLUME_GUID_ROOT_UNITS + 1]> {
    if path.len() < VOLUME_GUID_ROOT_UNITS
        || path.contains(&0)
        || !path[..11].iter().copied().eq(r"\\?\Volume{".encode_utf16())
        || path[47] != u16::from(b'}')
        || path[48] != u16::from(b'\\')
    {
        return Err(unsupported(
            "retained directory has no canonical volume-GUID root",
        ));
    }
    for (index, unit) in path[11..47].iter().copied().enumerate() {
        let valid = if matches!(index, 8 | 13 | 18 | 23) {
            unit == u16::from(b'-')
        } else {
            (u16::from(b'0')..=u16::from(b'9')).contains(&unit)
                || (u16::from(b'a')..=u16::from(b'f')).contains(&unit)
                || (u16::from(b'A')..=u16::from(b'F')).contains(&unit)
        };
        if !valid {
            return Err(unsupported(
                "retained directory has a malformed volume GUID",
            ));
        }
    }
    let mut root = [0_u16; VOLUME_GUID_ROOT_UNITS + 1];
    root[..VOLUME_GUID_ROOT_UNITS].copy_from_slice(&path[..VOLUME_GUID_ROOT_UNITS]);
    Ok(root)
}

/// Creates one protected child directory without resolving the parent's pathname.
pub(crate) fn create_directory_relative(parent: &File, component: &str) -> io::Result<File> {
    create_directory_relative_with_profile(
        parent,
        component,
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    )
}

/// Creates one child with the guard-owned install profile at object creation.
pub(crate) fn create_directory_relative_with_profile(
    parent: &File,
    component: &str,
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> io::Result<File> {
    let (mut name, length) = validated_relative_name(component)?;
    let mut unicode_name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: name.as_mut_ptr(),
    };
    if profile == keld_guard::WindowsInstallProtectionProfile::MachineSystem {
        keld_guard::require_windows_system_token()?;
    }
    let security = keld_guard::windows_install_directory_security(profile)?;
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).map_err(io::Error::other)?,
        RootDirectory: parent.as_raw_handle().cast(),
        ObjectName: &raw mut unicode_name,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        SecurityDescriptor: security.as_ptr().cast(),
        SecurityQualityOfService: ptr::null_mut(),
    };
    let mut completion = IO_STATUS_BLOCK {
        Anonymous: IO_STATUS_BLOCK_0 {
            Status: STATUS_SUCCESS,
        },
        Information: 0,
    };
    let mut handle = ptr::null_mut();
    let access = FILE_LIST_DIRECTORY
        | FILE_ADD_FILE
        | FILE_ADD_SUBDIRECTORY
        | FILE_TRAVERSE
        | FILE_READ_ATTRIBUTES
        | READ_CONTROL
        | SYNCHRONIZE;
    // SAFETY: `parent` retains the directory handle; the single-component name,
    // UNICODE_STRING, attributes and guard-owned descriptor remain live and fixed
    // for this synchronous call. Lengths are checked byte counts. The output
    // slots are writable. Fixed flags create only a new directory, refuse
    // reparses, omit handle inheritance/delete sharing, and request no async I/O.
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            access,
            &raw const attributes,
            &raw mut completion,
            ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: this pure status conversion takes no pointers or ownership.
        let error = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(error.cast_signed()));
    }
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::other(
            "directory creation returned no owned handle",
        ));
    }
    // SAFETY: a successful NtCreateFile returned this new valid handle. It has
    // no other owner and is transferred exactly once; File closes it on every
    // subsequent error or when its eventual retained owner is dropped.
    let directory = unsafe { File::from_raw_handle(handle.cast()) };
    if status != STATUS_SUCCESS || completion.Information != FILE_CREATED as usize {
        return Err(io::Error::other(
            "directory creation did not report a new directory",
        ));
    }
    keld_guard::validate_windows_install_directory(&directory, profile)?;
    Ok(directory)
}

/// Creates one protected regular file with its guard-owned profile before writes.
pub(crate) fn create_file_relative_with_profile(
    parent: &File,
    component: &str,
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> io::Result<File> {
    create_file_relative_with_profile_sharing(parent, component, profile, FILE_SHARE_READ)
}

/// Creates a protected regular file whose returned handle shares with no opener.
pub(crate) fn create_file_relative_exclusive_with_profile(
    parent: &File,
    component: &str,
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> io::Result<File> {
    create_file_relative_with_profile_sharing(parent, component, profile, 0)
}

fn create_file_relative_with_profile_sharing(
    parent: &File,
    component: &str,
    profile: keld_guard::WindowsInstallProtectionProfile,
    share_mode: u32,
) -> io::Result<File> {
    let (mut name, length) = validated_relative_name(component)?;
    let mut unicode_name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: name.as_mut_ptr(),
    };
    if profile == keld_guard::WindowsInstallProtectionProfile::MachineSystem {
        keld_guard::require_windows_system_token()?;
    }
    let security = keld_guard::windows_install_file_security(profile)?;
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).map_err(io::Error::other)?,
        RootDirectory: parent.as_raw_handle().cast(),
        ObjectName: &raw mut unicode_name,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        SecurityDescriptor: security.as_ptr().cast(),
        SecurityQualityOfService: ptr::null_mut(),
    };
    let mut completion = IO_STATUS_BLOCK {
        Anonymous: IO_STATUS_BLOCK_0 {
            Status: STATUS_SUCCESS,
        },
        Information: 0,
    };
    let mut handle = ptr::null_mut();
    let access = FILE_GENERIC_READ | FILE_GENERIC_WRITE | READ_CONTROL | SYNCHRONIZE | WRITE_DAC;
    // SAFETY: parent, the validated single-component name, explicit guard-owned
    // descriptor and output slots remain live for this synchronous relative create.
    // The fixed FILE_CREATE/non-directory/no-reparse options prevent open/truncate,
    // links and async I/O; a fixed internal sharing policy and exact profile are
    // applied before writes. WRITE_DAC is retained only for installer sealing.
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            access,
            &raw const attributes,
            &raw mut completion,
            ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            share_mode,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: this pure status conversion takes no pointers or ownership.
        let error = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(error.cast_signed()));
    }
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::other("file creation returned no owned handle"));
    }
    // SAFETY: successful NtCreateFile returned one new handle; transfer it exactly
    // once to File so every later error closes it through RAII.
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    if status != STATUS_SUCCESS || completion.Information != FILE_CREATED as usize {
        return Err(io::Error::other("file creation did not report a new file"));
    }
    keld_guard::validate_windows_install_file(&file, profile)?;
    Ok(file)
}

fn validated_relative_name(component: &str) -> io::Result<(Vec<u16>, u16)> {
    if component.is_empty() || component.contains('/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected one filesystem component",
        ));
    }
    keld_guard::validate_windows_package_paths(&[component])
        .map_err(|detail| io::Error::new(io::ErrorKind::InvalidInput, detail))?;
    let mut name = Vec::new();
    name.try_reserve_exact(component.len())
        .map_err(io::Error::other)?;
    name.extend(component.encode_utf16());
    let length = name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|bytes| u16::try_from(bytes).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "component is too long"))?;
    Ok((name, length))
}

fn unsupported(detail: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, detail)
}

#[cfg(test)]
#[allow(clippy::expect_used)] // Test setup and independent literal/OS assertions.
mod tests {
    use super::{
        create_directory_relative, create_file_relative_exclusive_with_profile,
        duplicate_activation_lease_for_keeper, publish_new, require_volume_root_handle,
        validate_volume_locator, volume_guid_root,
    };
    use std::fs::{File, OpenOptions};
    use std::io::Write as _;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use std::ptr;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{FSCTL_DELETE_REPARSE_POINT, FSCTL_SET_REPARSE_POINT};
    use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;

    #[test]
    fn guid_root_is_exact_and_excludes_the_directory_suffix() {
        let expected = "\\\\?\\Volume{01234567-89ab-CDEF-0123-456789abcdef}\\\0";
        for suffix in ["", "protected\\versions"] {
            let input = format!("\\\\?\\Volume{{01234567-89ab-CDEF-0123-456789abcdef}}\\{suffix}");
            assert_eq!(
                volume_guid_root(&input.encode_utf16().collect::<Vec<_>>())
                    .expect("valid GUID root")
                    .as_slice(),
                expected.encode_utf16().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn guid_root_rejects_fallback_names_and_malformed_identifiers() {
        for input in [
            "",
            "C:\\protected",
            "\\\\server\\share\\protected",
            "\\Device\\HarddiskVolume1\\protected",
            "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}",
            "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdeg}\\",
            "\\\\?\\Volume{01234567889ab-cdef-0123-456789abcdef}\\",
            "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\bad\0suffix",
        ] {
            assert!(
                volume_guid_root(&input.encode_utf16().collect::<Vec<_>>()).is_err(),
                "{input:?}"
            );
        }
    }

    #[test]
    fn relative_creation_protects_and_retains_new_directory_objects() {
        let fixture = tempfile::tempdir().expect("fixture");
        let parent = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(fixture.path())
            .expect("retain fixture parent");
        let child = create_directory_relative(&parent, "private").expect("create child");
        keld_guard::validate_windows_owner_private_directory(&child)
            .expect("actual private descriptor");
        let nested =
            create_directory_relative(&child, "nested").expect("create relative grandchild");
        assert!(fixture.path().join("private/nested").is_dir());
        assert!(create_directory_relative(&parent, "private").is_err());
        File::create(fixture.path().join("existing-file")).expect("collision file");
        assert!(create_directory_relative(&parent, "existing-file").is_err());
        for name in [
            "",
            "a/b",
            "a\\b",
            "..",
            "NUL",
            "alias~1",
            "decomposed-e\u{301}",
        ] {
            assert!(
                create_directory_relative(&parent, name).is_err(),
                "{name:?}"
            );
        }
        assert!(!fixture.path().join("a").exists());
        assert!(
            std::fs::rename(fixture.path().join("private"), fixture.path().join("moved")).is_err()
        );
        drop(nested);
        drop(child);
        std::fs::rename(fixture.path().join("private"), fixture.path().join("moved"))
            .expect("released handles permit rename");
        drop(parent);
    }

    #[test]
    fn exclusive_profiled_file_blocks_readers_until_its_handle_closes() {
        let fixture = tempfile::tempdir().expect("exclusive file fixture");
        let parent = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(fixture.path())
            .expect("retain fixture parent");
        let file = create_file_relative_exclusive_with_profile(
            &parent,
            "activation.lock",
            keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
        )
        .expect("create with exact profile and no sharing");
        keld_guard::validate_windows_owner_private_file(&file).expect("exact file profile");
        assert!(File::open(fixture.path().join("activation.lock")).is_err());
        drop(file);
        File::open(fixture.path().join("activation.lock"))
            .expect("readers can open the persistent lease file after release");
    }

    #[test]
    fn sibling_publication_never_replaces_or_copies() {
        let fixture = tempfile::tempdir().expect("publication fixture");
        let parent = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(fixture.path())
            .expect("retain publication parent");
        std::fs::write(fixture.path().join("pending"), b"verified bytes").expect("source");
        std::fs::write(fixture.path().join("existing"), b"existing sentinel").expect("target");
        assert!(publish_new(&parent, "pending", "existing").is_err());
        assert_eq!(
            std::fs::read(fixture.path().join("existing")).expect("sentinel"),
            b"existing sentinel"
        );
        assert_eq!(
            std::fs::read(fixture.path().join("pending")).expect("source preserved"),
            b"verified bytes"
        );
        for destination in ["../outside", "a/b", "a\\b", "pending", "PENDING"] {
            assert!(
                publish_new(&parent, "pending", destination).is_err(),
                "{destination}"
            );
        }
        publish_new(&parent, "pending", "published").expect("absent sibling publication");
        assert!(!fixture.path().join("pending").exists());
        assert_eq!(
            std::fs::read(fixture.path().join("published")).expect("published bytes"),
            b"verified bytes"
        );
        drop(parent);
    }

    #[test]
    fn reduced_activation_lease_handle_retains_share_zero_without_write_access() {
        let fixture = tempfile::tempdir().expect("lease fixture");
        let path = fixture.path().join("activation.lock");
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&path)
            .expect("create exact share-zero lease");
        let keeper_handle = duplicate_activation_lease_for_keeper(&lease)
            .expect("duplicate read-attributes/synchronize rights");
        drop(lease);

        let mut keeper_file = File::from(keeper_handle);
        assert_eq!(
            keeper_file
                .write_all(b"forbidden")
                .expect_err("keeper handle has no write access")
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert!(
            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .open(&path)
                .is_err(),
            "reduced duplicate retains the original share-zero open"
        );
        drop(keeper_file);
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&path)
            .expect("writer exclusion ends when the final duplicate closes");
    }

    #[test]
    fn trusted_volume_locator_is_one_root_not_a_path() {
        let root = r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\";
        validate_volume_locator(root).expect("literal root");
        assert!(validate_volume_locator(&format!("{root}child")).is_err());
        assert!(validate_volume_locator(r"C:\").is_err());
    }

    #[test]
    fn a_same_volume_subdirectory_is_not_a_drive_anchor() {
        let fixture = tempfile::tempdir().expect("anchor fixture");
        let directory = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(fixture.path())
            .expect("directory handle");
        assert!(require_volume_root_handle(&directory).is_err());
        let root_path: std::path::PathBuf = fixture.path().components().take(2).collect();
        let root = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(root_path)
            .expect("actual drive root");
        require_volume_root_handle(&root).expect("actual volume root positive control");
    }

    #[test]
    fn relative_creation_uses_the_acquired_parent_after_name_substitution() {
        let fixture = tempfile::tempdir().expect("fixture");
        let original = fixture.path().join("original");
        let moved = fixture.path().join("moved");
        let outside = fixture.path().join("outside");
        std::fs::create_dir(&original).expect("original parent");
        std::fs::create_dir(&outside).expect("outside directory");
        std::fs::write(outside.join("sentinel"), b"unchanged").expect("outside sentinel");
        // Deliberately omit the extraction owner's rename protection: this
        // tests the native RootDirectory relationship independently of pins.
        let parent = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&original)
            .expect("retain renameable parent");
        std::fs::rename(&original, &moved).expect("rename succeeds with delete sharing");
        let junction = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:KELD_PRIMITIVE_LINK -Target $env:KELD_PRIMITIVE_TARGET | Out-Null",
            ])
            .env("KELD_PRIMITIVE_LINK", &original)
            .env("KELD_PRIMITIVE_TARGET", &outside)
            .output()
            .expect("junction fixture command");
        assert!(
            junction.status.success(),
            "junction setup failed: {}",
            String::from_utf8_lossy(&junction.stderr)
        );
        // Prove the old spelling now actually resolves to the outside object.
        assert_eq!(
            std::fs::read(original.join("sentinel")).expect("redirected sentinel"),
            b"unchanged"
        );
        let child = create_directory_relative(&parent, "child").expect("create on retained object");
        assert!(moved.join("child").is_dir());
        assert!(!outside.join("child").exists());
        assert!(!original.join("child").exists());
        assert_eq!(
            std::fs::read(outside.join("sentinel")).expect("outside after create"),
            b"unchanged"
        );
        // Positive control for the unsafe alternative: an ambient writer using
        // the replaced spelling really escapes to the outside fixture directory.
        std::fs::create_dir(original.join("ambient-control")).expect("ambient write control");
        assert!(outside.join("ambient-control").is_dir());
        assert!(!moved.join("ambient-control").exists());
        drop(child);
        drop(parent);
        std::fs::remove_dir(&original).expect("remove junction itself before fixture cleanup");
    }

    fn mount_point_buffer(target: &Path) -> Vec<u8> {
        // Encode the documented MountPointReparseBuffer byte layout, rather
        // than defining another FFI struct. All offsets/lengths count bytes:
        // https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_reparse_data_buffer
        let resolved = std::fs::canonicalize(target).expect("resolve fixture target");
        let wide = resolved.as_os_str().encode_wide().collect::<Vec<_>>();
        assert!(wide.starts_with(&r"\\?\".encode_utf16().collect::<Vec<_>>()));
        let print = &wide[4..];
        let substitute = r"\??\"
            .encode_utf16()
            .chain(print.iter().copied())
            .collect::<Vec<_>>();
        let substitute_bytes =
            u16::try_from(substitute.len() * 2).expect("bounded substitute name");
        let print_bytes = u16::try_from(print.len() * 2).expect("bounded print name");
        let data_bytes = 8_u16
            .checked_add(substitute_bytes)
            .and_then(|n| n.checked_add(print_bytes))
            .and_then(|n| n.checked_add(4))
            .expect("bounded mount-point payload");
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        buffer.extend_from_slice(&data_bytes.to_le_bytes());
        buffer.extend_from_slice(&0_u16.to_le_bytes());
        buffer.extend_from_slice(&0_u16.to_le_bytes());
        buffer.extend_from_slice(&substitute_bytes.to_le_bytes());
        buffer.extend_from_slice(
            &substitute_bytes
                .checked_add(2)
                .expect("print offset")
                .to_le_bytes(),
        );
        buffer.extend_from_slice(&print_bytes.to_le_bytes());
        for unit in substitute
            .into_iter()
            .chain([0])
            .chain(print.iter().copied())
            .chain([0])
        {
            buffer.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(buffer.len(), usize::from(data_bytes) + 8);
        buffer
    }

    fn set_mount_point_on_open_directory(directory: &File, target: &Path) {
        let buffer = mount_point_buffer(target);
        let length = u32::try_from(buffer.len()).expect("bounded mount-point input");
        let mut returned = 0;
        // SAFETY: the independently opened synchronous directory handle is
        // live. `buffer` contains the fully sized mount-point payload, including
        // counted UTF-16 names, and remains readable for `length` bytes. No
        // output buffer/overlapped I/O is requested; `returned` is writable.
        let result = unsafe {
            DeviceIoControl(
                directory.as_raw_handle().cast(),
                FSCTL_SET_REPARSE_POINT,
                buffer.as_ptr().cast(),
                length,
                ptr::null_mut(),
                0,
                &raw mut returned,
                ptr::null_mut(),
            )
        };
        assert_ne!(
            result,
            0,
            "set reparse data on retained object: {}",
            std::io::Error::last_os_error()
        );
    }

    fn clear_mount_point_on_open_directory(directory: &File) {
        let mut buffer = [0_u8; 8];
        buffer[..4].copy_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        let mut returned = 0;
        // SAFETY: the handle still owns the fixture's reparse directory. The
        // eight-byte delete payload carries the matching tag and zero data
        // length/reserved fields. All pointers stay live for this synchronous
        // call; no output buffer or overlapped operation is requested.
        let result = unsafe {
            DeviceIoControl(
                directory.as_raw_handle().cast(),
                FSCTL_DELETE_REPARSE_POINT,
                buffer.as_ptr().cast(),
                8,
                ptr::null_mut(),
                0,
                &raw mut returned,
                ptr::null_mut(),
            )
        };
        assert_ne!(
            result,
            0,
            "remove fixture reparse data: {}",
            std::io::Error::last_os_error()
        );
    }

    #[test]
    fn retained_parent_live_reparse_mutation_cannot_redirect_relative_creates() {
        use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};

        let fixture = tempfile::tempdir().expect("fixture");
        let original = fixture.path().join("retained-empty");
        let outside = fixture.path().join("outside");
        std::fs::create_dir(&original).expect("empty parent");
        std::fs::create_dir(&outside).expect("outside target");
        std::fs::write(outside.join("sentinel"), b"outside-unchanged").expect("outside sentinel");
        let parent = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&original)
            .expect("retain parent without delete sharing");
        // Same-user adversarial handle: no pathname replacement or rename.
        // The first handle's write sharing permits this independent acquisition.
        let hostile = OpenOptions::new()
            .access_mode(FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&original)
            .expect("independent write/attribute handle");
        set_mount_point_on_open_directory(&hostile, &outside);
        assert_eq!(
            std::fs::read(original.join("sentinel")).expect("active junction positive control"),
            b"outside-unchanged"
        );

        let created = create_directory_relative(&parent, "native-child");
        let file_created = if created.is_ok() {
            let retained = cap_std::fs::Dir::from_std_file(
                parent.try_clone().expect("duplicate retained handle"),
            );
            let mut options = cap_std::fs::OpenOptions::new();
            options
                .write(true)
                .create_new(true)
                .follow(FollowSymlinks::No);
            match retained.open_with("cap-child", &options) {
                Ok(mut file) => {
                    file.write_all(b"retained-file")
                        .expect("write retained child");
                    true
                }
                Err(_) => false,
            }
        } else {
            false
        };
        let outside_native = outside.join("native-child").exists();
        let outside_file = outside.join("cap-child").exists();
        let sentinel =
            std::fs::read(outside.join("sentinel")).expect("outside sentinel after probes");
        // Remove only this same object's reparse data so its actual contents
        // become observable again, then drop handles before TempDir cleanup.
        clear_mount_point_on_open_directory(&hostile);
        assert_eq!(original.join("native-child").is_dir(), created.is_ok());
        if file_created {
            assert_eq!(
                std::fs::read(original.join("cap-child"))
                    .expect("retained file after clearing reparse"),
                b"retained-file"
            );
        }
        drop(created);
        drop(hostile);
        drop(parent);
        assert!(
            !outside_native,
            "relative native mkdir escaped through live parent reparse data"
        );
        assert!(
            !outside_file,
            "capability file create escaped through live parent reparse data"
        );
        assert_eq!(sentinel, b"outside-unchanged");
    }
}
