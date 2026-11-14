//! Bounded observations of the existing Registry for synchronous Windows
//! publication. These snapshots grant no caller/native permission by themselves.
use super::{
    Phase, RendererRoute, Teardown, WebviewId, WindowError, WindowEvent, WindowOwner, WindowRef,
    WindowRegistry,
};
use keld_wv::renderer_contract::CheckedNativeDocumentLease;
use keld_wv::resource_source::NativeNavigationUrl;
use std::ops::Bound::{Excluded, Unbounded};

pub(in crate::app_session) struct RetainedWindowDocument {
    pub(in crate::app_session) owner: WindowOwner,
    pub(in crate::app_session) window: WindowRef,
    pub(in crate::app_session) document: CheckedNativeDocumentLease,
    pub(in crate::app_session) native_url: NativeNavigationUrl,
}

/// Fixed observation on the current source stack, not an owner or permission.
pub(in crate::app_session) struct NativePublicationSource {
    pub(in crate::app_session) window: WindowRef,
    document: Option<CheckedNativeDocumentLease>,
}
const _: () = assert!(
    std::mem::size_of::<NativePublicationSource>()
        <= std::mem::size_of::<keld_wv::AppWindowEvent>() + 4 * std::mem::size_of::<usize>()
);

// NativeNavigationUrl::try_clone reserves its existing source account before
// sharing the original allocation. That owner charges an AppWindowEvent slot
// plus four words for every alias. This one stack snapshot fits that already
// charged metadata; it adds no Arc wrapper or separately allocated URL bytes.
const WINDOW_DOCUMENT_PUBLICATION_METADATA_BYTES: usize =
    std::mem::size_of::<RetainedWindowDocument>();
const _: () = assert!(
    WINDOW_DOCUMENT_PUBLICATION_METADATA_BYTES
        <= std::mem::size_of::<keld_wv::AppWindowEvent>() + 4 * std::mem::size_of::<usize>()
);

pub(in crate::app_session) enum ClosingCleanup {
    Required,
    Released,
}

impl WindowRegistry {
    /// One step through the same creation-ordered Registry, with no replay Vec.
    pub(in crate::app_session) fn next_created_after(
        &self,
        owner: WindowOwner,
        after: u32,
    ) -> Option<WindowEvent> {
        self.entries
            .range((Excluded(after), Unbounded))
            .find_map(|(_, entry)| entry.created_snapshot(owner))
    }

    pub(in crate::app_session) fn matches_created_snapshot(
        &self,
        owner: WindowOwner,
        fact: &WindowEvent,
    ) -> bool {
        let WindowEvent::Created { window, .. } = fact else {
            return false;
        };
        self.entries
            .get(&window.id.0)
            .and_then(|entry| entry.created_snapshot(owner))
            .as_ref()
            == Some(fact)
    }

    /// Reuse exactly the existing registered-route predicate and creation order.
    /// Physical document liveness remains a separate final admission condition.
    pub(in crate::app_session) fn next_registered_renderer_route_after(
        &self,
        owner: WindowOwner,
        after: u32,
    ) -> Option<RendererRoute> {
        self.entries
            .range((Excluded(after), Unbounded))
            .find_map(|(_, entry)| entry.registered_renderer_route(owner))
    }

    pub(in crate::app_session) fn retain_committed_document(
        &self,
        owner: WindowOwner,
        window: WindowRef,
        generation: u32,
    ) -> Result<Option<RetainedWindowDocument>, crate::app_session::HostAppError> {
        let Some(entry) = self.entries.get(&window.id.0) else {
            return Ok(None);
        };
        let Some(document) = entry.renderer.document.as_ref() else {
            return Ok(None);
        };
        let Some(binding) = entry.original_selection.as_ref() else {
            return Ok(None);
        };
        let Some(native_url) = entry.renderer.native_url.as_ref() else {
            return Ok(None);
        };
        if entry.window != window
            || entry.owner != owner
            || !entry.owner_live
            || entry.renderer.committed != Some(document.stamp())
            || document.stamp().observed_generation() != generation
            || entry.view() != Some(document.stamp().view())
            || !document.belongs_to_source(binding.selection.source())
            || !document.is_current()
        {
            return Ok(None);
        }
        Ok(Some(RetainedWindowDocument {
            owner,
            window,
            document: document.clone(),
            native_url: native_url.try_clone().map_err(|_| {
                super::super::app_detail(
                    "window URL publication",
                    "the same source retained-byte account cannot admit a publication alias",
                )
            })?,
        }))
    }

    pub(in crate::app_session) fn next_recovered_document_after(
        &self,
        owner: WindowOwner,
        after: u32,
    ) -> Result<Option<RetainedWindowDocument>, crate::app_session::HostAppError> {
        if self.require_ready().is_err() {
            return Ok(None);
        }
        for (_, entry) in self.entries.range((Excluded(after), Unbounded)) {
            if !entry.owner_live || entry.owner != owner || entry.renderer.bound_owner.is_some() {
                continue;
            }
            let Some((document, native_url)) = entry.recoverable_original_document() else {
                continue;
            };
            return Ok(Some(RetainedWindowDocument {
                owner,
                window: entry.window,
                document: document.clone(),
                native_url: native_url.try_clone().map_err(|_| {
                    super::super::app_detail(
                        "window recovery publication",
                        "the same source retained-byte account cannot admit a publication alias",
                    )
                })?,
            }));
        }
        Ok(None)
    }

    pub(in crate::app_session) fn retains_document_publication(
        &self,
        captured: &RetainedWindowDocument,
        recovered: bool,
    ) -> bool {
        let Some(entry) = self.entries.get(&captured.window.id.0) else {
            return false;
        };
        let Some(document) = entry.renderer.document.as_ref() else {
            return false;
        };
        let Some(binding) = entry.original_selection.as_ref() else {
            return false;
        };
        let Some(native_url) = entry.renderer.native_url.as_ref() else {
            return false;
        };
        if self.ending
            || !self.ready
            || entry.window != captured.window
            || !entry.owner_live
            || entry.owner != captured.owner
            || entry.view() != Some(captured.document.stamp().view())
            || entry.renderer.committed != Some(captured.document.stamp())
            || !document.same_binding(&captured.document)
            || !native_url.same_allocation(&captured.native_url)
            || !captured
                .document
                .belongs_to_source(binding.selection.source())
            || !captured.document.is_current()
        {
            return false;
        }
        !recovered
            || (entry.renderer.bound_owner.is_none()
                && entry.recoverable_original_document().is_some())
    }

    /// Fixed facts retain their actual Registry linearization. The publication
    /// turn prevents later ordinary native facts from overwriting this snapshot.
    /// URL facts use the charged carrier/native-lease check above instead.
    pub(in crate::app_session) fn matches_publication_fact(
        &self,
        owner: WindowOwner,
        fact: &WindowEvent,
    ) -> bool {
        let window = match fact {
            WindowEvent::Created { window, .. }
            | WindowEvent::Bounds { window, .. }
            | WindowEvent::Focus { window, .. }
            | WindowEvent::Maximized { window, .. }
            | WindowEvent::FullScreen { window, .. }
            | WindowEvent::CloseRequested { window, .. }
            | WindowEvent::Destroyed { window }
            | WindowEvent::NavigationFinished { window, .. } => *window,
            WindowEvent::NavigationCommitted { .. }
            | WindowEvent::DocumentRecovered { .. }
            | WindowEvent::DesktopSnapshot { .. } => return false,
        };
        let Some(entry) = self.entries.get(&window.id.0) else {
            return false;
        };
        if self.ending
            || !self.ready
            || entry.window != window
            || !entry.created
            || !entry.owner_live
            || entry.owner != owner
        {
            return false;
        }
        let native = match entry.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) => Some(native),
            _ => None,
        };
        match fact {
            WindowEvent::Created { .. } => self.matches_created_snapshot(owner, fact),
            WindowEvent::Bounds { bounds, .. } => native.is_some_and(|n| n.state.bounds == *bounds),
            WindowEvent::Focus { focused, .. } => {
                native.is_some_and(|n| n.state.focused == *focused)
            }
            WindowEvent::Maximized { maximized, .. } => {
                native.is_some_and(|n| n.state.maximized == *maximized)
            }
            WindowEvent::FullScreen { full_screen, .. } => {
                native.is_some_and(|n| n.state.full_screen == *full_screen)
            }
            WindowEvent::CloseRequested { close_seq, .. } => {
                matches!(entry.phase, Phase::ClosePending(_, actual) if actual == *close_seq)
            }
            WindowEvent::Destroyed { .. } => matches!(entry.phase, Phase::Destroyed(_)),
            WindowEvent::NavigationFinished {
                document_generation,
                ..
            } => {
                entry.renderer.finished
                    && entry
                        .renderer
                        .committed
                        .is_some_and(|stamp| stamp.observed_generation() == *document_generation)
                    && entry
                        .renderer
                        .document
                        .as_ref()
                        .is_some_and(CheckedNativeDocumentLease::is_current)
            }
            WindowEvent::NavigationCommitted { .. }
            | WindowEvent::DocumentRecovered { .. }
            | WindowEvent::DesktopSnapshot { .. } => false,
        }
    }

    /// An admitted Closing action remains owed even after A retires. Closing
    /// cannot be adopted by B. Native Released alone makes cleanup complete.
    pub(in crate::app_session) fn closing_cleanup(
        &self,
        action: Teardown,
    ) -> Result<ClosingCleanup, WindowError> {
        let entry = self
            .entries
            .get(&action.window.id.0)
            .ok_or(WindowError::StaleWindow)?;
        if entry.window != action.window {
            return Err(WindowError::StaleWindow);
        }
        match entry.phase {
            Phase::Closing(Some(native)) if native.view == action.view => {
                Ok(ClosingCleanup::Required)
            }
            Phase::Destroyed(Some(view)) if view == action.view => Ok(ClosingCleanup::Released),
            _ => Err(WindowError::StaleWindow),
        }
    }

    /// Same Entry allocation observation; caller/phase authority is checked by
    /// the enclosing canonical source cut. Destroyed tombstones retain identity.
    pub(in crate::app_session) fn retains_caller_window(
        &self,
        caller: &super::super::PrimaryCallerContext,
        window: WindowRef,
    ) -> bool {
        self.entries.get(&window.id.0).is_some_and(|entry| {
            entry.window == window
                && entry.owner == WindowOwner::new(caller.identity, caller.attempt)
                && entry.owner_allocation.as_ref().is_some_and(|allocation| {
                    std::sync::Arc::ptr_eq(allocation, &caller.reader_stop)
                })
        })
    }

    /// Capture the exact Registry row before its existing native transition.
    /// This does not reinterpret native facts or grant publication permission.
    pub(in crate::app_session) fn native_publication_window(
        &self,
        event: &keld_wv::AppWindowEvent,
    ) -> Option<WindowRef> {
        use keld_wv::AppWindowEvent;
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
            | AppWindowEvent::WindowFullScreen { view, .. }
            | AppWindowEvent::OriginalDocumentDestroyDecision { view, .. } => *view,
            AppWindowEvent::OriginalNavigation { context, .. } => context.view(),
            AppWindowEvent::NavigationReady
            | AppWindowEvent::LastWindowClosed
            | AppWindowEvent::HostInitialized { .. }
            | AppWindowEvent::DesktopObserved { .. }
            | AppWindowEvent::WindowsDesktopObserved { .. }
            | AppWindowEvent::HostWriterProgress
            | AppWindowEvent::DialogFinished { .. } => return None,
        };
        self.entries
            .values()
            .find(|entry| entry.view() == Some(view))
            .map(|entry| entry.window)
    }

    pub(in crate::app_session) fn native_publication_source(
        &self,
        event: &keld_wv::AppWindowEvent,
    ) -> Option<NativePublicationSource> {
        let window = self.native_publication_window(event)?;
        let document = match event {
            keld_wv::AppWindowEvent::OriginalNavigation {
                fact: keld_wv::OriginalNavigationFact::Committed { document, .. },
                ..
            } => Some(document.clone()),
            keld_wv::AppWindowEvent::OriginalNavigation {
                fact: keld_wv::OriginalNavigationFact::Finished { .. },
                ..
            } => self.entries.get(&window.id.0)?.renderer.document.clone(),
            _ => None,
        };
        Some(NativePublicationSource { window, document })
    }

    pub(in crate::app_session) fn retains_native_publication(
        &self,
        caller: &super::super::PrimaryCallerContext,
        source: &NativePublicationSource,
    ) -> bool {
        if !self.retains_caller_window(caller, source.window) {
            return false;
        }
        let Some(document) = source.document.as_ref() else {
            return true;
        };
        let Some(entry) = self.entries.get(&source.window.id.0) else {
            return false;
        };
        document.is_current()
            && entry
                .renderer
                .document
                .as_ref()
                .is_some_and(|held| held.same_binding(document))
            && entry.renderer.committed == Some(document.stamp())
            && entry
                .original_selection
                .as_ref()
                .is_some_and(|binding| document.belongs_to_source(binding.selection.source()))
    }
}

impl WindowRegistry {
    /// Actual Registered fact, including a just-retired native document. This is
    /// for the fixed retirement notification only; DATA uses the current route.
    pub(in crate::app_session) fn observed_registered_renderer_route(
        &self,
        caller: &crate::app_session::PrimaryCallerContext,
        view: WebviewId,
        document: &keld_wv::renderer_contract::CheckedNativeDocumentLease,
    ) -> Option<RendererRoute> {
        let owner = WindowOwner::new(caller.identity, caller.attempt);
        self.entries.values().find_map(|entry| {
            if !entry
                .owner_allocation
                .as_ref()
                .is_some_and(|allocation| std::sync::Arc::ptr_eq(allocation, &caller.reader_stop))
            {
                return None;
            }
            entry.registered_renderer_route(owner).filter(|route| {
                route.document.stamp().view() == view && route.document.same_binding(document)
            })
        })
    }
}

impl WindowRegistry {
    /// Scalar continuation over the SAME retired Entries; no live owner or
    /// successor route can be selected by this old caller's operation.
    pub(in crate::app_session) fn next_retired_renderer_route_after(
        &self,
        caller: &crate::app_session::PrimaryCallerContext,
        after: u32,
    ) -> Option<RendererRoute> {
        let owner = WindowOwner::new(caller.identity, caller.attempt);
        self.entries
            .range((Excluded(after), Unbounded))
            .find_map(|(_, entry)| {
                if entry.owner_live {
                    return None;
                }
                let document = entry.renderer.document.as_ref()?;
                if !self.retains_retiring_renderer_route(
                    owner,
                    &caller.reader_stop,
                    entry.window,
                    document,
                ) {
                    return None;
                }
                let binding = entry.original_selection.as_ref()?;
                Some(RendererRoute {
                    window: entry.window,
                    document: document.clone(),
                    selection: std::sync::Arc::clone(&binding.selection),
                })
            })
    }
}
