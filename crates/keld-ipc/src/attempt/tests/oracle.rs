//! The independent Win32 oracle that the `keld-attempt` endpoint and client
//! tests share: the expected SDDL literals, squatter pipes created directly with
//! `CreateNamedPipeW`, a no-access existence probe, and test-local reads of a
//! live pipe's descriptor, handle and pipe flags, of this process's session and
//! of a pipe client's impersonation level. No expected value comes from the
//! code under test; this process's own token facts come from the shared reader,
//! which `windows_named_pipe::token_facts_tests` checks independently.
#![allow(unsafe_code)] // test-only independent Win32 descriptor, pipe and token oracle

use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::time::{Duration, Instant};

use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{
    ConvertSecurityDescriptorToStringSecurityDescriptor,
    ConvertStringSecurityDescriptorToSecurityDescriptor, GetSecurityInfo,
};
use windows_permissions::{LocalBox, SecurityDescriptor};
use windows_sys::Win32::Foundation::{GetHandleInformation, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::{
    GetTokenInformation, RevertToSelf, SECURITY_ATTRIBUTES, SECURITY_IMPERSONATION_LEVEL,
    TOKEN_QUERY, TokenImpersonationLevel,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, GetNamedPipeInfo, ImpersonateNamedPipeClient, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};

use crate::WindowsPeerTokenFacts;
use crate::attempt::{
    WindowsAttemptEndpoint, WindowsAttemptEndpointError, WindowsPipeSecurityFact,
};
use crate::bootstrap::random_test_locator;
use crate::windows_named_pipe::current_process_query_token;

pub(super) const MASK: &str = "0x12019b";
/// The label as a creator writes it in SDDL.
pub(super) const LABEL: &str = "S:(ML;;NW;;;ME)";
/// The same label read back: Windows marks the assigned SACL auto-inherited.
pub(super) const READBACK_LABEL: &str = "S:AI(ML;;NW;;;ME)";

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

pub(super) fn random_attempt_name() -> io::Result<String> {
    Ok(format!(r"\\.\pipe\keld-attempt-{}", random_test_locator()?))
}

/// This process's own token facts, through the shared reader.
pub(super) fn own_token_facts() -> io::Result<WindowsPeerTokenFacts> {
    crate::query_windows_peer_token_facts(&current_process_query_token()?)
}

/// Binary `TokenUser` SID: the API's input, read through the production token
/// reader. Expected SDDL text comes independently from `current_process_sid`.
pub(super) fn own_user_sid_bytes() -> io::Result<Vec<u8>> {
    Ok(own_token_facts()?.user_sid)
}

pub(super) fn own_user_sid_text() -> io::Result<String> {
    sddl_owner_text(&current_process_sid()?.to_string())
}

/// A SID as Windows writes it inside a descriptor's SDDL: the `S-1-…` string,
/// or a well-known alias such as `LA` for this machine's built-in
/// Administrator (RID 500), the account hosted Windows runners use.
fn sddl_owner_text(sid: &str) -> io::Result<String> {
    let descriptor = ConvertStringSecurityDescriptorToSecurityDescriptor(&format!("O:{sid}"))?;
    let rendered = ConvertSecurityDescriptorToStringSecurityDescriptor(
        &descriptor,
        SecurityInformation::Owner,
    )?
    .to_string_lossy()
    .into_owned();
    rendered
        .strip_prefix("O:")
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other(format!("owner-only SDDL rendered as {rendered}")))
}

/// Hosted runners run as the built-in Administrator, whose SID the descriptor
/// SDDL writes as `LA`; this host's ordinary account keeps its `S-1-…` form.
#[test]
fn expected_owner_text_uses_the_descriptor_sddl_alias() -> io::Result<()> {
    let user = current_process_sid()?.to_string();
    let (domain, rid) = user
        .rsplit_once('-')
        .ok_or_else(|| io::Error::other(format!("SID without a RID: {user}")))?;
    assert_eq!(sddl_owner_text(&format!("{domain}-500"))?, "LA");
    if rid != "500" {
        assert_eq!(sddl_owner_text(&user)?, user);
    }
    Ok(())
}

pub(super) fn own_session_id() -> io::Result<u32> {
    let mut session = 0_u32;
    // SAFETY: `session` is writable storage for this process's own session.
    if unsafe { ProcessIdToSessionId(std::process::id(), &raw mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session)
}

/// Owner, DACL and label of a live pipe handle, as one SDDL string.
fn independent_sddl(handle: &OwnedHandle) -> io::Result<String> {
    let sections =
        SecurityInformation::Owner | SecurityInformation::Dacl | SecurityInformation::Label;
    let descriptor = GetSecurityInfo(handle, SeObjectType::SE_KERNEL_OBJECT, sections)?;
    Ok(
        ConvertSecurityDescriptorToStringSecurityDescriptor(&descriptor, sections)?
            .to_string_lossy()
            .into_owned(),
    )
}

pub(super) fn independent_handle_and_pipe_flags(handle: &OwnedHandle) -> io::Result<(u32, u32)> {
    let mut handle_flags = 0_u32;
    // SAFETY: the borrowed handle is live and the output is writable.
    if unsafe { GetHandleInformation(handle.as_raw_handle(), &raw mut handle_flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut pipe_flags = 0_u32;
    // SAFETY: the borrowed pipe handle is live; unused outputs are optional.
    if unsafe {
        GetNamedPipeInfo(
            handle.as_raw_handle(),
            &raw mut pipe_flags,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok((handle_flags, pipe_flags))
}

pub(super) fn endpoint_sddl(endpoint: &WindowsAttemptEndpoint) -> io::Result<String> {
    endpoint.server.inspect_owned_pipe(independent_sddl)
}

/// How a test squatter creates its pipe.
#[derive(Clone, Copy)]
pub(super) struct Squat {
    pub(super) reject_remote: bool,
    pub(super) unlimited_instances: bool,
    pub(super) inheritable: bool,
}

pub(super) const SQUAT: Squat = Squat {
    reject_remote: true,
    unlimited_instances: false,
    inheritable: false,
};

/// Creates `endpoint` directly with `CreateNamedPipeW`, as any same-user
/// process could, under the descriptor `sddl`.
pub(super) fn squat(endpoint: &str, sddl: &str, how: Squat) -> io::Result<OwnedHandle> {
    let descriptor: LocalBox<SecurityDescriptor> = sddl.parse()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).map_err(io::Error::other)?,
        lpSecurityDescriptor: descriptor.as_ptr().cast(),
        bInheritHandle: i32::from(how.inheritable),
    };
    let reject = if how.reject_remote {
        PIPE_REJECT_REMOTE_CLIENTS
    } else {
        0
    };
    let instances = if how.unlimited_instances {
        PIPE_UNLIMITED_INSTANCES
    } else {
        1
    };
    let name = wide(endpoint);
    // SAFETY: `name` is NUL terminated; `attributes` points at the live parsed
    // descriptor, and both outlive the call.
    let raw = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | reject,
            instances,
            4096,
            4096,
            0,
            &raw const attributes,
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateNamedPipeW returned one fresh owned handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

/// Opens an existing pipe name with no access, only to prove it exists.
pub(super) fn probe_exists(endpoint: &str) -> io::Result<()> {
    let name = wide(endpoint);
    // SAFETY: `name` is NUL terminated and no template or security is passed.
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            0,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW returned one fresh owned handle.
    drop(unsafe { OwnedHandle::from_raw_handle(raw.cast()) });
    Ok(())
}

pub(super) fn soon() -> Instant {
    Instant::now() + Duration::from_secs(2)
}

pub(super) fn mismatch_fact(error: WindowsAttemptEndpointError) -> WindowsPipeSecurityFact {
    match error {
        WindowsAttemptEndpointError::SecurityMismatch { fact } => fact,
        other => panic!("expected a security mismatch, got {other}"),
    }
}

/// Impersonates the pipe's client on this thread only long enough to read
/// the token's impersonation level, and aborts if it cannot revert.
pub(super) fn impersonated_level(pipe: &OwnedHandle) -> io::Result<SECURITY_IMPERSONATION_LEVEL> {
    // SAFETY: the borrowed server handle is live and its client wrote data.
    if unsafe { ImpersonateNamedPipeClient(pipe.as_raw_handle()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let read = (|| {
        let mut raw = std::ptr::null_mut();
        // SAFETY: this thread is impersonating; `raw` is writable. OpenAsSelf
        // uses the process context, as an identification token requires.
        if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: OpenThreadToken returned one fresh owned token handle.
        let token = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
        let mut level: SECURITY_IMPERSONATION_LEVEL = 0;
        let mut returned = 0_u32;
        let size =
            u32::try_from(size_of::<SECURITY_IMPERSONATION_LEVEL>()).map_err(io::Error::other)?;
        // SAFETY: the token is live and `level` is writable for `size` bytes.
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                TokenImpersonationLevel,
                (&raw mut level).cast(),
                size,
                &raw mut returned,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(level)
    })();
    // SAFETY: reverting this thread's own impersonation takes no pointers.
    if unsafe { RevertToSelf() } == 0 {
        std::process::abort();
    }
    read
}
