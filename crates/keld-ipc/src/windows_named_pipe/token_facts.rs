//! Exact Windows peer access-token facts (the single bounded token-information reader).
//!
//! Pipe lifecycle, peer impersonation and `RevertToSelf` stay with the named-pipe
//! transport owner; this module copies and validates only the queried token's
//! user SID, session, integrity, logon LUID and elevation facts.

use std::io;
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::ptr;

use windows_sys::Win32::Foundation::LUID;
use windows_sys::Win32::Security::{
    GetLengthSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, IsValidSid,
    TOKEN_ELEVATION_TYPE, TOKEN_INFORMATION_CLASS, TOKEN_MANDATORY_LABEL, TOKEN_STATISTICS,
    TOKEN_USER, TokenElevation, TokenElevationType, TokenElevationTypeDefault,
    TokenElevationTypeFull, TokenElevationTypeLimited, TokenIntegrityLevel, TokenSessionId,
    TokenStatistics, TokenUser,
};

/// Token facts observed from a process or the writer of a connected local pipe
/// message. The byte SID is the exact Windows SID encoding.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WindowsPeerTokenFacts {
    /// Binary Windows `TokenUser` SID.
    pub user_sid: Vec<u8>,
    /// Session ID from the token.
    pub session_id: u32,
    /// Mandatory integrity RID from the token.
    pub integrity_rid: u32,
    /// The logon session the token represents, `TokenStatistics.AuthenticationId`,
    /// as `(HighPart << 32) | LowPart` (the KEL-53 journal encoding). Many
    /// tokens can represent one logon session.
    pub authentication_id: u64,
    /// Whether `TokenElevation` reports the token as elevated.
    pub elevated: bool,
    /// The token's `TokenElevationType`.
    pub elevation_type: WindowsTokenElevationType,
}

/// A token's elevation type (`TOKEN_ELEVATION_TYPE`). Only the documented
/// values exist; the reader refuses any other.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WindowsTokenElevationType {
    /// `TokenElevationTypeDefault`: the token does not have a linked token.
    Default,
    /// `TokenElevationTypeFull`: the token is an elevated token.
    Full,
    /// `TokenElevationTypeLimited`: the token is a limited token.
    Limited,
}

/// Reads stable user, session, integrity, logon-session and elevation facts
/// from an owned Windows access token, primary or impersonation (including
/// identification-level). The token must have `TOKEN_QUERY` access; the
/// function returns copied values and retains no pointer into the token
/// buffers.
///
/// # Errors
///
/// Returns an I/O error if a token query fails, Windows returns malformed,
/// truncated or out-of-buffer SID data, a fixed-size class returns any other
/// size, or `TokenElevationType` is not one of its documented values.
pub fn query_windows_peer_token_facts(token: &OwnedHandle) -> io::Result<WindowsPeerTokenFacts> {
    let raw_token = token.as_raw_handle().cast();
    let (user_buffer, user_length) = token_information_buffer(raw_token, TokenUser)?;
    if user_length < std::mem::size_of::<TOKEN_USER>() {
        return Err(io::Error::other(
            "TokenUser buffer is shorter than its header",
        ));
    }
    let user = user_buffer.as_ptr().cast::<TOKEN_USER>();
    // SAFETY: GetTokenInformation populated an aligned buffer and the returned
    // length was checked against TOKEN_USER.
    let user_sid = unsafe { (*user).User.Sid };
    let user_sid = sid_bytes_in_token_buffer(&user_buffer, user_length, user_sid)?;

    let session_id = token_information_u32(raw_token, U32TokenClass::SessionId)?;

    let (integrity_buffer, integrity_length) =
        token_information_buffer(raw_token, TokenIntegrityLevel)?;
    if integrity_length < std::mem::size_of::<TOKEN_MANDATORY_LABEL>() {
        return Err(io::Error::other(
            "TokenIntegrityLevel buffer is shorter than its header",
        ));
    }
    let label = integrity_buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
    // SAFETY: GetTokenInformation populated an aligned buffer and the returned
    // length was checked against TOKEN_MANDATORY_LABEL.
    let integrity_label_sid = unsafe { (*label).Label.Sid };
    validate_sid_in_token_buffer(&integrity_buffer, integrity_length, integrity_label_sid)?;
    // SAFETY: SID is valid and bounded to the returned token buffer.
    let subauthority_count = unsafe { GetSidSubAuthorityCount(integrity_label_sid) };
    if subauthority_count.is_null() {
        return Err(io::Error::other("integrity SID has no subauthority count"));
    }
    // SAFETY: validated SID header owns its subauthority-count byte.
    let count = unsafe { *subauthority_count };
    if count == 0 {
        return Err(io::Error::other("integrity SID has no RID"));
    }
    // SAFETY: last subauthority index is in range for the validated SID.
    let rid = unsafe { GetSidSubAuthority(integrity_label_sid, u32::from(count - 1)) };
    if rid.is_null() {
        return Err(io::Error::other("integrity SID has no final RID"));
    }
    // SAFETY: GetSidSubAuthority returned the last in-range RID of a validated
    // SID contained within the returned token buffer.
    let integrity_rid = unsafe { *rid };

    let authentication_id = luid_value(token_statistics(raw_token)?.AuthenticationId);
    // TOKEN_ELEVATION is one DWORD, `TokenIsElevated`.
    let elevated = token_information_u32(raw_token, U32TokenClass::Elevation)? != 0;
    let elevation_type = elevation_type_from_raw(
        token_information_u32(raw_token, U32TokenClass::ElevationType)?.cast_signed(),
    )?;
    Ok(WindowsPeerTokenFacts {
        user_sid,
        session_id,
        integrity_rid,
        authentication_id,
        elevated,
        elevation_type,
    })
}

/// The token-information classes whose output is one 32-bit value. The closed
/// set is the allowlist: no other class can reach [`token_information_u32`].
#[derive(Debug, Clone, Copy)]
pub(super) enum U32TokenClass {
    /// `TokenSessionId`, a `DWORD`.
    SessionId,
    /// `TokenElevation`, `TOKEN_ELEVATION` (one `DWORD`, `TokenIsElevated`).
    Elevation,
    /// `TokenElevationType`, `TOKEN_ELEVATION_TYPE`.
    ElevationType,
}

impl U32TokenClass {
    pub(super) fn class(self) -> TOKEN_INFORMATION_CLASS {
        match self {
            Self::SessionId => TokenSessionId,
            Self::Elevation => TokenElevation,
            Self::ElevationType => TokenElevationType,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::SessionId => "TokenSessionId",
            Self::Elevation => "TokenElevation",
            Self::ElevationType => "TokenElevationType",
        }
    }
}

/// Reads one of the 32-bit token-information classes.
fn token_information_u32(
    token: windows_sys::Win32::Foundation::HANDLE,
    class: U32TokenClass,
) -> io::Result<u32> {
    let mut value = 0_u32;
    let size = u32::try_from(std::mem::size_of_val(&value)).map_err(io::Error::other)?;
    let mut returned = 0_u32;
    // SAFETY: the caller's token stays owned for this call; `value` is aligned,
    // writable storage for `size` bytes in which every bit pattern is a valid
    // `u32`, and `returned` is writable.
    if unsafe {
        GetTokenInformation(
            token,
            class.class(),
            (&raw mut value).cast(),
            size,
            &raw mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    exact_token_information_length(returned, size, class.name())?;
    Ok(value)
}

/// Reads `TokenStatistics`.
fn token_statistics(token: windows_sys::Win32::Foundation::HANDLE) -> io::Result<TOKEN_STATISTICS> {
    let mut statistics = TOKEN_STATISTICS::default();
    let size = u32::try_from(std::mem::size_of_val(&statistics)).map_err(io::Error::other)?;
    let mut returned = 0_u32;
    // SAFETY: the caller's token stays owned for this call; `statistics` is an
    // aligned, writable TOKEN_STATISTICS of `size` bytes whose fields are all
    // integers, so any bytes Windows writes form a valid value; `returned` is
    // writable.
    if unsafe {
        GetTokenInformation(
            token,
            TokenStatistics,
            (&raw mut statistics).cast(),
            size,
            &raw mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    exact_token_information_length(returned, size, "TokenStatistics")?;
    Ok(statistics)
}

/// Admits a fixed-size token-information class only when Windows reports
/// exactly its structure's size.
pub(super) fn exact_token_information_length(
    returned: u32,
    size: u32,
    name: &str,
) -> io::Result<()> {
    if returned == size {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{name} returned an unexpected size"
        )))
    }
}

/// `(HighPart << 32) | LowPart`, the KEL-53 journal's `authentication_id`.
pub(super) fn luid_value(luid: LUID) -> u64 {
    (u64::from(luid.HighPart.cast_unsigned()) << 32) | u64::from(luid.LowPart)
}

/// Maps the documented `TOKEN_ELEVATION_TYPE` values and refuses any other.
pub(super) fn elevation_type_from_raw(
    raw: TOKEN_ELEVATION_TYPE,
) -> io::Result<WindowsTokenElevationType> {
    if raw == TokenElevationTypeDefault {
        Ok(WindowsTokenElevationType::Default)
    } else if raw == TokenElevationTypeFull {
        Ok(WindowsTokenElevationType::Full)
    } else if raw == TokenElevationTypeLimited {
        Ok(WindowsTokenElevationType::Limited)
    } else {
        Err(io::Error::other(
            "TokenElevationType returned an undocumented value",
        ))
    }
}

fn token_information_buffer(
    token: windows_sys::Win32::Foundation::HANDLE,
    class: windows_sys::Win32::Security::TOKEN_INFORMATION_CLASS,
) -> io::Result<(Vec<usize>, usize)> {
    let mut required = 0_u32;
    // SAFETY: the null output/zero size query asks Windows only for the required
    // buffer size; `required` is writable output storage.
    unsafe {
        GetTokenInformation(token, class, ptr::null_mut(), 0, &raw mut required);
    }
    if required == 0 {
        return Err(io::Error::last_os_error());
    }
    let required_usize = usize::try_from(required).map_err(io::Error::other)?;
    let word = std::mem::size_of::<usize>();
    let words = required_usize
        .checked_add(word - 1)
        .ok_or_else(|| io::Error::other("token information buffer size overflow"))?
        / word;
    let mut buffer = vec![0_usize; words];
    let capacity = words
        .checked_mul(word)
        .ok_or_else(|| io::Error::other("token information buffer capacity overflow"))?;
    let capacity_u32 = u32::try_from(capacity).map_err(io::Error::other)?;
    let mut returned = 0_u32;
    // SAFETY: `buffer` is usize-aligned and writable for `capacity_u32` bytes;
    // its returned length is checked before any structure or SID is read.
    if unsafe {
        GetTokenInformation(
            token,
            class,
            buffer.as_mut_ptr().cast(),
            capacity_u32,
            &raw mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let returned = usize::try_from(returned).map_err(io::Error::other)?;
    if returned == 0 || returned > capacity {
        return Err(io::Error::other(
            "Windows returned an invalid token-information length",
        ));
    }
    Ok((buffer, returned))
}

fn sid_bytes_in_token_buffer(
    buffer: &[usize],
    returned: usize,
    sid: windows_sys::Win32::Security::PSID,
) -> io::Result<Vec<u8>> {
    let sid_length = validate_sid_in_token_buffer(buffer, returned, sid)?;
    // SAFETY: the SID pointer is non-null, valid, and the complete byte range
    // was checked to lie within the live GetTokenInformation buffer.
    Ok(unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), sid_length) }.to_vec())
}

fn validate_sid_in_token_buffer(
    buffer: &[usize],
    returned: usize,
    sid: windows_sys::Win32::Security::PSID,
) -> io::Result<usize> {
    let start = buffer.as_ptr() as usize;
    let end = start
        .checked_add(returned)
        .ok_or_else(|| io::Error::other("token buffer address overflow"))?;
    let sid_start = sid as usize;
    if sid.is_null() || sid_start < start || sid_start >= end {
        return Err(io::Error::other(
            "token SID pointer is outside the returned buffer",
        ));
    }
    // SAFETY: SID begins within a live returned TokenUser/TokenIntegrity buffer.
    if unsafe { IsValidSid(sid) } == 0 {
        return Err(io::Error::other("token SID is invalid"));
    }
    // SAFETY: SID validity was just confirmed by Windows.
    let sid_length = usize::try_from(unsafe { GetLengthSid(sid) }).map_err(io::Error::other)?;
    let sid_end = sid_start
        .checked_add(sid_length)
        .ok_or_else(|| io::Error::other("token SID address overflow"))?;
    if sid_length == 0 || sid_end > end {
        return Err(io::Error::other(
            "token SID extends beyond the returned buffer",
        ));
    }
    Ok(sid_length)
}
