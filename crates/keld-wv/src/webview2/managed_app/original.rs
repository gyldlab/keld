//! One Host resource session, retained by the existing managed/run owner.
#![forbid(unsafe_code)]

use super::super::original_resource::ResourceSession;
use super::super::original_view::OriginalBuildInputs;
use super::super::view_build::{BuildProgress, OriginalBuild};
use super::*;
use crate::WebviewId;
use crate::renderer_bridge::{RendererBridgeEndpoint, RendererBridgeOutcome};

impl ManagedWindows {
    pub(super) fn prepare_original_build(
        &mut self,
        inputs: OriginalBuildInputs,
    ) -> Result<OriginalBuild, WvError> {
        let endpoint = self.renderer_endpoint.as_ref().ok_or_else(|| {
            WvError::EventLoop(String::from(
                "original build lacks its actual Host renderer endpoint",
            ))
        })?;
        if endpoint.outcomes.is_some() {
            return Err(WvError::EventLoop(String::from(
                "renderer outcome receiver must remain in the existing Host wake worker",
            )));
        }
        let endpoint = RendererBridgeEndpoint::shared_view(
            endpoint.requests.clone(),
            endpoint.admitted_channel,
        );
        if self.original_session.is_none() {
            // Install this singleton before any ResourceView can enter native work.
            self.original_session = Some(ResourceSession::new(
                std::sync::Arc::clone(&inputs.source),
                self.proxy.clone(),
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.test_control
                    .as_ref()
                    .and_then(TestControl::original_resource_result_directory),
            )?);
        }
        let session = self.original_session.as_ref().ok_or_else(|| {
            WvError::EventLoop(String::from("retained Host resource session disappeared"))
        })?;
        Ok(OriginalBuild {
            inputs,
            session: Rc::clone(session),
            endpoint,
            events: self.events.clone(),
        })
    }

    pub(super) fn destroy_view(&mut self, view: WebviewId) -> Result<(), WvError> {
        let Some(native) = self.views.get(&view.0) else {
            eprintln!("KELD_WV_STALE_DESTROY view={}", view.0);
            return Ok(());
        };
        self.custody.park(
            native.window.id(),
            native.window.hwnd(),
            Releasing::Destroy(view),
            Instant::now(),
        )?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if self
            .test_control
            .as_ref()
            .is_some_and(TestControl::take_destroyed_prefill)
        {
            self.custody.arm_destroyed_prefill(native.window.id())?;
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        observe::releasing(self.custody.len());
        if native.retire_on_ui() {
            self.views.remove(&view.0);
        }
        Ok(())
    }

    /// Called every existing loop turn. A wake's old view may be absent; the
    /// Host session and original build candidate still receive actual service.
    pub(in crate::webview2) fn drive_original(
        &mut self,
        release: &Rc<ProfileReleaseWait>,
        ending: bool,
    ) -> Result<(), WvError> {
        if let Some(session) = &self.original_session {
            if ending {
                session.retire_on_ui()?;
            } else {
                session.drain_ui()?;
            }
        }
        if !ending && let Some((build, _)) = self.builder.pending_original() {
            match self.builder.resume_original(release) {
                Ok(BuildProgress::PendingOriginal) => {}
                Ok(BuildProgress::Complete(completed)) => self.complete_build(build, completed)?,
                Err(failure) => self.failed_build(build, failure)?,
            }
        }
        for native in self.views.values() {
            if let Some(original) = &native.original {
                original.drive_on_ui()?;
            }
        }
        self.views.retain(|_, native| {
            let retiring = ending
                || native
                    .original
                    .as_ref()
                    .is_some_and(super::super::original_view::OriginalView::is_retired);
            !retiring || !native.retire_on_ui()
        });
        if let Some(session) = &self.original_session {
            session.drain_ui()?;
        }
        Ok(())
    }

    pub(super) fn original_deadline(&self) -> Option<Instant> {
        self.builder
            .pending_original()
            .map(|(_, deadline)| deadline)
            .into_iter()
            .chain(self.views.values().filter_map(|view| {
                view.original
                    .as_ref()
                    .and_then(super::super::original_view::OriginalView::next_deadline)
            }))
            .min()
    }

    /// The exact currently-dispatched outcome stays in `AppRunCustody` until an
    /// already-attached bridge takes it. Unknown destination is never a Drop.
    pub(in crate::webview2) fn deliver_renderer_outcome(
        &mut self,
        pending: &mut Option<RendererBridgeOutcome>,
    ) -> Result<bool, WvError> {
        let Some(outcome) = pending.as_ref() else {
            return Ok(true);
        };
        if matches!(outcome, RendererBridgeOutcome::ControlCapacity) {
            for view in self.views.values() {
                if let Some(original) = &view.original {
                    original.drive_on_ui()?;
                }
            }
            let _ = pending.take();
            return Ok(true);
        }
        let Some(view) = outcome.webview() else {
            return Ok(false); // Keep the same current delivery when no target exists.
        };
        if let Some(original) = self
            .views
            .get(&view.0)
            .and_then(|view| view.original.as_ref())
        {
            return original.deliver_on_ui(pending);
        }
        Ok(false)
    }

    /// Called through the same managed `native_turn` after the loop, before any
    /// profile credit. A bool snapshot cannot hide a `ResourceSession` error.
    pub(in crate::webview2) fn finish_original_resources(&self) -> Result<(), WvError> {
        if let Some(session) = &self.original_session {
            session.retire_on_ui()?;
            session.drain_ui()?;
            if !session.is_drained() {
                return Err(WvError::EventLoop(String::from(
                    "Host original resource session has not physically drained",
                )));
            }
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            session.observe_resources("session-drained", None);
        }
        Ok(())
    }
}
