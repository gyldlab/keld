//! Passive scalars from the same `Pending`, `TaskLedger` and `Reader` owners.
//! No native call, result take, credit move, cancellation, wake or new ticket.
#![forbid(unsafe_code)]

use super::reader::Completed;
use super::{Pending, ResourceSession, State};
use crate::WebviewId;
use serde_json::Value;
use std::io::Write as _;
use std::sync::atomic::Ordering;

const ASSET: &str = "x14-held-resource.svg";

fn pending_snapshot(state: &State, pending: &Pending) -> Value {
    // Observe the existing drop-owned guard through this same Pending borrow.
    let Pending {
        _metadata_credit: metadata_credit,
        ..
    }: &Pending = pending;
    let account = pending.source.retained_credit_account();
    let task = state.ledger.observe_ticket(pending.ticket).map(|task| {
        serde_json::json!({
            "stopped": task.stopped(),
            "terminal": task.terminal(),
            "workerActive": task.worker_active,
            "queuedRead": task.queued_read,
            "deliveryActive": task.delivery_active,
            "retainedBytes": task.retained_bytes,
            "ledgerRetainedBytes": task.ledger_retained_bytes,
            "ledgerTasks": task.ledger_tasks,
        })
    });
    let completed_body = pending
        .completed
        .as_ref()
        .and_then(|completed| completed.result.as_ref().ok().map(|body| body.bytes.len()));
    serde_json::json!({
        "ticket": format!("{:?}", pending.ticket),
        "view": pending.view_id.0,
        "asset": pending.selected.path,
        "selectedBytes": pending.selected.size,
        "pendingInMap": state.pending.contains_key(&pending.ticket.native_identity()),
        "admitting": state.admitting.contains_key(&pending.ticket.native_identity()),
        "cancelled": pending.cancelled.load(Ordering::Acquire),
        "nativeRetained": pending.native.is_some(),
        "metadataCreditOwned": metadata_credit.covers(&account, 1),
        "bodyCreditInPending": pending.body_credit.is_some(),
        "completedInPending": pending.completed.is_some(),
        "completedBodyBytes": completed_body,
        "completedCreditCoversBody": pending.completed.as_ref().map(|completed| {
            // The completed body and its guard remain in the same owner.
            let Completed { _credit: credit, .. }: &Completed = completed;
            credit.covers(&account, completed_body.unwrap_or(0))
        }),
        "accountBytes": account.load(Ordering::Acquire),
        "ledger": task,
    })
}

fn emit(value: &Value) {
    // An observation error cannot become a synthetic product receipt.
    let _ = writeln!(std::io::stderr(), "KELD_ORIGINAL_RESOURCE_CUSTODY {value}");
}

impl ResourceSession {
    pub(super) fn snapshot_pending(&self, pending: &Pending) -> Option<Value> {
        if !self.test_resource_observation || pending.selected.path != ASSET {
            return None;
        }
        let state = self.state.try_borrow().ok()?;
        Some(pending_snapshot(&state, pending))
    }

    pub(super) fn observe_pending(&self, phase: &str, pending: &Pending) {
        if let Some(snapshot) = self.snapshot_pending(pending) {
            // snapshot_pending's state borrow ended before output.
            emit(&serde_json::json!({"phase": phase, "pending": [snapshot]}));
        }
    }

    pub(super) fn observe_released(&self, prior: Option<Value>, released: bool) {
        let Some(prior) = prior else { return };
        emit(&serde_json::json!({
            "phase": "pending-dropped",
            "prior": prior,
            "ledgerReleaseSucceeded": released,
            "accountBytes": self.custody.retained_credit_account().load(Ordering::Acquire),
        }));
    }

    pub(in crate::webview2) fn observe_resources(&self, phase: &str, view: Option<WebviewId>) {
        if !self.test_resource_observation {
            return;
        }
        let snapshot = {
            let Ok(state) = self.state.try_borrow() else {
                return;
            };
            let pending: Vec<_> = state
                .pending
                .values()
                .filter(|pending| {
                    pending.selected.path == ASSET && view.is_none_or(|id| id == pending.view_id)
                })
                .map(|pending| pending_snapshot(&state, pending))
                .collect();
            serde_json::json!({
                "phase": phase,
                "view": view.map(|id| id.0),
                "pending": pending,
                "pendingCount": state.pending.len(),
                "admittingCount": state.admitting.len(),
                "viewCount": state.views.len(),
                "filterTransitionCount": state.filter_transition.len(),
                "closingFilterCount": state.closing_filter_owner.len(),
                "ledgerEmpty": state.ledger.is_empty(),
                "ledgerActiveDelivery": state.ledger.has_active_delivery(),
            })
        };
        let reader = self.reader.try_borrow().map_or_else(
            |_| serde_json::json!({"phase": "borrowed"}),
            |reader| reader.observe_custody(),
        );
        emit(&serde_json::json!({
            "phase": phase,
            "session": snapshot,
            "reader": reader,
            "retiring": self.retiring.get(),
            "fault": self.fault.get(),
            "accountBytes": self.custody.retained_credit_account().load(Ordering::Acquire),
        }));
    }
}
