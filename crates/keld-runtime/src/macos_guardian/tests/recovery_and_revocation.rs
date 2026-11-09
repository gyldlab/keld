use std::io;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;

use super::super::{
    BootstrapListener, DEFAULT_ADMISSION_TIMEOUT, GUARDIAN_SHUTDOWN_DEADLINE, GuardedPrimary,
    GuardianBootstrap, HostGenerationState, RecoveryState, RegisteredPrimaryGeneration, RoleEvent,
    RoleGenerationOwner, RoleOwner, check_initial_recovery_decision, finish_generation_control_eof,
    handshake_client, lifecycle_error, lock_recovery, parse_app_link, require_close_on_exec,
    runtime_io_kind,
};

#[test]
fn guarded_primary_rejects_non_supervised_bootstrap_before_generation() {
    let registration = BootstrapListener::bind().expect("guarded registration");
    let app_link = registration.app_link();
    let peer = std::thread::spawn(move || {
        let (endpoint, token) = parse_app_link(&app_link).expect("parse registration link");
        let mut stream =
            std::os::unix::net::UnixStream::connect(endpoint).expect("connect registration link");
        handshake_client(&mut stream, &token).expect("authenticate registration link");
    });
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "read ignored || exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().expect("spawn guardian stand-in");
    let guardian_pid = child.id();
    let writer = child.stdin.take().expect("guardian liveness writer");
    require_close_on_exec(&writer).expect("test liveness CLOEXEC");
    let bootstrap = GuardianBootstrap {
        child: Some(child),
        liveness_writer: Some(writer),
        quit_ack_reader: None,
        registration,
    };
    let error = bootstrap
        .register_guarded_primary_until(Instant::now() + Duration::from_secs(5))
        .expect_err("plain bootstrap must not become guarded primary");
    peer.join().expect("registration peer joins");
    assert!(
        error.to_string().contains("requires the supervised"),
        "{error}"
    );
    let guardian_pid = i32::try_from(guardian_pid).expect("guardian pid fits i32");
    assert_eq!(kill(Pid::from_raw(guardian_pid), None), Err(Errno::ESRCH));
}

#[test]
fn recovery_release_requires_retired_clear_and_initial_arm_is_not_release() {
    let mut recovery = RecoveryState::new();
    assert!(recovery.permits(1));
    recovery.retire(1);
    recovery.arm();
    assert!(recovery.initial_ready);
    assert!(
        !recovery.permits(2),
        "late initial Ready released retirement"
    );
    recovery.arm();
    assert_eq!(recovery.release_attempt, Some(2));
    assert!(!recovery.permits(2), "None before Cleared released G2");
    recovery.cleared(1).expect("record completed G1 Clear");
    assert!(recovery.permits(2));
    assert!(
        !recovery.permits(3),
        "G1 release authorized a wrong attempt"
    );
    recovery.retire(2);
    assert!(!recovery.permits(3), "revocation retained native release");
    recovery.cleared(2).expect("record completed G2 Clear");
    assert!(!recovery.permits(3), "new Clear reused old release");
    recovery.arm();
    assert!(recovery.permits(3));
    recovery.deny();
    recovery.arm();
    assert!(!recovery.permits(3));
    assert!(recovery.release_attempt.is_none());
}

#[test]
fn unresolved_initial_ready_retains_its_deadline_after_native_wait_is_excluded() {
    let mut recovery = RecoveryState::new();
    let expired = Some(
        Instant::now()
            .checked_sub(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("test clock can represent one expired initial decision budget"),
    );
    let error = check_initial_recovery_decision(&recovery, expired)
        .expect_err("unresolved initial Ready remains bounded");
    assert_eq!(runtime_io_kind(&error), Some(io::ErrorKind::TimedOut));
    recovery.arm();
    check_initial_recovery_decision(&recovery, expired)
        .expect("decided eligibility does not time out legal native waiting");
    assert!(
        !recovery.permits(2),
        "eligibility alone released native waiting"
    );
}

#[test]
fn unclaimed_control_eof_explicitly_revokes_exact_lease_and_requires_attribution() {
    for acknowledged in [false, true] {
        let (events_tx, events_rx) = mpsc::channel();
        let mut owner = RoleGenerationOwner::new(
            RoleOwner::Primary,
            DEFAULT_ADMISSION_TIMEOUT,
            events_tx,
            None,
            None,
        );
        let provisioned = owner.provision(7).expect("own one unclaimed attempt");
        let (endpoint, _) = parse_app_link(&provisioned.app_link).expect("actual locator");
        let mut state = HostGenerationState {
            lease: Some(provisioned.lease),
            current_attempt: Some(7),
            ..HostGenerationState::default()
        };
        let recovery = Mutex::new(RecoveryState::new());
        if acknowledged {
            let mut recovery = lock_recovery(&recovery);
            recovery.deny();
            recovery.shutdown_acknowledged = true;
        }
        let result = finish_generation_control_eof(
            lifecycle_error("test EOF", io::Error::from(io::ErrorKind::UnexpectedEof)),
            0,
            &mut state,
            &Mutex::new(None),
            &recovery,
        );
        let events: Vec<_> = events_rx.try_iter().collect();
        let endpoint_closed = std::os::unix::net::UnixStream::connect(endpoint).is_err();
        assert!(state.lease.is_none() && state.current_attempt.is_none());
        assert!(
            endpoint_closed,
            "unclaimed listener survived explicit revoke"
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                RoleEvent::Revoked {
                    attempt: 7,
                    cause: crate::role::RoleRevocationCause::Shutdown,
                    ..
                }
            )),
            "exact owner emitted no Shutdown revocation: {events:?}"
        );
        assert_eq!(
            result.is_ok(),
            acknowledged,
            "denial/EOF fabricated attribution"
        );
    }
}

#[test]
fn attributed_control_eof_keeps_partial_and_registered_failures_fatal() {
    for (partial, registered) in [
        (1, None),
        (
            0,
            Some(RegisteredPrimaryGeneration {
                attempt: 2,
                group_pid: 42,
                group_identity: None, // inert EOF-state fixture, never signal this numeric PID
                native_signal: super::super::guarded_primary::NativeSignalAuthority::Armed,
            }),
        ),
    ] {
        let recovery = Mutex::new(RecoveryState::new());
        {
            let mut recovery = lock_recovery(&recovery);
            recovery.deny();
            recovery.shutdown_acknowledged = true;
        }
        let mut state = HostGenerationState::default();
        let result = finish_generation_control_eof(
            lifecycle_error("test EOF", io::Error::from(io::ErrorKind::UnexpectedEof)),
            partial,
            &mut state,
            &Mutex::new(registered),
            &recovery,
        );
        assert_eq!(
            runtime_io_kind(&result.expect_err("partial/active EOF stays fatal")),
            Some(io::ErrorKind::UnexpectedEof)
        );
    }
}

#[test]
fn denied_recovery_cannot_be_rearmed_by_late_ready() {
    let mut child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("owned liveness peer");
    let liveness_writer = child.stdin.take().expect("liveness writer");
    let quit_ack_reader = child.stdout.take().expect("ack reader");
    let (_updates_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child,
        liveness_writer: Some(liveness_writer),
        quit_ack_reader,
        current_generation: Arc::new(Mutex::new(None)),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: false,
        shutdown_failed: false,
        orderly_acknowledged: false,
        shutdown_sent: None,
        #[cfg(test)]
        shutdown_denial_fence: None,
    };
    primary.deny_recovery();
    primary.arm_recovery();
    primary.arm_recovery();
    let finally_denied = lock_recovery(&primary.recovery_decision).denied;
    primary.liveness_writer.take();
    primary.child.wait().expect("wait owned liveness peer");
    assert!(finally_denied, "late Ready reopened denial");
}
