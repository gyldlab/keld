use std::io;
use std::os::unix::process::CommandExt as _;
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use super::super::{
    CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REGISTERED,
    CONTROL_REVOKE, CONTROL_REVOKED, CONTROL_SPAWNED, GenerationControlRecord,
    GuardedPrimaryUpdate, RecoveryState, RestartPolicy, RevocationCause, RoleEvent, RuntimeError,
    Supervisor, SupervisorOutcome, handshake_client, interrupted_successor_preparation,
    is_owned_preparation_interruption, lock_recovery, parse_app_link, read_generation_control,
    run_generation_host, terminate_registered_group, write_generation_control,
};
use super::preparation_fixture::GatedUnmarkedFailurePreparer;
use super::test_support::long_running_command;

mod clear_authority;
mod duplicate_spawned;
mod invalid_spawn_after_revoke;

#[derive(Debug, Clone, Copy)]
enum PreparationAttribution {
    Accepted,
    Requested,
    Unattributed,
}

fn guardian_exchange(
    guardian: &mut std::os::unix::net::UnixStream,
    request: &GenerationControlRecord,
    expected: u8,
) -> GenerationControlRecord {
    write_generation_control(guardian, request).expect("write guardian control request");
    let response = read_generation_control(guardian).expect("read guardian control response");
    assert_eq!(response.kind, expected);
    assert_eq!(response.attempt, request.attempt);
    response
}

// Native generation-host fixture: group leaders below are real direct children
// of this test process, which is the deliberately retained host/Guardian owner.
fn start_generation_host_fixture(
    host: std::os::unix::net::UnixStream,
    updates_tx: mpsc::Sender<GuardedPrimaryUpdate>,
    current: Arc<Mutex<Option<super::super::RegisteredPrimaryGeneration>>>,
    recovery: Arc<Mutex<RecoveryState>>,
) -> std::thread::JoinHandle<Result<(), RuntimeError>> {
    std::thread::spawn(move || {
        run_generation_host(
            super::super::generation_host::GenerationHostAdmission::new(
                std::process::id(),
                Instant::now() + super::super::GUARDIAN_SHUTDOWN_DEADLINE,
            ),
            host,
            &updates_tx,
            &current,
            &recovery,
            None,
            None,
        )
    })
}

#[test]
fn generation_host_rejects_out_of_order_revoke_before_prepare() {
    let (host, mut guardian) = std::os::unix::net::UnixStream::pair().expect("KGC1 pair");
    let (updates_tx, _updates_rx) = mpsc::channel();
    let current = Mutex::new(None);
    write_generation_control(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_REVOKE, 1).with_cause(RevocationCause::ChildExited),
    )
    .expect("write out-of-order revoke");
    let error = run_generation_host(
        super::super::generation_host::GenerationHostAdmission::new(
            std::process::id(), // unit peer: direct child groups belong to this harness
            Instant::now() + super::super::GUARDIAN_SHUTDOWN_DEADLINE,
        ),
        host,
        &updates_tx,
        &current,
        &Mutex::new(RecoveryState::new()),
        None,
        None,
    )
    .expect_err("revoke before prepare must fail");
    assert!(
        error.to_string().contains("invalid revoke transition"),
        "{error}"
    );
}

#[test]
fn generation_host_revokes_and_clears_before_preparing_successor() {
    use std::os::unix::net::UnixStream;

    let (host, mut guardian) = UnixStream::pair().expect("KGC1 pair");
    guardian
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("guardian control deadline");
    let (updates_tx, updates_rx) = mpsc::channel();
    let current = Arc::new(Mutex::new(None));
    let recovery_armed = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery_armed).arm();
    let host_current = Arc::clone(&current);
    let host_recovery = Arc::clone(&recovery_armed);
    let host_thread = start_generation_host_fixture(host, updates_tx, host_current, host_recovery);

    let prepared_one = guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_PREPARE, 1),
        CONTROL_PREPARED,
    );
    let app_link = String::from_utf8(prepared_one.payload).expect("g1 app link UTF-8");
    let mut child_command = long_running_command();
    child_command.process_group(0);
    let mut child = child_command.spawn().expect("g1 group leader");
    let child_pid = child.id();
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(child_pid),
        CONTROL_REGISTERED,
    );
    let (endpoint, token) = parse_app_link(&app_link).expect("parse g1 app link");
    let mut app = UnixStream::connect(endpoint).expect("connect g1 app link");
    handshake_client(&mut app, &token).expect("authenticate g1 app link");
    let bound = loop {
        let update = updates_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("g1 bound update");
        if let GuardedPrimaryUpdate::Bound(bound) = update {
            break bound;
        }
    };
    assert_eq!(bound.attempt(), 1);

    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_REVOKE, 1).with_cause(RevocationCause::ChildExited),
        CONTROL_REVOKED,
    );
    terminate_registered_group(child_pid).expect("signal g1 group");
    let _ = child.wait().expect("wait g1 group leader");
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_CLEAR, 1).with_pid(child_pid),
        CONTROL_CLEARED,
    );
    lock_recovery(&recovery_armed).arm();
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_PREPARE, 2),
        CONTROL_PREPARED,
    );

    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_REVOKE, 2)
            .with_cause(RevocationCause::AdmissionFailed),
        CONTROL_REVOKED,
    );
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_CLEAR, 2),
        CONTROL_CLEARED,
    );
    {
        let mut recovery = lock_recovery(&recovery_armed);
        recovery.deny();
        recovery.shutdown_acknowledged = true;
    }
    drop(guardian);
    host_thread
        .join()
        .expect("generation host joins")
        .expect("clean generation host EOF");

    let events: Vec<RoleEvent> = updates_rx
        .try_iter()
        .filter_map(|update| match update {
            GuardedPrimaryUpdate::Role(event) => Some(event),
            GuardedPrimaryUpdate::Bound(_) => None,
        })
        .collect();
    let revoked = events
        .iter()
        .position(|event| matches!(event, RoleEvent::Revoked { attempt: 1, .. }))
        .expect("g1 Revoked event");
    let provisioned = events
        .iter()
        .position(|event| matches!(event, RoleEvent::Provisioned { attempt: 2, .. }))
        .expect("g2 Provisioned event");
    assert!(revoked < provisioned, "g2 provisioned before g1 revocation");
}

#[test]
fn unmarked_successor_preparation_errors_preserve_failed_outcome() {
    use crate::SupervisorEvent;
    use PreparationAttribution::{Accepted, Requested, Unattributed};
    use io::ErrorKind::{Interrupted, InvalidData};

    const DIAGNOSTIC_PHASE: &str = "macOS guardian generation control transition";
    // Ordinary I/O kinds and public diagnostic phases do not mark an
    // intentional runtime cancellation, even after a stop fact is set.
    for attribution in [Accepted, Requested, Unattributed] {
        for (phase, kind) in [
            (DIAGNOSTIC_PHASE, Interrupted),
            (DIAGNOSTIC_PHASE, InvalidData),
            ("test unrelated preparation I/O", Interrupted),
            ("test unrelated preparation I/O", InvalidData),
        ] {
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let mut command = Command::new("/bin/sh");
            command.args([
                "-c",
                "printf KEL143_UNMARKED_G1_STDOUT; \
                 printf KEL143_UNMARKED_G1_STDERR >&2; exit 1",
            ]);
            let supervisor = Supervisor::start_prepared(
                RestartPolicy::default(),
                GatedUnmarkedFailurePreparer {
                    initial: Some(command),
                    entered_tx,
                    release_rx,
                    phase,
                    kind,
                    owned: false,
                },
            )
            .expect("initial real child must spawn");
            // Only attempt 2 reaches this fence, after actual G1 retirement.
            let entered = entered_rx.recv_timeout(Duration::from_secs(5));
            match attribution {
                Accepted => supervisor.accept_shutdown(),
                Requested => supervisor.shutdown(),
                Unattributed => {}
            }
            if entered.is_err() {
                supervisor.shutdown();
            }
            // Release before asserting so Drop cannot wait on our own gate.
            let released = release_tx.send(());
            drop(release_tx);
            entered.expect("successor preparation must reach the existing trait fence");
            released.expect("release the controlled preparation error");

            let deadline = Instant::now() + Duration::from_secs(5);
            let terminal = loop {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .expect("successor preparation terminal event deadline");
                match supervisor.recv_event(remaining) {
                    Some(SupervisorEvent::Started { attempt: 2, .. }) => {
                        panic!("failed successor preparation spawned G2");
                    }
                    Some(event @ (SupervisorEvent::Stopped | SupervisorEvent::Failed { .. })) => {
                        break event;
                    }
                    Some(_) => {}
                    None => panic!("no terminal successor event for {attribution:?}/{kind:?}"),
                }
            };
            let outcome = supervisor.wait_for_outcome();
            assert!(
                supervisor.current_pid().is_none(),
                "G2 must not remain live"
            );
            let ledger = supervisor.crash_ledger();
            assert_eq!(ledger.self_termination_count, 1);
            assert!(
                ledger
                    .last_self_termination
                    .is_some_and(|exit| exit.exit_code == Some(1)),
                "shutdown must preserve the already-observed G1 exit"
            );
            let output = supervisor.output();
            assert!(output.stdout.contains("KEL143_UNMARKED_G1_STDOUT"));
            assert!(output.stderr.contains("KEL143_UNMARKED_G1_STDERR"));
            assert!(
                matches!(terminal, SupervisorEvent::Failed { attempt: 2 }),
                "unmarked error became a stop: {attribution:?}/{phase}/{kind:?}: {terminal:?}"
            );
            match outcome {
                SupervisorOutcome::Failed(RuntimeError::Lifecycle {
                    phase: actual,
                    source,
                }) => {
                    assert_eq!(actual, phase);
                    assert_eq!(source.kind(), kind);
                    assert_eq!(
                        source.to_string(),
                        "controlled successor preparation failure"
                    );
                }
                other => panic!("unmarked preparation failure was hidden: {other:?}"),
            }
        }
    }
}

#[test]
fn owned_successor_interruption_requires_supervisor_stop_and_exact_attempt() {
    use crate::SupervisorEvent;
    for attribution in [
        PreparationAttribution::Accepted,
        PreparationAttribution::Requested,
        PreparationAttribution::Unattributed,
    ] {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exit 1"]);
        let supervisor = Supervisor::start_prepared(
            RestartPolicy::default(),
            GatedUnmarkedFailurePreparer {
                initial: Some(command),
                entered_tx,
                release_rx,
                phase: "unused marked phase",
                kind: io::ErrorKind::Interrupted,
                owned: true,
            },
        )
        .expect("initial real child");
        let entered = entered_rx.recv_timeout(Duration::from_secs(5));
        match attribution {
            PreparationAttribution::Accepted => supervisor.accept_shutdown(),
            PreparationAttribution::Requested => supervisor.shutdown(),
            PreparationAttribution::Unattributed => {}
        }
        if entered.is_err() {
            supervisor.shutdown();
        }
        let released = release_tx.send(());
        drop(release_tx);
        entered.expect("actual successor preparation fence");
        released.expect("release owned interruption");
        let deadline = Instant::now() + Duration::from_secs(5);
        let terminal = loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("terminal event bound");
            match supervisor.recv_event(remaining) {
                Some(event @ (SupervisorEvent::Stopped | SupervisorEvent::Failed { .. })) => {
                    break event;
                }
                Some(SupervisorEvent::Started { attempt: 2, .. }) => {
                    panic!("cancelled G2 spawned")
                }
                Some(_) => {}
                None => panic!("missing owned preparation terminal"),
            }
        };
        let outcome = supervisor.wait_for_outcome();
        assert!(supervisor.current_pid().is_none());
        assert_eq!(supervisor.crash_ledger().self_termination_count, 1);
        match attribution {
            PreparationAttribution::Accepted | PreparationAttribution::Requested => {
                assert!(matches!(terminal, SupervisorEvent::Stopped));
                assert!(matches!(outcome, SupervisorOutcome::Stopped));
            }
            PreparationAttribution::Unattributed => {
                assert!(matches!(terminal, SupervisorEvent::Failed { attempt: 2 }));
                assert!(matches!(outcome, SupervisorOutcome::Failed(_)));
            }
        }
    }
    let marker = interrupted_successor_preparation(2);
    assert!(is_owned_preparation_interruption(&marker, 2));
    assert!(!is_owned_preparation_interruption(&marker, 1));
    assert!(!is_owned_preparation_interruption(&marker, 3));
}
