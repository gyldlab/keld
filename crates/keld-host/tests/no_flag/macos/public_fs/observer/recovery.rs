//! The recovery fixture's one-generation-at-a-time HTTP command and wait owner.
use super::respond;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpStream;
use std::sync::mpsc;

#[derive(Clone, Copy, Debug)]
pub(in crate::public_fs) enum RecoveryPhase {
    Pending,
    Before,
    After,
}

impl RecoveryPhase {
    pub(in crate::public_fs) fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

pub(in crate::public_fs) enum RecoveryCommand {
    Admit(u32),
    Invoke(RecoveryPhase),
    Quit(u32),
    RetirePending(u32),
}

/// Owns only ordinary fixture HTTP waits; two app generations, one real page.
pub(super) struct RecoveryObserver {
    commands: mpsc::Receiver<RecoveryCommand>,
    admitted: BTreeSet<u32>,
    admissions: BTreeMap<u32, TcpStream>,
    page: Option<TcpStream>,
    invoke: Option<RecoveryPhase>,
    quits: BTreeMap<u32, TcpStream>,
    quitting: Option<u32>,
    pending: Option<(u32, TcpStream)>,
}

impl RecoveryObserver {
    pub(super) fn new(commands: mpsc::Receiver<RecoveryCommand>) -> Self {
        Self {
            commands,
            admitted: BTreeSet::new(),
            admissions: BTreeMap::new(),
            page: None,
            invoke: None,
            quits: BTreeMap::new(),
            quitting: None,
            pending: None,
        }
    }

    pub(super) fn poll(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                RecoveryCommand::Admit(pid) => {
                    assert!(self.admitted.insert(pid), "one admission per owned app");
                    assert!(self.admitted.len() <= 2, "exactly two recovery apps");
                }
                RecoveryCommand::Invoke(phase) => {
                    assert!(self.invoke.replace(phase).is_none(), "one page command");
                }
                RecoveryCommand::RetirePending(pid) => {
                    let (held, stream) = self.pending.take().expect("one pending handler stream");
                    assert_eq!(held, pid, "retire only the observed old handler");
                    drop(stream);
                }
                RecoveryCommand::Quit(pid) => {
                    assert!(self.quitting.replace(pid).is_none(), "one recovery Quit");
                }
            }
        }
        self.respond_ready();
    }

    pub(super) fn accept_wait(&mut self, path: &str, stream: TcpStream) -> Option<Value> {
        if let Some(identity) = path.strip_prefix("/recovery-pending/") {
            return Some(self.hold(
                identity,
                stream,
                "recovery-handler-pending",
                "application-handler-only",
            ));
        }
        if let Some(identity) = path.strip_prefix("/recovery-register/") {
            return Some(self.hold(
                identity,
                stream,
                "recovery-registration-pending",
                "before-handler-registration",
            ));
        }
        if path == "/recovery-page" {
            assert!(self.page.replace(stream).is_none(), "one real page wait");
        } else if let Some(pid) = path.strip_prefix("/recovery-admit/") {
            let pid = pid.parse::<u32>().expect("owned recovery admission PID");
            assert!(self.admissions.insert(pid, stream).is_none());
            assert!(self.admissions.len() <= 2);
        } else if let Some(pid) = path.strip_prefix("/recovery-quit/") {
            let pid = pid.parse::<u32>().expect("owned recovery Quit PID");
            assert!(self.quits.insert(pid, stream).is_none());
            assert!(self.quits.len() <= 2);
        } else {
            panic!("unexpected recovery fixture route {path}");
        }
        self.respond_ready();
        None
    }

    /// Hold one admitted app's fixture wait until `RetirePending(pid)` drops
    /// it: the handler's own body (`/recovery-pending`) or, for GH-1095
    /// AT-API2b, the gap inside `whenReady` before the handler is registered
    /// (`/recovery-register`). One held wait at a time; the report names which.
    fn hold(&mut self, identity: &str, stream: TcpStream, phase: &str, barrier: &str) -> Value {
        let (pid, nonce) = identity
            .split_once('/')
            .expect("held fixture wait identity");
        let pid = pid.parse::<u32>().expect("owned held wait PID");
        assert!(self.admitted.contains(&pid));
        assert_eq!(nonce.len(), 32);
        assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(self.pending.replace((pid, stream)).is_none());
        serde_json::json!({"phase":phase,"pid":pid,"nonce":nonce,"barrier":barrier})
    }

    fn respond_ready(&mut self) {
        for pid in &self.admitted {
            if let Some(mut stream) = self.admissions.remove(pid) {
                respond(&mut stream, "admit");
            }
        }
        if self.page.is_some() && self.invoke.is_some() {
            let mut stream = self.page.take().expect("retained real page wait");
            let phase = self.invoke.take().expect("one page command");
            respond(&mut stream, phase.as_str());
        }
        if let Some(pid) = self.quitting
            && let Some(mut stream) = self.quits.remove(&pid)
        {
            // G1's dead HTTP wait is never answered or mistaken for G2.
            respond(&mut stream, "quit");
        }
    }
}
