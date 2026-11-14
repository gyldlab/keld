//! Debug-only source gate and independent native selector census owners.
use super::WkUserEvent;
use super::macos_resource_source::ResourceSourceError;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::os::fd::AsRawFd as _;
use std::os::unix::fs::MetadataExt as _;
use std::sync::mpsc;
use std::time::Duration;
use tao::event_loop::EventLoopProxy;

pub(super) struct HeldRead {
    pub(super) read_fd: i32,
    pub(super) root_fd: i32,
    pub(super) inode: u64,
}

pub(super) enum LifecycleEvent {
    Held(HeldRead),
    Abort,
    Release,
    TaskStopped(usize),
    ObserveFanout,
    FanoutSnapshot(String),
}

pub(super) struct ReadGate {
    pub(super) path: String,
    wake: EventLoopProxy<WkUserEvent>,
    release: mpsc::Receiver<()>,
}

impl ReadGate {
    pub(super) fn new(
        path: String,
        wake: EventLoopProxy<WkUserEvent>,
        release: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            path,
            wake,
            release,
        }
    }

    pub(super) fn hold(self, file: &File, root_fd: i32) -> Result<(), ResourceSourceError> {
        let metadata = file.metadata().map_err(|_| ResourceSourceError::Io)?;
        self.wake
            .send_event(WkUserEvent::ResourceLifecycle(LifecycleEvent::Held(
                HeldRead {
                    read_fd: file.as_raw_fd(),
                    root_fd,
                    inode: metadata.ino(),
                },
            )))
            .map_err(|_| ResourceSourceError::Io)?;
        // This is the actual already-open verified-read worker. No UI object,
        // descriptor clone, synthetic bytes or replacement reader is involved.
        self.release
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| ResourceSourceError::Io)
    }
}

pub(super) struct CallbackCensus {
    wake: EventLoopProxy<WkUserEvent>,
    stopped: RefCell<BTreeSet<usize>>,
    callbacks: RefCell<BTreeMap<usize, usize>>,
    after_stop: RefCell<usize>,
}

impl CallbackCensus {
    pub(super) fn new(wake: EventLoopProxy<WkUserEvent>) -> Self {
        Self {
            wake,
            stopped: RefCell::new(BTreeSet::new()),
            callbacks: RefCell::new(BTreeMap::new()),
            after_stop: RefCell::new(0),
        }
    }

    pub(super) fn after_stop(&self) -> usize {
        *self.after_stop.borrow()
    }

    pub(super) fn callback(&self, native_task: usize) {
        *self.callbacks.borrow_mut().entry(native_task).or_default() += 1;
        if self.stopped.borrow().contains(&native_task) {
            *self.after_stop.borrow_mut() += 1;
        }
    }

    pub(super) fn stopped(&self, native_task: usize) {
        self.stopped.borrow_mut().insert(native_task);
        let _ = self
            .wake
            .send_event(WkUserEvent::ResourceLifecycle(LifecycleEvent::TaskStopped(
                native_task,
            )));
    }

    pub(super) fn total(&self) -> usize {
        self.callbacks.borrow().values().sum()
    }
}
