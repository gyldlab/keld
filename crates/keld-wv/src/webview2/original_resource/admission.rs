//! Native callbacks reserve one shared task/body budget before copying or I/O.

use super::{NativePending, Pending, ResourceView, failure, native};
use crate::native_document::ResourceLease;
use crate::resource_intake::{
    IntakeReject, MAX_PENDING_METADATA_BYTES, ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit,
    TaskTicket, pending_metadata_charge, worst_case_charge,
};
use crate::resource_source::SelectedRead;
use std::rc::Rc;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::Instant;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT, ICoreWebView2,
    ICoreWebView2WebResourceRequestedEventArgs,
};
use webview2_com::WebResourceRequestedEventHandler;
use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED};
use windows::core::{IUnknown, Interface, Result as WinResult};

struct Admission<'a> {
    view: &'a ResourceView,
    ticket: TaskTicket,
    metadata: Option<RetainedByteCredit>,
    body: Option<RetainedByteCredit>,
    transferred: bool,
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        let mut state = self.view.session.state.borrow_mut();
        state.admitting.remove(&self.ticket.native_identity());
        if !self.transferred {
            state.ledger.stop(self.ticket.native_identity());
            let _ = state.ledger.release(self.ticket);
        }
        // Actual metadata/body credits remain owned until after this borrow
        // ends. No native interface is dropped under the state borrow.
    }
}

impl ResourceView {
    pub(super) fn install_callback_and_filters(self: &Rc<Self>) -> Result<(), crate::WvError> {
        let view = Rc::downgrade(self);
        let environment = self.environment.clone();
        let close = Rc::downgrade(&self.close);
        let navigation = Rc::downgrade(&self.navigation);
        let session = Rc::downgrade(&self.session);
        let id = self.id;
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |sender, args| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let Some(args) = args else {
                    if let Some(view) = view.upgrade() {
                        view.emergency_close("resource callback omitted native args");
                    }
                    return Err(E_POINTER.into());
                };
                let kind = native::source_kind(&args);
                // Install a fixed denial before any fallible metadata conversion.
                // It is provisional until return/Complete, never a served witness.
                if native::refuse(&environment, &args, native::Refusal::Forbidden).is_err() {
                    if let Some(view) = view.upgrade() {
                        view.hold_then_emergency_close(
                            &args,
                            "native refusal response could not be installed",
                        );
                    } else {
                        if let Some(nav) = navigation.upgrade()
                            && let Ok(mut nav) = nav.try_borrow_mut()
                        {
                            nav.destroy();
                        }
                        if let Some(close) = close.upgrade() {
                            let _ = close.close_on_ui();
                        }
                        if let Some(session) = session.upgrade() {
                            session.latch_failure(
                                "resource owner unavailable during native refusal failure",
                                id,
                            );
                        }
                    }
                    return Err(E_UNEXPECTED.into());
                }
                let Some(view) = view.upgrade() else {
                    return Ok(());
                };
                let Ok(kind) = kind else {
                    return Ok(());
                };
                // A transfer overlap may conservatively deny a worker request,
                // but must never infer another callback will prevent fallback.
                if !view.owns_source_kind(kind) {
                    return Ok(());
                }
                view.admit(sender.as_ref(), &args, kind)?;
                view.session.dispatch_waiting();
                Ok(())
            }));
            if let Ok(result) = result {
                result
            } else {
                // This recovery also has a barrier: no Rust panic, including
                // a poisoned wake queue, can unwind across the COM callback.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Some(view) = view.upgrade() {
                        view.emergency_close("resource callback panicked");
                    }
                }));
                Err(E_UNEXPECTED.into())
            }
        }));
        let token = native::register(&self.core, &handler)
            .map_err(|_| failure("cannot register the native resource callback"))?;
        self.handler_token.set(Some(token));
        native::add_filter(&self.core, false)
            .map_err(|_| failure("cannot install the document file resource filter"))?;
        self.session.ensure_worker_filter(self)?;
        Ok(())
    }

    fn reserve(&self, native_identity: usize) -> Result<Admission<'_>, native::Refusal> {
        pending_metadata_charge(
            ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units
                + ORIGINAL_WEBAPP_LIMITS.max_header_utf16_units,
            4 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
            1024,
            4 * self.source.response_policy().len(),
            ORIGINAL_WEBAPP_LIMITS,
        )
        .map_err(|_| native::Refusal::Forbidden)?;
        // One existing ceiling covers sequential header capture and sole-parser
        // scratch (including descriptor capacity), never concurrent body copies.
        let charge = MAX_PENDING_METADATA_BYTES;
        let ticket = self
            .session
            .state
            .borrow_mut()
            .ledger
            .begin(native_identity)
            .map_err(|error| match error {
                IntakeReject::NoTaskCapacity => native::Refusal::Capacity,
                _ => native::Refusal::Forbidden,
            })?;
        let mut admission = Admission {
            view: self,
            ticket,
            metadata: None,
            body: None,
            transferred: false,
        };
        let credit = RetainedByteCredit::reserve(&self.source.retained_credit_account(), charge)
            .map_err(|_| native::Refusal::Capacity)?;
        self.session
            .state
            .borrow_mut()
            .ledger
            .reserve_bytes(ticket, charge)
            .map_err(|_| native::Refusal::Capacity)?;
        admission.metadata = Some(credit);
        self.session
            .state
            .borrow_mut()
            .admitting
            .insert(native_identity, self.id);
        Ok(admission)
    }

    fn deny(&self, args: &ICoreWebView2WebResourceRequestedEventArgs, reason: native::Refusal) {
        if native::refuse(&self.environment, args, reason).is_err() {
            self.hold_then_emergency_close(args, "native resource refusal failed");
        }
    }

    fn owns_source_kind(&self, kind: COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS) -> bool {
        kind == COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT
            || self
                .session
                .state
                .borrow()
                .worker_filter_owner
                .get(&self.environment_identity)
                .is_none_or(|owner| *owner == self.id)
    }

    fn admit(
        self: &Rc<Self>,
        sender: Option<&ICoreWebView2>,
        args: &ICoreWebView2WebResourceRequestedEventArgs,
        kind: COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS,
    ) -> WinResult<()> {
        if self.retired.get()
            || self.controller_closed.get()
            || self.session.retiring.get()
            || self.session.fault.get().is_some()
        {
            return Ok(());
        }
        let Some(sender) = sender else {
            return Ok(());
        };
        if super::canonical_webview_identity(sender).ok() != Some(self.core_identity) {
            return Ok(());
        }
        let Some(lease) = self
            .navigation
            .try_borrow()
            .ok()
            .and_then(|nav| nav.resource_lease())
        else {
            return Ok(());
        };
        let identity = args.cast::<IUnknown>()?.as_raw() as usize;
        let mut admission = match self.reserve(identity) {
            Ok(admission) => admission,
            Err(reason) => {
                self.deny(args, reason);
                return Ok(());
            }
        };
        let Ok(request) = native::capture(args, kind) else {
            return Ok(()); // The fixed 403 is already installed.
        };
        if !self.is_live_resource(lease) {
            return Ok(());
        }
        if request.document && self.source.qualify_document_url(&request.uri).is_err() {
            return Ok(());
        }
        let Ok(selected) = self.source.qualify_url(&request.uri) else {
            return Ok(());
        };
        if let Err(reason) = self.reserve_body(&mut admission, selected.size) {
            self.deny(args, reason);
            return Ok(());
        }
        let claimed = if request.document
            && request.source_kind == COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT
        {
            match self
                .navigation
                .try_borrow_mut()
                .map_err(|_| E_UNEXPECTED)?
                .claim_document_request(
                    admission.ticket,
                    lease,
                    &request.uri,
                    request.nonce.as_deref(),
                    Instant::now(),
                ) {
                Ok(claimed) => claimed,
                Err(_) => return Ok(()),
            }
        } else {
            false
        };
        self.defer_admitted(admission, args, lease, selected, request.document, claimed);
        Ok(())
    }

    fn reserve_body(
        &self,
        admission: &mut Admission<'_>,
        size: u64,
    ) -> Result<(), native::Refusal> {
        let charge = worst_case_charge(size, 0, 0, ORIGINAL_WEBAPP_LIMITS).map_err(|error| {
            if error == IntakeReject::AssetLimit {
                native::Refusal::TooLarge
            } else {
                native::Refusal::Capacity
            }
        })?;
        let credit = RetainedByteCredit::reserve(&self.source.retained_credit_account(), charge)
            .map_err(|_| native::Refusal::Capacity)?;
        self.session
            .state
            .borrow_mut()
            .ledger
            .reserve_bytes(admission.ticket, charge)
            .map_err(|_| native::Refusal::Capacity)?;
        admission.body = Some(credit);
        Ok(())
    }

    fn defer_admitted(
        self: &Rc<Self>,
        mut admission: Admission<'_>,
        args: &ICoreWebView2WebResourceRequestedEventArgs,
        lease: ResourceLease,
        selected: SelectedRead,
        document: bool,
        claimed: bool,
    ) {
        let Some(metadata) = admission.metadata.take() else {
            self.session
                .latch_failure("admitted resource lost its metadata credit", self.id);
            return;
        };
        let Ok(deferral) = native::defer(args) else {
            return; // Fixed403; no asynchronous native job was acquired.
        };
        let native = NativePending {
            args: args.clone(),
            deferral: Some(deferral),
            environment: self.environment.clone(),
            response: None,
        };
        let queued = self.is_live_resource(lease)
            && self
                .session
                .state
                .borrow_mut()
                .ledger
                .queue_read(admission.ticket)
                .is_ok();
        let pending = Pending {
            ticket: admission.ticket,
            view: Rc::downgrade(self),
            view_id: self.id,
            lease,
            source: Arc::clone(&self.source),
            selected,
            document,
            claimed_document: claimed,
            native: Some(native),
            cancelled: Arc::new(AtomicBool::new(false)),
            _metadata_credit: metadata,
            body_credit: admission.body.take(),
            completed: None,
        };
        // This exact ledger ticket and its credits move to the native record,
        // including when reentry permits only refusal settlement. No native call
        // separates the ownership move from insertion into the same task map.
        admission.transferred = true;
        self.session
            .state
            .borrow_mut()
            .pending
            .insert(admission.ticket.native_identity(), pending);
        if !queued {
            self.session
                .cancel_ticket(self, admission.ticket.native_identity());
        }
    }
}
