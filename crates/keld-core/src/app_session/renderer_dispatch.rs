//! Windows' existing single renderer dispatcher with retained actual input.
//! This does not grant native/consumer completion or replace the per-View scope.
mod failure;
mod input;
mod retained;
mod service;
#[cfg(test)]
mod tests;

use super::{
    APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, Arc, AtomicBool, HostAppError, Instant, JoinHandle,
    Mutex, Ordering, PrimaryEchoReply, PrimaryRouterHandle, Receiver, RecvTimeoutError,
    RendererBridgeOutcome, RendererBridgeRequest, Sender, app_detail, app_io, compat_router, mpsc,
    numeric_echo, thread, window_router,
};
use failure::{Completion, DispatchFailure, returned_completion};
use input::Input;
use retained::{Returned, RunFailure};

pub(super) struct RendererDispatch {
    stop: Arc<AtomicBool>,
    prepared: Arc<Mutex<Option<Input>>>,
    worker: Worker,
    drain_deadline: Option<Instant>,
}

#[expect(
    clippy::large_enum_variant,
    reason = "The same worker retains its complete current input on return or unwind; boxing would add unaccounted storage."
)]
enum Worker {
    Prepared,
    StartFailed,
    Running(JoinHandle<Returned<Input, DispatchFailure>>),
    Finished {
        input: Option<Input>,
        completion: Completion,
    },
}

impl RendererDispatch {
    /// Own the real request receiver and both existing endpoint directions before
    /// spawn can fail. The thread closure captures only aliases to this owner.
    pub(super) fn prepare(
        requests: Receiver<RendererBridgeRequest>,
        outcomes: Sender<RendererBridgeOutcome>,
        router: PrimaryRouterHandle,
    ) -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            prepared: Arc::new(Mutex::new(Some(Input::new(requests, outcomes, router)))),
            worker: Worker::Prepared,
            drain_deadline: None,
        }
    }

    pub(super) fn start(&mut self) -> Result<(), HostAppError> {
        if !matches!(self.worker, Worker::Prepared) {
            return Err(app_detail(
                "renderer dispatch start",
                "worker was already attempted",
            ));
        }
        self.worker = Worker::StartFailed;
        let source = Arc::clone(&self.prepared);
        let stop = Arc::clone(&self.stop);
        let handle = thread::Builder::new()
            .name("keld-core-renderer-dispatch".to_owned())
            .spawn(move || retained::run(&source, |input| input.run(&stop)))
            .map_err(|source| app_io("renderer dispatch thread", &source))?;
        self.worker = Worker::Running(handle);
        Ok(())
    }

    pub(super) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        if let Worker::Running(handle) = &self.worker {
            handle.thread().unpark();
        }
    }

    /// No join until the actual worker has returned. A failed worker's actual
    /// current request/result/receiver remains here even after its error is read.
    pub(super) fn poll_failure(&mut self) -> Result<(), HostAppError> {
        let finished = matches!(&self.worker, Worker::Running(handle) if handle.is_finished());
        if finished {
            let old = std::mem::replace(&mut self.worker, Worker::StartFailed);
            if let Worker::Running(handle) = old {
                self.worker = match handle.join() {
                    Ok(returned) => {
                        let (input, completion) = returned_completion(returned);
                        Worker::Finished { input, completion }
                    }
                    Err(_) => Worker::Finished {
                        input: None,
                        completion: Completion::Failed(Some(app_detail(
                            "renderer dispatch thread",
                            "worker failed outside its retained boundary",
                        ))),
                    },
                };
            }
        }
        match &mut self.worker {
            Worker::StartFailed => Err(app_detail(
                "renderer dispatch thread",
                "worker start failed",
            )),
            Worker::Finished { completion, .. } => completion.poll_failure(),
            _ => Ok(()),
        }
    }

    /// Only an accepted final Drain calls this. Poll retains an old source
    /// without turning it into today's global failure; Drain must return its
    /// original cause before waiting on protocol records that it may keep held.
    fn refuse_retained_source_on_drain(&mut self) -> Result<(), HostAppError> {
        if let Worker::Finished { completion, .. } = &mut self.worker
            && let Some(error) = completion.retained_refusal()
        {
            return Err(error);
        }
        Ok(())
    }

    /// Recheck the actual returned owner before normal Host cleanup. This is
    /// additional to the native/per-View and global Core protocol gates.
    pub(super) fn release_ready(&mut self) -> Result<bool, HostAppError> {
        match &mut self.worker {
            Worker::Finished {
                input: Some(input),
                completion: Completion::Succeeded,
            } => {
                input.check_empty()?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Called only after the exact View scopes closed future native offers and
    /// their actual bridge/callback storage drained. Uses the existing app-link
    /// I/O deadline supplied by the one outer stop owner; no clock is renewed.
    /// Empty receiver is an additional storage check, never producer proof.
    pub(super) fn finish(&mut self, deadline: Instant) -> Result<(), HostAppError> {
        self.request_stop();
        loop {
            self.poll_failure()?;
            if !matches!(self.worker, Worker::Running(_)) {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(app_detail(
                    "renderer dispatch drain",
                    "worker has not returned within the original stop deadline",
                ));
            }
            thread::park_timeout(APP_LINK_READER_POLL.min(deadline.saturating_duration_since(now)));
        }
        self.refuse_retained_source_on_drain()?;
        match &mut self.worker {
            Worker::Prepared => {
                let mut source = self.prepared.try_lock().map_err(|_| {
                    app_detail(
                        "renderer dispatch drain",
                        "prepared input cannot be inspected",
                    )
                })?;
                source
                    .as_mut()
                    .ok_or_else(|| {
                        app_detail("renderer dispatch drain", "prepared input is absent")
                    })?
                    .check_empty()
            }
            Worker::Finished {
                input,
                completion: Completion::Succeeded,
            } => input
                .as_mut()
                .ok_or_else(|| app_detail("renderer dispatch drain", "returned input is absent"))?
                .check_empty(),
            _ => Err(app_detail(
                "renderer dispatch drain",
                "actual worker is not drained",
            )),
        }
    }
}
