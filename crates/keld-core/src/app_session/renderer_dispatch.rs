//! One bounded original renderer inbox and one retained active source.
//! The detached owner can be prepared before the native loop is attached.

mod registration;
#[cfg(test)]
mod tests;

use super::{HostAppError, PrimaryRouterHandle, app_detail, app_io};
use keld_wv::WvError;
use keld_wv::renderer_bridge::{
    RendererBridgeEndpoint, RendererBridgeOutcome, RendererBridgeRequest,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

type Wake = dyn Fn() -> Result<(), WvError> + Send + Sync;

pub(super) struct RendererDispatch {
    input: Arc<Mutex<RendererInput>>,
    outcomes: Arc<RendererOutcomes>,
    worker: Option<JoinHandle<()>>,
    terminal: Arc<OnceLock<Result<(), HostAppError>>>,
    #[cfg(test)]
    after_failure: Option<Receiver<()>>,
}

struct RendererInput {
    requests: Receiver<RendererBridgeRequest>,
    active: Option<RendererBridgeRequest>,
}

pub(super) struct RendererOutcomes {
    sender: Mutex<Option<SyncSender<RendererBridgeOutcome>>>,
    wake: OnceLock<Box<Wake>>,
}

impl RendererOutcomes {
    /// One capacity marker represents all actual releases before its next
    /// dequeue. Full means the same exact UI owner already has a retry marker.
    pub(super) fn capacity(&self) -> Result<(), HostAppError> {
        let sent = {
            let sender = self
                .sender
                .lock()
                .map_err(|_| app_detail("renderer outcome", "outcome sender lock poisoned"))?;
            sender
                .as_ref()
                .map(|sender| sender.try_send(RendererBridgeOutcome::ControlCapacity))
        };
        match sent {
            Some(Ok(())) => self.wake(),
            None | Some(Err(TrySendError::Full(_))) => Ok(()),
            Some(Err(TrySendError::Disconnected(_))) => Err(app_detail(
                "renderer outcome",
                "original native receiver disconnected",
            )),
        }
    }

    fn wake(&self) -> Result<(), HostAppError> {
        self.wake
            .get()
            .ok_or_else(|| app_detail("renderer outcome", "native wake is unattached"))?()
        .map_err(|error| app_detail("renderer outcome wake", error.to_string()))
    }

    fn close(&self) -> Result<(), HostAppError> {
        drop(
            self.sender
                .lock()
                .map_err(|_| app_detail("renderer outcome", "outcome sender lock poisoned"))?
                .take(),
        );
        self.wake()
    }
}

impl RendererDispatch {
    /// Share observation of the same worker result with the event coordinator.
    pub(super) fn completion(&self) -> Arc<OnceLock<Result<(), HostAppError>>> {
        Arc::clone(&self.terminal)
    }

    /// Collect the worker after the native owner proved release and the event
    /// coordinator released its observation of this exact result.
    pub(super) fn finish_released(mut self) -> Result<(), HostAppError> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.join().map_err(|_| worker_panic())?;
        Arc::try_unwrap(self.terminal)
            .map_err(|_| app_detail("renderer dispatch", "terminal observer still owns result"))?
            .into_inner()
            .ok_or_else(|| app_detail("renderer dispatch", "worker returned without its result"))?
    }

    pub(super) fn prepare() -> (Self, RendererBridgeEndpoint) {
        let (requests, receiver) = mpsc::sync_channel(1);
        let (outcomes, outcome_receiver) = mpsc::sync_channel(1);
        (
            Self {
                input: Arc::new(Mutex::new(RendererInput {
                    requests: receiver,
                    active: None,
                })),
                outcomes: Arc::new(RendererOutcomes {
                    sender: Mutex::new(Some(outcomes)),
                    wake: OnceLock::new(),
                }),
                worker: None,
                terminal: Arc::new(OnceLock::new()),
                #[cfg(test)]
                after_failure: None,
            },
            RendererBridgeEndpoint::new(
                requests,
                outcome_receiver,
                keld_ipc::channel_table::ECHO.wire_id(),
            ),
        )
    }

    pub(super) fn start(
        &mut self,
        router: PrimaryRouterHandle,
        wake: impl Fn() -> Result<(), WvError> + Send + Sync + 'static,
    ) -> Result<(), HostAppError> {
        let compat = router.renderer_compat_session()?.ok_or_else(|| {
            app_detail(
                "renderer dispatch",
                "original compatibility session is unavailable",
            )
        })?;
        self.outcomes.wake.set(Box::new(wake)).map_err(|_| {
            app_detail(
                "renderer dispatch",
                "native outcome wake is already attached",
            )
        })?;
        compat.attach_renderer_outcomes(&self.outcomes)?;
        let input = Arc::clone(&self.input);
        let outcomes = Arc::clone(&self.outcomes);
        let terminal = Arc::clone(&self.terminal);
        #[cfg(test)]
        let after_failure = self.after_failure.take();
        self.worker = Some(
            thread::Builder::new()
                .name(String::from("keld-core-renderer-dispatch"))
                .spawn(move || {
                    let result = catch_unwind(AssertUnwindSafe(|| run(&input, &outcomes, &router)))
                        .unwrap_or_else(|_| Err(worker_panic()));
                    let failed = result.is_err();
                    // Publish failure before its Core progress wake. The inbox
                    // guard is gone even when the original input remains.
                    let _ = terminal.set(result);
                    if failed {
                        compat.wake_renderer_failure();
                        #[cfg(test)]
                        if let Some(after_failure) = after_failure {
                            let _ = after_failure.recv();
                        }
                    }
                })
                .map_err(|error| app_io("renderer dispatch thread", &error))?,
        );
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::app_session) fn hold_after_failure_for_test(&mut self) -> mpsc::Sender<()> {
        let (resume, held) = mpsc::channel();
        self.after_failure = Some(held);
        resume
    }

    /// A successful terminal observation means every producer disconnected,
    /// the actual inbox drained, and the sole outcome sender closed.
    /// On fault the source and the original failure remain in this owner.
    pub(super) fn poll_finished(&mut self) -> Result<bool, &HostAppError> {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
            && let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            self.terminal.get_or_init(|| Err(worker_panic()));
        }
        match self.terminal.get() {
            Some(Ok(())) => Ok(true),
            Some(Err(error)) => Err(error),
            None => Ok(false),
        }
    }
}

fn worker_panic() -> HostAppError {
    app_detail("renderer dispatch", "original worker panicked")
}

fn run(
    source: &Mutex<RendererInput>,
    outcomes: &RendererOutcomes,
    router: &PrimaryRouterHandle,
) -> Result<(), HostAppError> {
    let mut input = source
        .lock()
        .map_err(|_| app_detail("renderer dispatch", "original input lock poisoned"))?;
    while let Ok(request) = input.requests.recv() {
        input.active = Some(request);
        outcomes.capacity()?; // the original receiver physically dequeued a record
        let request = input
            .active
            .as_ref()
            .ok_or_else(|| app_detail("renderer dispatch", "dequeued request is unavailable"))?;
        router.observe_registered_renderer(request)?;
        drop(input.active.take()); // only after disposition on this same source
    }
    drop(input);
    outcomes.close()
}
