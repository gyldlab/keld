//! Actual `WebView2` navigation events feed the sole checked document owner.
//! All callbacks are STA-owned; no native handle or pointer crosses to a worker.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::num::NonZeroU64;
use std::rc::Rc;
use std::time::Instant;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2HttpRequestHeaders, ICoreWebView2NavigationStartingEventArgs,
};
use webview2_com::{
    ContentLoadingEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
};
use windows::Win32::Foundation::{E_INVALIDARG, E_POINTER, E_UNEXPECTED};
use windows::core::{BOOL, HSTRING};

use super::guarded_navigation::canonical_webview_identity;
use super::native_text::read_task_text;
use super::original_navigation::{HOST_ATTEMPT_HEADER, OriginalNavigation};
use super::original_resource::ResourceSession;
use crate::engine::{NativeNavigationContext, OriginalNavigationFact};
use crate::native_document::NativeAttempt;
use crate::renderer_contract::{NativeAttemptObservation, NativeDocumentStamp};
use crate::resource_intake::{
    MAX_PENDING_METADATA_BYTES, ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit,
};
use crate::resource_source::NativeNavigationUrl;

type Observation = Rc<dyn Fn(Option<NativeNavigationContext>, OriginalNavigationFact)>;

pub(in crate::webview2) mod boundary;
mod lifetime;
pub(in crate::webview2) mod registration;

use boundary::{CallbackTurn, FailureSink, boundary, callback, capture_failure, fail_observed};
use registration::Registration;

pub(super) struct NavigationCallbacks {
    view: ICoreWebView2,
    navigation: Rc<RefCell<OriginalNavigation>>,
    failure: FailureSink,
    _session: Rc<ResourceSession>,
    starting: Registration,
    loading: Registration,
    completed: Registration,
    install_attempted: Cell<bool>,
    installing: Cell<bool>,
    retired: Cell<bool>,
    controller_closed: Cell<bool>,
}

#[derive(Clone, Copy)]
struct FailureObservation {
    context: Option<NativeNavigationContext>,
    attempt: Option<NativeAttempt>,
    previous: Option<NativeDocumentStamp>,
}

#[allow(unsafe_code)] // Same creating-STA subscription, getters and cancellation.
fn install_starting(
    owner: &NavigationCallbacks,
    identity: usize,
    observe: Observation,
) -> windows::core::Result<()> {
    let state = Rc::downgrade(&owner.navigation);
    let sink = owner.failure.clone();
    let handler = NavigationStartingEventHandler::create(Box::new(move |sender, args| {
        let failed = |detail| sink.latch(detail);
        boundary(&failed, || {
            let captured = (sender, args); // Native argument disposal stays inside the barrier.
            let Some(state) = state.upgrade() else {
                failed("navigation callback lost its retained document owner");
                let args = captured.1.ok_or(E_POINTER)?;
                // SAFETY: actual live starting args; a missing owner never admits navigation.
                return unsafe { args.SetCancel(true) };
            };
            let failure = match capture_failure(&state) {
                Ok(failure) => failure,
                Err(error) => {
                    failed("navigation failure capture met a held document-owner borrow");
                    if let Some(args) = captured.1 {
                        // SAFETY: same callback args, no Rust source borrow is held.
                        let _ = unsafe { args.SetCancel(true) };
                    }
                    return Err(error);
                }
            };
            let turn = match CallbackTurn::enter(&state, &failed) {
                Ok(turn) => turn,
                Err(error) => {
                    if let Some(args) = captured.1 {
                        // SAFETY: failure was latched before cancellation; the same actual args are live.
                        let _ = unsafe { args.SetCancel(true) };
                    }
                    return Err(error);
                }
            };
            let result = {
                let (sender, args) = captured; // Released before CallbackTurn leaves.
                let Some(args) = args else {
                    failed("native starting callback omitted its event arguments");
                    return Err(E_POINTER.into());
                };
                let observed = boundary(&failed, || {
                    starting_result(&state, identity, sender.as_ref(), &args, &failed)
                });
                let retired = failure.attempt.is_some()
                    && state
                        .try_borrow()
                        .is_ok_and(|state| state.active_attempt() != failure.attempt);
                let settled = if retired {
                    fail_observed(&state, &observe, failure, &failed)
                } else {
                    Ok(())
                };
                observed.and(settled)
            };
            drop(turn);
            result
        })
    }));
    owner.require_live()?;
    owner.starting.add(
        |token| {
            // SAFETY: this attached Cell owns the writable token for the entire STA call.
            unsafe { owner.view.add_NavigationStarting(&handler, token.as_ptr()) }
        },
        || {
            owner
                .failure
                .latch("native navigation-starting registration panicked");
        },
    )
}

fn starting_result(
    state: &Rc<RefCell<OriginalNavigation>>,
    identity: usize,
    sender: Option<&ICoreWebView2>,
    args: &ICoreWebView2NavigationStartingEventArgs,
    failed: &dyn Fn(&'static str),
) -> windows::core::Result<()> {
    // Catch the observation itself so denial still runs after any getter panic.
    let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        starting(state, identity, sender, args)
    }))
    .unwrap_or_else(|_| {
        failed("navigation starting observation panicked");
        Err(E_UNEXPECTED.into())
    });
    if !matches!(&observed, Ok((_, true))) {
        let id = observed.as_ref().ok().map(|(id, _)| *id);
        cancel_start(state, args, id)?;
    }
    observed.map(|_| ())
}

#[allow(unsafe_code)] // Same creating-STA subscription and actual native getters.
fn install_loading(
    owner: &NavigationCallbacks,
    identity: usize,
    observe: Observation,
) -> windows::core::Result<()> {
    let state = Rc::downgrade(&owner.navigation);
    let sink = owner.failure.clone();
    let handler = ContentLoadingEventHandler::create(Box::new(move |sender, args| {
        let failed = |detail| sink.latch(detail);
        boundary(&failed, || {
            let captured = (sender, args);
            let Some(state) = state.upgrade() else {
                failed("content callback lost its retained document owner");
                return Err(E_POINTER.into());
            };
            let failure = capture_failure(&state).inspect_err(|_| {
                failed("content failure capture met a held document-owner borrow");
            })?;
            callback(&state, &failed, || {
                let (sender, args) = captured;
                let result = boundary(&failed, || {
                    let sender = checked_sender(identity, sender.as_ref())?;
                    let args = args.as_ref().ok_or(E_POINTER)?;
                    let _credit = callback_credit(&state)?;
                    let mut id = 0;
                    let mut error = BOOL(1);
                    // SAFETY: actual callback-owned args and writable stack outputs.
                    unsafe {
                        args.NavigationId(&raw mut id)?;
                        args.IsErrorPage(&raw mut error)?;
                    }
                    let id = NonZeroU64::new(id).ok_or(E_INVALIDARG)?;
                    let uri = source_uri(sender)?;
                    let source = state
                        .try_borrow()
                        .map_err(|_| E_UNEXPECTED)?
                        .source()
                        .clone();
                    let native_url = NativeNavigationUrl::capture_bounded(
                        &uri,
                        uri.encode_utf16().count(),
                        &source.retained_credit_account(),
                    )
                    .map_err(|_| E_INVALIDARG)?;
                    let (stamp, document) = {
                        let mut state = state.try_borrow_mut().map_err(|_| E_UNEXPECTED)?;
                        let stamp = state
                            .content_loading(id, &uri, error.as_bool(), Instant::now())
                            .map_err(|_| E_INVALIDARG)?;
                        let document = state.current_document().ok_or(E_UNEXPECTED)?;
                        (stamp, document)
                    };
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    super::managed_test_control::navigation_post::navigation(
                        &state,
                        "content-committed",
                        Some(id.get()),
                        Some(true),
                    );
                    observe(
                        failure.context,
                        OriginalNavigationFact::Committed {
                            stamp: NativeDocumentStamp(stamp),
                            native_url,
                            document,
                        },
                    );
                    Ok(())
                });
                if result.is_err() {
                    let settled = fail_observed(&state, &observe, failure, &failed);
                    result.and(settled)
                } else {
                    result
                }
            })
        })
    }));
    owner.require_live()?;
    owner.loading.add(
        |token| {
            // SAFETY: this attached Cell owns the writable token throughout native entry.
            unsafe { owner.view.add_ContentLoading(&handler, token.as_ptr()) }
        },
        || {
            owner
                .failure
                .latch("native content-loading registration panicked");
        },
    )
}

#[allow(unsafe_code)] // Same creating-STA subscription and actual native getters.
fn install_completed(
    owner: &NavigationCallbacks,
    identity: usize,
    observe: Observation,
) -> windows::core::Result<()> {
    let state = Rc::downgrade(&owner.navigation);
    let sink = owner.failure.clone();
    let handler = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
        let failed = |detail| sink.latch(detail);
        boundary(&failed, || {
            let captured = (sender, args);
            let Some(state) = state.upgrade() else {
                failed("completion callback lost its retained document owner");
                return Err(E_POINTER.into());
            };
            callback(&state, &failed, || {
                let (sender, args) = captured;
                let failure = Cell::new(None);
                let result = boundary(&failed, || {
                    checked_sender(identity, sender.as_ref())?;
                    let args = args.as_ref().ok_or(E_POINTER)?;
                    let mut id = 0;
                    let mut success = BOOL(0);
                    // SAFETY: actual completion args and live stack outputs.
                    unsafe { args.NavigationId(&raw mut id) }?;
                    let id = NonZeroU64::new(id).ok_or(E_INVALIDARG)?;
                    if !state
                        .try_borrow()
                        .map_err(|_| E_UNEXPECTED)?
                        .owns_native_id(id)
                    {
                        return Ok(()); // Late A cannot borrow B's failure correlation.
                    }
                    failure.set(Some(capture_failure(&state)?));
                    // SAFETY: actual native ID is correlated before this fallible getter.
                    unsafe { args.IsSuccess(&raw mut success) }?;
                    let (context, attempt, previous, result) = {
                        let mut state = state.try_borrow_mut().map_err(|_| E_UNEXPECTED)?;
                        let attempt = state.active_attempt();
                        let previous = state.current_document().map(|lease| lease.stamp());
                        (
                            state.host_context(),
                            attempt,
                            previous,
                            state.navigation_completed(id, success.as_bool()),
                        )
                    };
                    match result {
                        Some(Ok(stamp)) => observe(
                            context,
                            OriginalNavigationFact::Finished {
                                stamp: NativeDocumentStamp(stamp),
                            },
                        ),
                        Some(Err(_)) => {
                            if let Some(attempt) = attempt {
                                observe(
                                    context,
                                    OriginalNavigationFact::Failed {
                                        attempt: NativeAttemptObservation(attempt),
                                        revoked: previous,
                                    },
                                );
                            }
                        }
                        None => {}
                    }
                    Ok(())
                });
                if result.is_err()
                    && let Some(failure) = failure.get()
                {
                    let settled = fail_observed(&state, &observe, failure, &failed);
                    result.and(settled)
                } else {
                    result
                }
            })
        })
    }));
    owner.require_live()?;
    owner.completed.add(
        |token| {
            // SAFETY: same actual STA view and attached out-token storage.
            unsafe { owner.view.add_NavigationCompleted(&handler, token.as_ptr()) }
        },
        || {
            owner
                .failure
                .latch("native navigation-completed registration panicked");
        },
    )
}

fn checked_sender(
    identity: usize,
    sender: Option<&ICoreWebView2>,
) -> windows::core::Result<&ICoreWebView2> {
    let sender = sender.ok_or(E_POINTER)?;
    if canonical_webview_identity(sender)? != identity {
        return Err(E_INVALIDARG.into());
    }
    Ok(sender)
}

pub(super) fn callback_credit(
    state: &Rc<RefCell<OriginalNavigation>>,
) -> windows::core::Result<RetainedByteCredit> {
    let source = state
        .try_borrow()
        .map_err(|_| E_UNEXPECTED)?
        .source()
        .clone();
    // The existing ceiling includes the bounded URI, sole-parser descriptor
    // capacity and transient NLS scratch, not just the UTF8 string itself.
    let charge = MAX_PENDING_METADATA_BYTES;
    RetainedByteCredit::reserve(&source.retained_credit_account(), charge)
        .map_err(|_| E_INVALIDARG.into())
}

#[allow(unsafe_code)] // Exact native source getter on the callback's retained STA view.
pub(super) fn source_uri(view: &ICoreWebView2) -> windows::core::Result<String> {
    // SAFETY: view is the identity-checked actual sender. The bounded helper
    // owns/frees its returned COM task allocation and rejects invalid UTF16.
    read_task_text(
        |out| unsafe { view.Source(out) },
        ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
        4 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
    )
}

#[allow(unsafe_code)] // Exact bounded request-header getter, no new native object.
pub(super) fn attempt_header(
    headers: &ICoreWebView2HttpRequestHeaders,
) -> windows::core::Result<Option<String>> {
    let mut present = BOOL(0);
    // SAFETY: native headers and writable BOOL remain alive on the current STA.
    unsafe { headers.Contains(&HSTRING::from(HOST_ATTEMPT_HEADER), &raw mut present) }?;
    if !present.as_bool() {
        return Ok(None);
    }
    // Duplicate/combined or overlong values cannot equal the one 32-byte nonce.
    // SAFETY: same live headers; helper frees the exact returned task allocation.
    read_task_text(
        |out| unsafe { headers.GetHeader(&HSTRING::from(HOST_ATTEMPT_HEADER), out) },
        32,
        32,
    )
    .map(Some)
}

#[allow(unsafe_code)] // Actual NavigationStarting getters/cancellation on the owning STA.
fn starting(
    state: &Rc<RefCell<OriginalNavigation>>,
    identity: usize,
    sender: Option<&ICoreWebView2>,
    args: &ICoreWebView2NavigationStartingEventArgs,
) -> windows::core::Result<(NonZeroU64, bool)> {
    checked_sender(identity, sender)?;
    let _credit = callback_credit(state)?;
    let mut id = 0;
    let mut redirected = BOOL(1);
    // SAFETY: actual callback args with live writable stack outputs.
    unsafe {
        args.NavigationId(&raw mut id)?;
        args.IsRedirected(&raw mut redirected)?;
    }
    let id = NonZeroU64::new(id).ok_or(E_INVALIDARG)?;
    // SAFETY: callback args remain alive; helper strictly bounds the native URI.
    let uri = read_task_text(
        |out| unsafe { args.Uri(out) },
        ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
        4 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
    )?;
    // SAFETY: returned request headers remain within this callback on its STA.
    let header = attempt_header(&unsafe { args.RequestHeaders() }?)?;
    let admitted = state
        .try_borrow_mut()
        .map_err(|_| E_UNEXPECTED)?
        .navigation_starting(
            id,
            &uri,
            header.as_deref(),
            redirected.as_bool(),
            Instant::now(),
        );
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    super::managed_test_control::navigation_post::navigation(
        state,
        "navigation-starting",
        Some(id.get()),
        Some(admitted),
    );
    Ok((id, admitted))
}

#[allow(unsafe_code)] // Deny the same callback while its state hold remains live.
fn cancel_start(
    state: &Rc<RefCell<OriginalNavigation>>,
    args: &ICoreWebView2NavigationStartingEventArgs,
    id: Option<NonZeroU64>,
) -> windows::core::Result<()> {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let cancellation =
        super::original_navigation::cancellation_observation::PendingCancellation::capture(
            state, id,
        );
    // SAFETY: same live callback args, no Rust borrow crosses the COM call.
    let result = unsafe { args.SetCancel(true) };
    let mut state = state.try_borrow_mut().map_err(|_| E_UNEXPECTED)?;
    if result.is_ok() {
        if let Some(id) = id {
            state.page_navigation_cancelled(id);
        }
    } else if let Some(attempt) = state.active_attempt() {
        // Cancellation could not be proven; A must not resume posting.
        state.fail_attempt(attempt);
    }
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    {
        let record = cancellation.map(|before| before.complete(&state, result.is_ok()));
        // Neither the observer's read borrow nor this existing mutation borrow
        // crosses native entry or output. The observation grants no authority.
        drop(state);
        if let Some(record) = record {
            super::original_navigation::cancellation_observation::emit(&record);
        }
    }
    result
}

#[cfg(test)]
mod tests;
