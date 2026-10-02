//! Admission for the one-shot explicitly elevated machine-wide installer.

// SAFETY policy: all raw Windows token FFI is confined to this module, whose callers
// receive only a fail-closed admission result and never access its handles or buffers.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::ptr::{addr_of, null_mut};

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_TOKEN, GetLastError};
use windows_sys::Win32::Security::{
    GetTokenInformation, SID_AND_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_GROUPS, TOKEN_QUERY,
    TokenElevation, TokenGroups,
};
#[cfg(test)]
use windows_sys::Win32::Security::{ImpersonateSelf, RevertToSelf, SecurityImpersonation};
use windows_sys::Win32::System::SystemServices::{
    SE_GROUP_ENABLED, SE_GROUP_OWNER, SE_GROUP_USE_FOR_DENY_ONLY,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
};

const ADMINISTRATORS_AUTHORITY: [u8; 6] = [0, 0, 0, 0, 0, 5];
const ADMINISTRATORS_SUB_AUTHORITIES: [u32; 2] = [32, 544];
const MAX_TOKEN_GROUPS_BYTES: usize = 1024 * 1024;

/// Requires an elevated, non-impersonating process token that owns objects for the
/// built-in Administrators group.
///
/// # Errors
/// Refuses impersonation, a non-elevated or filtered administrator token, malformed
/// token data, and every token-query failure.
pub fn require_windows_machine_uac_owner_token() -> io::Result<()> {
    require_no_thread_impersonation()?;
    let token = open_process_token()?;
    require_elevated(&token)?;
    require_administrators_owner_group(&token)
}

fn require_no_thread_impersonation() -> io::Result<()> {
    let mut raw = null_mut();
    // SAFETY: GetCurrentThread returns a pseudo-handle valid for this call. The output
    // points to writable storage. TOKEN_QUERY requests no mutation right.
    let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut raw) };
    if opened != 0 {
        if raw.is_null() {
            return Err(io::Error::other(
                "thread-token query returned a null handle",
            ));
        }
        // SAFETY: successful OpenThreadToken returned one owned handle.
        let _thread_token = unsafe { OwnedHandle::from_raw_handle(raw) };
        return thread_token_query_result(true, 0);
    }
    // Only ERROR_NO_TOKEN proves that this thread has no impersonation token.
    // SAFETY: GetLastError has no pointer or lifetime requirements.
    let code = unsafe { GetLastError() };
    thread_token_query_result(false, code)
}

fn thread_token_query_result(opened: bool, error_code: u32) -> io::Result<()> {
    if opened {
        Err(io::Error::other("initializer thread is impersonating"))
    } else if error_code == ERROR_NO_TOKEN {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(error_code.cast_signed()))
    }
}

fn open_process_token() -> io::Result<OwnedHandle> {
    let mut raw = null_mut();
    // SAFETY: GetCurrentProcess returns a pseudo-handle valid for this call. `raw`
    // points to writable output storage and the access mask is query-only.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw) };
    if opened == 0 {
        // SAFETY: GetLastError has no pointer or lifetime requirements.
        return Err(io::Error::from_raw_os_error(
            unsafe { GetLastError() }.cast_signed(),
        ));
    }
    if raw.is_null() {
        return Err(io::Error::other(
            "process-token query returned a null handle",
        ));
    }
    // SAFETY: successful OpenProcessToken returned one owned handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

fn require_elevated(token: &OwnedHandle) -> io::Result<()> {
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let capacity = u32::try_from(size_of::<TOKEN_ELEVATION>())
        .map_err(|_| io::Error::other("token-elevation buffer size overflow"))?;
    let mut returned = 0;
    // SAFETY: token is a live query-only token handle; `elevation` has the exact
    // writable size for TOKEN_ELEVATION and `returned` is writable output storage.
    let success = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            TokenElevation,
            (&raw mut elevation).cast(),
            capacity,
            &raw mut returned,
        )
    };
    if success == 0 {
        // SAFETY: GetLastError has no pointer or lifetime requirements.
        return Err(io::Error::from_raw_os_error(
            unsafe { GetLastError() }.cast_signed(),
        ));
    }
    if returned < capacity || elevation.TokenIsElevated == 0 {
        return Err(io::Error::other("installer process token is not elevated"));
    }
    Ok(())
}

fn require_administrators_owner_group(token: &OwnedHandle) -> io::Result<()> {
    let (storage, returned) = query_token_groups(token)?;
    validate_administrators_group(&storage, returned)
}

fn query_token_groups(token: &OwnedHandle) -> io::Result<(Vec<usize>, usize)> {
    let mut required = 0;
    // SAFETY: this is the documented size query: null buffer and zero length. The token
    // handle is live and query-only; `required` is writable output storage.
    let first = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            TokenGroups,
            null_mut(),
            0,
            &raw mut required,
        )
    };
    if first != 0 {
        return Err(io::Error::other(
            "token-groups size query unexpectedly succeeded",
        ));
    }
    // SAFETY: GetLastError has no pointer or lifetime requirements.
    let code = unsafe { GetLastError() };
    if code != ERROR_INSUFFICIENT_BUFFER || required == 0 {
        return Err(io::Error::from_raw_os_error(code.cast_signed()));
    }
    let byte_len = usize::try_from(required)
        .map_err(|_| io::Error::other("token-groups buffer size overflow"))?;
    if byte_len > MAX_TOKEN_GROUPS_BYTES {
        return Err(io::Error::other(
            "token-groups buffer exceeds the safety bound",
        ));
    }
    let word_size = size_of::<usize>();
    let word_count = byte_len
        .checked_add(word_size - 1)
        .ok_or_else(|| io::Error::other("token-groups allocation size overflow"))?
        / word_size;
    let mut storage = vec![0usize; word_count];
    let capacity = u32::try_from(
        word_count
            .checked_mul(word_size)
            .ok_or_else(|| io::Error::other("token-groups allocation size overflow"))?,
    )
    .map_err(|_| io::Error::other("token-groups allocation exceeds Win32 limits"))?;
    let mut returned = 0;
    // SAFETY: the allocation is word-aligned and at least `required` bytes long. The
    // OS fills no more than `capacity`; `returned` is writable output storage.
    let success = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            TokenGroups,
            storage.as_mut_ptr().cast(),
            capacity,
            &raw mut returned,
        )
    };
    if success == 0 {
        // SAFETY: GetLastError has no pointer or lifetime requirements.
        return Err(io::Error::from_raw_os_error(
            unsafe { GetLastError() }.cast_signed(),
        ));
    }
    let returned = usize::try_from(returned)
        .map_err(|_| io::Error::other("token-groups returned size overflow"))?;
    let capacity = usize::try_from(capacity)
        .map_err(|_| io::Error::other("token-groups capacity conversion overflow"))?;
    if returned > byte_len || returned > capacity {
        return Err(io::Error::other(
            "token-groups returned size exceeds its buffer",
        ));
    }
    Ok((storage, returned))
}

fn validate_administrators_group(storage: &[usize], returned: usize) -> io::Result<()> {
    let storage_bytes = storage
        .len()
        .checked_mul(size_of::<usize>())
        .ok_or_else(|| io::Error::other("token-groups allocation size overflow"))?;
    if returned > storage_bytes || returned < std::mem::offset_of!(TOKEN_GROUPS, Groups) {
        return Err(io::Error::other(
            "token-groups returned size is outside its allocation",
        ));
    }
    // SAFETY: GetTokenInformation populated a TOKEN_GROUPS structure in the aligned
    // buffer; validate every flexible-array access and SID pointer before reading it.
    let groups = storage.as_ptr().cast::<TOKEN_GROUPS>();
    let count = unsafe { addr_of!((*groups).GroupCount).read_unaligned() } as usize;
    let groups_offset = std::mem::offset_of!(TOKEN_GROUPS, Groups);
    let group_size = size_of::<SID_AND_ATTRIBUTES>();
    let groups_bytes = count
        .checked_mul(group_size)
        .ok_or_else(|| io::Error::other("token-groups count overflow"))?;
    let groups_end = groups_offset
        .checked_add(groups_bytes)
        .ok_or_else(|| io::Error::other("token-groups boundary overflow"))?;
    if groups_end > returned {
        return Err(io::Error::other("token-groups array is truncated"));
    }
    let mut administrators = 0usize;
    for index in 0..count {
        let offset = groups_offset
            .checked_add(
                index
                    .checked_mul(group_size)
                    .ok_or_else(|| io::Error::other("token-groups entry offset overflow"))?,
            )
            .ok_or_else(|| io::Error::other("token-groups entry offset overflow"))?;
        // SAFETY: the checked entry range lies within returned bytes and the base
        // allocation is suitably aligned for SID_AND_ATTRIBUTES.
        let entry = unsafe {
            storage
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<SID_AND_ATTRIBUTES>()
                .read_unaligned()
        };
        if is_builtin_administrators_sid(entry.Sid.cast(), storage.as_ptr().cast(), returned)? {
            administrators = administrators
                .checked_add(1)
                .ok_or_else(|| io::Error::other("duplicate Administrators group count overflow"))?;
            if !administrators_owner_enabled(entry.Attributes) {
                return Err(io::Error::other(
                    "Administrators group is not enabled and owner-capable",
                ));
            }
        }
    }
    if administrators != 1 {
        return Err(io::Error::other(
            "token must contain exactly one built-in Administrators group",
        ));
    }
    Ok(())
}

fn administrators_owner_enabled(attributes: u32) -> bool {
    attributes & SE_GROUP_ENABLED as u32 != 0
        && attributes & SE_GROUP_OWNER as u32 != 0
        && attributes & SE_GROUP_USE_FOR_DENY_ONLY as u32 == 0
}

fn is_builtin_administrators_sid(
    sid: *mut windows_sys::Win32::Security::SID,
    buffer: *const u8,
    buffer_len: usize,
) -> io::Result<bool> {
    if sid.is_null() {
        return Err(io::Error::other("token group contains a null SID"));
    }
    let base = buffer as usize;
    let start = sid as usize;
    let end = base
        .checked_add(buffer_len)
        .ok_or_else(|| io::Error::other("token-groups buffer address overflow"))?;
    if start < base
        || start
            .checked_add(8)
            .is_none_or(|header_end| header_end > end)
    {
        return Err(io::Error::other(
            "token group SID header is outside its buffer",
        ));
    }
    // SAFETY: checked above that the fixed SID header is fully inside the returned
    // token buffer. Read individual bytes to avoid alignment assumptions.
    let revision = unsafe { sid.cast::<u8>().read() };
    // SAFETY: the second header byte is inside the validated fixed header.
    let sub_authority_count = unsafe { sid.cast::<u8>().add(1).read() } as usize;
    let sid_len = 8usize
        .checked_add(
            sub_authority_count
                .checked_mul(size_of::<u32>())
                .ok_or_else(|| io::Error::other("SID sub-authority length overflow"))?,
        )
        .ok_or_else(|| io::Error::other("SID length overflow"))?;
    if start
        .checked_add(sid_len)
        .is_none_or(|sid_end| sid_end > end)
    {
        return Err(io::Error::other("token group SID is truncated"));
    }
    if revision != 1 || sub_authority_count != 2 {
        return Ok(false);
    }
    let mut authority = [0u8; 6];
    // SAFETY: the six-byte identifier authority lies within the validated header.
    unsafe { std::ptr::copy_nonoverlapping(sid.cast::<u8>().add(2), authority.as_mut_ptr(), 6) };
    if authority != ADMINISTRATORS_AUTHORITY {
        return Ok(false);
    }
    for (index, expected) in ADMINISTRATORS_SUB_AUTHORITIES.iter().enumerate() {
        let offset = 8 + index * size_of::<u32>();
        // SAFETY: the SID length check covers both sub-authorities; `read_unaligned`
        // avoids assuming alignment for the embedded SID pointer.
        let actual = unsafe { sid.cast::<u8>().add(offset).cast::<u32>().read_unaligned() };
        if actual != *expected {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::addr_of_mut;

    fn administrators_groups_fixture(count: u32, attributes: u32) -> (Vec<usize>, usize) {
        let group_offset = std::mem::offset_of!(TOKEN_GROUPS, Groups);
        let group_size = size_of::<SID_AND_ATTRIBUTES>();
        let count_usize = usize::try_from(count).unwrap();
        let sid_offset = group_offset
            .checked_add(count_usize.checked_mul(group_size).unwrap())
            .unwrap();
        let returned = sid_offset
            .checked_add(count_usize.checked_mul(16).unwrap())
            .unwrap();
        let word_size = size_of::<usize>();
        let words = returned.checked_add(word_size - 1).unwrap() / word_size;
        let mut storage = vec![0usize; words];
        // SAFETY: checked offsets fit the word-aligned allocation. Each group entry
        // and 16-byte SID lies wholly inside the bytes reported as returned below.
        unsafe {
            let base = storage.as_mut_ptr().cast::<u8>();
            base.cast::<u32>().write_unaligned(count);
            let groups = storage.as_mut_ptr().cast::<TOKEN_GROUPS>();
            let entries = addr_of_mut!((*groups).Groups).cast::<SID_AND_ATTRIBUTES>();
            for index in 0..count_usize {
                let sid = base.add(sid_offset + index * 16);
                entries.add(index).write_unaligned(SID_AND_ATTRIBUTES {
                    Sid: sid.cast(),
                    Attributes: attributes,
                });
                sid.write(1);
                sid.add(1).write(2);
                sid.add(7).write(5);
                sid.add(8).cast::<u32>().write_unaligned(32);
                sid.add(12).cast::<u32>().write_unaligned(544);
            }
        }
        (storage, returned)
    }

    #[test]
    fn built_in_administrators_sid_is_matched_exactly() {
        let mut sid = [0u8; 16];
        sid[0] = 1;
        sid[1] = 2;
        sid[7] = 5;
        sid[8..12].copy_from_slice(&32u32.to_le_bytes());
        sid[12..16].copy_from_slice(&544u32.to_le_bytes());
        assert!(
            is_builtin_administrators_sid(sid.as_mut_ptr().cast(), sid.as_ptr(), sid.len())
                .unwrap()
        );
        sid[12..16].copy_from_slice(&545u32.to_le_bytes());
        assert!(
            !is_builtin_administrators_sid(sid.as_mut_ptr().cast(), sid.as_ptr(), sid.len())
                .unwrap()
        );
    }

    #[test]
    fn malformed_and_out_of_buffer_sids_are_rejected() {
        let mut sid = [0u8; 15];
        sid[0] = 1;
        sid[1] = 2;
        sid[7] = 5;
        assert!(
            is_builtin_administrators_sid(sid.as_mut_ptr().cast(), sid.as_ptr(), sid.len())
                .is_err()
        );
        let mut sid = [0u8; 16];
        sid[0] = 1;
        sid[1] = 2;
        sid[7] = 5;
        assert!(
            is_builtin_administrators_sid(
                sid.as_mut_ptr().cast(),
                sid.as_ptr().wrapping_add(1),
                15,
            )
            .is_err()
        );
    }

    #[test]
    fn administrators_group_requires_enabled_owner_and_rejects_deny_only() {
        let accepted = (SE_GROUP_ENABLED | SE_GROUP_OWNER) as u32;
        assert!(administrators_owner_enabled(accepted));
        assert!(!administrators_owner_enabled(SE_GROUP_OWNER as u32));
        assert!(!administrators_owner_enabled(SE_GROUP_ENABLED as u32));
        assert!(!administrators_owner_enabled(
            accepted | SE_GROUP_USE_FOR_DENY_ONLY as u32
        ));
        assert!(!administrators_owner_enabled(0));
    }

    #[test]
    fn complete_token_groups_parser_requires_one_exact_owner_enabled_admin() {
        let accepted = (SE_GROUP_ENABLED | SE_GROUP_OWNER) as u32;
        let (storage, returned) = administrators_groups_fixture(1, accepted);
        assert!(validate_administrators_group(&storage, returned).is_ok());

        let (storage, returned) =
            administrators_groups_fixture(1, accepted | SE_GROUP_USE_FOR_DENY_ONLY as u32);
        assert!(validate_administrators_group(&storage, returned).is_err());

        let (storage, returned) = administrators_groups_fixture(1, SE_GROUP_ENABLED as u32);
        assert!(validate_administrators_group(&storage, returned).is_err());

        let (storage, returned) = administrators_groups_fixture(2, accepted);
        assert!(validate_administrators_group(&storage, returned).is_err());
    }

    #[test]
    fn complete_token_groups_parser_rejects_truncation_large_counts_and_foreign_sid() {
        let accepted = (SE_GROUP_ENABLED | SE_GROUP_OWNER) as u32;
        let (storage, returned) = administrators_groups_fixture(1, accepted);
        assert_eq!(
            validate_administrators_group(&storage, returned - 1)
                .unwrap_err()
                .to_string(),
            "token group SID is truncated"
        );

        let (storage, _) = administrators_groups_fixture(2, accepted);
        let one_entry_returned =
            std::mem::offset_of!(TOKEN_GROUPS, Groups) + size_of::<SID_AND_ATTRIBUTES>();
        assert_eq!(
            validate_administrators_group(&storage, one_entry_returned)
                .unwrap_err()
                .to_string(),
            "token-groups array is truncated"
        );

        let (mut storage, returned) = administrators_groups_fixture(1, accepted);
        let group_offset = std::mem::offset_of!(TOKEN_GROUPS, Groups);
        // SAFETY: this checked offset selects the sole in-bounds fixture entry; the
        // substituted pointer intentionally targets four bytes before allocation end.
        unsafe {
            let base = storage.as_mut_ptr().cast::<u8>();
            let entry = base
                .add(group_offset)
                .cast::<SID_AND_ATTRIBUTES>()
                .read_unaligned();
            let mut substituted = entry;
            substituted.Sid = base
                .wrapping_add(storage.len() * size_of::<usize>() - 4)
                .cast();
            base.add(group_offset)
                .cast::<SID_AND_ATTRIBUTES>()
                .write_unaligned(substituted);
        }
        assert_eq!(
            validate_administrators_group(&storage, returned)
                .unwrap_err()
                .to_string(),
            "token group SID header is outside its buffer"
        );
    }

    #[test]
    fn only_absent_thread_token_is_accepted_and_query_errors_fail_closed() {
        assert!(thread_token_query_result(false, ERROR_NO_TOKEN).is_ok());
        assert!(thread_token_query_result(true, 0).is_err());
        assert!(thread_token_query_result(false, 5).is_err());
    }

    struct RevertThreadToken;

    impl Drop for RevertThreadToken {
        fn drop(&mut self) {
            // SAFETY: this test guard is created only after ImpersonateSelf succeeds;
            // RevertToSelf applies to the current test thread and owns no pointers.
            assert_ne!(
                unsafe { RevertToSelf() },
                0,
                "restore the test thread token"
            );
        }
    }

    #[test]
    fn production_thread_check_rejects_a_live_self_impersonation_token() {
        // SAFETY: ImpersonateSelf operates on the current test thread and does not
        // transfer or retain caller pointers. The drop guard always reverts it.
        assert_ne!(
            unsafe { ImpersonateSelf(SecurityImpersonation) },
            0,
            "create an actual thread impersonation token"
        );
        let _revert = RevertThreadToken;
        assert!(
            require_no_thread_impersonation().is_err(),
            "the production OpenThreadToken check must refuse impersonation"
        );
    }
}
