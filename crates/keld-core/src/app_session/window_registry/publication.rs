//! Point observations of the existing Registry for ordered Window publication.

use super::{Phase, Teardown, WindowError, WindowEvent, WindowOwner, WindowRef, WindowRegistry};
use keld_wv::AppWindowEvent;
#[cfg(windows)]
use keld_wv::WebviewId;
use std::ops::Bound::{Excluded, Unbounded};

pub(in crate::app_session) enum ClosingCleanup {
    Required,
    Released,
}

pub(in crate::app_session) fn fixed_event_window(fact: &WindowEvent) -> Option<WindowRef> {
    match fact {
        WindowEvent::Created { window, .. }
        | WindowEvent::Bounds { window, .. }
        | WindowEvent::Focus { window, .. }
        | WindowEvent::Maximized { window, .. }
        | WindowEvent::FullScreen { window, .. }
        | WindowEvent::CloseRequested { window, .. }
        | WindowEvent::Destroyed { window } => Some(*window),
        WindowEvent::NavigationCommitted { .. }
        | WindowEvent::DocumentRecovered { .. }
        | WindowEvent::NavigationFinished { .. }
        | WindowEvent::NavigationFinishedUnitPinchV1 { .. }
        | WindowEvent::OriginalRendererViewBound { .. }
        | WindowEvent::DesktopSnapshot { .. } => None,
    }
}

impl WindowRegistry {
    /// Observe the retained selected view; never derive it from a logical ID.
    /// Recovery transfers the Registry owner before the original caller is
    /// rebound at Adopt, so the old caller pointer is not a replay predicate.
    #[cfg(windows)]
    pub(in crate::app_session) fn original_view_for_publication(
        &self,
        owner: WindowOwner,
        window: WindowRef,
    ) -> Option<WebviewId> {
        if !self.ready || self.ending {
            return None;
        }
        let entry = self.entries.get(&window.id.0)?;
        if entry.window != window
            || !entry.created
            || !entry.owner_live
            || entry.owner != owner
            || entry.original.is_none()
        {
            return None;
        }
        match entry.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) => Some(native.view),
            Phase::Opening(_) | Phase::Closing(_) | Phase::Destroyed(_) => None,
        }
    }

    /// A second scalar cursor follows the entire Created replay census.
    #[cfg(windows)]
    pub(in crate::app_session) fn next_original_view_after(
        &self,
        owner: WindowOwner,
        after: u32,
    ) -> Option<(WindowRef, WebviewId)> {
        self.entries
            .range((Excluded(after), Unbounded))
            .find_map(|(_, entry)| {
                self.original_view_for_publication(owner, entry.window)
                    .map(|view| (entry.window, view))
            })
    }

    /// A single ordered snapshot from the retained Registry. The source holds
    /// only the cursor and one fact through writer capacity and physical write.
    pub(in crate::app_session) fn next_created_after(
        &self,
        owner: WindowOwner,
        after: u32,
    ) -> Option<WindowEvent> {
        self.entries
            .range((Excluded(after), Unbounded))
            .find_map(|(_, entry)| entry.created_snapshot(owner))
    }

    pub(in crate::app_session) fn retains_publication_window(
        &self,
        owner: WindowOwner,
        window: WindowRef,
    ) -> bool {
        self.entries
            .get(&window.id.0)
            .is_some_and(|entry| entry.window == window && entry.owner_live && entry.owner == owner)
    }

    pub(in crate::app_session) fn matches_publication_fact(
        &self,
        owner: WindowOwner,
        fact: &WindowEvent,
    ) -> bool {
        let Some(window) = fixed_event_window(fact) else {
            return false;
        };
        let Some(entry) = self.entries.get(&window.id.0).filter(|entry| {
            entry.window == window && entry.created && entry.owner_live && entry.owner == owner
        }) else {
            return false;
        };
        if !self.ready || self.ending {
            return false;
        }
        let live = match entry.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) => Some(native),
            _ => None,
        };
        match fact {
            WindowEvent::Created { .. } => entry.created_snapshot(owner).as_ref() == Some(fact),
            WindowEvent::Bounds { bounds, .. } => {
                live.is_some_and(|native| native.state.bounds == *bounds)
            }
            WindowEvent::Focus { focused, .. } => {
                live.is_some_and(|native| native.state.focused == *focused)
            }
            WindowEvent::Maximized { maximized, .. } => {
                live.is_some_and(|native| native.state.maximized == *maximized)
            }
            WindowEvent::FullScreen { full_screen, .. } => {
                live.is_some_and(|native| native.state.full_screen == *full_screen)
            }
            WindowEvent::CloseRequested { close_seq, .. } => {
                matches!(entry.phase, Phase::ClosePending(_, actual) if actual == *close_seq)
            }
            WindowEvent::Destroyed { .. } => matches!(entry.phase, Phase::Destroyed(_)),
            WindowEvent::NavigationCommitted { .. }
            | WindowEvent::DocumentRecovered { .. }
            | WindowEvent::NavigationFinished { .. }
            | WindowEvent::NavigationFinishedUnitPinchV1 { .. }
            | WindowEvent::OriginalRendererViewBound { .. }
            | WindowEvent::DesktopSnapshot { .. } => false,
        }
    }

    pub(in crate::app_session) fn native_publication_window(
        &self,
        event: &AppWindowEvent,
    ) -> Option<WindowRef> {
        let view = match event {
            AppWindowEvent::WindowBuilt { build, .. }
            | AppWindowEvent::WindowBuildFailed { build, .. } => {
                return self
                    .entries
                    .values()
                    .find(|entry| entry.window.incarnation.0 == *build)
                    .map(|entry| entry.window);
            }
            AppWindowEvent::WindowReleased { view }
            | AppWindowEvent::WindowCloseRequested { view }
            | AppWindowEvent::WindowBounds { view, .. }
            | AppWindowEvent::WindowFocus { view, .. }
            | AppWindowEvent::WindowMaximized { view, .. }
            | AppWindowEvent::WindowFullScreen { view, .. } => *view,
            _ => return None,
        };
        self.entries
            .values()
            .find(|entry| entry.view() == Some(view))
            .map(|entry| entry.window)
    }

    /// Closing is owed by the actual native view even after its Runtime role
    /// retires. Receipt of WindowReleased alone marks this cleanup complete.
    pub(in crate::app_session) fn closing_cleanup(
        &self,
        action: Teardown,
    ) -> Result<ClosingCleanup, WindowError> {
        let entry = self
            .entries
            .get(&action.window.id.0)
            .filter(|entry| entry.window == action.window)
            .ok_or(WindowError::StaleWindow)?;
        match entry.phase {
            Phase::Closing(Some(native)) if native.view == action.view => {
                Ok(ClosingCleanup::Required)
            }
            Phase::Destroyed(Some(view)) if view == action.view => Ok(ClosingCleanup::Released),
            _ => Err(WindowError::StaleWindow),
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::app_session::{
        PrimaryCallerContext, PrimaryLinkIdentity,
        graph_contract::tests::retained_original_window_resources,
        window_registry::{BuildOutcome, CreateOutcome, OriginalWindowResources},
    };
    use keld_ipc::CorrelationId;
    use keld_ipc::window::{Rect, WindowState};
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn selected_view_snapshot_uses_retained_pair_and_successor_owner() {
        let (_stage, source, preload) = retained_original_window_resources();
        let caller = PrimaryCallerContext {
            identity: PrimaryLinkIdentity::Fixture,
            attempt: 1,
            reader_stop: Arc::new(AtomicBool::new(false)),
        };
        let old = WindowOwner::new(caller.identity, caller.attempt);
        let successor = WindowOwner::new(caller.identity, 2);
        let mut registry = WindowRegistry::new();
        assert!(registry.mark_ready());
        let CreateOutcome::Build { build, window, .. } = registry
            .create_original_for_caller(
                &caller,
                CorrelationId(1),
                (640, 480),
                (None, None),
                OriginalWindowResources { source, preload },
            )
            .expect("selected entry")
        else {
            panic!("new selected entry expected")
        };
        assert!(
            registry
                .original_view_for_publication(old, window)
                .is_none(),
            "Opening must not publish"
        );
        let state = WindowState {
            bounds: Rect {
                x: 0,
                y: 0,
                width: 640,
                height: 480,
            },
            focused: false,
            maximized: false,
            full_screen: false,
        };
        assert!(matches!(
            registry.native_built(build, WebviewId(7), state),
            BuildOutcome::Created { .. }
        ));
        assert_eq!(
            registry.original_view_for_publication(old, window),
            Some(WebviewId(7))
        );
        let CreateOutcome::Build {
            build: ordinary_build,
            window: ordinary_window,
            ..
        } = registry
            .create(old, CorrelationId(2), 640, 480, None, None)
            .expect("ordinary entry")
        else {
            panic!("new ordinary entry expected")
        };
        assert!(matches!(
            registry.native_built(ordinary_build, WebviewId(8), state),
            BuildOutcome::Created { .. }
        ));
        assert_eq!(
            registry.original_view_for_publication(old, ordinary_window),
            None,
            "ordinary windows never receive selected binding facts"
        );
        registry.role_lost(old, |_| panic!("no pending close"));
        assert_eq!(registry.original_view_for_publication(old, window), None);
        registry.subscribe_successor(successor);
        assert_eq!(
            registry.next_original_view_after(successor, 0),
            Some((window, WebviewId(7)))
        );
        assert_eq!(
            registry.next_original_view_after(successor, window.id.0),
            None
        );
        registry.session_end(|_| {});
        assert_eq!(
            registry.original_view_for_publication(successor, window),
            None,
            "session end excludes Closing"
        );
    }
}
