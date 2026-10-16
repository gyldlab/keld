// Responsibility-local Windows native contract tests: support.

use std::fs::{File, OpenOptions};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::AsRawHandle as _;
use std::path::Path;
use std::ptr;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_BOTH_DIR_INFO,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{FSCTL_DELETE_REPARSE_POINT, FSCTL_SET_REPARSE_POINT};
use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;

pub(super) fn retained_directory(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .expect("retain fixture directory")
}

pub(super) fn junction(link: &Path, target: &Path) {
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:KELD_PRIMITIVE_LINK -Target $env:KELD_PRIMITIVE_TARGET | Out-Null",
        ])
        .env("KELD_PRIMITIVE_LINK", link)
        .env("KELD_PRIMITIVE_TARGET", target)
        .output()
        .expect("junction fixture command");
    assert!(
        output.status.success(),
        "junction setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Encodes one `FILE_ID_BOTH_DIR_INFO` record, padded to eight bytes.
pub(super) fn directory_record(next: u32, name_bytes: u32, name: &[u16]) -> Vec<u8> {
    let name_offset = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
    let mut record = vec![0_u8; name_offset];
    let next_at = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset);
    record[next_at..next_at + 4].copy_from_slice(&next.to_ne_bytes());
    let length_at = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength);
    record[length_at..length_at + 4].copy_from_slice(&name_bytes.to_ne_bytes());
    for unit in name {
        record.extend_from_slice(&unit.to_ne_bytes());
    }
    record.resize(record.len().next_multiple_of(8), 0);
    record
}

pub(super) fn chain(names: &[&str]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let units = name.encode_utf16().collect::<Vec<_>>();
        let length = u32::try_from(units.len() * 2).expect("short fixture name");
        let size = directory_record(0, length, &units).len();
        let next = if index + 1 == names.len() {
            0
        } else {
            u32::try_from(size).expect("short fixture record")
        };
        bytes.extend(directory_record(next, length, &units));
    }
    bytes
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
    let substitute_bytes = u16::try_from(substitute.len() * 2).expect("bounded substitute name");
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

pub(super) fn set_mount_point_on_open_directory(directory: &File, target: &Path) {
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

pub(super) fn clear_mount_point_on_open_directory(directory: &File) {
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
