//! Narrow Windows filesystem operations for protected extraction.

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(unsafe_code)] // Exact volume, relative-directory, never-follow deletion and sibling-publication operations in AGENTS.md.

use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
    FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FILE_SYNCHRONOUS_IO_NONALERT,
    FILE_WRITE_THROUGH, FileRenameInformation, NtCreateFile, NtSetInformationFile,
};
use windows_sys::Win32::Foundation::{
    DuplicateHandle, ERROR_NO_MORE_FILES, GetHandleInformation, HANDLE_FLAG_INHERIT,
    INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, RtlNtStatusToDosError,
    STATUS_DELETE_PENDING, STATUS_OBJECT_NAME_NOT_FOUND, STATUS_SUCCESS, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO_EX, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY,
    FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE, FileAttributeTagInfo, FileDispositionInfoEx,
    FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, GetDriveTypeW,
    GetFileInformationByHandleEx, GetFinalPathNameByHandleW, GetVolumeInformationByHandleW,
    READ_CONTROL, SYNCHRONIZE, SetFileInformationByHandle, VOLUME_NAME_GUID, WRITE_DAC,
};
use windows_sys::Win32::System::IO::{IO_STATUS_BLOCK, IO_STATUS_BLOCK_0};
use windows_sys::Win32::System::SystemServices::{FILE_PERSISTENT_ACLS, FILE_READ_ONLY_VOLUME};
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, FILE_CREATED};

// Pure parsing is a separate private owner; all Win32 and unsafe filesystem
// operations remain in this file as required by keld-update/AGENTS.md.
mod directory_entries;
use directory_entries::{DirectoryChild, parse_directory_entries};

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

/// Publishes to an absent sibling under the retained parent HANDLE.
///
/// Unlike a pathname reconstructed from the parent, the source and destination
/// remain bound to the retained directory even if its live reparse data changes.
pub(crate) fn publish_new(parent: &File, source: &str, destination: &str) -> io::Result<()> {
    rename_relative(parent, source, destination, false)
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
    rename_relative(parent, source, slot.leaf(), true)
}

/// Atomically renames one entry under one retained parent, with no ambient path
/// lookup. The source's NT open is write-through: NTFS flushes metadata changes
/// (including a rename) made through a `FILE_WRITE_THROUGH` handle. This targets
/// the prior same-volume write-through durability intent without pathname re-resolution;
/// sudden power-loss equivalence still requires separate physical durability proof.
/// No copy, fallback, retry or delayed work is permitted.
///
/// `FileRenameInfo`'s `RootDirectory` resolves the *destination* relative to the same
/// pinned parent whose `RootDirectory` resolved the source. Replacement is a typed
/// choice by the two callers, never inferred from an attacker-supplied name.
fn rename_relative(
    parent: &File,
    source: &str,
    destination: &str,
    replace: bool,
) -> io::Result<()> {
    if source.eq_ignore_ascii_case(destination) {
        return Err(unsupported(
            "publication source and destination must differ",
        ));
    }
    let (source_name, _) = validated_relative_name(source)?;
    let (destination_name, _) = validated_relative_name(destination)?;
    // Test seam: a pathname-based rename would already have resolved its paths here.
    // It sits before the source open because NTFS refuses to rename a directory
    // while any child is open, so after that open the parent cannot be substituted.
    #[cfg(test)]
    run_before_rename_binding();
    let source =
        open_units_for_delete_with_options(parent, source_name, DeletePurpose::Remove, true)?
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "publication source is absent")
            })?;

    // FILE_RENAME_INFORMATION has a trailing, variable-length UTF-16 name. Allocate
    // at least its full struct size, plus the validated name and a terminator.
    // u64 storage provides the pointer alignment required by HANDLE fields.
    let name_bytes = destination_name
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or_else(|| unsupported("rename component too long"))?;
    let required = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName)
        .checked_add(name_bytes)
        .and_then(|length| length.checked_add(size_of::<u16>()))
        .ok_or_else(|| unsupported("rename buffer too long"))?;
    let buffer_size = required.max(size_of::<FILE_RENAME_INFORMATION>());
    let size = u32::try_from(buffer_size).map_err(io::Error::other)?;
    let mut storage = vec![0_u64; buffer_size.div_ceil(size_of::<u64>())];
    let name_length = u32::try_from(name_bytes).map_err(io::Error::other)?;
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    // SAFETY: storage is writable, 8-byte aligned and large enough for the
    // complete FILE_RENAME_INFORMATION header, the checked UTF-16 name and a trailing
    // zero. Each field is written in place through `addr_of_mut!`, so no whole-struct
    // copy overwrites the zero-filled padding or terminator with undefined bytes:
    // every byte of the buffer is initialized. The union is set through its full
    // 32-bit `Flags` member (on little-endian Windows targets the BOOLEAN
    // `ReplaceIfExists` aliases its low byte), so its high bytes are zero rather
    // than undefined. The pointers target this one owned buffer, which stays live
    // through the synchronous NtSetInformationFile call.
    unsafe {
        ptr::addr_of_mut!((*info).Anonymous).write(FILE_RENAME_INFORMATION_0 {
            Flags: u32::from(replace),
        });
        ptr::addr_of_mut!((*info).RootDirectory).write(parent.as_raw_handle().cast());
        ptr::addr_of_mut!((*info).FileNameLength).write(name_length);
        ptr::copy_nonoverlapping(
            destination_name.as_ptr(),
            ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            destination_name.len(),
        );
    }
    // SAFETY: source is the live DELETE-access synchronous file handle opened
    // without following a reparse point relative to parent. NtSetInformationFile
    // honors the live RootDirectory handle for the single-component destination,
    // unlike the Win32 FileRenameInfo wrapper, which may re-resolve relative
    // names against the ambient current directory. The aligned initialized
    // buffer and IO_STATUS_BLOCK out-parameter live through the syscall. The
    // FILE_WRITE_THROUGH source open flushes NTFS rename metadata before return.
    let mut io_status = IO_STATUS_BLOCK {
        Anonymous: IO_STATUS_BLOCK_0 {
            Status: STATUS_SUCCESS,
        },
        Information: 0,
    };
    let status = unsafe {
        NtSetInformationFile(
            source.file().as_raw_handle().cast(),
            &raw mut io_status,
            info.cast(),
            size,
            FileRenameInformation,
        )
    };
    if status < 0 {
        // SAFETY: this status-to-DOS-error conversion takes no pointers or ownership.
        let error = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(error.cast_signed()));
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// Test-only seam: one action that `rename_relative` runs on this thread after
    /// validating both names and before opening the source or binding the destination.
    static BEFORE_RENAME_BINDING: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Installs the one-shot action for the next relative rename on this thread.
#[cfg(test)]
fn set_before_rename_binding(action: impl FnOnce() + 'static) {
    BEFORE_RENAME_BINDING.with(|slot| *slot.borrow_mut() = Some(Box::new(action)));
}

/// Whether the installed action is still waiting to run on this thread.
#[cfg(test)]
fn before_rename_binding_pending() -> bool {
    BEFORE_RENAME_BINDING.with(|slot| slot.borrow().is_some())
}

#[cfg(test)]
fn run_before_rename_binding() {
    let action = BEFORE_RENAME_BINDING.with(|slot| slot.borrow_mut().take());
    if let Some(action) = action {
        action();
    }
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

/// What an entry opened for deletion is, read from its own handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    File,
    Directory,
    /// Any reparse point. It is deleted as itself and never traversed.
    Link,
}

/// Rights requested beside DELETE when an entry is opened for deletion. Each purpose
/// asks only for what its caller reads, so deleting never needs read-data access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeletePurpose {
    /// Delete only.
    Remove,
    /// Also read the descriptor, sharing only with readers while it is checked.
    Admit,
    /// Also list the directory's children.
    List,
}

impl DeletePurpose {
    const fn access(self) -> u32 {
        let base = DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE;
        match self {
            Self::Remove => base,
            Self::Admit => base | READ_CONTROL,
            Self::List => base | FILE_LIST_DIRECTORY,
        }
    }

    const fn share(self) -> u32 {
        match self {
            Self::Admit => FILE_SHARE_READ,
            Self::Remove | Self::List => FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        }
    }
}

/// One entry opened with DELETE access relative to a retained parent handle,
/// never through a reparse point. Deleting it acts on this exact object.
pub(crate) struct DeletableEntry {
    file: File,
    kind: EntryKind,
}

impl DeletableEntry {
    pub(crate) fn kind(&self) -> EntryKind {
        self.kind
    }

    /// The open handle, for checks that must bind to the object being deleted.
    pub(crate) fn file(&self) -> &File {
        &self.file
    }

    /// Marks this object for POSIX deletion and closes the handle, which removes its
    /// name. There is no fallback to non-POSIX deletion and no retry: an unsupported
    /// filesystem, a mapped image or a sharing conflict is returned as an error.
    pub(crate) fn delete(self) -> io::Result<()> {
        let disposition = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_FLAG_DELETE
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        };
        let size =
            u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>()).map_err(io::Error::other)?;
        // SAFETY: `self.file` owns a live handle opened with DELETE access; the
        // disposition value is a fixed, initialized FILE_DISPOSITION_INFO_EX whose
        // exact size is passed with its information class for this synchronous call.
        let success = unsafe {
            SetFileInformationByHandle(
                self.file.as_raw_handle().cast(),
                FileDispositionInfoEx,
                (&raw const disposition).cast(),
                size,
            )
        };
        if success == 0 {
            return Err(io::Error::last_os_error());
        }
        drop(self.file);
        Ok(())
    }
}

/// Opens `component` beneath the retained `parent` for deletion. The name is one
/// validated component; the open never follows a reparse point and never creates.
/// An absent name, or one whose deletion is already pending, is `NotFound`.
pub(crate) fn open_for_delete(
    parent: &File,
    component: &str,
    purpose: DeletePurpose,
) -> io::Result<DeletableEntry> {
    let (name, _) = validated_relative_name(component)?;
    open_units_for_delete(parent, name, purpose)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "entry is absent or already being deleted",
        )
    })
}

/// Deletes the regular file `component` beneath `parent`. A link, a directory or an
/// absent name refuses.
pub(crate) fn remove_file_relative(parent: &File, component: &str) -> io::Result<()> {
    let entry = open_for_delete(parent, component, DeletePurpose::Remove)?;
    if entry.kind() != EntryKind::File {
        return Err(io::Error::other("expected a regular file to delete"));
    }
    entry.delete()
}

/// Deletes the directory `component` beneath `parent` and everything beneath it.
///
/// Every entry is opened relative to its retained parent handle, children before
/// their directory; a reparse point is deleted as itself and never traversed. Only
/// the directories on the current path stay open. A root that is absent or is not a
/// directory refuses; a child that vanishes during the walk is skipped. The first
/// failure stops the walk and is returned with nothing retried.
pub(crate) fn remove_directory_tree(parent: &File, component: &str) -> io::Result<()> {
    let root = open_for_delete(parent, component, DeletePurpose::List)?;
    if root.kind() != EntryKind::Directory {
        return Err(io::Error::other("expected a directory tree to delete"));
    }
    let children = directory_children(root.file())?;
    let mut path = vec![(root, children.into_iter())];
    while let Some((directory, children)) = path.last_mut() {
        match children.next() {
            Some(child) => {
                // The listing only chooses the rights requested; the kind that decides
                // traversal is read again from the opened handle.
                let purpose = if child.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
                    && child.attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                {
                    DeletePurpose::List
                } else {
                    DeletePurpose::Remove
                };
                match open_units_for_delete(directory.file(), child.name, purpose)? {
                    Some(entry) if entry.kind() == EntryKind::Directory => {
                        let children = directory_children(entry.file())?;
                        path.push((entry, children.into_iter()));
                    }
                    Some(entry) => entry.delete()?,
                    None => {}
                }
            }
            None => {
                if let Some((directory, _)) = path.pop() {
                    directory.delete()?;
                }
            }
        }
    }
    Ok(())
}

/// Lists the UTF-8 names inside a retained directory through its handle, omitting `.`,
/// `..` and names that are not valid UTF-16. The handle must be synchronous and carry
/// list rights; otherwise the listing call fails and its error is returned.
pub(crate) fn child_names(directory: &File) -> io::Result<Vec<String>> {
    Ok(directory_children(directory)?
        .into_iter()
        .filter_map(|child| String::from_utf16(&child.name).ok())
        .collect())
}

fn open_units_for_delete(
    parent: &File,
    name: Vec<u16>,
    purpose: DeletePurpose,
) -> io::Result<Option<DeletableEntry>> {
    open_units_for_delete_with_options(parent, name, purpose, false)
}

/// The write-through form is used only for metadata-changing relative renames.
/// Existing relative deletion keeps its original access and I/O mode unchanged.
fn open_units_for_delete_with_options(
    parent: &File,
    mut name: Vec<u16>,
    purpose: DeletePurpose,
    write_through: bool,
) -> io::Result<Option<DeletableEntry>> {
    // An empty relative name would reopen the parent itself.
    if name.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected one filesystem component",
        ));
    }
    let length = name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|bytes| u16::try_from(bytes).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "component is too long"))?;
    let mut unicode_name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: name.as_mut_ptr(),
    };
    // Names here are exact: generated by this writer or listed from the parent.
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).map_err(io::Error::other)?,
        RootDirectory: parent.as_raw_handle().cast(),
        ObjectName: &raw mut unicode_name,
        Attributes: OBJ_DONT_REPARSE,
        SecurityDescriptor: ptr::null_mut(),
        SecurityQualityOfService: ptr::null_mut(),
    };
    let mut completion = IO_STATUS_BLOCK {
        Anonymous: IO_STATUS_BLOCK_0 {
            Status: STATUS_SUCCESS,
        },
        Information: 0,
    };
    let mut handle = ptr::null_mut();
    // SAFETY: `parent` retains the directory handle; the non-empty name buffer,
    // UNICODE_STRING and attributes stay live and fixed for this synchronous call,
    // and the length is a checked byte count. The output slots are writable.
    // FILE_OPEN never creates; OBJ_DONT_REPARSE and FILE_OPEN_REPARSE_POINT open a
    // link itself instead of its target; no inheritance or async I/O is requested.
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            purpose.access(),
            &raw const attributes,
            &raw mut completion,
            ptr::null(),
            0,
            purpose.share(),
            FILE_OPEN,
            FILE_OPEN_REPARSE_POINT
                | FILE_SYNCHRONOUS_IO_NONALERT
                | if write_through { FILE_WRITE_THROUGH } else { 0 },
            ptr::null(),
            0,
        )
    };
    if status == STATUS_OBJECT_NAME_NOT_FOUND || status == STATUS_DELETE_PENDING {
        return Ok(None);
    }
    if status < 0 {
        // SAFETY: this pure status conversion takes no pointers or ownership.
        let error = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(error.cast_signed()));
    }
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::other("delete open returned no owned handle"));
    }
    // SAFETY: a successful NtCreateFile returned this new valid handle. It has no
    // other owner and is transferred exactly once; File closes it on every path.
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    let kind = entry_kind(&file)?;
    Ok(Some(DeletableEntry { file, kind }))
}

fn entry_kind(file: &File) -> io::Result<EntryKind> {
    let mut tag = FILE_ATTRIBUTE_TAG_INFO {
        FileAttributes: 0,
        ReparseTag: 0,
    };
    let size = u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>()).map_err(io::Error::other)?;
    // SAFETY: `file` owns a live handle opened with FILE_READ_ATTRIBUTES; `tag` is a
    // writable FILE_ATTRIBUTE_TAG_INFO of exactly `size` bytes for its class.
    let success = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle().cast(),
            FileAttributeTagInfo,
            (&raw mut tag).cast(),
            size,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    // A reparse point is a link whether or not it also carries the directory bit.
    Ok(if tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        EntryKind::Link
    } else if tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        EntryKind::Directory
    } else {
        EntryKind::File
    })
}

/// Bytes requested per enumeration call; a multiple of eight keeps the buffer aligned.
const DIRECTORY_BUFFER_WORDS: usize = 8 * 1024;

/// Lists the children of an open directory through its handle, omitting `.` and `..`.
/// Without list rights on that handle the OS call fails and the error is returned.
fn directory_children(directory: &File) -> io::Result<Vec<DirectoryChild>> {
    let mut buffer = vec![0_u64; DIRECTORY_BUFFER_WORDS];
    let size = u32::try_from(buffer.len() * size_of::<u64>()).map_err(io::Error::other)?;
    let mut children = Vec::new();
    let mut class = FileIdBothDirectoryRestartInfo;
    loop {
        // No byte from an earlier call can be read as part of this one.
        buffer.fill(0);
        // SAFETY: `directory` borrows a live synchronous handle; if it lacks list
        // rights the call fails without writing. `buffer` is a writable,
        // eight-byte-aligned allocation of exactly `size` bytes that outlives this
        // synchronous call.
        let success = unsafe {
            GetFileInformationByHandleEx(
                directory.as_raw_handle().cast(),
                class,
                buffer.as_mut_ptr().cast(),
                size,
            )
        };
        if success == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_NO_MORE_FILES.cast_signed()) {
                return Ok(children);
            }
            return Err(error);
        }
        let bytes: Vec<u8> = buffer.iter().flat_map(|word| word.to_ne_bytes()).collect();
        children.extend(parse_directory_entries(&bytes)?);
        class = FileIdBothDirectoryInfo;
    }
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
mod tests;
