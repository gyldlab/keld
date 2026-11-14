//! Windows native attribution adapter for the shared document owner.
//!
//! Only the native adapter supplies event IDs, request headers and the exact
//! resource ticket whose verified response was installed. A matching URL alone
//! never supplies a document witness. Q22 qualifies the header observation;
//! the same-URI frame race and production path still require native acceptance.

use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use crate::WebviewId;
use crate::engine::{NativeNavigationContext, NativeNavigationRefusal, OriginalEntryQuery};
use crate::native_document::{
    NativeAttempt, NativeCommitIdentity, NativeCommittedStamp, NativeDocumentOwner, ResourceLease,
};
use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::resource_intake::TaskTicket;
use crate::resource_source::AdmittedWebAppSource;

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub(super) mod cancellation_observation;
mod prepared;
pub(super) use prepared::PreparedLoad;

/// Exact header owned by the Host-created GET request, never a page credential.
pub(super) const HOST_ATTEMPT_HEADER: &str = "X-Keld-Host-Attempt";

struct HostAttempt {
    attempt: NativeAttempt,
    prepared: Rc<PreparedLoad>,
    context: Option<NativeNavigationContext>,
    role: Option<RendererRoleWitness>,
    qualified_at: Option<Instant>,
    deadline: Instant,
    native_id: Option<NonZeroU64>,
    requested: Option<TaskTicket>,
    served: Option<TaskTicket>,
}

/// One adapter retains the sole `NativeDocumentOwner` for this actual native view.
/// It owns no independent document generation, resource queue or IPC authority.
pub(super) struct OriginalNavigation {
    source: Arc<AdmittedWebAppSource>,
    document: NativeDocumentOwner,
    active: Option<HostAttempt>,
    page_pending: Option<NonZeroU64>,
    callback_depth: u32,
    posting: bool,
    reentry_failed: bool,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    post_observer: Rc<super::managed_test_control::navigation_post::Probe>,
}

impl OriginalNavigation {
    pub(super) fn new(
        view: WebviewId,
        source: Arc<AdmittedWebAppSource>,
    ) -> Result<Self, NativeNavigationRefusal> {
        let mut document = NativeDocumentOwner::new(view);
        document
            .bind_source(&source)
            .map_err(|_| NativeNavigationRefusal::MissingSource)?;
        Ok(Self {
            source,
            document,
            active: None,
            page_pending: None,
            callback_depth: 0,
            posting: false,
            reentry_failed: false,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            post_observer: Rc::new(super::managed_test_control::navigation_post::Probe::new()),
        })
    }

    pub(super) fn source(&self) -> &Arc<AdmittedWebAppSource> {
        &self.source
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn post_observer(&self) -> &Rc<super::managed_test_control::navigation_post::Probe> {
        &self.post_observer
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn post_observation(
        &self,
    ) -> super::managed_test_control::navigation_post::NavigationState {
        let stamp = self.document.current_stamp();
        super::managed_test_control::navigation_post::NavigationState {
            view: self.document.view().0,
            generation: stamp.map(|value| value.generation.get()),
            native_commit: stamp.map(|value| value.native_commit.0.get()),
            callback_depth: self.callback_depth,
            posting: self.posting,
            reentry_failed: self.reentry_failed,
        }
    }

    /// All allocation/entropy/native request construction precedes `begin_load`.
    pub(super) fn prepare(
        &self,
        query: &OriginalEntryQuery,
    ) -> Result<Rc<PreparedLoad>, NativeNavigationRefusal> {
        PreparedLoad::new(&self.source, query)
    }

    /// Called only after `CreateWebResourceRequest` succeeds and immediately
    /// before the retained view's `NavigateWithWebResourceRequest` call.
    #[cfg(test)]
    pub(super) fn begin_load(
        &mut self,
        prepared: Rc<PreparedLoad>,
        now: Instant,
    ) -> Result<NativeAttempt, NativeNavigationRefusal> {
        self.begin_with_context(prepared, None, None, now)
    }

    /// The actual Core operation is correlation only, retained by this same attempt.
    pub(super) fn begin_host_load(
        &mut self,
        prepared: Rc<PreparedLoad>,
        context: NativeNavigationContext,
        role: RendererRoleWitness,
        now: Instant,
    ) -> Result<NativeAttempt, NativeNavigationRefusal> {
        if context.view() != self.document.view() {
            return Err(NativeNavigationRefusal::UnknownView);
        }
        if !role.is_live() {
            return Err(NativeNavigationRefusal::Unavailable);
        }
        self.begin_with_context(prepared, Some(context), Some(role), now)
    }

    fn begin_with_context(
        &mut self,
        prepared: Rc<PreparedLoad>,
        context: Option<NativeNavigationContext>,
        role: Option<RendererRoleWitness>,
        now: Instant,
    ) -> Result<NativeAttempt, NativeNavigationRefusal> {
        if self.reentry_failed
            || self.posting
            || self.callback_depth != 0
            || !prepared.belongs_to(&self.source)
            || !prepared.is_prepared()
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.native_id.is_none())
        {
            return Err(NativeNavigationRefusal::Unavailable);
        }
        let deadline = now
            .checked_add(crate::INITIAL_MILESTONE_DEADLINE)
            .ok_or(NativeNavigationRefusal::Unavailable)?;
        let attempt = self
            .document
            .begin_host_load(self.source.source_id())
            .map_err(|_| NativeNavigationRefusal::Unavailable)?;
        if !prepared.admit_native_call() {
            self.document.fail_attempt(attempt);
            self.active = None;
            return Err(NativeNavigationRefusal::Unavailable);
        }
        self.page_pending = None;
        self.active = Some(HostAttempt {
            attempt,
            prepared,
            context,
            role,
            qualified_at: None,
            deadline,
            native_id: None,
            requested: None,
            served: None,
        });
        Ok(attempt)
    }

    /// Observe actual top-level `NavigationStarting` before calling `SetCancel`.
    /// False requires native cancellation. Pending page navigation holds posts
    /// but does not revoke the existing document until replacement is observed.
    pub(super) fn navigation_starting(
        &mut self,
        native_id: NonZeroU64,
        uri: &str,
        header: Option<&str>,
        redirected: bool,
        now: Instant,
    ) -> bool {
        let owned = self
            .active
            .as_ref()
            .is_some_and(|active| header == Some(active.prepared.header_value()));
        if !owned {
            self.page_pending = Some(native_id);
            return false;
        }
        let admitted = self.active.as_ref().is_some_and(|active| {
            !redirected
                && now < active.deadline
                && active.native_id.is_none_or(|id| id == native_id)
                && self.matches_entry(uri, &active.prepared)
                && self
                    .document
                    .resource_lease(active.attempt, self.source.source_id())
                    .is_some()
        });
        if !admitted {
            self.fail_active();
            return false;
        }
        if let Some(active) = &mut self.active {
            active.native_id = Some(native_id);
        }
        true
    }

    /// Call only after SetCancel(true) succeeds for this exact callback.
    /// A reentrant actual commit already revoked A and cannot be undone here.
    pub(super) fn page_navigation_cancelled(&mut self, native_id: NonZeroU64) {
        if self.page_pending == Some(native_id) {
            self.page_pending = None;
        }
    }

    /// Called only for the native GET/no-body DOCUMENT request after bounded
    /// metadata capture. The existing `TaskLedger` supplied this unique ticket.
    /// An ordinary frame/resource may read admitted bytes but cannot record a
    /// top-level served witness by matching URI, order or resource-context alone.
    pub(super) fn claim_document_request(
        &mut self,
        ticket: TaskTicket,
        lease: ResourceLease,
        uri: &str,
        header: Option<&str>,
        now: Instant,
    ) -> Result<bool, &'static str> {
        let Some(active) = &self.active else {
            return Ok(false);
        };
        if header != Some(active.prepared.header_value()) {
            return Ok(false);
        }
        if active.native_id.is_none()
            || now >= active.deadline
            || active.requested.is_some()
            || lease.attempt != active.attempt
            || !self.document.is_current_lease(lease)
            || !self.matches_entry(uri, &active.prepared)
        {
            self.fail_active();
            return Err("Host document request attribution refused");
        }
        if let Some(active) = &mut self.active {
            active.requested = Some(ticket);
        }
        Ok(true)
    }

    /// Called after SetResponse(verified 200) succeeds and before completing
    /// that request's deferral. Neither verified bytes nor the reader can call
    /// `commit_admitted`; only actual `ContentLoading` may do so below.
    pub(super) fn document_served(
        &mut self,
        ticket: TaskTicket,
        lease: ResourceLease,
        now: Instant,
    ) -> bool {
        let Some(active) = &mut self.active else {
            return false;
        };
        if now >= active.deadline
            || active.attempt != lease.attempt
            || active.requested != Some(ticket)
            || active.served.is_some()
            || !self.document.is_current_lease(lease)
        {
            return false;
        }
        active.served = Some(ticket);
        true
    }

    /// Only the real top-level `ContentLoading` callback supplies this identity.
    /// Observe `IsErrorPage` and the retained view's actual Source before entry.
    pub(super) fn content_loading(
        &mut self,
        native_id: NonZeroU64,
        uri: &str,
        is_error: bool,
        now: Instant,
    ) -> Result<NativeCommittedStamp, &'static str> {
        let Some(active) = &self.active else {
            self.document.unexpected_mainframe_commit();
            return Err("unattributed native main-frame commit");
        };
        if active.native_id != Some(native_id) {
            self.document.unexpected_mainframe_commit();
            self.active = None;
            return Err("native main-frame identity mismatch");
        }
        if !active.prepared.native_succeeded()
            || is_error
            || (self.document.current_stamp().is_none() && now >= active.deadline)
            || active.served.is_none()
            || active.served != active.requested
            || !self.matches_entry(uri, &active.prepared)
        {
            self.fail_active();
            return Err("unqualified native main-frame commit");
        }
        let result = self
            .document
            .commit_admitted(
                active.attempt,
                self.source.source_id(),
                NativeCommitIdentity(native_id),
            )
            .map_err(|_| "native document owner refused commit");
        if result.is_ok() {
            self.page_pending = None;
            if let Some(active) = &mut self.active {
                active.qualified_at.get_or_insert(now);
            }
        }
        result
    }

    /// Late A completion cannot fail or finish a newer B attempt.
    pub(super) fn navigation_completed(
        &mut self,
        native_id: NonZeroU64,
        success: bool,
    ) -> Option<Result<NativeCommittedStamp, &'static str>> {
        let active = self.active.as_ref()?;
        if active.native_id != Some(native_id) {
            return None;
        }
        if !success {
            self.fail_active();
            return Some(Err("Host native navigation failed"));
        }
        let result = self
            .document
            .current_stamp()
            .filter(|stamp| stamp.native_commit == NativeCommitIdentity(native_id))
            .ok_or("native completion preceded qualified document");
        if result.is_err() {
            self.fail_active();
        }
        Some(result)
    }

    pub(super) fn expire_uncommitted(&mut self, now: Instant) -> bool {
        if self.document.current_stamp().is_none()
            && self
                .active
                .as_ref()
                .is_some_and(|active| now >= active.deadline)
        {
            return self.fail_active();
        }
        false
    }

    pub(super) fn current_document(&self) -> Option<CheckedNativeDocumentLease> {
        self.document
            .current_stamp()
            .and_then(|stamp| self.document.lease_for(stamp))
            .map(CheckedNativeDocumentLease)
    }

    pub(super) fn owns_native_id(&self, id: NonZeroU64) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.native_id == Some(id))
    }

    pub(super) fn host_context(&self) -> Option<NativeNavigationContext> {
        self.active.as_ref().and_then(|active| active.context)
    }

    /// The exact accepted load caller fences its first bound challenge; never
    /// substitute the View's original creator or a later global current role.
    pub(super) fn host_role(&self) -> Option<&RendererRoleWitness> {
        self.active.as_ref().and_then(|active| active.role.as_ref())
    }

    /// The first actual successful native qualification starts registration's
    /// original bound. Duplicate callbacks and later service turns never renew it.
    pub(super) fn qualified_at(&self) -> Option<Instant> {
        self.active.as_ref().and_then(|active| active.qualified_at)
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        if self.document.current_stamp().is_some() {
            return None;
        }
        self.active.as_ref().map(|active| active.deadline)
    }

    /// Borrow the sole document owner; no parallel owner is minted for the bridge.
    pub(super) fn document(&self) -> &NativeDocumentOwner {
        &self.document
    }

    #[cfg(test)]
    pub(super) fn document_mut(&mut self) -> &mut NativeDocumentOwner {
        &mut self.document
    }

    pub(super) fn active_attempt(&self) -> Option<NativeAttempt> {
        self.active.as_ref().map(|active| active.attempt)
    }

    pub(super) fn resource_lease(&self) -> Option<ResourceLease> {
        self.document.active_resource_lease(self.source.source_id())
    }

    pub(super) fn is_current_resource(&self, lease: ResourceLease) -> bool {
        self.document.is_current_lease(lease)
    }

    /// Observation of the existing callback hold, never document or role authority.
    pub(super) fn callbacks_inactive(&self) -> bool {
        self.callback_depth == 0
    }

    pub(super) fn posts_held(&self) -> bool {
        self.page_pending.is_some() || self.callback_depth != 0 || self.posting
    }

    /// Must run at callback entry, before any getter/QI/other native operation.
    /// Unexpected in-call navigation reentry revokes immediately and latches a
    /// qualification failure; a post-return check cannot undo disclosed bytes.
    pub(super) fn enter_navigation_callback(&mut self) {
        if self.posting || self.callback_depth != 0 {
            self.reentry_failed = true;
            self.fail_active();
        }
        if let Some(depth) = self.callback_depth.checked_add(1) {
            self.callback_depth = depth;
        } else {
            self.reentry_failed = true;
            self.document.destroy();
        }
    }

    pub(super) fn leave_navigation_callback(&mut self) {
        if let Some(depth) = self.callback_depth.checked_sub(1) {
            self.callback_depth = depth;
        } else {
            self.reentry_failed = true;
            self.fail_active();
        }
    }

    /// Caller also validates the canonical current-role fence and native view.
    /// Drop this Rust borrow, then immediately enter `PostWebMessageAsString`:
    /// no getters, callbacks, waits or native operations may intervene.
    #[cfg(test)]
    pub(super) fn begin_post(&mut self, expected: &CheckedNativeDocumentLease) -> bool {
        self.begin_post_with(expected, &mut |_| true)
    }

    /// Same final predicate and post hold. The callback may commit only its
    /// already-retained operation using this SAME living owner; no native call,
    /// allocation, wait, interior guard retention or document mutation.
    /// A false callback leaves posting unchanged. After true no fallible step
    /// remains before this owner fixes the same posting hold.
    pub(super) fn begin_post_with(
        &mut self,
        expected: &CheckedNativeDocumentLease,
        enter: &mut dyn FnMut(&NativeDocumentOwner) -> bool,
    ) -> bool {
        if self.reentry_failed
            || self.posts_held()
            || !expected.is_current()
            || !expected.belongs_to_source(&self.source)
            || !self
                .current_document()
                .is_some_and(|current| current.same_binding(expected))
        {
            return false;
        }
        if !enter(&self.document) {
            return false;
        }
        self.posting = true;
        true
    }

    /// False is outcome-unknown, not permission to release the retained charge.
    pub(super) fn finish_post(&mut self) -> bool {
        let entered = self.posting;
        self.posting = false;
        entered && !self.reentry_failed
    }

    pub(super) fn fail_attempt(&mut self, attempt: NativeAttempt) -> bool {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.attempt == attempt)
        {
            return self.fail_active();
        }
        false
    }

    pub(super) fn destroy(&mut self) {
        self.document.destroy();
        self.active = None;
        self.page_pending = None;
    }

    fn matches_entry(&self, uri: &str, prepared: &PreparedLoad) -> bool {
        self.source
            .qualify_document_url(uri)
            .is_ok_and(|parsed| prepared.query_matches(parsed.raw_query.as_deref()))
    }

    fn fail_active(&mut self) -> bool {
        self.active
            .take()
            .is_some_and(|active| self.document.fail_attempt(active.attempt))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "original_navigation/post_entry_tests.rs"]
mod post_entry_tests;
