//! Passive observations around the existing same-args cancellation call.
//! The retained object is a read-only lease witness, never a native owner.
use std::cell::RefCell;
use std::io::Write as _;
use std::num::NonZeroU64;
use std::rc::Rc;

use serde_json::{Value, json};

use super::OriginalNavigation;
use crate::renderer_contract::CheckedNativeDocumentLease;

pub(in crate::webview2) struct PendingCancellation {
    id: NonZeroU64,
    pending: NonZeroU64,
    callback_depth: u32,
    posts_held: bool,
    document_current: bool,
    source_current: bool,
    document: CheckedNativeDocumentLease,
}

impl PendingCancellation {
    pub(in crate::webview2) fn capture(
        state: &Rc<RefCell<OriginalNavigation>>,
        id: Option<NonZeroU64>,
    ) -> Option<Self> {
        let id = id?;
        let state = state.try_borrow().ok()?;
        let pending = state.page_pending?;
        if pending != id {
            return None;
        }
        // Initial-load refusal is outside this observation. Only an existing A
        // and the actual page-pending ID are observed; no state is changed.
        let document = state.current_document()?;
        Some(Self {
            id,
            pending,
            callback_depth: state.callback_depth,
            posts_held: state.posts_held(),
            document_current: document.is_current(),
            source_current: document.belongs_to_source(state.source()),
            document,
        })
    }

    pub(in crate::webview2) fn complete(
        self,
        state: &OriginalNavigation,
        set_cancel_succeeded: bool,
    ) -> Value {
        let after = state.current_document();
        json!({
            "cancel_native_id": self.id.get(),
            "set_cancel_succeeded": set_cancel_succeeded,
            "before": {
                "pending_id": self.pending.get(),
                "callback_depth": self.callback_depth,
                "posts_held": self.posts_held,
                "document_current": self.document_current,
                "source_current": self.source_current,
                "document": document(&self.document),
            },
            "after": {
                "pending_id": state.page_pending.map(NonZeroU64::get),
                "callback_depth": state.callback_depth,
                "posts_held": state.posts_held(),
                "retained_a_current": self.document.is_current(),
                "document_current": after.as_ref().is_some_and(CheckedNativeDocumentLease::is_current),
                "source_current": after.as_ref().is_some_and(|lease| lease.belongs_to_source(state.source())),
                "document": after.as_ref().map(document),
            },
            "same_native_lease": after.as_ref().is_some_and(|lease| self.document.same_binding(lease)),
        })
    }
}

fn document(lease: &CheckedNativeDocumentLease) -> Value {
    let stamp = lease.stamp().0;
    json!({
        "view": stamp.view.0,
        "generation": stamp.generation.get(),
        "native_commit": stamp.native_commit.0.get(),
        "source": stamp.source.0,
    })
}

pub(in crate::webview2) fn emit(record: &Value) {
    // The callback's original error/result remains authoritative if output fails.
    let _ = writeln!(
        std::io::stderr().lock(),
        "KELD_ORIGINAL_PAGE_CANCELLATION {record}"
    );
}
