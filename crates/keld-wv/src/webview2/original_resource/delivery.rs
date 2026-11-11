//! A reader result retains its body and native records through Complete/Close.

use super::{Pending, ResourceSession, ResourceView, native, reader};
use crate::resource_source::ResourceSourceError;
use std::sync::{Arc, atomic::Ordering};
use std::time::Instant;

impl ResourceSession {
    pub(super) fn dispatch_waiting(&self) {
        if self.retiring.get() || self.fault.get().is_some() {
            return;
        }
        let work = {
            let mut state = self.state.borrow_mut();
            let Some(ticket) = state.ledger.next_queued_read() else {
                return;
            };
            if state.ledger.worker_started(ticket).is_err() {
                return;
            }
            let Some(pending) = state.pending.get_mut(&ticket.native_identity()) else {
                self.fault.set(Some("queued resource has no native record"));
                return;
            };
            let Some(credit) = pending.body_credit.take() else {
                self.fault
                    .set(Some("queued resource lost its reserved body credit"));
                return;
            };
            reader::Work {
                ticket,
                view: pending.view_id,
                source: Arc::clone(&pending.source),
                selected: pending.selected.clone(),
                cancelled: Arc::clone(&pending.cancelled),
                credit,
            }
        };
        let mut staged = Some(work);
        let accepted = self.reader.borrow_mut().submit(&mut staged, &self.proxy);
        if accepted.is_err() {
            let Some(work) = staged else {
                self.fault.set(Some(
                    "reader submission failed without returning its custody",
                ));
                return;
            };
            let reader::Work {
                ticket,
                view,
                credit,
                source,
                selected,
                cancelled,
            } = work;
            drop(source);
            drop(selected);
            drop(cancelled);
            self.latch_failure("bounded reader could not accept the reserved task", view);
            // No work escaped. The same admitted ticket enters the ordinary
            // refusal settlement with its still-owned body reservation.
            self.deliver(reader::Completed {
                ticket,
                result: Err(ResourceSourceError::Io),
                _credit: credit,
            });
        }
    }

    pub(super) fn deliver(&self, completed: reader::Completed) {
        let ticket = completed.ticket;
        let pending = {
            let mut state = self.state.borrow_mut();
            if state.ledger.delivery_started(ticket).is_err() {
                None
            } else {
                state.pending.remove(&ticket.native_identity())
            }
        };
        let Some(mut pending) = pending else {
            self.reader.borrow_mut().retain_unproved(completed);
            self.fault.set(Some(
                "resource completion lost its exact pending incarnation",
            ));
            return;
        };
        pending.completed = Some(completed);
        // These owning values stay OUTSIDE the unwind barrier. A panic cannot
        // release the real buffer or native references before closure is proved.
        let delivered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.deliver_inner(&mut pending)
        }));
        let settled = match delivered {
            Ok(settled) => settled,
            Err(_) => {
                if let Some(view) = pending.view.upgrade() {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        view.emergency_close("native resource delivery panicked");
                    }));
                    view.controller_closed.get()
                } else {
                    false
                }
            }
        };
        if settled {
            self.release_pending(pending, true);
        } else {
            // Not a second queue: the same ledger ticket/native record retains
            // its one result until the actual controller-closed receipt arrives.
            self.state
                .borrow_mut()
                .pending
                .insert(ticket.native_identity(), pending);
        }
    }

    fn deliver_inner(&self, pending: &mut Pending) -> bool {
        let Some(view) = pending.view.upgrade() else {
            return self.refuse_pending(pending);
        };
        if view.controller_closed.get() || pending.native.is_none() {
            return true;
        }
        let can_deliver = view.is_live_resource(pending.lease)
            && !pending.cancelled.load(Ordering::Acquire)
            && pending
                .completed
                .as_ref()
                .is_some_and(|result| result.result.is_ok())
            && self
                .state
                .borrow_mut()
                .ledger
                .begin_terminal_callback(pending.ticket);
        if !can_deliver {
            return self.refuse_pending(pending);
        }
        let Some(native) = pending.native.as_mut() else {
            return true;
        };
        let Some(Ok(body)) = pending
            .completed
            .as_ref()
            .map(|completed| &completed.result)
        else {
            return self.refuse_pending(pending);
        };
        let policy = pending.document.then(|| pending.source.response_policy());
        native.response = match native::response(&native.environment, body, policy) {
            Ok(response) => Some(response),
            Err(_) => return self.refuse_pending(pending),
        };
        if !self.may_continue(&view, pending) {
            return self.refuse_pending(pending);
        }
        let Some(native) = pending.native.as_ref() else {
            return true;
        };
        let Some(response) = native.response.as_ref() else {
            return self.refuse_pending(pending);
        };
        if native::set_response(&native.args, response).is_err()
            || !self.may_continue(&view, pending)
        {
            return self.refuse_pending(pending);
        }
        if pending.claimed_document {
            let served = view.navigation.try_borrow_mut().is_ok_and(|mut nav| {
                nav.document_served(pending.ticket, pending.lease, Instant::now())
            });
            if !served {
                return self.refuse_pending(pending);
            }
        }
        // The verified response is installed and (only for an attributed top
        // request) marked served before this one deferral Complete boundary.
        self.complete_installed(pending)
    }

    fn may_continue(&self, view: &ResourceView, pending: &Pending) -> bool {
        view.is_live_resource(pending.lease)
            && !pending.cancelled.load(Ordering::Acquire)
            && self
                .state
                .borrow()
                .ledger
                .may_continue_native_delivery(pending.ticket)
    }

    pub(super) fn refuse_pending(&self, pending: &mut Pending) -> bool {
        if pending
            .view
            .upgrade()
            .is_some_and(|view| view.controller_closed.get())
        {
            drop(pending.native.take());
            return true;
        }
        let Some(native) = pending.native.as_ref() else {
            return true;
        };
        if native::refuse(
            &native.environment,
            &native.args,
            native::Refusal::Forbidden,
        )
        .is_err()
        {
            if let Some(view) = pending.view.upgrade() {
                view.emergency_close("native deferred refusal failed");
                return view.controller_closed.get();
            }
            self.latch_failure(
                "native deferred refusal has no retained view owner",
                pending.view_id,
            );
            return false;
        }
        self.complete_installed(pending)
    }

    fn complete_installed(&self, pending: &mut Pending) -> bool {
        if pending
            .view
            .upgrade()
            .is_some_and(|view| view.controller_closed.get())
        {
            drop(pending.native.take());
            return true;
        }
        let Some(native) = pending.native.as_mut() else {
            return true;
        };
        let Some(deferral) = native.deferral.take() else {
            return false;
        };
        if native::complete(deferral).is_err() {
            if let Some(view) = pending.view.upgrade() {
                view.emergency_close("native resource Complete failed");
                return view.controller_closed.get();
            }
            self.latch_failure(
                "native Complete failed without a retained view owner",
                pending.view_id,
            );
            return false;
        }
        drop(pending.native.take());
        true
    }

    pub(super) fn release_pending(&self, pending: Pending, delivery: bool) {
        let ticket = pending.ticket;
        let view = pending.view_id;
        // All native releases and owned buffer/credit drops occur outside the
        // RefCell while this ledger ticket still blocks another native delivery.
        drop(pending);
        let result = {
            let mut state = self.state.borrow_mut();
            state.ledger.stop(ticket.native_identity());
            if delivery {
                let _ = state.ledger.delivery_quiesced(ticket);
            }
            state.ledger.release(ticket)
        };
        if result.is_err() {
            self.latch_failure(
                "resource credit released before its real work quiesced",
                view,
            );
        }
    }
}
