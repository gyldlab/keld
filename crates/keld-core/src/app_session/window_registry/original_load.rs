//! The selected Window entry owns its admitted operation and native receipts.

use super::{PendingWindowCall, Phase, WindowCounter, WindowError, WindowOwner, WindowRegistry};
use crate::app_session::PrimaryCallerContext;
use keld_ipc::{CorrelationId, window::WindowRef};
use keld_wv::{
    NativeNavigationContext, NativeNavigationRefusal, OriginalEntryQuery, OriginalNavigationFact,
    renderer_contract::{
        CheckedNativeDocumentLease, NativeAttemptObservation, NativeDocumentStamp,
    },
    resource_source::NativeNavigationUrl,
};
use std::num::NonZeroU64;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(in crate::app_session) enum OriginalLoadRefusal {
    Registry(WindowError),
    Uri(&'static str),
}

pub(in crate::app_session) struct PreparedOriginalLoad {
    pub(in crate::app_session) context: NativeNavigationContext,
    pub(in crate::app_session) query: OriginalEntryQuery,
}

pub(super) struct PendingOriginalLoad {
    pub(super) call: PendingWindowCall,
    context: NativeNavigationContext,
    attempt: Option<NativeAttemptObservation>,
    commit: Option<NavigationCommit>,
    terminal: Option<NavigationTerminal>,
    finish_published: bool,
}

struct NavigationCommit {
    document: CheckedNativeDocumentLease,
    url: NativeNavigationUrl,
    published: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app_session) enum NavigationTerminal {
    Refused(NativeNavigationRefusal),
    Failed,
    Interrupted,
    Finished(NativeDocumentStamp),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app_session) enum NavigationPublicationKind {
    Commit(NativeDocumentStamp),
    Finish(NativeDocumentStamp),
    Reply(NativeDocumentStamp),
    Error(NavigationTerminal),
}

#[derive(Clone, Copy)]
pub(in crate::app_session) struct NavigationPublication {
    pub(in crate::app_session) window: WindowRef,
    pub(in crate::app_session) correlation: CorrelationId,
    context: NativeNavigationContext,
    pub(in crate::app_session) kind: NavigationPublicationKind,
}

impl PendingOriginalLoad {
    fn publication_kind(&self) -> Option<NavigationPublicationKind> {
        use NavigationPublicationKind::{Commit, Error, Finish, Reply};
        if let Some(terminal) = self.terminal
            && !matches!(terminal, NavigationTerminal::Finished(_))
        {
            return Some(Error(terminal));
        }
        let commit = self.commit.as_ref()?;
        if !commit.document.is_current() {
            return Some(Error(NavigationTerminal::Interrupted));
        }
        let stamp = commit.document.stamp();
        if !commit.published {
            return Some(Commit(stamp));
        }
        if self.terminal != Some(NavigationTerminal::Finished(stamp)) {
            return None;
        }
        Some(if self.finish_published {
            Reply(stamp)
        } else {
            Finish(stamp)
        })
    }
}

impl WindowRegistry {
    /// Qualify the caller and selected entry before retaining native work.
    pub(in crate::app_session) fn prepare_original_load(
        &mut self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        window: WindowRef,
        uri: &str,
    ) -> Result<PreparedOriginalLoad, OriginalLoadRefusal> {
        use OriginalLoadRefusal::{Registry, Uri};
        let owner = WindowOwner::new(caller.identity, caller.attempt);
        let entry = self.checked_entry(owner, window).map_err(Registry)?;
        let native = match entry.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) => native,
            _ => return Err(Registry(WindowError::StaleWindow)),
        };
        if !self.current_original_binding(caller, window) {
            return Err(Registry(WindowError::WrongOwner));
        }
        let source = entry
            .original
            .as_ref()
            .ok_or(Registry(WindowError::StaleWindow))?
            .source();
        let query = source.qualify_original_entry_uri(uri).map_err(Uri)?;
        let context = self
            .reserve_original_load(owner, correlation, window, native.view)
            .map_err(Registry)?;
        Ok(PreparedOriginalLoad { context, query })
    }

    fn reserve_original_load(
        &mut self,
        owner: WindowOwner,
        correlation: CorrelationId,
        window: WindowRef,
        view: keld_wv::WebviewId,
    ) -> Result<NativeNavigationContext, WindowError> {
        let entry = self.checked_entry(owner, window)?;
        if entry.original_load.is_some() {
            return Err(WindowError::OriginalLoadBusy);
        }
        let operation = self
            .last_original_load_operation
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(WindowError::Exhausted(WindowCounter::OriginalLoadOperation))?;
        let context = NativeNavigationContext::new(operation, view);
        self.admitted_entry(owner, window)?.original_load = Some(PendingOriginalLoad {
            call: PendingWindowCall { owner, correlation },
            context,
            attempt: None,
            commit: None,
            terminal: None,
            finish_published: false,
        });
        self.last_original_load_operation = operation.get();
        Ok(context)
    }

    /// A failed UI send retains no pending correlation or native effect.
    pub(in crate::app_session) fn abandon_original_load(
        &mut self,
        window: WindowRef,
        context: NativeNavigationContext,
    ) {
        if let Some(entry) = self.entries.get_mut(&window.id.0)
            && entry.window == window
            && entry
                .original_load
                .as_ref()
                .is_some_and(|pending| pending.context == context)
        {
            entry.original_load = None;
        }
    }

    /// Record an already observed native fact without publishing or waiting.
    /// Returns whether the existing coordinator has an eligible publication.
    pub(in crate::app_session) fn capture_original_navigation(
        &mut self,
        caller: &PrimaryCallerContext,
        context: NativeNavigationContext,
        fact: OriginalNavigationFact,
    ) -> bool {
        let owner = WindowOwner::new(caller.identity, caller.attempt);
        let Some(entry) = self.entries.values_mut().find(|entry| {
            entry.owner_live
                && entry.owner == owner
                && entry.view() == Some(context.view())
                && entry
                    .original_load
                    .as_ref()
                    .is_some_and(|pending| pending.context == context)
        }) else {
            return false;
        };
        let (Some(binding), Some(pending)) =
            (entry.original.as_ref(), entry.original_load.as_mut())
        else {
            return false;
        };
        if self.ending || !binding.owns_caller(caller) {
            return false;
        }
        match fact {
            OriginalNavigationFact::Started { attempt }
                if pending.attempt.is_none()
                    && pending.terminal.is_none()
                    && attempt.view() == context.view() =>
            {
                pending.attempt = Some(attempt);
                false
            }
            OriginalNavigationFact::Committed {
                stamp,
                native_url,
                document,
            } if pending.terminal.is_none()
                && pending.commit.is_none()
                && pending
                    .attempt
                    .is_none_or(|attempt| attempt == stamp.attempt())
                && stamp.view() == context.view()
                && document.stamp() == stamp
                && document.is_current()
                && document.belongs_to_source(binding.source()) =>
            {
                pending.attempt = Some(stamp.attempt());
                pending.commit = Some(NavigationCommit {
                    document,
                    url: native_url,
                    published: false,
                });
                true
            }
            OriginalNavigationFact::Finished { stamp }
                if pending.terminal.is_none()
                    && pending.commit.as_ref().is_some_and(|commit| {
                        commit.document.stamp() == stamp && commit.document.is_current()
                    }) =>
            {
                pending.terminal = Some(NavigationTerminal::Finished(stamp));
                true
            }
            OriginalNavigationFact::Refused { error }
                if pending.attempt.is_none() && pending.terminal.is_none() =>
            {
                pending.terminal = Some(NavigationTerminal::Refused(error));
                true
            }
            OriginalNavigationFact::Failed { attempt, .. }
                if pending.terminal.is_none()
                    && attempt.view() == context.view()
                    && pending.attempt.is_none_or(|known| known == attempt) =>
            {
                pending.terminal = Some(NavigationTerminal::Failed);
                true
            }
            OriginalNavigationFact::Revoked {
                previous: Some(previous),
            } if pending.commit.as_ref().is_some_and(|commit| {
                commit.document.stamp() == previous && !commit.document.is_current()
            }) =>
            {
                pending.terminal = Some(NavigationTerminal::Interrupted);
                true
            }
            _ => false,
        }
    }

    pub(in crate::app_session) fn original_load_can_enter_native(
        &self,
        caller: &PrimaryCallerContext,
        window: WindowRef,
        context: NativeNavigationContext,
    ) -> bool {
        self.current_original_binding(caller, window)
            && self
                .active_original_load(caller, window)
                .is_some_and(|pending| {
                    pending.context == context
                        && pending.attempt.is_none()
                        && pending.terminal.is_none()
                })
    }

    pub(in crate::app_session) fn interrupt_original_load(
        &mut self,
        caller: &PrimaryCallerContext,
        window: WindowRef,
    ) -> Option<NavigationPublication> {
        let entry = self.entries.get_mut(&window.id.0)?;
        if entry.window != window
            || !entry.owner_live
            || entry.owner != WindowOwner::new(caller.identity, caller.attempt)
            || !matches!(entry.phase, Phase::Closing(_))
            || !entry.original.as_ref()?.owns_caller(caller)
        {
            return None;
        }
        let pending = entry.original_load.as_mut()?;
        pending.terminal = Some(NavigationTerminal::Interrupted);
        Some(NavigationPublication {
            window,
            correlation: pending.call.correlation,
            context: pending.context,
            kind: NavigationPublicationKind::Error(NavigationTerminal::Interrupted),
        })
    }

    pub(in crate::app_session) fn next_original_navigation(
        &self,
        caller: &PrimaryCallerContext,
    ) -> Option<NavigationPublication> {
        self.entries.values().find_map(|entry| {
            let pending = self.active_original_load(caller, entry.window)?;
            Some(NavigationPublication {
                window: entry.window,
                correlation: pending.call.correlation,
                context: pending.context,
                kind: pending.publication_kind()?,
            })
        })
    }

    fn active_original_load(
        &self,
        caller: &PrimaryCallerContext,
        window: WindowRef,
    ) -> Option<&PendingOriginalLoad> {
        let entry = self
            .checked_entry(WindowOwner::new(caller.identity, caller.attempt), window)
            .ok()?;
        entry.original.as_ref()?.owns_caller(caller).then_some(())?;
        entry.original_load.as_ref()
    }

    pub(in crate::app_session) fn is_current_navigation_publication(
        &self,
        caller: &PrimaryCallerContext,
        publication: NavigationPublication,
    ) -> bool {
        self.active_original_load(caller, publication.window)
            .is_some_and(|pending| {
                pending.context == publication.context
                    && pending.publication_kind() == Some(publication.kind)
            })
    }

    pub(in crate::app_session) fn retain_original_navigation_url(
        &self,
        caller: &PrimaryCallerContext,
        publication: NavigationPublication,
    ) -> Result<Option<NativeNavigationUrl>, NativeNavigationRefusal> {
        if !self.is_current_navigation_publication(caller, publication) {
            return Ok(None);
        }
        self.active_original_load(caller, publication.window)
            .and_then(|pending| pending.commit.as_ref())
            .map(|commit| commit.url.try_clone())
            .transpose()
    }

    /// Caller already validated this exact operation and admitted its packet.
    /// All remaining work is in-memory and infallible until this guard drops.
    pub(in crate::app_session) fn commit_navigation_publication(
        &mut self,
        publication: NavigationPublication,
    ) {
        if let Some(entry) = self.entries.get_mut(&publication.window.id.0)
            && let Some(pending) = entry.original_load.as_mut()
        {
            match publication.kind {
                NavigationPublicationKind::Commit(_) => {
                    if let Some(commit) = pending.commit.as_mut() {
                        commit.published = true;
                    }
                }
                NavigationPublicationKind::Finish(_) => pending.finish_published = true,
                NavigationPublicationKind::Reply(_) | NavigationPublicationKind::Error(_) => {
                    entry.original_load = None;
                }
            }
        }
    }
}
