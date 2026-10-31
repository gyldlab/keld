use std::fs::File;
use std::io::{self, Read, Write as _};
use std::os::fd::AsFd;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread::JoinHandle;

use nix::fcntl::{FcntlArg, OFlag, fcntl};

use super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, RestartPolicy, RuntimeError, SUPERVISED_QUIT_ACCEPTED,
    SUPERVISED_QUIT_ACK, SUPERVISED_QUIT_ACK_DEADLINE, Supervisor, SupervisorOutcome,
    await_guarded_primary_liveness, nix_io_error, set_liveness_nonblocking,
    validate_liveness_bootstrap,
};
use super::preparation_fixture::GatedUnmarkedFailurePreparer;
use super::test_support::liveness_pipe;

struct TerminalLivenessReadProbe<'a> {
    reader: File,
    supervisor: &'a Supervisor,
    observed: mpsc::SyncSender<(bool, u32)>,
    terminal_reads: u32,
    reported: bool,
}

impl Read for TerminalLivenessReadProbe<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let flags = fcntl(self.reader.as_fd(), FcntlArg::F_GETFL).map_err(nix_io_error)?;
        let nonblocking = OFlag::from_bits_truncate(flags).contains(OFlag::O_NONBLOCK);
        if self
            .supervisor
            .thread
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            self.terminal_reads = self.terminal_reads.saturating_add(1);
        }
        if !self.reported && (!nonblocking || self.terminal_reads >= 64) {
            let _ = self.observed.send((nonblocking, self.terminal_reads));
            self.reported = true;
        }
        self.reader.read(bytes)
    }
}

impl AsFd for TerminalLivenessReadProbe<'_> {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.reader.as_fd()
    }
}

fn run_terminal_liveness_probe(
    mut reader: File,
    mut acknowledgment: std::os::unix::net::UnixStream,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    observed: mpsc::SyncSender<(bool, u32)>,
    initialized: &mpsc::SyncSender<()>,
) -> (Result<(), RuntimeError>, bool, bool, SupervisorOutcome, u32) {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 1"]);
    let supervisor = Supervisor::start_prepared(
        RestartPolicy::default(),
        GatedUnmarkedFailurePreparer {
            initial: Some(command),
            entered_tx: entered,
            release_rx: release,
            phase: "unused marked phase",
            kind: io::ErrorKind::Interrupted,
            owned: true,
        },
    )
    .expect("real controlled Supervisor child");
    validate_liveness_bootstrap(&mut reader).expect("actual live owned pipe");
    set_liveness_nonblocking(&reader, true).expect("existing active-loop descriptor mode");
    initialized
        .send(())
        .expect("descriptor validation precedes actual Q");
    let mut probe = TerminalLivenessReadProbe {
        reader,
        supervisor: &supervisor,
        observed,
        terminal_reads: 0,
        reported: false,
    };
    let (result, accepted, _orderly, dead) = await_guarded_primary_liveness(
        &mut probe,
        &supervisor,
        &mut acknowledgment,
        &AtomicBool::new(false),
        &AtomicBool::new(false),
    );
    drop(probe);
    let outcome = supervisor.wait_for_outcome();
    let terminations = supervisor.crash_ledger().self_termination_count;
    (result, accepted, dead, outcome, terminations)
}

#[test]
fn attributed_stopped_liveness_wait_blocks_on_owned_descriptor() {
    let (reader, mut writer) = liveness_pipe();
    let (acknowledgment, mut ack_reader) =
        std::os::unix::net::UnixStream::pair().expect("actual Q ACK pair");
    ack_reader
        .set_read_timeout(Some(SUPERVISED_QUIT_ACK_DEADLINE))
        .expect("existing acknowledgment kill switch");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (observed_tx, observed_rx) = mpsc::sync_channel(1);
    let (initialized_tx, initialized_rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        run_terminal_liveness_probe(
            reader,
            acknowledgment,
            entered_tx,
            release_rx,
            observed_tx,
            &initialized_tx,
        )
    });
    let initialized = initialized_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE);
    let entered = entered_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE);
    let write = if initialized.is_ok() && entered.is_ok() {
        writer.write_all(&[SUPERVISED_QUIT_ACCEPTED])
    } else {
        Err(io::Error::other(
            "controlled preparation did not reach attempt2",
        ))
    };
    let mut ack = [0_u8; 3];
    let acknowledged = if write.is_ok() {
        ack_reader.read_exact(&mut ack)
    } else {
        Err(io::Error::other("Q was not written"))
    };
    // Release only after the actual ACK cut; always release during failed
    // collection so no assertion strands the owned preparer or pipe read.
    let released = release_tx.send(());
    drop(release_tx);
    let observation = if acknowledged.is_ok() {
        observed_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
    } else {
        Err(mpsc::RecvTimeoutError::Disconnected)
    };
    drop(writer);
    let joined = worker.join();
    println!(
        "KEL143_TERMINAL_WAIT acknowledged={acknowledged:?} ack={ack:?} observation={observation:?}"
    );
    initialized.expect("actual descriptor validation/mode setup");
    entered.expect("actual retired G1/attempt2 fence");
    write.expect("actual Q byte");
    acknowledged.expect("actual Q ACK bytes");
    released.expect("owned interruption release");
    let (nonblocking, terminal_reads) = observation.expect("bounded terminal wait witness");
    let (result, accepted, dead, outcome, terminations) = joined.expect("all owners joined");
    result.expect("actual host EOF completes liveness wait");
    assert_eq!(ack, SUPERVISED_QUIT_ACK);
    assert!(accepted && dead);
    assert!(matches!(outcome, SupervisorOutcome::Stopped));
    assert_eq!(terminations, 1, "retained actual G1 exit fact");
    assert!(
        !nonblocking,
        "terminal loop remained nonblocking after sender exit; reads={terminal_reads}"
    );
    assert!(
        terminal_reads < 64,
        "terminal disconnected-event loop repeated reads"
    );
}
