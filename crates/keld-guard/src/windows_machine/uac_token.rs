//! Process-token admission: the one-shot explicitly elevated machine-wide installer, and
//! the KEL-270 T4d updater helper's own-session and `SeImpersonatePrivilege`
//! launch-readiness proofs.

// SAFETY policy: all raw Windows token FFI is confined to this module, whose callers
// receive only a fail-closed admission result and never access its handles or buffers.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::fmt;
use std::io;
use std::mem::{offset_of, size_of};
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::ptr::{addr_of, null, null_mut};

use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, ERROR_NO_TOKEN, GetLastError, LUID,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_IMPERSONATE_NAME,
    SE_PRIVILEGE_ENABLED, SE_PRIVILEGE_REMOVED, SID_AND_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_GROUPS,
    TOKEN_INFORMATION_CLASS, TOKEN_PRIVILEGES, TOKEN_QUERY, TokenElevation, TokenGroups,
    TokenPrivileges, TokenSessionId,
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
const MAX_TOKEN_INFORMATION_BYTES: usize = 1024 * 1024;
const FIX_TOKEN_OS: &str = "Retry from a fresh launch of the installed host; Keld never \
     substitutes another token, changes a token's owner or DACL, or enables a privilege \
     to get past this.";

/// A refusal by a `keld-guard` Windows token wrapper.
///
/// [`code`](Self::code) is its stable `KELD-GUARD*` code; the `Display` text names the
/// refusal and ends with its fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsTokenError {
    /// A token open, query, privilege lookup, duplication or impersonation call failed,
    /// or returned data outside its bounds (`KELD-GUARD018`).
    Os {
        /// The refused Windows operation.
        operation: &'static str,
        /// What Windows reported, or which bound the returned data broke.
        detail: String,
    },
    /// The current process token runs in another session than the initiating token
    /// (`KELD-GUARD019`).
    SessionMismatch {
        /// The initiating token's session.
        expected: u32,
        /// The current process token's session.
        actual: u32,
    },
    /// The current process token does not hold `SeImpersonatePrivilege` enabled
    /// (`KELD-GUARD020`).
    ImpersonatePrivilegeUnavailable {
        /// `absent`, `disabled` or `removed`.
        state: &'static str,
    },
    /// The calling thread already impersonates a token (`KELD-GUARD021`).
    ThreadImpersonating,
    /// The initiating token is elevated or not at Medium integrity (`KELD-GUARD025`).
    InitiatingTokenProfile {
        /// Whether the token is elevated.
        elevated: bool,
        /// The token's mandatory integrity RID.
        integrity_rid: u32,
    },
}

impl WindowsTokenError {
    /// Stable `KELD-GUARD*` code for this refusal.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Os { .. } => "KELD-GUARD018",
            Self::SessionMismatch { .. } => "KELD-GUARD019",
            Self::ImpersonatePrivilegeUnavailable { .. } => "KELD-GUARD020",
            Self::ThreadImpersonating => "KELD-GUARD021",
            Self::InitiatingTokenProfile { .. } => "KELD-GUARD025",
        }
    }

    pub(super) fn os(operation: &'static str, source: &io::Error) -> Self {
        Self::Os {
            operation,
            detail: source.to_string(),
        }
    }
}

impl fmt::Display for WindowsTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Os { operation, detail } => write!(
                f,
                "KELD-GUARD018: Windows token operation `{operation}` failed: {detail}. {FIX_TOKEN_OS}"
            ),
            Self::SessionMismatch { expected, actual } => write!(
                f,
                "KELD-GUARD019: this process runs in session {actual}, not the initiating \
                 session {expected}. Approve the elevation prompt in the initiating user's \
                 interactive session; a process in another session cannot launch the \
                 candidate there."
            ),
            Self::ImpersonatePrivilegeUnavailable { state } => write!(
                f,
                "KELD-GUARD020: SeImpersonatePrivilege is {state} in this process token. \
                 Approve the full elevated administrator prompt; Keld never enables a \
                 privilege as a fallback."
            ),
            Self::ThreadImpersonating => write!(
                f,
                "KELD-GUARD021: the calling thread is already impersonating. Revert the \
                 existing impersonation first; Keld refuses nested impersonation."
            ),
            Self::InitiatingTokenProfile {
                elevated,
                integrity_rid,
            } => write!(
                f,
                "KELD-GUARD025: the initiating token is {} at integrity level \
                 0x{integrity_rid:04X}; only a non-elevated Medium (0x2000) token may start \
                 the candidate. Start the update from the installed host as the signed-in \
                 user without elevation; Keld never substitutes another token.",
                if *elevated {
                    "elevated"
                } else {
                    "not elevated"
                }
            ),
        }
    }
}

impl std::error::Error for WindowsTokenError {}

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
    let (opened, error_code) = probe_thread_token()?;
    thread_token_query_result(opened, error_code)
}

/// Whether the calling thread holds an impersonation token. Only `ERROR_NO_TOKEN`
/// proves that it does not; every other query failure is an error.
pub(super) fn thread_is_impersonating() -> io::Result<bool> {
    let (opened, error_code) = probe_thread_token()?;
    thread_token_state(opened, error_code)
}

fn probe_thread_token() -> io::Result<(bool, u32)> {
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
        // SAFETY: successful OpenThreadToken returned one owned handle, closed here.
        drop(unsafe { OwnedHandle::from_raw_handle(raw) });
        return Ok((true, 0));
    }
    // SAFETY: GetLastError has no pointer or lifetime requirements.
    Ok((false, unsafe { GetLastError() }))
}

fn thread_token_state(opened: bool, error_code: u32) -> io::Result<bool> {
    if opened {
        Ok(true)
    } else if error_code == ERROR_NO_TOKEN {
        // Only ERROR_NO_TOKEN proves that this thread has no impersonation token.
        Ok(false)
    } else {
        Err(io::Error::from_raw_os_error(error_code.cast_signed()))
    }
}

fn thread_token_query_result(opened: bool, error_code: u32) -> io::Result<()> {
    if thread_token_state(opened, error_code)? {
        Err(io::Error::other("initializer thread is impersonating"))
    } else {
        Ok(())
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
    let (storage, returned) = query_token_information(token, TokenGroups)?;
    validate_administrators_group(&storage, returned)
}

/// Requires the current process token to run in `initiating_session_id`, the session of
/// the initiating token. A process that `CreateProcessWithTokenW` creates runs in its
/// caller's session, not in the session of the token it is given.
///
/// # Errors
/// [`WindowsTokenError::SessionMismatch`] for any other session, and
/// [`WindowsTokenError::Os`] for a token open or query failure.
pub fn require_windows_own_token_session(
    initiating_session_id: u32,
) -> Result<(), WindowsTokenError> {
    let token = open_process_token()
        .map_err(|source| WindowsTokenError::os("OpenProcessToken(TOKEN_QUERY)", &source))?;
    let actual = token_session_id(&token)?;
    own_session_result(initiating_session_id, actual)
}

fn own_session_result(expected: u32, actual: u32) -> Result<(), WindowsTokenError> {
    if actual == expected {
        Ok(())
    } else {
        Err(WindowsTokenError::SessionMismatch { expected, actual })
    }
}

fn token_session_id(token: &OwnedHandle) -> Result<u32, WindowsTokenError> {
    const OPERATION: &str = "GetTokenInformation(TokenSessionId)";
    let mut session_id = 0u32;
    let capacity = u32::try_from(size_of::<u32>())
        .map_err(|_| WindowsTokenError::os(OPERATION, &io::Error::other("size overflow")))?;
    let mut returned = 0;
    // SAFETY: token is a live query-only token handle; `session_id` is writable storage of
    // exactly `capacity` bytes and `returned` is writable output storage.
    let success = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            TokenSessionId,
            (&raw mut session_id).cast(),
            capacity,
            &raw mut returned,
        )
    };
    if success == 0 {
        return Err(WindowsTokenError::os(
            OPERATION,
            &io::Error::last_os_error(),
        ));
    }
    if returned != capacity {
        return Err(WindowsTokenError::os(
            OPERATION,
            &io::Error::other(format!("returned {returned} bytes instead of {capacity}")),
        ));
    }
    Ok(session_id)
}

/// Requires the current process token to hold `SeImpersonatePrivilege` enabled and not
/// removed, which `CreateProcessWithTokenW` requires of its caller.
///
/// Keld never enables a privilege, so a present but disabled privilege refuses.
///
/// # Errors
/// [`WindowsTokenError::ImpersonatePrivilegeUnavailable`] when the privilege is absent,
/// disabled or removed, and [`WindowsTokenError::Os`] for a lookup, open or query
/// failure or a malformed privilege list.
pub fn require_windows_own_impersonate_privilege() -> Result<(), WindowsTokenError> {
    let privilege = impersonate_privilege_luid()?;
    let token = open_process_token()
        .map_err(|source| WindowsTokenError::os("OpenProcessToken(TOKEN_QUERY)", &source))?;
    let (storage, returned) = query_token_information(&token, TokenPrivileges)
        .map_err(|source| WindowsTokenError::os("GetTokenInformation(TokenPrivileges)", &source))?;
    impersonate_privilege_result(&storage, returned, privilege)
}

fn impersonate_privilege_luid() -> Result<LUID, WindowsTokenError> {
    let mut luid = LUID {
        LowPart: 0,
        HighPart: 0,
    };
    // SAFETY: a null system name selects the local system; SE_IMPERSONATE_NAME is a
    // static NUL-terminated UTF-16 string and `luid` is writable output storage.
    let found = unsafe { LookupPrivilegeValueW(null(), SE_IMPERSONATE_NAME, &raw mut luid) };
    if found == 0 {
        return Err(WindowsTokenError::os(
            "LookupPrivilegeValueW(SeImpersonatePrivilege)",
            &io::Error::last_os_error(),
        ));
    }
    Ok(luid)
}

fn impersonate_privilege_result(
    storage: &[usize],
    returned: usize,
    privilege: LUID,
) -> Result<(), WindowsTokenError> {
    let attributes = privilege_attributes(storage, returned, privilege)
        .map_err(|source| WindowsTokenError::os("GetTokenInformation(TokenPrivileges)", &source))?;
    let state = match attributes {
        None => "absent",
        Some(attributes) if attributes & SE_PRIVILEGE_REMOVED != 0 => "removed",
        Some(attributes) if attributes & SE_PRIVILEGE_ENABLED == 0 => "disabled",
        Some(_) => return Ok(()),
    };
    Err(WindowsTokenError::ImpersonatePrivilegeUnavailable { state })
}

/// The attributes of `privilege` in a returned `TOKEN_PRIVILEGES` buffer, or `None` when
/// it is not listed. A privilege listed twice is malformed.
fn privilege_attributes(
    storage: &[usize],
    returned: usize,
    privilege: LUID,
) -> io::Result<Option<u32>> {
    let storage_bytes = storage
        .len()
        .checked_mul(size_of::<usize>())
        .ok_or_else(|| io::Error::other("token-privileges allocation size overflow"))?;
    let entries_offset = offset_of!(TOKEN_PRIVILEGES, Privileges);
    if returned > storage_bytes || returned < entries_offset {
        return Err(io::Error::other(
            "token-privileges returned size is outside its allocation",
        ));
    }
    // SAFETY: `returned` covers the header that holds PrivilegeCount at offset 0 inside
    // the word-aligned allocation; read_unaligned makes no alignment assumption.
    let count = unsafe {
        addr_of!((*storage.as_ptr().cast::<TOKEN_PRIVILEGES>()).PrivilegeCount).read_unaligned()
    } as usize;
    let entry_size = size_of::<LUID_AND_ATTRIBUTES>();
    let entries_end = count
        .checked_mul(entry_size)
        .and_then(|bytes| entries_offset.checked_add(bytes))
        .ok_or_else(|| io::Error::other("token-privileges count overflow"))?;
    if entries_end > returned {
        return Err(io::Error::other("token-privileges array is truncated"));
    }
    let mut found = None;
    for index in 0..count {
        let offset = index
            .checked_mul(entry_size)
            .and_then(|bytes| entries_offset.checked_add(bytes))
            .ok_or_else(|| io::Error::other("token-privileges entry offset overflow"))?;
        // SAFETY: the checked entry range lies within the returned bytes of the live
        // allocation; read_unaligned makes no alignment assumption.
        let entry = unsafe {
            storage
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<LUID_AND_ATTRIBUTES>()
                .read_unaligned()
        };
        if entry.Luid.LowPart == privilege.LowPart && entry.Luid.HighPart == privilege.HighPart {
            if found.is_some() {
                return Err(io::Error::other(
                    "token-privileges lists the privilege twice",
                ));
            }
            found = Some(entry.Attributes);
        }
    }
    Ok(found)
}

fn query_token_information(
    token: &OwnedHandle,
    class: TOKEN_INFORMATION_CLASS,
) -> io::Result<(Vec<usize>, usize)> {
    let mut required = 0;
    // SAFETY: this is the documented size query: null buffer and zero length. The token
    // handle is live and query-only; `required` is writable output storage.
    let first = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            class,
            null_mut(),
            0,
            &raw mut required,
        )
    };
    if first != 0 {
        return Err(io::Error::other(
            "token-information size query unexpectedly succeeded",
        ));
    }
    // SAFETY: GetLastError has no pointer or lifetime requirements.
    let code = unsafe { GetLastError() };
    if code != ERROR_INSUFFICIENT_BUFFER || required == 0 {
        return Err(io::Error::from_raw_os_error(code.cast_signed()));
    }
    let byte_len = usize::try_from(required)
        .map_err(|_| io::Error::other("token-information buffer size overflow"))?;
    if byte_len > MAX_TOKEN_INFORMATION_BYTES {
        return Err(io::Error::other(
            "token-information buffer exceeds the safety bound",
        ));
    }
    let word_size = size_of::<usize>();
    let word_count = byte_len
        .checked_add(word_size - 1)
        .ok_or_else(|| io::Error::other("token-information allocation size overflow"))?
        / word_size;
    let mut storage = vec![0usize; word_count];
    let capacity = u32::try_from(
        word_count
            .checked_mul(word_size)
            .ok_or_else(|| io::Error::other("token-information allocation size overflow"))?,
    )
    .map_err(|_| io::Error::other("token-information allocation exceeds Win32 limits"))?;
    let mut returned = 0;
    // SAFETY: the allocation is word-aligned and at least `required` bytes long. The
    // OS fills no more than `capacity`; `returned` is writable output storage.
    let success = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            class,
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
        .map_err(|_| io::Error::other("token-information returned size overflow"))?;
    let capacity = usize::try_from(capacity)
        .map_err(|_| io::Error::other("token-information capacity conversion overflow"))?;
    if returned > byte_len || returned > capacity {
        return Err(io::Error::other(
            "token-information returned size exceeds its buffer",
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

/// Test-only token readers that serve as independent oracles for the wrapper tests.
#[cfg(test)]
pub(super) mod test_support {
    use super::{open_process_token, query_token_information};
    use std::io;
    use std::mem::size_of;
    use std::os::windows::io::{
        AsHandle as _, AsRawHandle as _, BorrowedHandle, FromRawHandle as _, OwnedHandle,
    };
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{ERROR_NO_TOKEN, GetLastError, LUID};
    use windows_sys::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_ELEVATION,
        TOKEN_ELEVATION_TYPE, TOKEN_INFORMATION_CLASS, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
        TOKEN_STATISTICS, TokenElevation, TokenElevationType, TokenIntegrityLevel, TokenSessionId,
        TokenStatistics,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, OpenProcessToken, OpenThreadToken,
    };

    /// Token facts read without any wrapper under test.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct TokenFacts {
        pub(crate) token_id: u64,
        pub(crate) authentication_id: u64,
        pub(crate) modified_id: u64,
        pub(crate) token_type: i32,
        pub(crate) impersonation_level: i32,
        pub(crate) elevation_type: TOKEN_ELEVATION_TYPE,
        pub(crate) elevated: bool,
        pub(crate) integrity_rid: u32,
        pub(crate) session_id: u32,
    }

    pub(crate) fn luid_u64(luid: LUID) -> u64 {
        (u64::from(luid.HighPart.cast_unsigned()) << 32) | u64::from(luid.LowPart)
    }

    pub(crate) fn token_facts(token: BorrowedHandle<'_>) -> TokenFacts {
        let statistics: TOKEN_STATISTICS = fixed(token, TokenStatistics);
        let elevation: TOKEN_ELEVATION = fixed(token, TokenElevation);
        TokenFacts {
            token_id: luid_u64(statistics.TokenId),
            authentication_id: luid_u64(statistics.AuthenticationId),
            modified_id: luid_u64(statistics.ModifiedId),
            token_type: statistics.TokenType,
            impersonation_level: statistics.ImpersonationLevel,
            elevation_type: fixed(token, TokenElevationType),
            elevated: elevation.TokenIsElevated != 0,
            integrity_rid: integrity_rid(token),
            session_id: fixed(token, TokenSessionId),
        }
    }

    fn fixed<T: Copy + Default>(token: BorrowedHandle<'_>, class: TOKEN_INFORMATION_CLASS) -> T {
        let mut value = T::default();
        let size = u32::try_from(size_of::<T>()).expect("token-information size fits u32");
        let mut returned = 0;
        // SAFETY: the borrowed token is live; `value` is writable storage of `size` bytes.
        let ok = unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                class,
                (&raw mut value).cast(),
                size,
                &raw mut returned,
            )
        };
        assert_ne!(ok, 0, "token class {class}: {}", io::Error::last_os_error());
        assert_eq!(returned, size, "token class {class} size");
        value
    }

    fn integrity_rid(token: BorrowedHandle<'_>) -> u32 {
        let owned = token
            .try_clone_to_owned()
            .expect("duplicate the token handle");
        let (storage, returned) =
            query_token_information(&owned, TokenIntegrityLevel).expect("query integrity");
        assert!(returned >= size_of::<TOKEN_MANDATORY_LABEL>());
        // SAFETY: Windows filled a TOKEN_MANDATORY_LABEL in this live aligned buffer; its
        // label SID lies in the same buffer, and the RID index is its last subauthority.
        unsafe {
            let sid = (*storage.as_ptr().cast::<TOKEN_MANDATORY_LABEL>())
                .Label
                .Sid;
            let count = *GetSidSubAuthorityCount(sid);
            *GetSidSubAuthority(sid, u32::from(count) - 1)
        }
    }

    pub(crate) fn current_process_token() -> OwnedHandle {
        open_process_token().expect("open the current process token")
    }

    pub(crate) fn process_token(process: BorrowedHandle<'_>) -> OwnedHandle {
        let mut raw = null_mut();
        // SAFETY: the borrowed process handle is live; `raw` is writable output storage.
        let ok =
            unsafe { OpenProcessToken(process.as_raw_handle().cast(), TOKEN_QUERY, &raw mut raw) };
        assert_ne!(
            ok,
            0,
            "open the process token: {}",
            io::Error::last_os_error()
        );
        // SAFETY: the successful open returned one owned token handle.
        unsafe { OwnedHandle::from_raw_handle(raw) }
    }

    pub(crate) fn current_thread_token() -> Option<OwnedHandle> {
        let mut raw = null_mut();
        // SAFETY: the pseudo-handle names this thread; `raw` is writable output storage.
        if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut raw) } == 0 {
            // SAFETY: GetLastError has no pointer or lifetime requirements.
            assert_eq!(
                unsafe { GetLastError() },
                ERROR_NO_TOKEN,
                "thread-token query"
            );
            return None;
        }
        // SAFETY: the successful open returned one owned token handle.
        Some(unsafe { OwnedHandle::from_raw_handle(raw) })
    }

    pub(crate) fn current_thread_facts() -> Option<TokenFacts> {
        current_thread_token().map(|token| token_facts(token.as_handle()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsHandle as _;
    use std::ptr::addr_of_mut;

    use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
    use windows_sys::Win32::Security::{PRIVILEGE_SET, PrivilegeCheck};
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::SystemServices::PRIVILEGE_SET_ALL_NECESSARY;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    /// `SE_IMPERSONATE_PRIVILEGE` in `wdm.h`: the well-known LUID low part.
    const SE_IMPERSONATE_PRIVILEGE_VALUE: u32 = 29;

    fn os_session_of_this_process() -> u32 {
        let mut session = 0;
        // SAFETY: GetCurrentProcessId has no requirements; `session` is writable storage.
        let ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &raw mut session) };
        assert_ne!(
            ok,
            0,
            "ProcessIdToSessionId: {}",
            io::Error::last_os_error()
        );
        session
    }

    /// `PrivilegeCheck` on a self-impersonation token, independent of the parser and of
    /// `LookupPrivilegeValueW`.
    fn os_privilege_check_holds_impersonate() -> bool {
        // SAFETY: ImpersonateSelf acts on this test thread; the guard always reverts it.
        assert_ne!(unsafe { ImpersonateSelf(SecurityImpersonation) }, 0);
        let _revert = RevertThreadToken;
        let token = test_support::current_thread_token().expect("self-impersonation token");
        let mut set = PRIVILEGE_SET {
            PrivilegeCount: 1,
            Control: PRIVILEGE_SET_ALL_NECESSARY,
            Privilege: [LUID_AND_ATTRIBUTES {
                Luid: LUID {
                    LowPart: SE_IMPERSONATE_PRIVILEGE_VALUE,
                    HighPart: 0,
                },
                Attributes: 0,
            }],
        };
        let mut holds = 0;
        // SAFETY: the impersonation token is live with TOKEN_QUERY; both outputs are writable.
        let ok =
            unsafe { PrivilegeCheck(token.as_raw_handle().cast(), &raw mut set, &raw mut holds) };
        assert_ne!(ok, 0, "PrivilegeCheck: {}", io::Error::last_os_error());
        holds != 0
    }

    /// A `TOKEN_PRIVILEGES` buffer laid out from `winnt.h`: a `u32` count, then 12-byte
    /// `{LowPart u32, HighPart i32, Attributes u32}` entries at offset 4.
    fn privileges_fixture(entries: &[(u32, i32, u32)]) -> (Vec<usize>, usize) {
        assert_eq!(offset_of!(TOKEN_PRIVILEGES, Privileges), 4);
        assert_eq!(size_of::<LUID_AND_ATTRIBUTES>(), 12);
        let returned = 4 + entries.len() * 12;
        let mut bytes = vec![0u8; returned];
        bytes[..4].copy_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
        for (index, (low, high, attributes)) in entries.iter().enumerate() {
            let at = 4 + index * 12;
            bytes[at..at + 4].copy_from_slice(&low.to_le_bytes());
            bytes[at + 4..at + 8].copy_from_slice(&high.to_le_bytes());
            bytes[at + 8..at + 12].copy_from_slice(&attributes.to_le_bytes());
        }
        let word = size_of::<usize>();
        let mut storage = vec![0usize; returned.div_ceil(word)];
        for (index, byte) in bytes.iter().enumerate() {
            storage[index / word] |= usize::from(*byte) << (8 * (index % word));
        }
        (storage, returned)
    }

    fn impersonate_luid() -> LUID {
        LUID {
            LowPart: SE_IMPERSONATE_PRIVILEGE_VALUE,
            HighPart: 0,
        }
    }

    #[test]
    fn token_refusals_have_exact_codes_and_fix_text() {
        let os = WindowsTokenError::Os {
            operation: "DuplicateTokenEx",
            detail: "Access is denied. (os error 5)".into(),
        };
        assert_eq!(os.code(), "KELD-GUARD018");
        assert_eq!(
            os.to_string(),
            "KELD-GUARD018: Windows token operation `DuplicateTokenEx` failed: Access is \
             denied. (os error 5). Retry from a fresh launch of the installed host; Keld \
             never substitutes another token, changes a token's owner or DACL, or enables a \
             privilege to get past this."
        );
        let session = WindowsTokenError::SessionMismatch {
            expected: 2,
            actual: 3,
        };
        assert_eq!(session.code(), "KELD-GUARD019");
        assert_eq!(
            session.to_string(),
            "KELD-GUARD019: this process runs in session 3, not the initiating session 2. \
             Approve the elevation prompt in the initiating user's interactive session; a \
             process in another session cannot launch the candidate there."
        );
        let privilege = WindowsTokenError::ImpersonatePrivilegeUnavailable { state: "disabled" };
        assert_eq!(privilege.code(), "KELD-GUARD020");
        assert_eq!(
            privilege.to_string(),
            "KELD-GUARD020: SeImpersonatePrivilege is disabled in this process token. \
             Approve the full elevated administrator prompt; Keld never enables a privilege \
             as a fallback."
        );
        let nested = WindowsTokenError::ThreadImpersonating;
        assert_eq!(nested.code(), "KELD-GUARD021");
        assert_eq!(
            nested.to_string(),
            "KELD-GUARD021: the calling thread is already impersonating. Revert the existing \
             impersonation first; Keld refuses nested impersonation."
        );
        let elevated = WindowsTokenError::InitiatingTokenProfile {
            elevated: true,
            integrity_rid: 0x3000,
        };
        assert_eq!(elevated.code(), "KELD-GUARD025");
        assert_eq!(
            elevated.to_string(),
            "KELD-GUARD025: the initiating token is elevated at integrity level 0x3000; only \
             a non-elevated Medium (0x2000) token may start the candidate. Start the update \
             from the installed host as the signed-in user without elevation; Keld never \
             substitutes another token."
        );
        let low = WindowsTokenError::InitiatingTokenProfile {
            elevated: false,
            integrity_rid: 0x1000,
        };
        assert_eq!(low.code(), "KELD-GUARD025");
        assert_eq!(
            low.to_string(),
            "KELD-GUARD025: the initiating token is not elevated at integrity level 0x1000; \
             only a non-elevated Medium (0x2000) token may start the candidate. Start the \
             update from the installed host as the signed-in user without elevation; Keld \
             never substitutes another token."
        );
    }

    #[test]
    fn own_session_check_admits_only_the_session_windows_reports_for_this_process() {
        let actual = os_session_of_this_process();
        assert_eq!(require_windows_own_token_session(actual), Ok(()));
        for expected in [actual ^ 1, actual.wrapping_add(2), u32::MAX] {
            assert_eq!(
                require_windows_own_token_session(expected),
                Err(WindowsTokenError::SessionMismatch { expected, actual })
            );
        }
    }

    #[test]
    fn own_session_comparison_is_exact() {
        assert_eq!(own_session_result(0, 0), Ok(()));
        assert_eq!(own_session_result(7, 7), Ok(()));
        assert_eq!(
            own_session_result(1, 0),
            Err(WindowsTokenError::SessionMismatch {
                expected: 1,
                actual: 0
            })
        );
    }

    #[test]
    fn impersonate_privilege_lookup_returns_the_documented_luid() {
        let luid = impersonate_privilege_luid().expect("look up SeImpersonatePrivilege");
        assert_eq!(
            (luid.LowPart, luid.HighPart),
            (SE_IMPERSONATE_PRIVILEGE_VALUE, 0)
        );
    }

    #[test]
    fn own_impersonate_privilege_proof_agrees_with_the_os_privilege_check() {
        let holds = os_privilege_check_holds_impersonate();
        let proof = require_windows_own_impersonate_privilege();
        if holds {
            assert_eq!(proof, Ok(()), "an elevated token holds it enabled");
        } else {
            assert!(
                matches!(
                    proof,
                    Err(WindowsTokenError::ImpersonatePrivilegeUnavailable { .. })
                ),
                "a filtered or standard token must refuse: {proof:?}"
            );
        }
    }

    #[test]
    #[ignore = "operator row: run from an elevated prompt with `cargo test -p keld-guard --lib \
                windows_machine::uac_token::tests::elevated_token_holds_enabled_impersonate_privilege \
                -- --ignored --exact`"]
    fn elevated_token_holds_enabled_impersonate_privilege() {
        let facts = test_support::token_facts(test_support::current_process_token().as_handle());
        assert!(
            facts.elevated,
            "run this operator row from an elevated prompt"
        );
        assert!(os_privilege_check_holds_impersonate());
        assert_eq!(require_windows_own_impersonate_privilege(), Ok(()));
    }

    #[test]
    fn impersonate_privilege_requires_one_enabled_unremoved_entry() {
        let luid = impersonate_luid();
        let enabled = SE_PRIVILEGE_ENABLED;
        let by_default = windows_sys::Win32::Security::SE_PRIVILEGE_ENABLED_BY_DEFAULT;
        let unavailable = |state: &'static str| -> Result<(), WindowsTokenError> {
            Err(WindowsTokenError::ImpersonatePrivilegeUnavailable { state })
        };

        let (storage, returned) = privileges_fixture(&[(23, 0, enabled), (29, 0, enabled)]);
        assert_eq!(
            impersonate_privilege_result(&storage, returned, luid),
            Ok(())
        );
        let (storage, returned) = privileges_fixture(&[(29, 0, enabled | by_default)]);
        assert_eq!(
            impersonate_privilege_result(&storage, returned, luid),
            Ok(())
        );

        let (storage, returned) = privileges_fixture(&[]);
        assert_eq!(
            impersonate_privilege_result(&storage, returned, luid),
            unavailable("absent")
        );
        let (storage, returned) = privileges_fixture(&[(23, 0, enabled), (29, 1, enabled)]);
        assert_eq!(
            impersonate_privilege_result(&storage, returned, luid),
            unavailable("absent"),
            "the high part is part of the LUID"
        );
        let (storage, returned) = privileges_fixture(&[(29, 0, by_default)]);
        assert_eq!(
            impersonate_privilege_result(&storage, returned, luid),
            unavailable("disabled")
        );
        let (storage, returned) = privileges_fixture(&[(29, 0, 0)]);
        assert_eq!(
            impersonate_privilege_result(&storage, returned, luid),
            unavailable("disabled")
        );
        let (storage, returned) = privileges_fixture(&[(29, 0, enabled | SE_PRIVILEGE_REMOVED)]);
        assert_eq!(
            impersonate_privilege_result(&storage, returned, luid),
            unavailable("removed")
        );
    }

    #[test]
    fn impersonate_privilege_parser_rejects_duplicates_and_truncation() {
        let luid = impersonate_luid();
        let enabled = SE_PRIVILEGE_ENABLED;
        let os_detail = |result: Result<(), WindowsTokenError>| match result {
            Err(WindowsTokenError::Os { detail, .. }) => detail,
            other => panic!("expected a malformed-data refusal, got {other:?}"),
        };

        let (storage, returned) = privileges_fixture(&[(29, 0, enabled), (29, 0, enabled)]);
        assert_eq!(
            os_detail(impersonate_privilege_result(&storage, returned, luid)),
            "token-privileges lists the privilege twice"
        );
        let (storage, returned) = privileges_fixture(&[(23, 0, enabled), (29, 0, enabled)]);
        assert_eq!(
            os_detail(impersonate_privilege_result(&storage, returned - 1, luid)),
            "token-privileges array is truncated"
        );
        assert_eq!(
            os_detail(impersonate_privilege_result(&storage, 3, luid)),
            "token-privileges returned size is outside its allocation"
        );
        assert_eq!(
            os_detail(impersonate_privilege_result(
                &storage,
                storage.len() * size_of::<usize>() + 1,
                luid
            )),
            "token-privileges returned size is outside its allocation"
        );
    }

    #[test]
    fn thread_impersonation_state_follows_a_live_self_impersonation() {
        assert!(!thread_is_impersonating().expect("query this thread"));
        // SAFETY: ImpersonateSelf acts on this test thread; the guard always reverts it.
        assert_ne!(unsafe { ImpersonateSelf(SecurityImpersonation) }, 0);
        {
            let _revert = RevertThreadToken;
            assert!(thread_is_impersonating().expect("query this thread"));
        }
        assert!(!thread_is_impersonating().expect("query this thread"));
        assert_eq!(
            thread_token_state(false, ERROR_ACCESS_DENIED)
                .unwrap_err()
                .raw_os_error(),
            Some(5)
        );
    }

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
