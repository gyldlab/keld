//! Test-owned gated successor failure fixture shared by Guardian scenarios.
use std::io;
use std::process::Command;
use std::sync::mpsc;

use super::super::{ChildPreparer, PreparedChild, RuntimeError, interrupted_successor_preparation};

pub(super) struct GatedUnmarkedFailurePreparer {
    pub(super) initial: Option<Command>,
    pub(super) entered_tx: mpsc::Sender<()>,
    pub(super) release_rx: mpsc::Receiver<()>,
    pub(super) phase: &'static str,
    pub(super) kind: io::ErrorKind,
    pub(super) owned: bool,
}

impl ChildPreparer for GatedUnmarkedFailurePreparer {
    type Lease = crate::NoLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        if attempt == 1 {
            return Ok(PreparedChild {
                command: self.initial.take().expect("one initial command").into(),
                lease: crate::NoLease,
            });
        }
        assert_eq!(attempt, 2, "the failed preparation must not be retried");
        self.entered_tx.send(()).expect("observe successor prepare");
        self.release_rx
            .recv()
            .expect("controlled preparation release");
        if self.owned {
            return Err(interrupted_successor_preparation(attempt));
        }
        Err(RuntimeError::Lifecycle {
            phase: self.phase,
            source: io::Error::new(self.kind, "controlled successor preparation failure"),
        })
    }
}
