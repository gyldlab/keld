//! Independent retained-handle census, identity and lease isolation observations.

use super::process::open_process_for_census;
use crate::PRODUCT_DEADLINE;
use std::ffi::c_void;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CompareObjectHandles, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE,
};
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

const SYSTEM_EXTENDED_HANDLE_INFORMATION: u32 = 64;

const STATUS_INFO_LENGTH_MISMATCH: i32 = -1_073_741_820;

const OBJ_INHERIT: u32 = 0x0000_0002;

unsafe extern "system" {
    fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SystemHandleEntry {
    object: *mut c_void,
    unique_process_id: usize,
    handle_value: usize,
    granted_access: u32,
    creator_backtrace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

#[derive(Debug)]
pub(crate) struct LeaseHandleCensus {
    pub(crate) cli_count: usize,
    pub(crate) host_count: usize,
    pub(crate) bun_count: usize,
    pub(crate) lease_host_handle: usize,
}

pub(crate) fn assert_dev_lease_handle_isolation(
    cli_pid: u32,
    host_pid: u32,
    bun_pid: u32,
    known_file_handle: HANDLE,
    lease_handle_value: usize,
) -> LeaseHandleCensus {
    let cli = open_process_for_census(cli_pid);
    let host = open_process_for_census(host_pid);
    let bun = open_process_for_census(bun_pid);
    let cli_entries = census_matching_handle_count(&cli, cli_pid);
    let host_entries = census_matching_handle_count(&host, host_pid);
    let bun_entries = census_matching_handle_count(&bun, bun_pid);
    let current_entries = raw_process_handle_census(std::process::id());
    let known_value = known_file_handle.addr();
    let file_type_index = current_entries
        .iter()
        .find(|entry| entry.handle_value == known_value)
        .map(|entry| entry.object_type_index)
        .expect("known controller pipe exists in the raw handle table");
    let lease = host_entries
        .iter()
        .find(|entry| entry.handle_value == lease_handle_value)
        .expect("reported host stdin lease handle exists in raw table");
    assert_eq!(
        lease.object_type_index, file_type_index,
        "reported host lease is not a kernel File/pipe object"
    );
    assert_eq!(
        lease.granted_access, FILE_GENERIC_READ,
        "host lease is not the read-only pipe endpoint"
    );
    assert_eq!(
        lease.handle_attributes & OBJ_INHERIT,
        0,
        "host lease reader remained inheritable in the raw handle table"
    );
    let lease_object = duplicate_process_handle(&host, lease.handle_value)
        .expect("duplicate exact host lease reader for comparison");
    assert!(
        !process_contains_same_object(&bun, &bun_entries, file_type_index, &lease_object),
        "Bun inherited the host's exact lease reader object"
    );
    LeaseHandleCensus {
        cli_count: cli_entries.len(),
        host_count: host_entries.len(),
        bun_count: bun_entries.len(),
        lease_host_handle: lease.handle_value,
    }
}

fn raw_process_handle_census(pid: u32) -> Vec<SystemHandleEntry> {
    let mut bytes = 1_u32 << 20;
    loop {
        let words = usize::try_from(bytes)
            .expect("NT handle table size fits usize")
            .div_ceil(std::mem::size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let mut returned = 0_u32;
        // SAFETY: aligned storage supplies `bytes` writable bytes and returned
        // length is live. Class 64 is the extended system handle table.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                storage.as_mut_ptr().cast(),
                bytes,
                &raw mut returned,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            bytes = returned.max(bytes.saturating_mul(2));
            continue;
        }
        assert_eq!(
            status,
            0,
            "NtQuerySystemInformation failed: 0x{:08x}",
            status.cast_unsigned()
        );
        let count = storage[0];
        let header_bytes = 2 * std::mem::size_of::<usize>();
        let available = usize::try_from(bytes)
            .expect("NT handle table size fits usize")
            .saturating_sub(header_bytes)
            / std::mem::size_of::<SystemHandleEntry>();
        assert!(count <= available, "NT handle table count exceeds buffer");
        // SAFETY: class 64 begins with two usize fields followed by `count`
        // aligned entries, and count was bounded by the allocated buffer.
        let entries = unsafe {
            std::slice::from_raw_parts(storage.as_ptr().add(2).cast::<SystemHandleEntry>(), count)
        };
        return entries
            .iter()
            .copied()
            .filter(|entry| entry.unique_process_id == pid as usize)
            .collect();
    }
}

fn process_handle_count(process: &OwnedHandle) -> usize {
    let mut count = 0_u32;
    // SAFETY: process is live and count is writable storage.
    assert_ne!(
        unsafe { GetProcessHandleCount(process.as_raw_handle().cast(), &raw mut count) },
        0,
        "GetProcessHandleCount failed: {}",
        std::io::Error::last_os_error()
    );
    usize::try_from(count).expect("process handle count fits usize")
}

fn census_matching_handle_count(process: &OwnedHandle, pid: u32) -> Vec<SystemHandleEntry> {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    loop {
        let entries = raw_process_handle_census(pid);
        let reported_count = process_handle_count(process);
        if entries.len() == reported_count {
            return entries;
        }
        assert!(
            Instant::now() < deadline,
            "raw handle census for PID {pid} never agreed with GetProcessHandleCount: raw={}, reported={reported_count}",
            entries.len()
        );
        thread::park_timeout(Duration::from_millis(20));
    }
}

fn duplicate_process_handle(process: &OwnedHandle, value: usize) -> Option<OwnedHandle> {
    let mut duplicate: HANDLE = std::ptr::null_mut();
    // SAFETY: process is live; value came from its raw handle table; target is
    // the current-process pseudo-handle; output receives one handle on success.
    if unsafe {
        DuplicateHandle(
            process.as_raw_handle().cast(),
            std::ptr::with_exposed_provenance_mut(value),
            GetCurrentProcess(),
            &raw mut duplicate,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
        || duplicate.is_null()
    {
        return None;
    }
    // SAFETY: duplicate is the fresh non-null owning handle returned above.
    Some(unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) })
}

fn process_contains_same_object(
    process: &OwnedHandle,
    entries: &[SystemHandleEntry],
    file_type_index: u16,
    object: &OwnedHandle,
) -> bool {
    entries.iter().any(|entry| {
        if entry.object_type_index != file_type_index {
            return false;
        }
        let Some(candidate) = duplicate_process_handle(process, entry.handle_value) else {
            return false;
        };
        // SAFETY: both duplicated handles are live for this comparison.
        (unsafe {
            CompareObjectHandles(
                object.as_raw_handle().cast(),
                candidate.as_raw_handle().cast(),
            )
        }) != 0
    })
}
