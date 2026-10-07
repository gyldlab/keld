//! Owner side of a `keld-attempt` endpoint (KEL-53 §4 "Candidate connect-back"
//! *Creation*; §7 "8 (endpoint squatting)"): the closed descriptor forms and
//! their SID input, first-instance creation and readback, the shared descriptor
//! comparison, refusal of a pre-created or live name, and each error's code and
//! fix.

use std::io;
use std::os::windows::io::OwnedHandle;

use windows_permissions::{LocalBox, SecurityDescriptor};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INVALID_OWNER, ERROR_PIPE_BUSY,
    HANDLE_FLAG_INHERIT,
};
use windows_sys::Win32::System::Pipes::PIPE_REJECT_REMOTE_CLIENTS;

use super::oracle::{
    LABEL, MASK, READBACK_LABEL, SQUAT, Squat, UNRELATED_IDS, endpoint_sddl,
    independent_handle_and_pipe_flags, mismatch_fact, own_token_facts, own_user_sid_bytes,
    own_user_sid_text, probe_exists, random_attempt_name, soon, squat,
};
use crate::attempt::{
    WindowsAttemptClient, WindowsAttemptEndpoint, WindowsAttemptEndpointError,
    WindowsAttemptEndpointSecurity, WindowsAttemptLocatorError, WindowsAttemptLocatorInput,
    WindowsPipeSecurityFact,
};
use crate::windows_named_pipe::{
    PipeSecuritySections, WindowsNamedPipeServer, read_pipe_descriptor,
};

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

/// A squatter that pre-created the name and grants everyone every right,
/// `FILE_CREATE_PIPE_INSTANCE` included, still cannot hand the owner a second
/// instance of its pipe. Both name-in-use codes are reachable: with spare
/// instances `FILE_FLAG_FIRST_PIPE_INSTANCE` refuses with
/// `ERROR_ACCESS_DENIED`, and at its instance limit Windows refuses first with
/// `ERROR_PIPE_BUSY` (observed on Windows 11 26300). A live owner, whose DACL
/// withholds `FILE_CREATE_PIPE_INSTANCE`, refuses with `ERROR_ACCESS_DENIED`
/// (`a_live_owner_name_cannot_be_created_again`).
#[test]
fn a_pre_created_name_is_refused_even_when_the_squatter_allows_more_instances() -> io::Result<()> {
    let security = WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)?;
    for (unlimited_instances, expected) in [(true, ERROR_ACCESS_DENIED), (false, ERROR_PIPE_BUSY)] {
        let name = random_attempt_name()?;
        let _squatter = squat(
            &name,
            "D:P(A;;GA;;;WD)",
            Squat {
                unlimited_instances,
                ..SQUAT
            },
        )?;
        match WindowsAttemptEndpoint::create(&name, &security) {
            Err(WindowsAttemptEndpointError::NameInUse { source }) => assert_eq!(
                source.raw_os_error(),
                Some(expected.cast_signed()),
                "unlimited instances: {unlimited_instances}"
            ),
            other => panic!("a squatted name must be NameInUse: {other:?}"),
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
    let error = WindowsAttemptEndpoint::create_from(&name, UNRELATED_IDS, &security, &deviating)
        .expect_err("a deviating readback must not be admitted");
    assert_eq!(mismatch_fact(error), WindowsPipeSecurityFact::Label);
    let absent = probe_exists(&name).expect_err("the refused instance is closed");
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
        (
            WindowsAttemptEndpointError::Locator(WindowsAttemptLocatorError::ZeroInput {
                input: WindowsAttemptLocatorInput::AttemptId,
            }),
            "KELD-IPC-014",
            "keld-update minted",
        ),
    ];
    for (error, code, fix) in cases {
        let text = error.to_string();
        assert!(text.starts_with(code), "{text}");
        assert!(text.contains(fix), "{text}");
    }
}
