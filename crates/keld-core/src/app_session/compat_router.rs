//! GH540 compatibility storage for the existing primary application session.
//! Native CONTROL is physical custody only. The Runtime and Window Registry
//! keep the authority to admit the operation that holds it.

mod native_control;

use super::primary_writer::PrimaryWriter;
use super::renderer_dispatch::RendererOutcomes;
use super::{HostAppError, PrimaryRouterHandle, app_detail};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

pub(super) struct CompatSession {
    ledger: Mutex<native_control::CompatLedger>,
    writer: PrimaryWriter,
    backing_released: AtomicBool,
    renderer_outcomes: OnceLock<Weak<RendererOutcomes>>,
}

impl CompatSession {
    pub(super) fn new(writer: PrimaryWriter) -> Self {
        Self {
            ledger: Mutex::new(native_control::CompatLedger::default()),
            writer,
            backing_released: AtomicBool::new(false),
            renderer_outcomes: OnceLock::new(),
        }
    }

    pub(super) fn attach_renderer_outcomes(
        &self,
        owner: &Arc<RendererOutcomes>,
    ) -> Result<(), HostAppError> {
        self.renderer_outcomes
            .set(Arc::downgrade(owner))
            .map_err(|_| {
                app_detail(
                    "renderer outcome",
                    "the session already owns its original outcome sender",
                )
            })
    }

    /// Wake the existing Core progress owner after the dispatcher retains its
    /// exact failure. This wake has no terminal, capacity or recovery authority.
    pub(super) fn wake_renderer_failure(&self) {
        let _ = self.writer.completed_source_progress();
    }

    fn forward_native_capacity(&self) -> Result<(), HostAppError> {
        if let Some(owner) = self.renderer_outcomes.get().and_then(Weak::upgrade)
            && self.backing_released.swap(false, Ordering::AcqRel)
        {
            owner.capacity()?;
        }
        Ok(())
    }
}

impl PrimaryRouterHandle {
    pub(super) fn renderer_compat_session(
        &self,
    ) -> Result<Option<Arc<CompatSession>>, HostAppError> {
        let Some(windows) = &self.windows else {
            return Ok(None);
        };
        Ok(windows
            .lock()
            .map_err(|_| {
                app_detail(
                    "renderer compatibility session",
                    "Window owner lock poisoned",
                )
            })?
            .compat()
            .cloned())
    }

    pub(super) fn service_renderer_native_capacity(&self) -> Result<(), HostAppError> {
        self.renderer_compat_session()?
            .map_or(Ok(()), |compat| compat.forward_native_capacity())
    }
}
