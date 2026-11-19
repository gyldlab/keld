//! The existing GUI owner retains selected builds, the source session and the actual UI endpoint.
#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};

use super::super::managed_windows::RELEASING_DESTROYED_BOUND;
use super::super::original_resource::ResourceSession;
use super::super::original_view::ViewInputs;
use super::super::view_build::original::OriginalBuild;
use super::super::{PROFILE_LAUNCH_DEADLINE, SelectedNavigationObserver};
use super::{Instant, ManagedWindows, ProfileReleaseWait, Rc, WindowsLoopEvent, WvError};
use crate::ApplicationNameForUserAgent;
use crate::preload_image::AppPreloadImage;
use crate::renderer_bridge::{
    RendererBridgeEndpoint, RendererBridgeOutcome, RendererBridgeRequest, RendererRoleWitness,
};
use crate::resource_source::AdmittedWebAppSource;

pub(super) struct SelectedCreate {
    pub(super) source: Arc<AdmittedWebAppSource>,
    pub(super) image: Arc<AppPreloadImage>,
    pub(super) role: RendererRoleWitness,
    pub(super) start_by: Instant,
    pub(super) application_name_for_user_agent: Option<ApplicationNameForUserAgent>,
}

pub(in crate::webview2) struct SelectedRenderer {
    requests: Option<SyncSender<RendererBridgeRequest>>,
    outcomes: Receiver<RendererBridgeOutcome>,
    admitted_channel: u16,
    observe: Rc<SelectedNavigationObserver>,
    session: Option<Rc<ResourceSession>>,
    pending: Option<RendererBridgeOutcome>,
    outcomes_closed: bool,
    retire_by: Option<Instant>,
}

impl SelectedRenderer {
    pub(in crate::webview2) fn new(
        endpoint: RendererBridgeEndpoint,
        observe: Rc<SelectedNavigationObserver>,
    ) -> Option<Self> {
        let RendererBridgeEndpoint {
            requests,
            outcomes,
            admitted_channel,
        } = endpoint;
        Some(Self {
            requests: Some(requests),
            outcomes: outcomes?,
            admitted_channel,
            observe,
            session: None,
            pending: None,
            outcomes_closed: false,
            retire_by: None,
        })
    }

    fn retains_resources(&self) -> bool {
        self.requests.is_some()
            || !self.outcomes_closed
            || self.pending.is_some()
            || self
                .session
                .as_ref()
                .is_some_and(|session| !session.is_drained())
    }

    /// Attach the exact received value before classification or a native turn.
    /// The Core worker owns the sender and posts the existing unit wake.
    fn receive_outcome(&mut self) -> Result<(), WvError> {
        if self.pending.is_some() || self.outcomes_closed {
            return Ok(());
        }
        match self.outcomes.try_recv() {
            Ok(outcome) => self.pending = Some(outcome),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.outcomes_closed = true;
                if self.retire_by.is_none() {
                    return Err(failure(
                        "selected outcome producer disconnected before retirement",
                    ));
                }
            }
        }
        Ok(())
    }
}

impl ManagedWindows {
    pub(super) fn prepare_original_build(
        &mut self,
        inputs: SelectedCreate,
        now: Instant,
    ) -> Result<OriginalBuild, WvError> {
        if now >= inputs.start_by || !inputs.role.is_live() || !inputs.role.has_native_view_scope()
        {
            return Err(failure(
                "selected create lacks a live admitted creator before its cutoff",
            ));
        }
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| failure("selected create has no Host endpoint"))?;
        if renderer.retire_by.is_some() || renderer.outcomes_closed {
            return Err(failure(
                "selected create cannot use a retiring Host endpoint",
            ));
        }
        let requests = renderer
            .requests
            .as_ref()
            .ok_or_else(|| failure("selected request producer already retired"))?;
        let endpoint =
            RendererBridgeEndpoint::shared_view(requests.clone(), renderer.admitted_channel);
        if renderer.session.is_none() {
            renderer.session = Some(ResourceSession::new(
                Arc::clone(&inputs.source),
                self.proxy.clone(),
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                None,
            )?);
        }
        let session = Rc::clone(
            renderer
                .session
                .as_ref()
                .ok_or_else(|| failure("selected resource session disappeared"))?,
        );
        let observe = Rc::clone(&renderer.observe);
        let observed_session = Rc::clone(&session);
        let proxy = self.proxy.clone();
        Ok(OriginalBuild {
            inputs: ViewInputs {
                source: inputs.source,
                image: inputs.image,
                creator: inputs.role,
                session,
                renderer_lifetime: None,
                endpoint,
            },
            observer: Rc::new(move |context, fact| {
                if let Some(context) = context
                    && observe(context, fact).is_err()
                {
                    observed_session
                        .latch_failure("selected navigation fact receiver ended", context.view());
                }
                if proxy.send_event(WindowsLoopEvent::OriginalWake).is_err()
                    && let Some(context) = context
                {
                    observed_session.latch_failure("selected resource wake failed", context.view());
                }
            }),
            deadline: now + PROFILE_LAUNCH_DEADLINE,
            application_name_for_user_agent: inputs.application_name_for_user_agent,
        })
    }

    pub(super) fn stop_selected(&mut self) -> Result<(), WvError> {
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        let _ = renderer
            .retire_by
            .get_or_insert_with(|| Instant::now() + RELEASING_DESTROYED_BOUND);
        if let Some(session) = &renderer.session {
            session.retire_on_ui()?;
        }
        Ok(())
    }

    pub(in crate::webview2) fn retains_native_app_run(&self) -> bool {
        self.has_build_candidate()
            || self.views.values().any(|view| view.original.is_some())
            || self
                .renderer
                .as_ref()
                .is_some_and(SelectedRenderer::retains_resources)
    }

    pub(in crate::webview2) fn drive_original(
        &mut self,
        release: &ProfileReleaseWait,
        ending: bool,
    ) -> Result<(), WvError> {
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        renderer.receive_outcome()?;
        if let Some(outcome) = renderer.pending.as_ref() {
            if !matches!(outcome, RendererBridgeOutcome::ControlCapacity) {
                return Err(failure(
                    "selected renderer outcome has no attached native delivery owner",
                ));
            }
            for native in self.views.values() {
                if let Some(view) = &native.original {
                    view.observe_renderer_capacity(outcome)?;
                }
            }
            // Capacity is a unit observation; no charged outcome is released.
            let _ = renderer.pending.take();
            self.proxy
                .send_event(WindowsLoopEvent::OriginalWake)
                .map_err(|_| failure("selected outcome service wake failed"))?;
        }
        if let Some(session) = &renderer.session {
            session.drain_ui()?;
        }
        if !ending && let Some((build, _)) = self.builder.pending_original() {
            match self
                .builder
                .resume_original_preload(build, Instant::now(), release)
            {
                Ok(Some(completed)) => self.complete_build(build, completed)?,
                Ok(None) => {}
                Err(failure) => self.failed_build(build, failure)?,
            }
        }
        for native in self.views.values() {
            if !ending
                && !self.custody.contains(native.window.id())
                && let Some(view) = &native.original
            {
                view.drive_renderer_on_ui()?;
            }
        }
        let retiring = self
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.retire_by.is_some());
        self.views.retain(|_, native| {
            native.original.is_none()
                || !(retiring || self.custody.contains(native.window.id()))
                || !native.retire_on_ui()
        });
        if retiring
            && !self.has_build_candidate()
            && self.views.values().all(|view| view.original.is_none())
            && let Some(renderer) = self.renderer.as_mut()
        {
            // All actual per-View producers have physically retired. Release
            // the final request sender without dropping the outcome receiver.
            // The Core dispatcher wakes after it observes request EOF and
            // closes its own outcome sender; that observation has no UI credit.
            let _ = renderer.requests.take();
        }
        if self
            .renderer
            .as_ref()
            .and_then(|renderer| renderer.retire_by)
            .is_some_and(|deadline| Instant::now() >= deadline && self.retains_native_app_run())
        {
            return Err(failure(
                "selected native resources or outcomes have not physically retired",
            ));
        }
        Ok(())
    }

    pub(super) fn original_deadline(&self, ending: bool) -> Option<Instant> {
        if ending {
            return self.renderer.as_ref()?.retire_by;
        }
        self.builder
            .pending_original()
            .map(|(_, deadline)| deadline)
            .into_iter()
            .chain(
                self.views
                    .values()
                    .filter(|native| !self.custody.contains(native.window.id()))
                    .filter_map(|native| native.original.as_ref()?.renderer_deadline()),
            )
            .min()
    }
}

fn failure(detail: &'static str) -> WvError {
    super::super::profile_failure_with(crate::profile::ProfileErrorKind::LifecycleUnproven, detail)
}

#[cfg(test)]
mod tests {
    use super::{Instant, Rc, RendererBridgeEndpoint, RendererBridgeOutcome, SelectedRenderer};
    use std::sync::mpsc::sync_channel;

    #[test]
    fn renderer_receiver_waits_for_actual_terminal_disconnect() {
        let (requests, requests_received) = sync_channel(1);
        let (send, outcomes) = sync_channel(2);
        let endpoint = RendererBridgeEndpoint::new(requests, outcomes, 1);
        let mut renderer = SelectedRenderer::new(endpoint, Rc::new(|_, _| Ok(()))).unwrap();
        renderer.retire_by = Some(Instant::now());
        let view = renderer.requests.as_ref().unwrap().clone();
        let _ = renderer.requests.take();
        assert!(matches!(
            requests_received.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        drop(view);
        assert!(matches!(
            requests_received.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        ));
        send.try_send(RendererBridgeOutcome::ControlCapacity)
            .unwrap();
        send.try_send(RendererBridgeOutcome::ControlCapacity)
            .unwrap();
        drop(send);

        renderer.receive_outcome().unwrap();
        assert!(renderer.pending.is_some());
        renderer.receive_outcome().unwrap(); // Cannot replace the occupied owner.
        assert!(renderer.retains_resources());
        let _ = renderer.pending.take();
        renderer.receive_outcome().unwrap();
        assert!(renderer.pending.is_some());
        let _ = renderer.pending.take();
        assert!(
            renderer.retains_resources(),
            "emptiness is not producer retirement"
        );
        renderer.receive_outcome().unwrap();
        assert!(renderer.outcomes_closed);
        assert!(!renderer.retains_resources());
    }
}
