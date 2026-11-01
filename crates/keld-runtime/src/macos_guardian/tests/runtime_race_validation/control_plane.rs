//! Real host-generation control fixture, with one owner for relay/host thread cleanup.
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::private_record_relay::private_record_relay;
use super::records_and_cut_types::{
    COLLECTION, DeliveryCut, RelayCommand, RelayReceipt, timed_pair,
};
use super::{
    GuardedPrimaryUpdate, RecoveryState, RegisteredPrimaryGeneration, RuntimeError, Supervisor,
    lock_generation, lock_recovery, run_generation_host,
};

// Test control custody only. The actual production host owner remains
// run_generation_host; the relay never mints or chooses authority.
pub(super) struct ControlPlane {
    pub(super) host: Option<JoinHandle<Result<(), RuntimeError>>>,
    pub(super) relay: Option<JoinHandle<Result<(), RuntimeError>>>,
    pub(super) commands: Sender<RelayCommand>,
    pub(super) receipts: Receiver<RelayReceipt>,
    pub(super) recovery: Arc<Mutex<RecoveryState>>,
    pub(super) updates: Receiver<GuardedPrimaryUpdate>,
    pub(super) consumption_probe: Option<UnixStream>,
    pub(super) current: Arc<Mutex<Option<RegisteredPrimaryGeneration>>>,
    pub(super) guardian_control: Option<UnixStream>,
}

impl ControlPlane {
    pub(super) fn start(cut: DeliveryCut) -> Self {
        let (host_control, relay_host) = timed_pair();
        let (guardian_control, relay_guardian) = timed_pair();
        // This clone observes FIONREAD only, never consumes bytes.
        // It is dropped before cancellation/EOF to preserve custody.
        let consumption_probe = (cut == DeliveryCut::Partial).then(|| {
            guardian_control
                .try_clone()
                .expect("sole-consumer observation alias")
        });
        let (updates_tx, updates) = mpsc::channel();
        let current = Arc::new(Mutex::new(None));
        let recovery = Arc::new(Mutex::new(RecoveryState::new()));
        lock_recovery(&recovery).arm();
        let host_current = Arc::clone(&current);
        let host_recovery = Arc::clone(&recovery);
        let host = thread::spawn(move || {
            run_generation_host(
                super::super::super::generation_host::GenerationHostAdmission::new(
                    std::process::id(), // actual child groups originate in this test process
                    Instant::now() + super::GUARDIAN_SHUTDOWN_DEADLINE,
                ),
                host_control,
                &updates_tx,
                &host_current,
                &host_recovery,
                None,
                None,
            )
        });
        let (commands, commands_rx) = mpsc::channel();
        let (receipts_tx, receipts) = mpsc::channel();
        let relay = thread::spawn(move || {
            private_record_relay(relay_host, relay_guardian, &commands_rx, &receipts_tx)
        });
        Self {
            host: Some(host),
            relay: Some(relay),
            commands,
            receipts,
            recovery,
            updates,
            consumption_probe,
            current,
            guardian_control: Some(guardian_control),
        }
    }
}

impl Drop for ControlPlane {
    fn drop(&mut self) {
        lock_recovery(&self.recovery).deny();
        self.consumption_probe.take();
        // Disconnect the relay command channel before joining, so a relay
        // parked in `recv_timeout(COLLECTION)` returns now, not after the
        // full collection window. Already-queued commands are still received.
        drop(std::mem::replace(&mut self.commands, mpsc::channel().0));
        if let Some(stream) = self.guardian_control.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        for worker in [self.relay.take(), self.host.take()].into_iter().flatten() {
            report_rescue_join(&worker.join());
        }
    }
}

fn report_rescue_join(result: &thread::Result<Result<(), RuntimeError>>) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(_)) => eprintln!("KEL143_RACE_RESCUE owner_error"),
        Err(_) => eprintln!("KEL143_RACE_RESCUE worker_panic"),
    }
}

pub(super) fn await_initial_registration(
    supervisor: &Supervisor,
    current: &Mutex<Option<RegisteredPrimaryGeneration>>,
) -> u32 {
    let pid = match supervisor.recv_event(COLLECTION) {
        Some(crate::SupervisorEvent::Started { pid, attempt: 1 }) => pid,
        Some(event) => panic!("unexpected initial enrollment event: {event:?}"),
        None => panic!("actual initial group registration absent"),
    };
    assert_eq!(
        lock_generation(current).map(|generation| generation.group_pid),
        Some(pid)
    );
    pid
}

#[cfg(test)]
mod private_diagnostic_tests {
    use super::RuntimeError;
    use super::report_rescue_join;
    use std::io;
    use std::process::Command;

    /// The subprocess exercises the same reporting function called by Drop.
    /// Its own stderr is captured, so neither a sentinel nor a private value
    /// needs to be printed by a failing parent regression.
    #[test]
    fn rescue_worker_stderr_uses_only_fixed_categories() {
        const PRIVATE: &str = "KELD_SYNTHETIC_PRIVATE_RESCUE_ERROR_SENTINEL";
        if std::env::var_os("KELD_PRIVATE_RESCUE_CHILD").is_some() {
            let owned = RuntimeError::Lifecycle {
                phase: "test-owned rescue failure",
                source: io::Error::other(PRIVATE),
            };
            report_rescue_join(&Ok(Err(owned)));
            report_rescue_join(&Err(Box::new(PRIVATE.to_owned())));
            return;
        }
        let output = Command::new(std::env::current_exe().expect("test runner executable"))
            .args([
                "rescue_worker_stderr_uses_only_fixed_categories",
                "--nocapture",
            ])
            .env("KELD_PRIVATE_RESCUE_CHILD", "1")
            .output()
            .expect("run the owned diagnostic child");
        assert!(output.status.success(), "diagnostic child completed");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains(PRIVATE),
            "the rescue diagnostic leaked private error data"
        );
        assert!(
            stderr.contains("KEL143_RACE_RESCUE owner_error")
                && stderr.contains("KEL143_RACE_RESCUE worker_panic"),
            "both terminal results need fixed diagnostic categories"
        );
    }
}
