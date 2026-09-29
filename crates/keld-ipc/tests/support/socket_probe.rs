//! Passive socket observations for the physical AC9 fixture.
//! No bytes, errors, deadlines, retries, or stream ownership are substituted.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;

use keld_ipc::link::AppLinkDeadlines;

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub bytes: usize,
    pub eof_reads: usize,
    pub reads_after_boundary: usize,
    pub boundary_seen: bool,
}

pub enum Boundary {
    Bytes(usize),
    Eof,
}

struct Gate {
    boundary: Boundary,
    reached: mpsc::Sender<Snapshot>,
    release: mpsc::Receiver<()>,
}

#[derive(Default)]
struct State {
    armed: bool,
    snapshot: Snapshot,
    gate: Option<Gate>,
}

#[derive(Clone, Default)]
pub struct Probe(Arc<Mutex<State>>);

impl Probe {
    pub fn arm(&self, boundary: Boundary) -> (mpsc::Receiver<Snapshot>, mpsc::Sender<()>) {
        let (reached_tx, reached_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(!state.armed, "one fault observation per accepted stream");
        state.armed = true;
        state.gate = Some(Gate {
            boundary,
            reached: reached_tx,
            release: release_rx,
        });
        (reached_rx, release_tx)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .snapshot
    }

    fn read_entry(&self) {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if state.armed && state.snapshot.boundary_seen {
            state.snapshot.reads_after_boundary += 1;
        }
    }

    fn read_result(&self, result: &io::Result<usize>) {
        let paused = {
            let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            if !state.armed {
                return;
            }
            if let Ok(bytes) = result {
                state.snapshot.bytes += bytes;
                if *bytes == 0 {
                    state.snapshot.eof_reads += 1;
                }
            }
            let reached = state.gate.as_ref().is_some_and(|gate| match gate.boundary {
                Boundary::Bytes(bytes) => state.snapshot.bytes >= bytes,
                Boundary::Eof => matches!(result, Ok(0)),
            });
            if reached {
                state.snapshot.boundary_seen = true;
                Some((
                    state.gate.take().expect("one observation gate"),
                    state.snapshot,
                ))
            } else {
                None
            }
        };
        if let Some((gate, snapshot)) = paused {
            gate.reached
                .send(snapshot)
                .expect("record actual socket read");
            gate.release
                .recv_timeout(Duration::from_secs(10))
                .expect("release observed read without replacing its result");
        }
    }
}

pub struct Observed<S> {
    inner: S,
    probe: Option<Probe>,
}

impl<S> Observed<S> {
    pub fn new(inner: S, probe: Option<Probe>) -> Self {
        Self { inner, probe }
    }
}

impl<S: Read> Read for Observed<S> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(probe) = &self.probe {
            probe.read_entry();
        }
        let result = self.inner.read(buffer);
        if let Some(probe) = &self.probe {
            probe.read_result(&result);
        }
        result
    }
}

impl<S: Write> Write for Observed<S> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<S: AppLinkDeadlines> AppLinkDeadlines for Observed<S> {
    fn set_app_link_read_deadline(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.set_app_link_read_deadline(timeout)
    }

    fn set_app_link_write_deadline(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.set_app_link_write_deadline(timeout)
    }

    fn app_link_read_deadline(&self) -> io::Result<Option<Duration>> {
        self.inner.app_link_read_deadline()
    }

    fn app_link_write_deadline(&self) -> io::Result<Option<Duration>> {
        self.inner.app_link_write_deadline()
    }

    fn shutdown_app_link(&self) -> io::Result<()> {
        self.inner.shutdown_app_link()
    }
}
