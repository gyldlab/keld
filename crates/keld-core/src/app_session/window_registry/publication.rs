//! Point observations of the existing Registry for ordered Window publication.

use super::{Phase, Teardown, WindowError, WindowEvent, WindowOwner, WindowRef, WindowRegistry};
use keld_wv::AppWindowEvent;
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
        | WindowEvent::DesktopSnapshot { .. } => None,
    }
}

impl WindowRegistry {
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
