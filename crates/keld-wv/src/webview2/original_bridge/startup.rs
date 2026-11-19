//! Approved Windows startup receipt metadata on the existing native binding.
//! `PreparedControl` owns actual native storage. Entry here proves neither API
//! return, Function completion, consumption, nor quota/storage release.

use crate::native_document::NativeDocumentOwner;
use crate::original_binding::OriginalBinding;
use crate::renderer_contract::CheckedNativeDocumentLease;
use std::num::NonZeroU64;
use std::time::{Duration, Instant};

const RECEIPT_BOUND: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoundEntry {
    Initial,
    DomRetry,
}

pub(super) struct StartupReceipt {
    document: CheckedNativeDocumentLease,
    deadline: Instant,
    initial_entered: bool,
    dom_observed: bool,
    retry_entered: bool,
    registered_post_returned: bool,
}

impl StartupReceipt {
    /// `qualified_at` comes from this actual `HostAttempt`'s first successful
    /// `ContentLoading` transition. Later drive/bind/duplicate cannot replace it.
    pub(super) fn new(
        owner: &NativeDocumentOwner,
        document: CheckedNativeDocumentLease,
        qualified_at: Instant,
    ) -> Result<Self, &'static str> {
        if !document.belongs_to_owner(owner) {
            return Err("startup receipt lacks its current exact native document");
        }
        let deadline = qualified_at
            .checked_add(RECEIPT_BOUND)
            .ok_or("startup receipt deadline overflow")?;
        Ok(Self {
            document,
            deadline,
            initial_entered: false,
            dom_observed: false,
            retry_entered: false,
            registered_post_returned: false,
        })
    }

    #[cfg(test)]
    pub(super) fn matches_document(&self, document: &CheckedNativeDocumentLease) -> bool {
        self.document.same_binding(document)
    }

    fn received(&self, binding: &OriginalBinding, owner: &NativeDocumentOwner) -> bool {
        self.document.belongs_to_owner(owner)
            && binding.current(owner) == Some(self.document.0.stamp())
            && self.registered_post_returned
    }

    /// Only the charged AppRegistered operation supplies this actual native
    /// return. Endpoint acceptance or local Registered alone cannot settle it.
    pub(super) fn observe_registered_post_return(
        &mut self,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        lease: &CheckedNativeDocumentLease,
        now: Instant,
    ) -> bool {
        if self.registered_post_returned
            || now >= self.deadline
            || !self.document.same_binding(lease)
            || !lease.belongs_to_owner(owner)
            || binding.current(owner) != Some(lease.0.stamp())
            || !binding.registered()
            || binding.registration_notice().is_some()
        {
            return false;
        }
        self.registered_post_returned = true;
        true
    }

    /// The actual same-view `DOMContentLoaded` adapter calls this after checking
    /// sender and native `NavigationId`. No page envelope selects this observation.
    pub(super) fn observe_dom_content_loaded(
        &mut self,
        owner: &NativeDocumentOwner,
        navigation: NonZeroU64,
    ) -> bool {
        if !self.document.belongs_to_owner(owner)
            || self.document.0.stamp().native_commit.0 != navigation
            || self.dom_observed
        {
            return false;
        }
        self.dom_observed = true;
        true
    }

    pub(super) fn next_deadline(
        &self,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
    ) -> Option<Instant> {
        (self.document.belongs_to_owner(owner) && !self.received(binding, owner))
            .then_some(self.deadline)
    }

    pub(super) fn expired(
        &self,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        now: Instant,
    ) -> bool {
        self.next_deadline(binding, owner)
            .is_some_and(|deadline| now >= deadline)
    }

    pub(super) fn next_bound(
        &self,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        now: Instant,
    ) -> Option<BoundEntry> {
        if !self.document.belongs_to_owner(owner)
            || binding.current(owner) != Some(self.document.0.stamp())
            || binding.registered()
            || self.received(binding, owner)
            || now >= self.deadline
        {
            return None;
        }
        if !self.initial_entered {
            Some(BoundEntry::Initial)
        } else if self.dom_observed && !self.retry_entered {
            Some(BoundEntry::DomRetry)
        } else {
            None
        }
    }

    /// Only inside the final canonical role fence, with the exact `PreparedControl`
    /// already retained. An inconsistent fence/native outcome retains that
    /// operation; this metadata cannot grant a retry or prove native success.
    pub(super) fn begin_bound(
        &mut self,
        kind: BoundEntry,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        now: Instant,
    ) -> bool {
        if self.next_bound(binding, owner, now) != Some(kind) {
            return false;
        }
        match kind {
            BoundEntry::Initial => self.initial_entered = true,
            BoundEntry::DomRetry => self.retry_entered = true,
        }
        true
    }
}

#[cfg(test)]
mod tests;
