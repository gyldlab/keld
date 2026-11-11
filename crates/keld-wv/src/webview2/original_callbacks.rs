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
use crate::engine::OriginalNavigationFact;
use crate::native_document::NativeAttempt;
use crate::renderer_contract::{NativeAttemptObservation, NativeDocumentStamp};
use crate::resource_intake::{
    MAX_PENDING_METADATA_BYTES, ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit,
};
use crate::resource_source::NativeNavigationUrl;

type Observation = Rc<dyn Fn(OriginalNavigationFact)>;

pub(super) struct NavigationCallbacks {
    view: ICoreWebView2,
    starting: Option<i64>,
    loading: Option<i64>,
    completed: Option<i64>,
}

impl NavigationCallbacks {
    pub(super) fn install(
        view: &ICoreWebView2,
        navigation: &Rc<RefCell<OriginalNavigation>>,
        observe: Observation,
    ) -> windows::core::Result<Self> {
        let identity = canonical_webview_identity(view)?;
        let mut installed = Self {
            view: view.clone(),
            starting: None,
            loading: None,
            completed: None,
        };
        installed.starting = Some(install_starting(
            view,
            navigation,
            identity,
            Rc::clone(&observe),
        )?);
        installed.loading = Some(install_loading(
            view,
            navigation,
            identity,
            Rc::clone(&observe),
        )?);
        installed.completed = Some(install_completed(view, navigation, identity, observe)?);
        Ok(installed)
    }
}

#[allow(unsafe_code)] // Exact callback getters/subscription, retained only on the creating STA.
fn install_starting(
    view: &ICoreWebView2,
    navigation: &Rc<RefCell<OriginalNavigation>>,
    identity: usize,
    observe: Observation,
) -> windows::core::Result<i64> {
    let state = Rc::downgrade(navigation);
    let handler = NavigationStartingEventHandler::create(Box::new(move |sender, args| {
        let Some(args) = args else {
            return Err(E_POINTER.into());
        };
        let Some(state) = state.upgrade() else {
            // SAFETY: live event args; a released owner never admits navigation.
            return unsafe { args.SetCancel(true) };
        };
        let failure = capture_failure(&state)?;
        let result = callback(&state, || {
            let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                starting(&state, identity, sender.as_ref(), &args)
            }))
            .unwrap_or_else(|_| Err(E_UNEXPECTED.into()));
            if !matches!(&observed, Ok((_, true))) {
                let id = observed.as_ref().ok().map(|(id, _)| *id);
                cancel_start(&state, &args, id)?;
            }
            observed.map(|_| ())
        });
        let retired = failure.attempt.is_some()
            && state
                .try_borrow()
                .is_ok_and(|state| state.active_attempt() != failure.attempt);
        if retired {
            // Denying an owned start or failing cancellation settles that exact
            // Started attempt. A successfully canceled page request preserves A.
            fail_observed(&state, &observe, failure);
        }
        result
    }));
    let mut token = 0;
    // SAFETY: view and out-token are live on their owning STA. The native
    // view retains the handler; partial installation is removed by Drop.
    unsafe { view.add_NavigationStarting(&handler, &raw mut token) }?;
    Ok(token)
}

#[allow(unsafe_code)] // Exact callback getters/subscription, retained only on the creating STA.
fn install_loading(
    view: &ICoreWebView2,
    navigation: &Rc<RefCell<OriginalNavigation>>,
    identity: usize,
    observe: Observation,
) -> windows::core::Result<i64> {
    let mut token = 0;
    let state = Rc::downgrade(navigation);
    let observations = Rc::clone(&observe);
    let handler = ContentLoadingEventHandler::create(Box::new(move |sender, args| {
        let state = state.upgrade().ok_or(E_POINTER)?;
        let failure = capture_failure(&state)?;
        let result = callback(&state, || {
            let sender = checked_sender(identity, sender.as_ref())?;
            let args = args.ok_or(E_POINTER)?;
            let _credit = callback_credit(&state)?;
            let mut id = 0;
            let mut error = BOOL(1);
            // SAFETY: callback-owned args and valid stack outputs on this STA.
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
            observations(OriginalNavigationFact::Committed {
                stamp: NativeDocumentStamp(stamp),
                native_url,
                document,
            });
            Ok(())
        });
        if result.is_err() {
            fail_observed(&state, &observations, failure);
        }
        result
    }));
    // SAFETY: same retained creating-STA view and a live writable event token.
    unsafe { view.add_ContentLoading(&handler, &raw mut token) }?;
    Ok(token)
}

#[allow(unsafe_code)] // Exact callback getters/subscription, retained only on the creating STA.
fn install_completed(
    view: &ICoreWebView2,
    navigation: &Rc<RefCell<OriginalNavigation>>,
    identity: usize,
    observe: Observation,
) -> windows::core::Result<i64> {
    let mut token = 0;
    let state = Rc::downgrade(navigation);
    let handler = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
        let state = state.upgrade().ok_or(E_POINTER)?;
        let failure = Cell::new(None);
        let result = callback(&state, || {
            checked_sender(identity, sender.as_ref())?;
            let args = args.ok_or(E_POINTER)?;
            let mut id = 0;
            let mut success = BOOL(0);
            // SAFETY: actual native completion args and stack outputs on this STA.
            unsafe { args.NavigationId(&raw mut id) }?;
            let id = NonZeroU64::new(id).ok_or(E_INVALIDARG)?;
            if !state
                .try_borrow()
                .map_err(|_| E_UNEXPECTED)?
                .owns_native_id(id)
            {
                return Ok(()); // Late A never borrows B's failure correlation.
            }
            failure.set(Some(capture_failure(&state)?));
            // SAFETY: the actual completion is now correlated before this getter.
            unsafe { args.IsSuccess(&raw mut success) }?;
            let (attempt, previous, result) = {
                let mut state = state.try_borrow_mut().map_err(|_| E_UNEXPECTED)?;
                let attempt = state.active_attempt();
                let previous = state.current_document().map(|lease| lease.stamp());
                (
                    attempt,
                    previous,
                    state.navigation_completed(id, success.as_bool()),
                )
            };
            match result {
                Some(Ok(stamp)) => observe(OriginalNavigationFact::Finished {
                    stamp: NativeDocumentStamp(stamp),
                }),
                Some(Err(_)) => {
                    if let Some(attempt) = attempt {
                        observe(OriginalNavigationFact::Failed {
                            attempt: NativeAttemptObservation(attempt),
                            revoked: previous,
                        });
                    }
                }
                None => {}
            }
            Ok(())
        });
        if result.is_err()
            && let Some(failure) = failure.get()
        {
            fail_observed(&state, &observe, failure);
        }
        result
    }));
    // SAFETY: same creating-STA view retains the callback until removal/close.
    unsafe { view.add_NavigationCompleted(&handler, &raw mut token) }?;
    Ok(token)
}

impl Drop for NavigationCallbacks {
    #[allow(unsafe_code)] // Remove only this owner's successful registrations on its STA.
    fn drop(&mut self) {
        // SAFETY: the retained view outlives these exact native subscription tokens.
        // Controller Close is the final fallback if the runtime rejects removal.
        unsafe {
            if let Some(token) = self.starting.take() {
                let _ = self.view.remove_NavigationStarting(token);
            }
            if let Some(token) = self.loading.take() {
                let _ = self.view.remove_ContentLoading(token);
            }
            if let Some(token) = self.completed.take() {
                let _ = self.view.remove_NavigationCompleted(token);
            }
        }
    }
}

struct CallbackTurn(Rc<RefCell<OriginalNavigation>>);

impl CallbackTurn {
    fn enter(state: &Rc<RefCell<OriginalNavigation>>) -> windows::core::Result<Self> {
        state
            .try_borrow_mut()
            .map_err(|_| E_UNEXPECTED)?
            .enter_navigation_callback();
        Ok(Self(Rc::clone(state)))
    }
}

impl Drop for CallbackTurn {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.try_borrow_mut() {
            state.leave_navigation_callback();
        }
    }
}

fn callback(
    state: &Rc<RefCell<OriginalNavigation>>,
    run: impl FnOnce() -> windows::core::Result<()>,
) -> windows::core::Result<()> {
    // Pure state hold precedes QI/getters; no Rust borrow crosses native entry.
    let _turn = CallbackTurn::enter(state)?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
        .unwrap_or_else(|_| Err(E_UNEXPECTED.into()))
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
    Ok((id, admitted))
}

#[allow(unsafe_code)] // Deny the same callback while its state hold remains live.
fn cancel_start(
    state: &Rc<RefCell<OriginalNavigation>>,
    args: &ICoreWebView2NavigationStartingEventArgs,
    id: Option<NonZeroU64>,
) -> windows::core::Result<()> {
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
    result
}

#[derive(Clone, Copy)]
struct FailureObservation {
    attempt: Option<NativeAttempt>,
    previous: Option<NativeDocumentStamp>,
}

fn capture_failure(
    state: &Rc<RefCell<OriginalNavigation>>,
) -> windows::core::Result<FailureObservation> {
    let state = state.try_borrow().map_err(|_| E_UNEXPECTED)?;
    Ok(FailureObservation {
        attempt: state.active_attempt(),
        previous: state.current_document().map(|lease| lease.stamp()),
    })
}

fn fail_observed(
    state: &Rc<RefCell<OriginalNavigation>>,
    observe: &Observation,
    failure: FailureObservation,
) {
    let Ok(mut navigation) = state.try_borrow_mut() else {
        return;
    };
    let previous = failure.previous;
    if let Some(attempt) = failure.attempt {
        navigation.fail_attempt(attempt);
        drop(navigation);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            observe(OriginalNavigationFact::Failed {
                attempt: NativeAttemptObservation(attempt),
                revoked: previous,
            });
        }));
    } else {
        drop(navigation);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            observe(OriginalNavigationFact::Revoked { previous });
        }));
    }
}

#[cfg(test)]
mod tests;
