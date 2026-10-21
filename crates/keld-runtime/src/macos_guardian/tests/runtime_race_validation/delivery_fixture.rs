//! Process fixture and unwind-safe owner for four native Prepared2 delivery cuts.
use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

use super::control_plane::{ControlPlane, await_initial_registration};
use super::records_and_cut_types::{
    COLLECTION, DeliveryCut, PhaseGate, RelayCommand, RelayReceipt, ReturnedLeasePreparer,
    timed_pair,
};
use super::{
    GenerationControlPeer, GenerationControlReader, GenerationControlRecord,
    GuardedPrimaryPreparer, RestartPolicy, SUPERVISED_QUIT_ACCEPTED, Supervisor, liveness_pipe,
    lock_generation, lock_recovery, observe_guarded_liveness, parse_app_link,
    set_liveness_nonblocking, validate_liveness_bootstrap,
};

pub(super) struct CutFixture {
    pub(super) supervisor: Option<Supervisor>,
    pub(super) plane: ControlPlane,
    pub(super) phase_entered: Receiver<()>,
    pub(super) phase_release: Sender<()>,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) no_effect: tempfile::TempDir,
    pub(super) initial_pid: u32,
}

impl CutFixture {
    pub(super) fn start(cut: DeliveryCut) -> Self {
        let mut plane = ControlPlane::start(cut);
        let guardian_control = plane
            .guardian_control
            .take()
            .expect("one guardian private stream");
        let (phase_tx, phase_entered) = mpsc::channel();
        let (phase_release, phase_rx) = mpsc::channel();
        let mut factory_gate = None;
        let mut lease_gate = None;
        match cut {
            DeliveryCut::FullBeforeLease => {
                factory_gate = Some(PhaseGate {
                    entered: phase_tx,
                    release: phase_rx,
                });
            }
            DeliveryCut::LeaseBeforeSpawn => {
                lease_gate = Some(PhaseGate {
                    entered: phase_tx,
                    release: phase_rx,
                });
            }
            DeliveryCut::Zero | DeliveryCut::Partial => {}
        }
        let no_effect = tempfile::tempdir().expect("owned successor no-effect sentinel");
        let successor_effect = no_effect.path().join("unexpected-g2-effect");
        let initial_release = no_effect.path().join("g1-release");
        let initial_release_for_child = initial_release.clone();
        let mut calls = 0;
        let command_factory = move |_app_link: &str| {
            calls += 1;
            let mut command = Command::new("/bin/sh");
            if calls == 1 {
                command.args([
                    "-c",
                    "printf KEL143_RACE_G1; while [ ! -e \"$1\" ]; do :; done; exit 1",
                    "keld-race",
                ]);
                command.arg(&initial_release_for_child);
            } else {
                if let Some(gate) = &factory_gate {
                    gate.wait()?;
                }
                command.args([
                    "-c",
                    "printf KEL143_RACE_BAD_G2; printf bad > \"$1\"; exit 0",
                    "keld-race",
                ]);
                command.arg(&successor_effect);
            }
            Ok(command)
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let supervisor = Supervisor::start_prepared(
            RestartPolicy::default(),
            ReturnedLeasePreparer {
                inner: GuardedPrimaryPreparer {
                    command_factory,
                    control: Arc::new(Mutex::new(GenerationControlPeer {
                        stream: guardian_control,
                        reader: GenerationControlReader::new(),
                    })),
                    host_unavailable: Arc::new(AtomicBool::new(false)),
                    preparation_cancelled: Arc::clone(&cancelled),
                    initial_prepared: Arc::new(AtomicBool::new(false)),
                },
                gate: lease_gate,
            },
        )
        .expect("real initial Supervisor child with actual guarded preparation");
        // Startup spawn precedes group enrollment. The production
        // Started receipt follows actual child_spawned/host enrollment.
        let initial_pid = await_initial_registration(&supervisor, &plane.current);
        std::fs::write(initial_release, b"release-g1").expect("release actual registered G1");
        Self {
            supervisor: Some(supervisor),
            plane,
            phase_entered,
            phase_release,
            cancelled,
            no_effect,
            initial_pid,
        }
    }

    pub(super) fn release_after_clear(&self) -> GenerationControlRecord {
        assert!(matches!(
            self.plane
                .receipts
                .recv_timeout(COLLECTION)
                .expect("actual Clear1"),
            RelayReceipt::ClearedOne
        ));
        assert!(lock_generation(&self.plane.current).is_none());
        lock_recovery(&self.plane.recovery).arm();
        match self
            .plane
            .receipts
            .recv_timeout(COLLECTION)
            .expect("actual host Prepared2 emission")
        {
            RelayReceipt::PreparedTwo(record) => record,
            _ => panic!("unexpected protocol receipt before Prepared2"),
        }
    }

    pub(super) fn accept_actual_q(&self) {
        lock_recovery(&self.plane.recovery).deny();
        let (mut reader, mut writer) = liveness_pipe();
        validate_liveness_bootstrap(&mut reader).expect("owned validated liveness pipe");
        set_liveness_nonblocking(&reader, true).expect("active liveness poll");
        let (mut ack_writer, mut ack_reader) = timed_pair();
        writer
            .write_all(&[SUPERVISED_QUIT_ACCEPTED])
            .expect("actual Q byte");
        let mut quit = false;
        let mut orderly = false;
        assert!(
            !observe_guarded_liveness(
                &mut reader,
                self.supervisor.as_ref().expect("Supervisor owner"),
                &mut quit,
                &mut orderly,
                &mut ack_writer,
                &self.cancelled
            )
            .expect("existing guardian Q handling")
        );
        let mut bytes = [0; 3];
        ack_reader
            .read_exact(&mut bytes)
            .expect("actual KQA receipt");
        assert_eq!(bytes, *b"KQA");
        assert!(quit && !orderly);
        lock_recovery(&self.plane.recovery).shutdown_acknowledged = true;
    }
}

pub(super) struct CutObservation {
    pub(super) terminal: crate::SupervisorEvent,
    /// Receipt time of the terminal event, after every lease revocation.
    pub(super) terminal_at: Instant,
    pub(super) started_two: bool,
    pub(super) outcome: crate::SupervisorOutcome,
    pub(super) current_pid: Option<u32>,
    pub(super) ledger: crate::CrashLedger,
    pub(super) output: crate::CapturedOutput,
    pub(super) relay: Result<(), super::RuntimeError>,
    pub(super) host: Result<(), super::RuntimeError>,
    pub(super) updates: Vec<super::GuardedPrimaryUpdate>,
    pub(super) receipts: Vec<super::records_and_cut_types::RelayReceipt>,
    pub(super) endpoint_gone: bool,
    pub(super) no_effect: bool,
    pub(super) current_group_pid: Option<u32>,
    pub(super) initial_pid: u32,
}

fn collect_terminal_event(supervisor: &Supervisor) -> (crate::SupervisorEvent, Instant, bool) {
    let deadline = Instant::now() + COLLECTION;
    let mut started_two = false;
    let terminal = loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("bounded terminal event collection");
        match supervisor.recv_event(remaining) {
            Some(crate::SupervisorEvent::Started { attempt: 2, .. }) => started_two = true,
            Some(
                event @ (crate::SupervisorEvent::Stopped | crate::SupervisorEvent::Failed { .. }),
            ) => break event,
            Some(_) => {}
            None => panic!("actual Supervisor terminal event absent"),
        }
    };
    (terminal, Instant::now(), started_two)
}

impl CutFixture {
    pub(super) fn collect(&mut self, response: &GenerationControlRecord) -> CutObservation {
        // All releases/joins precede assertions. The fixture Drop also
        // releases every fence on failed collection or an unwind.
        let supervisor = self.supervisor.as_ref().expect("Supervisor owner");
        let (terminal, terminal_at, started_two) = collect_terminal_event(supervisor);
        let outcome = supervisor.wait_for_outcome();
        let current_pid = supervisor.current_pid();
        let ledger = supervisor.crash_ledger();
        let output = supervisor.output();
        // wait_for_outcome observes completion; dropping this sole
        // Supervisor joins its actual direct child/capture owner.
        drop(self.supervisor.take());
        let relay = self
            .plane
            .relay
            .take()
            .expect("one relay")
            .join()
            .expect("relay thread joins");
        let host = self
            .plane
            .host
            .take()
            .expect("one actual host owner")
            .join()
            .expect("host thread joins");
        let updates: Vec<_> = self.plane.updates.try_iter().collect();
        let cleanup_receipts: Vec<_> = self.plane.receipts.try_iter().collect();
        let (endpoint, _) =
            parse_app_link(std::str::from_utf8(&response.payload).expect("actual app link UTF8"))
                .expect("actual minted locator");
        let endpoint_gone = UnixStream::connect(endpoint).is_err();
        let no_effect = !self.no_effect.path().join("unexpected-g2-effect").exists();
        let current_group_pid =
            lock_generation(&self.plane.current).map(|generation| generation.group_pid);
        CutObservation {
            terminal,
            terminal_at,
            started_two,
            outcome,
            current_pid,
            ledger,
            output,
            relay,
            host,
            updates,
            receipts: cleanup_receipts,
            endpoint_gone,
            no_effect,
            current_group_pid,
            initial_pid: self.initial_pid,
        }
    }
}

impl Drop for CutFixture {
    fn drop(&mut self) {
        lock_recovery(&self.plane.recovery).deny();
        self.plane.consumption_probe.take();
        if let Some(supervisor) = &self.supervisor {
            supervisor.shutdown();
        }
        self.cancelled.store(true, Ordering::Release);
        let _ = self.phase_release.send(());
        let _ = self.plane.commands.send(RelayCommand::AwaitClosure);
        drop(self.supervisor.take());
    }
}
