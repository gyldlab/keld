//! Claimant side of a `keld-attempt` endpoint (KEL-53 §4 "Candidate
//! connect-back" *Claim*; §7 "8 (endpoint squatting)" and "17 (argument
//! shape)"): the client, like the owner, refuses another `keld-*` namespace
//! before any open; before it sends anything it requires the server's session
//! to be its own and the server descriptor to be exactly the expected form,
//! refuses each squatted descriptor fact, and grants any server identification
//! only.

use std::io::{self, Read as _, Write as _};

use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows_sys::Win32::Security::SecurityIdentification;

use super::oracle::{
    LABEL, MASK, SQUAT, Squat, impersonated_level, mismatch_fact, own_session_id, own_token_facts,
    own_user_sid_bytes, own_user_sid_text, probe_exists, random_attempt_name, soon, squat,
};
use crate::attempt::{
    WindowsAttemptClient, WindowsAttemptEndpoint, WindowsAttemptEndpointError,
    WindowsAttemptEndpointSecurity, WindowsPipeSecurityFact,
};
use crate::bootstrap::random_test_locator;
use crate::windows_named_pipe::{WaitOutcome, WindowsNamedPipeServer, current_process_session_id};

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

/// A live pipe sits at each foreign name, so a client or owner that opened or
/// created it would observe it: `EndpointShape` proves the refusal came first.
#[test]
fn attempt_client_and_owner_refuse_other_keld_namespaces_before_any_open() -> io::Result<()> {
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    let locator = random_test_locator()?;
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
    let unused = format!(r"\\.\pipe\keld-{}", random_test_locator()?);
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
