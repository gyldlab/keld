//! The shared token-fact reader `query_windows_peer_token_facts` (KEL-53 §5:
//! `TokenStatistics`, `TokenElevation` and `TokenElevationType` join
//! `TokenUser`, `TokenSessionId` and `TokenIntegrityLevel`, and the reader
//! stays the one source of token facts for the claim writer's token and the
//! initiating token alike).
//!
//! Oracles: literal values of the KEL-53 journal's LUID encoding and of the
//! documented `TOKEN_ELEVATION_TYPE` constants; the Windows SDK's
//! `ANONYMOUS_LOGON_LUID` and the ANONYMOUS LOGON SID; and test-local Win32
//! reads through other information classes (`TokenGroupsAndPrivileges`,
//! `TokenLinkedToken`) and of `TokenElevation`, none of which calls the reader
//! under test.
#![allow(unsafe_code)] // test-only independent Win32 token oracle

use std::io;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};

use windows_sys::Win32::Foundation::{ERROR_NO_SUCH_LOGON_SESSION, LUID};
use windows_sys::Win32::Security::{
    GetTokenInformation, ImpersonateAnonymousToken, RevertToSelf, TOKEN_ELEVATION,
    TOKEN_GROUPS_AND_PRIVILEGES, TOKEN_LINKED_TOKEN, TOKEN_QUERY, TokenElevation,
    TokenGroupsAndPrivileges, TokenLinkedToken,
};
use windows_sys::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};

use super::{
    U32TokenClass, WindowsTokenElevationType, current_process_query_token, elevation_type_from_raw,
    exact_token_information_length, luid_value,
};
use crate::query_windows_peer_token_facts;

/// KEL-53 §4 journal encoding of `TokenStatistics.AuthenticationId`:
/// `(HighPart << 32) | LowPart` as an unsigned 64-bit value.
#[test]
fn authentication_id_is_the_journal_luid_encoding() {
    for (low, high, expected) in [
        (0x0000_03e6_u32, 0_i32, 0x0000_0000_0000_03e6_u64),
        (0x0002_a5f3, 1, 0x0000_0001_0002_a5f3),
        (0x8000_0000, 0x7fff_ffff, 0x7fff_ffff_8000_0000),
        (0, i32::MIN, 0x8000_0000_0000_0000),
        (u32::MAX, -1, u64::MAX),
    ] {
        assert_eq!(
            luid_value(LUID {
                LowPart: low,
                HighPart: high,
            }),
            expected,
            "LowPart {low:#x}, HighPart {high:#x}"
        );
    }
}

/// winnt.h: `TokenElevationTypeDefault = 1`, then `TokenElevationTypeFull` and
/// `TokenElevationTypeLimited`. Any other value is refused, never mapped.
#[test]
fn elevation_type_admits_exactly_the_documented_values() {
    for (raw, expected) in [
        (1, WindowsTokenElevationType::Default),
        (2, WindowsTokenElevationType::Full),
        (3, WindowsTokenElevationType::Limited),
    ] {
        assert_eq!(elevation_type_from_raw(raw).ok(), Some(expected), "{raw}");
    }
    for raw in [0, 4, -1, i32::MIN, i32::MAX] {
        let error = elevation_type_from_raw(raw).expect_err("an undocumented value is refused");
        assert_eq!(
            error.to_string(),
            "TokenElevationType returned an undocumented value"
        );
    }
}

/// The closed set of 32-bit classes is exactly the allowlisted classes, as
/// winnt.h numbers them (`TokenSessionId = 12`, `TokenElevationType = 18`,
/// `TokenElevation = 20`), each reporting its own name.
#[test]
fn the_32_bit_token_classes_are_the_allowlisted_classes() {
    for (class, value, name) in [
        (U32TokenClass::SessionId, 12, "TokenSessionId"),
        (U32TokenClass::ElevationType, 18, "TokenElevationType"),
        (U32TokenClass::Elevation, 20, "TokenElevation"),
    ] {
        assert_eq!(class.class(), value, "{name}");
        assert_eq!(class.name(), name);
    }
}

/// A fixed-size class is admitted only when Windows reports exactly its
/// structure's size; a shorter or longer report is refused with the class
/// named.
#[test]
fn fixed_size_classes_refuse_any_other_returned_length() {
    exact_token_information_length(4, 4, "TokenSessionId").expect("exact DWORD");
    exact_token_information_length(56, 56, "TokenStatistics").expect("exact TOKEN_STATISTICS");
    for (returned, size) in [(0, 56), (4, 56), (55, 56), (57, 56)] {
        let error = exact_token_information_length(returned, size, "TokenStatistics")
            .expect_err("another length is refused");
        assert_eq!(
            error.to_string(),
            "TokenStatistics returned an unexpected size"
        );
    }
}

/// The anonymous logon token belongs to the well-known anonymous logon session
/// (winnt.h `ANONYMOUS_LOGON_LUID` is `{ 0x3e6, 0x0 }`) and its user is
/// ANONYMOUS LOGON (S-1-5-7), so the reader reports the queried token's own
/// session and user, not this process's. It has no linked token, so its
/// elevation type is `Default`; this row reaches the no-linked-token path on
/// every machine, including a split-token one whose own token never does.
#[test]
fn anonymous_logon_token_reports_the_anonymous_session_and_user() -> io::Result<()> {
    let token = anonymous_logon_token()?;
    let anonymous = query_windows_peer_token_facts(&token)?;
    assert_eq!(anonymous.authentication_id, 0x3e6);
    assert_eq!(anonymous.user_sid, [1, 1, 0, 0, 0, 0, 0, 5, 7, 0, 0, 0]);
    assert!(linked_token(&token)?.is_none());
    assert_eq!(anonymous.elevation_type, WindowsTokenElevationType::Default);
    let own = query_windows_peer_token_facts(&current_process_query_token()?)?;
    assert_ne!(own.authentication_id, anonymous.authentication_id);
    assert_ne!(own.user_sid, anonymous.user_sid);
    Ok(())
}

/// For this process's own primary token, the new facts equal what other
/// information classes report: `TokenGroupsAndPrivileges` carries the same
/// `AuthenticationId`, `TokenElevation` the same elevation, and a linked token
/// exists exactly when the elevation type is not `Default` ("The token does not
/// have a linked token"). A split pair's other half has the opposite type and
/// elevation, the same user and its own logon session.
#[test]
fn own_token_facts_agree_with_independent_information_classes() -> io::Result<()> {
    let token = current_process_query_token()?;
    let facts = query_windows_peer_token_facts(&token)?;
    let session = groups_and_privileges_authentication_id(&token)?;
    assert_eq!(
        facts.authentication_id & 0xffff_ffff,
        u64::from(session.LowPart)
    );
    assert_eq!(
        facts.authentication_id >> 32,
        u64::from(session.HighPart.cast_unsigned())
    );
    assert_eq!(facts.elevated, token_is_elevated(&token)?);
    let (other_half, linked) = match (facts.elevation_type, linked_token(&token)?) {
        (WindowsTokenElevationType::Default, None) => return Ok(()),
        (WindowsTokenElevationType::Limited, Some(linked)) => {
            assert!(!facts.elevated, "a limited token is not elevated");
            (WindowsTokenElevationType::Full, linked)
        }
        (WindowsTokenElevationType::Full, Some(linked)) => {
            assert!(facts.elevated, "a full token is elevated");
            (WindowsTokenElevationType::Limited, linked)
        }
        (elevation_type, linked) => panic!(
            "elevation type {elevation_type:?} disagrees with linked token present = {}",
            linked.is_some()
        ),
    };
    let other = query_windows_peer_token_facts(&linked)?;
    assert_eq!(other.elevation_type, other_half);
    assert_eq!(other.elevated, !facts.elevated);
    assert_eq!(other.user_sid, facts.user_sid);
    assert_ne!(other.authentication_id, facts.authentication_id);
    Ok(())
}

/// Opens the system's anonymous logon token by impersonating it on this
/// thread only long enough to open it; aborts if the thread cannot revert.
fn anonymous_logon_token() -> io::Result<OwnedHandle> {
    // SAFETY: the pseudo-handle names this thread, which may impersonate.
    if unsafe { ImpersonateAnonymousToken(GetCurrentThread()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut raw = std::ptr::null_mut();
    // SAFETY: this thread is impersonating and `raw` is writable. OpenAsSelf
    // checks access against the process token, not the anonymous one.
    let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut raw) } != 0;
    let open_error = io::Error::last_os_error();
    // SAFETY: reverting this thread's own impersonation takes no pointers.
    if unsafe { RevertToSelf() } == 0 {
        std::process::abort();
    }
    if !opened {
        return Err(open_error);
    }
    // SAFETY: OpenThreadToken returned one fresh owned token handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

/// `TokenElevation`, read directly.
fn token_is_elevated(token: &OwnedHandle) -> io::Result<bool> {
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0_u32;
    let size = u32::try_from(size_of::<TOKEN_ELEVATION>()).map_err(io::Error::other)?;
    // SAFETY: the token is live and the output buffer is a writable TOKEN_ELEVATION.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            TokenElevation,
            (&raw mut elevation).cast(),
            size,
            &raw mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(elevation.TokenIsElevated != 0)
}

/// `TokenGroupsAndPrivileges.AuthenticationId`: a variable-size class whose
/// fixed header carries the token's logon session.
fn groups_and_privileges_authentication_id(token: &OwnedHandle) -> io::Result<LUID> {
    let raw = token.as_raw_handle().cast();
    let mut required = 0_u32;
    // SAFETY: a null, zero-length query only reports the required size.
    unsafe {
        GetTokenInformation(
            raw,
            TokenGroupsAndPrivileges,
            std::ptr::null_mut(),
            0,
            &raw mut required,
        );
    }
    let words = usize::try_from(required)
        .map_err(io::Error::other)?
        .div_ceil(size_of::<u64>());
    let mut buffer = vec![0_u64; words];
    let capacity = u32::try_from(words * size_of::<u64>()).map_err(io::Error::other)?;
    let mut returned = 0_u32;
    // SAFETY: the u64-aligned buffer is writable for `capacity` bytes.
    if unsafe {
        GetTokenInformation(
            raw,
            TokenGroupsAndPrivileges,
            buffer.as_mut_ptr().cast(),
            capacity,
            &raw mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if usize::try_from(returned).map_err(io::Error::other)?
        < size_of::<TOKEN_GROUPS_AND_PRIVILEGES>()
    {
        return Err(io::Error::other(
            "TokenGroupsAndPrivileges is shorter than its header",
        ));
    }
    // SAFETY: Windows filled at least the header of a TOKEN_GROUPS_AND_PRIVILEGES
    // in this 8-byte-aligned buffer; only its LUID field is copied out.
    Ok(unsafe { (*buffer.as_ptr().cast::<TOKEN_GROUPS_AND_PRIVILEGES>()).AuthenticationId })
}

/// The other half of a split UAC pair, or `None` when Windows reports no
/// linked logon session.
fn linked_token(token: &OwnedHandle) -> io::Result<Option<OwnedHandle>> {
    let mut linked = TOKEN_LINKED_TOKEN::default();
    let mut returned = 0_u32;
    let size = u32::try_from(size_of::<TOKEN_LINKED_TOKEN>()).map_err(io::Error::other)?;
    // SAFETY: the token is live and `linked` is a writable TOKEN_LINKED_TOKEN.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            TokenLinkedToken,
            (&raw mut linked).cast(),
            size,
            &raw mut returned,
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_NO_SUCH_LOGON_SESSION.cast_signed()) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: GetTokenInformation returned one fresh owned token handle.
    Ok(Some(unsafe {
        OwnedHandle::from_raw_handle(linked.LinkedToken.cast())
    }))
}
