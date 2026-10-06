//! `keld-attempt` endpoint creation, readback and client refusal on real
//! Windows pipes (KEL-53 §4 "Candidate connect-back", "Machine-UAC bootstrap"
//! item 1; §7 "8 (endpoint squatting)" and "17 (argument shape)").
//!
//! Oracles: hand-written SDDL literals, test-local Win32 reads of handle and
//! pipe flags and of a client token's impersonation level, and squatter pipes
//! that this module creates directly with `CreateNamedPipeW`, never through the
//! code under test. This process's own token facts come from the shared reader,
//! which `windows_named_pipe::token_facts_tests` checks against independent
//! token reads.
#![allow(unsafe_code)] // test-only independent Win32 descriptor, pipe and token oracle

use std::fmt::Write as _;
use std::io::{self, Read as _, Write as _};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::time::{Duration, Instant};

use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{
    ConvertSecurityDescriptorToStringSecurityDescriptor, GetSecurityInfo,
};
use windows_permissions::{LocalBox, SecurityDescriptor};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INVALID_OWNER, GetHandleInformation,
    HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, RevertToSelf, SECURITY_ATTRIBUTES, SECURITY_IMPERSONATION_LEVEL,
    SecurityIdentification, TOKEN_QUERY, TokenImpersonationLevel,
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

use super::{
    WindowsAttemptClient, WindowsAttemptEndpoint, WindowsAttemptEndpointError,
    WindowsAttemptEndpointSecurity, WindowsPipeSecurityFact,
};
use crate::WindowsPeerTokenFacts;
use crate::windows_named_pipe::{
    PipeSecuritySections, WaitOutcome, WindowsNamedPipeServer, current_process_query_token,
    current_process_session_id, read_pipe_descriptor,
};

const MASK: &str = "0x12019b";
/// The label as a creator writes it in SDDL.
const LABEL: &str = "S:(ML;;NW;;;ME)";
/// The same label read back: Windows marks the assigned SACL auto-inherited.
const READBACK_LABEL: &str = "S:AI(ML;;NW;;;ME)";

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

fn random_locator() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let mut locator = String::with_capacity(64);
    for byte in bytes {
        write!(locator, "{byte:02x}").map_err(io::Error::other)?;
    }
    Ok(locator)
}

fn random_attempt_name() -> io::Result<String> {
    Ok(format!(r"\\.\pipe\keld-attempt-{}", random_locator()?))
}

/// This process's own token facts, through the shared reader.
fn own_token_facts() -> io::Result<WindowsPeerTokenFacts> {
    crate::query_windows_peer_token_facts(&current_process_query_token()?)
}

/// Binary `TokenUser` SID: the API's input, read through the production token
/// reader. Expected SDDL text comes independently from `current_process_sid`.
fn own_user_sid_bytes() -> io::Result<Vec<u8>> {
    Ok(own_token_facts()?.user_sid)
}

fn own_user_sid_text() -> io::Result<String> {
    Ok(current_process_sid()?.to_string())
}

fn own_session_id() -> io::Result<u32> {
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

fn independent_handle_and_pipe_flags(handle: &OwnedHandle) -> io::Result<(u32, u32)> {
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

fn endpoint_sddl(endpoint: &WindowsAttemptEndpoint) -> io::Result<String> {
    endpoint.server.inspect_owned_pipe(independent_sddl)
}

/// How a test squatter creates its pipe.
#[derive(Clone, Copy)]
struct Squat {
    reject_remote: bool,
    unlimited_instances: bool,
    inheritable: bool,
}

const SQUAT: Squat = Squat {
    reject_remote: true,
    unlimited_instances: false,
    inheritable: false,
};

/// Creates `endpoint` directly with `CreateNamedPipeW`, as any same-user
/// process could, under the descriptor `sddl`.
fn squat(endpoint: &str, sddl: &str, how: Squat) -> io::Result<OwnedHandle> {
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
fn probe_exists(endpoint: &str) -> io::Result<()> {
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

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(2)
}

fn mismatch_fact(error: WindowsAttemptEndpointError) -> WindowsPipeSecurityFact {
    match error {
        WindowsAttemptEndpointError::SecurityMismatch { fact } => fact,
        other => panic!("expected a security mismatch, got {other}"),
    }
}

#[test]
fn per_user_connect_back_endpoint_has_the_exact_descriptor_and_flags() -> io::Result<()> {
    let user = own_user_sid_text()?;
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let name = random_attempt_name()?;
    let endpoint = WindowsAttemptEndpoint::create(&name, &security).map_err(io::Error::other)?;
    assert_eq!(endpoint.endpoint(), name);
    assert_eq!(
        endpoint_sddl(&endpoint)?,
        format!("O:{user}D:P(A;;{MASK};;;{user}){READBACK_LABEL}")
    );
    let (handle_flags, pipe_flags) = endpoint
        .server
        .inspect_owned_pipe(independent_handle_and_pipe_flags)?;
    assert_eq!(handle_flags & HANDLE_FLAG_INHERIT, 0);
    assert_ne!(pipe_flags & PIPE_REJECT_REMOTE_CLIENTS, 0);
    Ok(())
}

#[test]
fn bootstrap_endpoint_grants_exactly_the_host_user_and_administrators() -> io::Result<()> {
    let user = own_user_sid_text()?;
    let security = WindowsAttemptEndpointSecurity::bootstrap(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let endpoint = WindowsAttemptEndpoint::create(&random_attempt_name()?, &security)
        .map_err(io::Error::other)?;
    assert_eq!(
        endpoint_sddl(&endpoint)?,
        format!("O:{user}D:P(A;;{MASK};;;{user})(A;;{MASK};;;BA){READBACK_LABEL}")
    );
    Ok(())
}

/// D5: only an elevated creator can assign `O:BA`. A Medium creator, like a
/// Medium squatter, gets `ERROR_INVALID_OWNER` and leaves no pipe behind. The
/// branch follows this process's own `TokenElevation`, since a hosted runner may
/// run elevated.
#[test]
fn administrators_owner_is_assignable_only_by_an_elevated_creator() -> io::Result<()> {
    let user = own_user_sid_text()?;
    let security = WindowsAttemptEndpointSecurity::machine_uac_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let name = random_attempt_name()?;
    let created = WindowsAttemptEndpoint::create(&name, &security);
    if own_token_facts()?.elevated {
        let endpoint = created.map_err(io::Error::other)?;
        assert_eq!(
            endpoint_sddl(&endpoint)?,
            format!("O:BAD:P(A;;{MASK};;;{user}){READBACK_LABEL}")
        );
        let client = WindowsAttemptClient::connect_until(&name, &security, soon())
            .map_err(io::Error::other)?;
        assert_eq!(
            client.server_process_id().map_err(io::Error::other)?,
            std::process::id()
        );
    } else {
        match created {
            Err(WindowsAttemptEndpointError::Os { source, .. }) => {
                assert_eq!(
                    source.raw_os_error(),
                    Some(ERROR_INVALID_OWNER.cast_signed())
                );
            }
            other => panic!("a Medium creator must not assign O:BA: {other:?}"),
        }
        let absent = probe_exists(&name).expect_err("a refused creation leaves no pipe");
        assert_eq!(
            absent.raw_os_error(),
            Some(ERROR_FILE_NOT_FOUND.cast_signed())
        );
    }
    Ok(())
}

#[test]
fn a_live_owner_name_cannot_be_created_again() -> io::Result<()> {
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let name = random_attempt_name()?;
    let _owner = WindowsAttemptEndpoint::create(&name, &security).map_err(io::Error::other)?;
    match WindowsAttemptEndpoint::create(&name, &security) {
        Err(WindowsAttemptEndpointError::NameInUse { source }) => {
            assert_eq!(
                source.raw_os_error(),
                Some(ERROR_ACCESS_DENIED.cast_signed())
            );
        }
        other => panic!("second creation must be NameInUse: {other:?}"),
    }
    Ok(())
}

/// A squatter that pre-created the name and allows unlimited instances to
/// everyone still cannot hand the owner a second instance of its pipe.
#[test]
fn a_pre_created_name_is_refused_even_when_the_squatter_allows_more_instances() -> io::Result<()> {
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let name = random_attempt_name()?;
    let _squatter = squat(
        &name,
        "D:P(A;;GA;;;WD)",
        Squat {
            unlimited_instances: true,
            ..SQUAT
        },
    )?;
    assert!(matches!(
        WindowsAttemptEndpoint::create(&name, &security),
        Err(WindowsAttemptEndpointError::NameInUse { .. })
    ));
    Ok(())
}

/// KEL-53 §4: before it sends anything the client requires
/// `GetNamedPipeServerSessionId` to equal its own session. The oracle for that
/// session is this process token's `TokenSessionId`, not the
/// `ProcessIdToSessionId` call that the client makes.
#[test]
fn client_admits_a_server_in_its_own_session() -> io::Result<()> {
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let name = random_attempt_name()?;
    let _owner = WindowsAttemptEndpoint::create(&name, &security).map_err(io::Error::other)?;
    let token_session = own_token_facts()?.session_id;
    assert_eq!(current_process_session_id()?, token_session);
    let client =
        WindowsAttemptClient::connect_until(&name, &security, soon()).map_err(io::Error::other)?;
    assert_eq!(
        client.server_session_id().map_err(io::Error::other)?,
        token_session
    );
    Ok(())
}

/// Seam: no second Windows session exists on a test host, so the client's own
/// session is replaced by another value to stand in for a server in a
/// different session. The endpoint is the exact form, so the session is the
/// only fact that differs, and the client refuses on it.
#[test]
fn client_refuses_a_server_in_another_session() -> io::Result<()> {
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let token_session = own_token_facts()?.session_id;
    let other_session = token_session.wrapping_add(1);
    let name = random_attempt_name()?;
    let _owner = WindowsAttemptEndpoint::create(&name, &security).map_err(io::Error::other)?;
    match WindowsAttemptClient::connect_in_session(&name, &security, soon(), other_session) {
        Err(WindowsAttemptEndpointError::ServerSession {
            server_session,
            own_session,
        }) => {
            assert_eq!(server_session, token_session);
            assert_eq!(own_session, other_session);
        }
        other => panic!("a server in another session must be refused: {other:?}"),
    }
    // Positive control: a second exact endpoint, since the refused client
    // occupied the first one's only instance.
    let control = random_attempt_name()?;
    let _control_owner =
        WindowsAttemptEndpoint::create(&control, &security).map_err(io::Error::other)?;
    WindowsAttemptClient::connect_in_session(&control, &security, soon(), token_session)
        .map_err(io::Error::other)?;
    Ok(())
}

#[test]
fn client_admits_the_exact_form_and_reports_the_creating_process() -> io::Result<()> {
    let sid = own_user_sid_bytes()?;
    for security in [
        WindowsAttemptEndpointSecurity::per_user_connect_back(&sid),
        WindowsAttemptEndpointSecurity::bootstrap(&sid),
    ] {
        let security = security.map_err(io::Error::other)?;
        let name = random_attempt_name()?;
        let _owner = WindowsAttemptEndpoint::create(&name, &security).map_err(io::Error::other)?;
        let client = WindowsAttemptClient::connect_until(&name, &security, soon())
            .map_err(io::Error::other)?;
        assert_eq!(
            client.server_process_id().map_err(io::Error::other)?,
            std::process::id()
        );
        assert_eq!(
            client.server_session_id().map_err(io::Error::other)?,
            own_session_id()?
        );
    }
    Ok(())
}

/// A server, the owner or a squatter, can at most identify an attempt
/// client: the impersonation token it obtains is identification-level. What
/// the owner identifies through it, the claim writer's token facts (KEL-53
/// "Candidate connect-back" *Acceptance*), equals the facts of the client
/// process's own primary token: user, session, integrity, logon session,
/// elevation and elevation type.
#[test]
fn attempt_client_grants_the_server_identification_only() -> io::Result<()> {
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let name = random_attempt_name()?;
    let endpoint = WindowsAttemptEndpoint::create(&name, &security).map_err(io::Error::other)?;
    let mut client =
        WindowsAttemptClient::connect_until(&name, &security, soon()).map_err(io::Error::other)?;
    assert_eq!(
        endpoint.server.accept_until(Some(soon()))?,
        WaitOutcome::Ready
    );
    client.stream.write_all(&[0x5a])?;
    let mut server = endpoint.server.stream()?;
    let mut byte = [0_u8; 1];
    server.read_exact(&mut byte)?;
    assert_eq!(byte, [0x5a]);
    let level = endpoint.server.inspect_owned_pipe(impersonated_level)?;
    assert_eq!(level, SecurityIdentification);
    assert_eq!(server.last_client_token_facts()?, own_token_facts()?);
    Ok(())
}

/// Impersonates the pipe's client on this thread only long enough to read
/// the token's impersonation level, and aborts if it cannot revert.
fn impersonated_level(pipe: &OwnedHandle) -> io::Result<SECURITY_IMPERSONATION_LEVEL> {
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

/// Each row squats the name as this same user with one fact wrong; the client
/// must refuse on exactly that fact. The last row is the positive control: an
/// exact same-user copy passes, which is criterion 12's stated residual.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one literal row per squatted fact keeps the refusal table auditable"
)]
fn client_refuses_each_squatted_descriptor_fact() -> io::Result<()> {
    let user = own_user_sid_text()?;
    let sid = own_user_sid_bytes()?;
    let per_user =
        WindowsAttemptEndpointSecurity::per_user_connect_back(&sid).map_err(io::Error::other)?;
    let machine =
        WindowsAttemptEndpointSecurity::machine_uac_connect_back(&sid).map_err(io::Error::other)?;
    let bootstrap = WindowsAttemptEndpointSecurity::bootstrap(&sid).map_err(io::Error::other)?;
    let exact = format!("O:{user}D:P(A;;{MASK};;;{user}){LABEL}");
    let rows: Vec<(
        &str,
        String,
        Squat,
        &WindowsAttemptEndpointSecurity,
        Option<WindowsPipeSecurityFact>,
    )> = vec![
        (
            "missing label",
            format!("O:{user}D:P(A;;{MASK};;;{user})"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Label),
        ),
        (
            "low label",
            format!("O:{user}D:P(A;;{MASK};;;{user})S:(ML;;NW;;;LW)"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Label),
        ),
        (
            "no-read-up instead of no-write-up",
            format!("O:{user}D:P(A;;{MASK};;;{user})S:(ML;;NR;;;ME)"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Label),
        ),
        (
            "extra Everyone ACE",
            format!("O:{user}D:P(A;;{MASK};;;{user})(A;;{MASK};;;WD){LABEL}"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "FILE_CREATE_PIPE_INSTANCE",
            format!("O:{user}D:P(A;;0x12019f;;;{user}){LABEL}"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "WRITE_DAC",
            format!("O:{user}D:P(A;;0x16019b;;;{user}){LABEL}"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "WRITE_OWNER",
            format!("O:{user}D:P(A;;0x1a019b;;;{user}){LABEL}"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "unprotected DACL",
            format!("O:{user}D:(A;;{MASK};;;{user}){LABEL}"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "Everyone instead of the user",
            format!("O:{user}D:P(A;;{MASK};;;WD){LABEL}"),
            SQUAT,
            &per_user,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "bootstrap without Administrators",
            exact.clone(),
            SQUAT,
            &bootstrap,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "user owner where D5 requires O:BA",
            exact.clone(),
            SQUAT,
            &machine,
            Some(WindowsPipeSecurityFact::Owner),
        ),
        (
            "remote clients admitted",
            exact.clone(),
            Squat {
                reject_remote: false,
                ..SQUAT
            },
            &per_user,
            Some(WindowsPipeSecurityFact::RemoteClients),
        ),
        ("exact same-user copy", exact, SQUAT, &per_user, None),
    ];
    for (row, sddl, how, expected, refused_on) in rows {
        let name = random_attempt_name()?;
        let _squatter = squat(&name, &sddl, how)?;
        let outcome = WindowsAttemptClient::connect_until(&name, expected, soon());
        match (refused_on, outcome) {
            (Some(fact), Err(error)) => assert_eq!(mismatch_fact(error), fact, "{row}"),
            (None, Ok(_)) => {}
            (Some(fact), Ok(_)) => panic!("{row}: admitted, expected refusal on {fact}"),
            (None, Err(error)) => panic!("{row}: refused the positive control: {error}"),
        }
    }
    Ok(())
}

/// The owner-side check of the same rule: a pipe that admits remote clients or
/// whose handle is inheritable is not an attempt endpoint, whatever its DACL.
#[test]
fn owner_side_readback_refuses_remote_admission_and_inheritable_handles() -> io::Result<()> {
    let user = own_user_sid_text()?;
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let exact = format!("O:{user}D:P(A;;{MASK};;;{user}){LABEL}");
    for (how, fact) in [
        (
            Squat {
                reject_remote: false,
                ..SQUAT
            },
            WindowsPipeSecurityFact::RemoteClients,
        ),
        (
            Squat {
                inheritable: true,
                ..SQUAT
            },
            WindowsPipeSecurityFact::InheritableHandle,
        ),
    ] {
        let pipe = squat(&random_attempt_name()?, &exact, how)?;
        let readback = read_pipe_descriptor(&pipe)?;
        let error = security
            .verify(&readback)
            .expect_err("a weaker pipe must not verify");
        assert_eq!(mismatch_fact(error), fact);
    }
    let pipe = squat(&random_attempt_name()?, &exact, SQUAT)?;
    security
        .verify(&read_pipe_descriptor(&pipe)?)
        .map_err(io::Error::other)?;
    Ok(())
}

/// The app-link and lifecycle pipes share the one comparison in its DACL-only
/// form: owner and label are the creating token's defaults and are not
/// compared, while the protected DACL and both pipe flags are. The expected
/// descriptor is a literal; a pipe that the shipped `bind` created is the
/// positive control, and the same Low-label pipe that the DACL-only form admits
/// is refused by an attempt form.
#[test]
fn the_dacl_only_form_compares_the_dacl_and_pipe_flags_but_not_owner_or_label() -> io::Result<()> {
    let user = own_user_sid_text()?;
    let expected: LocalBox<SecurityDescriptor> = format!("D:P(A;;{MASK};;;{user})").parse()?;
    let dacl_only = |pipe: &OwnedHandle| {
        read_pipe_descriptor(pipe)?.first_mismatch(&expected, PipeSecuritySections::Dacl)
    };

    let shipped = random_attempt_name()?;
    let server = WindowsNamedPipeServer::bind(&shipped)?;
    assert_eq!(server.inspect_owned_pipe(dacl_only)?, None, "shipped bind");

    let low_label = format!("O:{user}D:P(A;;{MASK};;;{user})S:(ML;;NW;;;LW)");
    for (row, sddl, how, fact) in [
        (
            "default owner, no label",
            format!("D:P(A;;{MASK};;;{user})"),
            SQUAT,
            None,
        ),
        ("explicit owner, Low label", low_label.clone(), SQUAT, None),
        (
            "extra Everyone ACE",
            format!("D:P(A;;{MASK};;;{user})(A;;{MASK};;;WD)"),
            SQUAT,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "unprotected DACL",
            format!("D:(A;;{MASK};;;{user})"),
            SQUAT,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "FILE_CREATE_PIPE_INSTANCE",
            format!("D:P(A;;0x12019f;;;{user})"),
            SQUAT,
            Some(WindowsPipeSecurityFact::Dacl),
        ),
        (
            "remote clients admitted",
            format!("D:P(A;;{MASK};;;{user})"),
            Squat {
                reject_remote: false,
                ..SQUAT
            },
            Some(WindowsPipeSecurityFact::RemoteClients),
        ),
        (
            "inheritable handle",
            format!("D:P(A;;{MASK};;;{user})"),
            Squat {
                inheritable: true,
                ..SQUAT
            },
            Some(WindowsPipeSecurityFact::InheritableHandle),
        ),
    ] {
        let pipe = squat(&random_attempt_name()?, &sddl, how)?;
        assert_eq!(dacl_only(&pipe)?, fact, "{row}");
    }

    let pipe = squat(&random_attempt_name()?, &low_label, SQUAT)?;
    let attempt_form =
        WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
            .map_err(io::Error::other)?;
    let error = attempt_form
        .verify(&read_pipe_descriptor(&pipe)?)
        .expect_err("an attempt form compares the label");
    assert_eq!(mismatch_fact(error), WindowsPipeSecurityFact::Label);
    Ok(())
}

/// Seam: Windows assigns a descriptor other than the requested form (here, no
/// label). The owner must refuse its own instance and leave no pipe behind.
#[test]
fn owner_refuses_and_closes_an_instance_whose_readback_deviates() -> io::Result<()> {
    let user = own_user_sid_text()?;
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let deviating: LocalBox<SecurityDescriptor> =
        format!("O:{user}D:P(A;;{MASK};;;{user})").parse()?;
    let name = random_attempt_name()?;
    let error = WindowsAttemptEndpoint::create_from(&name, &security, &deviating)
        .expect_err("a deviating readback must not be admitted");
    assert_eq!(mismatch_fact(error), WindowsPipeSecurityFact::Label);
    let absent = probe_exists(&name).expect_err("the refused instance is closed");
    assert_eq!(
        absent.raw_os_error(),
        Some(ERROR_FILE_NOT_FOUND.cast_signed())
    );
    Ok(())
}

/// A live pipe sits at each foreign name, so a client or owner that opened or
/// created it would observe it: `EndpointShape` proves the refusal came first.
#[test]
fn attempt_client_and_owner_refuse_other_keld_namespaces_before_any_open() -> io::Result<()> {
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let locator = random_locator()?;
    for name in [
        format!(r"\\.\pipe\keld-{locator}"),
        format!(r"\\.\pipe\keld-lifecycle-{locator}"),
    ] {
        let _live = WindowsNamedPipeServer::bind(&name)?;
        assert!(matches!(
            WindowsAttemptClient::connect_until(&name, &security, soon()),
            Err(WindowsAttemptEndpointError::EndpointShape)
        ));
        assert!(matches!(
            WindowsAttemptEndpoint::create(&name, &security),
            Err(WindowsAttemptEndpointError::EndpointShape)
        ));
        probe_exists(&name)?;
    }
    let unused = format!(r"\\.\pipe\keld-{}", random_locator()?);
    assert!(matches!(
        WindowsAttemptEndpoint::create(&unused, &security),
        Err(WindowsAttemptEndpointError::EndpointShape)
    ));
    let absent = probe_exists(&unused).expect_err("a refused name is never created");
    assert_eq!(
        absent.raw_os_error(),
        Some(ERROR_FILE_NOT_FOUND.cast_signed())
    );
    Ok(())
}

#[test]
fn malformed_binary_sids_are_refused() -> io::Result<()> {
    let sid = own_user_sid_bytes()?;
    let mut wrong_revision = sid.clone();
    wrong_revision[0] = 2;
    let mut count_too_high = sid.clone();
    count_too_high[1] = count_too_high[1].saturating_add(1);
    let mut sixteen = vec![1_u8, 16, 0, 0, 0, 0, 0, 5];
    sixteen.extend(std::iter::repeat_n(0_u8, 64));
    let mut trailing = sid.clone();
    trailing.push(0);
    for (row, bytes) in [
        ("empty", Vec::new()),
        ("header only", vec![1_u8, 1]),
        ("wrong revision", wrong_revision),
        ("count larger than the bytes", count_too_high),
        ("sixteen subauthorities", sixteen),
        ("truncated", sid[..sid.len() - 1].to_vec()),
        ("trailing byte", trailing),
    ] {
        assert!(
            matches!(
                WindowsAttemptEndpointSecurity::per_user_connect_back(&bytes),
                Err(WindowsAttemptEndpointError::InvalidSid)
            ),
            "{row}"
        );
    }
    // A SID with an identifier authority above 2^32 renders in hex SDDL form.
    let wide_authority = [1_u8, 1, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 7, 0, 0, 0];
    WindowsAttemptEndpointSecurity::bootstrap(&wide_authority).map_err(io::Error::other)?;
    Ok(())
}

#[test]
fn every_error_names_its_code_and_fix() {
    let cases = [
        (
            WindowsAttemptEndpointError::EndpointShape,
            "KELD-IPC-008",
            r"\\.\pipe\keld-attempt-<64 lowercase hex>",
        ),
        (
            WindowsAttemptEndpointError::NameInUse {
                source: io::Error::from_raw_os_error(5),
            },
            "KELD-IPC-009",
            "never reuse",
        ),
        (
            WindowsAttemptEndpointError::SecurityMismatch {
                fact: WindowsPipeSecurityFact::Label,
            },
            "KELD-IPC-010",
            "mandatory label",
        ),
        (
            WindowsAttemptEndpointError::InvalidSid,
            "KELD-IPC-011",
            "TokenUser",
        ),
        (
            WindowsAttemptEndpointError::Os {
                operation: "create the endpoint",
                source: io::Error::from_raw_os_error(1307),
            },
            "KELD-IPC-012",
            "BUILTIN Administrators",
        ),
        (
            WindowsAttemptEndpointError::ServerSession {
                server_session: 0,
                own_session: 1,
            },
            "KELD-IPC-013",
            "own session",
        ),
    ];
    for (error, code, fix) in cases {
        let text = error.to_string();
        assert!(text.starts_with(code), "{text}");
        assert!(text.contains(fix), "{text}");
    }
}
