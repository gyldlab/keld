//! GH542 native load/commit ownership for the one GH473 original editor.
//!
//! A transparent `WKNavigationDelegate` forwards Wry's existing seven selectors
//! and records real Host-owned loadRequest `WKNavigation` objects. It never
//! synthesizes a commit from URL equality, provisional start, page data or Bun.
//! The same `NativeDocumentOwner` is read by app/preload/named/eval owners later.

#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSString, NSURL, NSURLRequest};
use objc2_web_kit::{
    WKDownload, WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
    WKNavigationResponse, WKNavigationResponsePolicy, WKWebView,
};

use super::macos_document::{
    NativeAttempt, NativeCommittedStamp, NativeDocumentOwner, RetainedNavigationWitness,
};
use super::macos_resource_intake::ORIGINAL_WEBAPP_LIMITS;
use super::macos_resource_source::{AdmittedWebAppSource, MacNavigationUrl};
use super::{MacNativeAttempt, MacNativeDocumentLease, MacNativeDocumentStamp};
use crate::WebviewId;
use crate::engine::{
    AppWindowEvent, MacOriginalNavigationFact, NativeNavigationContext, NativeNavigationRefusal,
};
use crate::error::WvError;

/// Optional composition with the existing UI-owned original preload bridge.
/// The hook receives actual native commit evidence and must recheck the same
/// shared document owner before binding its world credentials.
pub(super) type NativeCommitHook =
    Rc<dyn Fn(&WKWebView, MacNativeDocumentStamp) -> Result<(), WvError>>;

/// Prepare bridge-owned document-start bootstrap for the exact admitted native
/// attempt, after old-document revocation and before the actual native load.
/// This callback cannot mint a commit or activate document credentials.
pub(super) type NativeAttemptHook = Rc<dyn Fn(&WKWebView, MacNativeAttempt) -> Result<(), WvError>>;

/// The WK property is weak: NativeBuild/View must retain this proxy for the
/// entire real `WKWebView` lifetime. Keep Wry's original delegate strongly
/// retained as well, so policy/download/process callbacks still run.
pub(super) struct NavigationIvars {
    view: WebviewId,
    expected_webview: usize,
    original: Retained<ProtocolObject<dyn WKNavigationDelegate>>,
    document: Rc<RefCell<NativeDocumentOwner>>,
    source: Arc<AdmittedWebAppSource>,
    actual_load: RefCell<Option<NativeLoad>>,
    finished: Cell<Option<NativeAttempt>>,
    observations: Sender<AppWindowEvent>,
    commit_hook: Option<NativeCommitHook>,
    attempt_hook: Option<NativeAttemptHook>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fixture_native_load_calls: Cell<u32>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fixture_native_failures: RefCell<FixtureNativeFailures>,
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[derive(Clone, Copy, Debug)]
enum FixtureNativeFailurePhase {
    Provisional,
    Committed,
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
impl FixtureNativeFailurePhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Provisional => "provisional",
            Self::Committed => "committed",
        }
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[derive(Clone, Copy, Debug)]
struct FixtureNativeFailure {
    phase: FixtureNativeFailurePhase,
    code: isize,
    navigation_present: bool,
    receiving_view_matches: bool,
    retained_navigation_matches: bool,
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[derive(Default)]
struct FixtureNativeFailures {
    entries: [Option<FixtureNativeFailure>; 8],
    next: usize,
    observed: u64,
    overwritten: u64,
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
impl FixtureNativeFailures {
    fn record(&mut self, observation: FixtureNativeFailure) {
        self.observed = self.observed.saturating_add(1);
        if self.entries[self.next].is_some() {
            self.overwritten = self.overwritten.saturating_add(1);
        }
        self.entries[self.next] = Some(observation);
        self.next = (self.next + 1) % self.entries.len();
    }
    fn ordered(&self) -> impl Iterator<Item = FixtureNativeFailure> + '_ {
        (0..self.entries.len())
            .filter_map(|offset| self.entries[(self.next + offset) % self.entries.len()])
    }
}

// Immutable original completion context belongs to this exact retained load.
struct NativeLoad {
    context: NativeNavigationContext,
    witness: RetainedNavigationWitness,
}

// Cold, native-owner preparation only. This function neither mints a document
// nor revokes an old one, and receives only the retained source's selected URI.
fn native_entry_request(entry: &str) -> Result<Retained<NSURLRequest>, WvError> {
    let url = NSURL::URLWithString(&NSString::from_str(entry))
        .ok_or_else(|| WvError::Navigate(String::from("verified entry is not a native NSURL")))?;
    // Measure Foundation's actual serialized absolute URL, after any Unicode
    // or space escaping. Reuse the native resource-intake owner's full budget.
    let actual = url.absoluteString().ok_or_else(|| {
        WvError::Navigate(String::from("verified entry has no native absolute URL"))
    })?;
    let units = actual.length();
    if units == 0 || units > ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units {
        return Err(WvError::Navigate(String::from(
            "verified entry exceeds the native resource URL budget before navigation",
        )));
    }
    Ok(NSURLRequest::requestWithURL(&url))
}

define_class!(
    // SAFETY: MainThreadOnly NSObject retains its original native delegate,
    // source and checked document owner; no background/foreign WK object.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = NavigationIvars]
    pub(super) struct KeldNavigationOwner;

    // SAFETY: inherited NSObject protocol is implemented by NSObject.
    unsafe impl NSObjectProtocol for KeldNavigationOwner {}

    // SAFETY: objc2-web-kit 0.3.2 native selector signatures. The seven
    // Wry-owned callbacks are forwarded to the previously installed exact
    // original delegate, retaining media/profile/navigation/download policy.
    unsafe impl WKNavigationDelegate for KeldNavigationOwner {
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn navigation_action(
            &self,
            view: &WKWebView,
            action: &WKNavigationAction,
            handler: &block2::Block<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            // SAFETY: original Wry delegate implements this exact selector,
            // still retained on the UI thread by this proxy and Wry's webview.
            unsafe {
                let _: () = msg_send![
                    &*self.ivars().original,
                    webView: view,
                    decidePolicyForNavigationAction: action,
                    decisionHandler: handler
                ];
            }
        }

        #[unsafe(method(webView:decidePolicyForNavigationResponse:decisionHandler:))]
        fn navigation_response(
            &self,
            view: &WKWebView,
            response: &WKNavigationResponse,
            handler: &block2::Block<dyn Fn(WKNavigationResponsePolicy)>,
        ) {
            // SAFETY: exact Wry response decision selector, original lifetime
            // retained; no Wry security decision is replaced or bypassed.
            unsafe {
                let _: () = msg_send![
                    &*self.ivars().original,
                    webView: view,
                    decidePolicyForNavigationResponse: response,
                    decisionHandler: handler
                ];
            }
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        fn did_commit(&self, view: &WKWebView, navigation: Option<&WKNavigation>) {
            let Some(navigation) = navigation else {
                self.record_missing_navigation(view);
                return;
            };
            self.record_native_commit(view, navigation);
            self.forward_commit(view, navigation);
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, view: &WKWebView, navigation: Option<&WKNavigation>) {
            let Some(navigation) = navigation else {
                self.record_missing_navigation(view);
                return;
            };
            self.record_native_finish(view, navigation);
            self.forward_finish(view, navigation);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional(
            &self,
            view: &WKWebView,
            navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.fixture_observe_native_failure(
                view,
                navigation,
                error,
                FixtureNativeFailurePhase::Provisional,
            );
            #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
            let _ = error;
            self.record_native_fail(view, navigation);
            // Wry 0.56.1 does not implement this optional selector; no
            // forwarding call or guessed error-handling fallback.
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail_committed(
            &self,
            view: &WKWebView,
            navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.fixture_observe_native_failure(
                view,
                navigation,
                error,
                FixtureNativeFailurePhase::Committed,
            );
            #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
            let _ = error;
            self.record_native_fail(view, navigation);
            // Wry's original optional delegate does not implement this method.
        }

        #[unsafe(method(webView:navigationAction:didBecomeDownload:))]
        fn navigation_download_action(
            &self,
            view: &WKWebView,
            action: &WKNavigationAction,
            download: &WKDownload,
        ) {
            // SAFETY: Wry retains and implements its download owner; we forward
            // unchanged to preserve caller policy and progress.
            unsafe {
                let _: () = msg_send![
                    &*self.ivars().original,
                    webView: view,
                    navigationAction: action,
                    didBecomeDownload: download
                ];
            }
        }

        #[unsafe(method(webView:navigationResponse:didBecomeDownload:))]
        fn navigation_download_response(
            &self,
            view: &WKWebView,
            response: &WKNavigationResponse,
            download: &WKDownload,
        ) {
            // SAFETY: same response-to-download selector and original delegate.
            unsafe {
                let _: () = msg_send![
                    &*self.ivars().original,
                    webView: view,
                    navigationResponse: response,
                    didBecomeDownload: download
                ];
            }
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn content_process_died(&self, view: &WKWebView) {
            if self.owns(view) {
                self.revoke_unexpected();
                self.ivars().actual_load.borrow_mut().take();
                self.ivars().finished.set(None);
            }
            // SAFETY: native content-process termination is forwarded to
            // Wry's same native delegate so its existing notifier still runs.
            unsafe {
                let _: () = msg_send![
                    &*self.ivars().original,
                    webViewWebContentProcessDidTerminate: view
                ];
            }
        }
    }
);

impl KeldNavigationOwner {
    /// Wrap Wry's real installed navigation delegate after the guarded empty
    /// facade `WKWebView` exists. Original delegate remains live/retained;
    /// create exactly one wrapper per native view on the UI owner.
    #[allow(unsafe_code)] // Read/set WK weak delegate, one MainThreadOnly init.
    pub(super) fn install(
        view_id: WebviewId,
        view: &WKWebView,
        source: Arc<AdmittedWebAppSource>,
        document: Rc<RefCell<NativeDocumentOwner>>,
        observations: Sender<AppWindowEvent>,
        commit_hook: Option<NativeCommitHook>,
        attempt_hook: Option<NativeAttemptHook>,
    ) -> Result<Retained<Self>, WvError> {
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            WvError::Webview(String::from(
                "native GH542 delegate needs AppKit main thread",
            ))
        })?;
        // SAFETY: live Wry-owned WKWebView on its UI thread. Returned
        // original is a RETAINED native protocol object, no dangling borrow.
        let original = unsafe { view.navigationDelegate() }.ok_or_else(|| {
            WvError::Webview(String::from(
                "Wry native navigation delegate missing; preserve policy owner",
            ))
        })?;
        let fresh = mtm.alloc::<Self>().set_ivars(NavigationIvars {
            view: view_id,
            expected_webview: std::ptr::from_ref(view) as usize,
            original,
            source,
            document,
            actual_load: RefCell::new(None),
            finished: Cell::new(None),
            observations,
            commit_hook,
            attempt_hook,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            fixture_native_load_calls: Cell::new(0),
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            fixture_native_failures: RefCell::new(FixtureNativeFailures::default()),
        });
        // SAFETY: native NSObject super init after all required ivars.
        let proxy: Retained<Self> = unsafe { msg_send![super(fresh), init] };
        // SAFETY: WKWebView.navigationDelegate is weak; View retains the
        // proxy. No page/world can replace it through this private API.
        unsafe { view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*proxy))) };
        Ok(proxy)
    }

    fn owns(&self, view: &WKWebView) -> bool {
        self.ivars().expected_webview == std::ptr::from_ref(view) as usize
    }

    fn publish(&self, context: NativeNavigationContext, fact: MacOriginalNavigationFact) -> bool {
        if self
            .ivars()
            .observations
            .send(AppWindowEvent::MacOriginalNavigation { context, fact })
            .is_err()
        {
            // Core cannot consume a result. Revoke native authority; do not
            // treat successful WK work or an unobserved reply as app success.
            self.ivars().document.borrow_mut().destroy();
            self.ivars().actual_load.borrow_mut().take();
            self.ivars().finished.set(None);
            eprintln!(
                "KELD-WV-004: native observation channel closed; native document revoked; end the app session before retrying"
            );
            return false;
        }
        true
    }

    fn revoke_unexpected(&self) {
        let accepted = self
            .ivars()
            .actual_load
            .borrow()
            .as_ref()
            .map(|load| (load.context, load.witness.attempt));
        let previous = self.ivars().document.borrow().current_stamp();
        self.ivars()
            .document
            .borrow_mut()
            .unexpected_mainframe_commit();
        if let Some((context, attempt)) = accepted {
            self.publish(
                context,
                MacOriginalNavigationFact::Failed {
                    attempt: MacNativeAttempt(attempt),
                    revoked: previous.map(MacNativeDocumentStamp),
                },
            );
        }
    }

    /// A stamped event is historical evidence only. Each native operation
    /// must perform this exact live-owner test on the UI turn that uses it.
    #[allow(dead_code)] // GH484/GH462 native page/eval admission consumers
    pub(super) fn still_current(&self, expected: MacNativeDocumentStamp) -> bool {
        self.ivars().document.borrow().current_stamp() == Some(expected.0)
    }

    /// Debug fixture observation of actual loadRequest invocation cuts.
    /// Zero proves no call was made by this owner; it grants no authority and
    /// says nothing about completion. Saturation keeps a used owner nonzero.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn fixture_native_load_calls(&self) -> u32 {
        self.ivars().fixture_native_load_calls.get()
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fn fixture_observe_native_failure(
        &self,
        view: &WKWebView,
        navigation: Option<&WKNavigation>,
        error: &NSError,
        phase: FixtureNativeFailurePhase,
    ) {
        let receiving_view_matches = self.owns(view);
        let retained_navigation_matches = receiving_view_matches
            && navigation.is_some_and(|native| {
                self.ivars()
                    .actual_load
                    .borrow()
                    .as_ref()
                    .is_some_and(|load| load.witness.matches(self.ivars().view, native))
            });
        // Borrowed callback NSError: sanctioned scalar read only. No native
        // object, pointer, URL, userInfo or text is retained in this census.
        let code = error.code();
        self.ivars()
            .fixture_native_failures
            .borrow_mut()
            .record(FixtureNativeFailure {
                phase,
                code,
                navigation_present: navigation.is_some(),
                receiving_view_matches,
                retained_navigation_matches,
            });
    }

    /// Bounded actual callback observations, separate from correlated facts.
    /// Raw error code lacks domain/text and never determines native authority.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn fixture_native_failures(&self) -> serde_json::Value {
        let census = self.ivars().fixture_native_failures.borrow();
        let entries = census.ordered().map(|entry| serde_json::json!({
            "phase":entry.phase.as_str(),"code":entry.code,
            "navigation_present":entry.navigation_present,"receiving_view_matches":entry.receiving_view_matches,
            "retained_navigation_matches":entry.retained_navigation_matches,
        })).collect::<Vec<_>>();
        serde_json::json!({"callback_count":census.observed,"overwritten":census.overwritten,"capacity":census.entries.len(),"entries":entries})
    }

    /// Run one accepted Host operation on the actual UI-owned WK view.
    /// Normal refusals and native failures are correlated observations, never
    /// a fatal command error or an app-visible load-success acknowledgement.
    #[allow(unsafe_code)] // Same actual WK loadRequest operation and retained result.
    pub(super) fn load_verified_entry(
        &self,
        view: &WKWebView,
        context: NativeNavigationContext,
        original_query: &str,
    ) {
        let Some(request) = self.prepare_entry_request(view, context, original_query) else {
            return;
        };
        let Some(attempt) = self.begin_prepared_load(view, context) else {
            return;
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.ivars().fixture_native_load_calls.set(
            self.ivars()
                .fixture_native_load_calls
                .get()
                .saturating_add(1),
        );
        // SAFETY: same verified source/request, UI-owned WK view and one
        // actual NativeDocumentOwner attempt; context is completion data only.
        let actual = unsafe { view.loadRequest(&request) };
        let Some(actual) = actual else {
            self.ivars().document.borrow_mut().fail_attempt(attempt);
            self.publish(
                context,
                MacOriginalNavigationFact::Failed {
                    attempt: MacNativeAttempt(attempt),
                    revoked: None,
                },
            );
            return;
        };
        let Some(witness) =
            RetainedNavigationWitness::from_host_load_result(self.ivars().view, attempt, &actual)
        else {
            self.ivars().document.borrow_mut().fail_attempt(attempt);
            self.publish(
                context,
                MacOriginalNavigationFact::Failed {
                    attempt: MacNativeAttempt(attempt),
                    revoked: None,
                },
            );
            return;
        };
        // Reentrant foreign commit before the retained witness exists must
        // already have failed this attempt closed; never repair from a URL.
        if self
            .ivars()
            .document
            .borrow()
            .resource_lease(attempt, self.ivars().source.source_id())
            .is_none()
        {
            self.publish(
                context,
                MacOriginalNavigationFact::Failed {
                    attempt: MacNativeAttempt(attempt),
                    revoked: None,
                },
            );
            return;
        }
        self.ivars()
            .actual_load
            .borrow_mut()
            .replace(NativeLoad { context, witness });
        self.publish(
            context,
            MacOriginalNavigationFact::Started {
                attempt: MacNativeAttempt(attempt),
            },
        );
    }

    fn prepare_entry_request(
        &self,
        view: &WKWebView,
        context: NativeNavigationContext,
        original_query: &str,
    ) -> Option<Retained<NSURLRequest>> {
        if context.view() != self.ivars().view || !self.owns(view) {
            self.publish(
                context,
                MacOriginalNavigationFact::Refused {
                    error: NativeNavigationRefusal::UnknownView,
                },
            );
            return None;
        }
        let Some(entry) = self.ivars().source.admitted_entry_url(original_query) else {
            self.publish(
                context,
                MacOriginalNavigationFact::Refused {
                    error: NativeNavigationRefusal::InvalidQuery,
                },
            );
            return None;
        };
        let Ok(request) = native_entry_request(&entry) else {
            self.publish(
                context,
                MacOriginalNavigationFact::Refused {
                    error: NativeNavigationRefusal::NativeUrlRejected,
                },
            );
            return None;
        };
        Some(request)
    }

    fn begin_prepared_load(
        &self,
        view: &WKWebView,
        context: NativeNavigationContext,
    ) -> Option<NativeAttempt> {
        // Everything above is preparation: existing live state is unchanged.
        let previous = self.ivars().document.borrow().current_stamp();
        let old_context = self
            .ivars()
            .actual_load
            .borrow()
            .as_ref()
            .map(|load| load.context);
        let admitted = self
            .ivars()
            .document
            .borrow_mut()
            .begin_host_load(self.ivars().source.source_id());
        let Ok(attempt) = admitted else {
            self.publish(
                context,
                MacOriginalNavigationFact::Refused {
                    error: NativeNavigationRefusal::Unavailable,
                },
            );
            return None;
        };
        if previous.is_some()
            && let Some(old_context) = old_context
            && !self.publish(
                old_context,
                MacOriginalNavigationFact::Revoked {
                    previous: previous.map(MacNativeDocumentStamp),
                },
            )
        {
            return None;
        }
        self.ivars().actual_load.borrow_mut().take();
        self.ivars().finished.set(None);
        if let Some(hook) = self.ivars().attempt_hook.as_ref()
            && let Err(error) = hook(view, MacNativeAttempt(attempt))
        {
            eprintln!("{error}");
            self.ivars().document.borrow_mut().fail_attempt(attempt);
            self.publish(
                context,
                MacOriginalNavigationFact::Failed {
                    attempt: MacNativeAttempt(attempt),
                    revoked: None,
                },
            );
            return None;
        }
        // The callback runs without a document/actual-load RefCell borrow.
        // Reentrant destruction/replacement must not start this old request.
        if self
            .ivars()
            .document
            .borrow()
            .resource_lease(attempt, self.ivars().source.source_id())
            .is_none()
        {
            self.ivars().document.borrow_mut().fail_attempt(attempt);
            self.publish(
                context,
                MacOriginalNavigationFact::Failed {
                    attempt: MacNativeAttempt(attempt),
                    revoked: None,
                },
            );
            return None;
        }
        Some(attempt)
    }

    /// `WebKit` can omit the navigation for back/forward-cache callbacks
    /// (NavigationState.mm at f938431b42f442a4763090f03ca96ac2d0fd631e).
    /// No retained witness can corroborate such a callback, so retire this
    /// view's document before returning. Wry's non-null callback must not run.
    fn record_missing_navigation(&self, view: &WKWebView) {
        if !self.owns(view) {
            return;
        }
        self.revoke_unexpected();
        self.ivars().actual_load.borrow_mut().take();
        self.ivars().finished.set(None);
    }

    /// Native callback evidence; a same-URL unexpected commit is denied.
    fn record_native_commit(&self, view: &WKWebView, native: &WKNavigation) {
        if !self.owns(view) {
            return;
        }
        let expected = self.ivars().actual_load.borrow();
        let Some(witness) = expected
            .as_ref()
            .filter(|load| load.witness.matches(self.ivars().view, native))
        else {
            drop(expected);
            self.revoke_unexpected();
            return;
        };
        let Some(identity) = witness.witness.identity() else {
            drop(expected);
            self.revoke_unexpected();
            return;
        };
        let attempt = witness.witness.attempt;
        let context = witness.context;
        drop(expected);
        // SAFETY: UI-owned native WKWebView actual committed URL (post-redirect)
        // is corroborated with the selected original entry and source object.
        let actual_url = unsafe { view.URL() }.and_then(|url| url.absoluteString());
        let Some(actual_url) = actual_url else {
            self.revoke_unexpected();
            return;
        };
        let Ok(actual_url) =
            MacNavigationUrl::capture(&actual_url, &self.ivars().source.retained_credit_account())
        else {
            self.record_native_fail(view, Some(native));
            return;
        };
        if self
            .ivars()
            .source
            .qualify_document_url(actual_url.as_str())
            .is_err()
        {
            self.revoke_unexpected();
            return;
        }
        // This is ONE native mint: page URL and Bun/JS cannot select it.
        let previous = self.ivars().document.borrow().current_stamp();
        let result = self.ivars().document.borrow_mut().commit_admitted(
            attempt,
            self.ivars().source.source_id(),
            identity,
        );
        match result {
            Ok(stamp) if previous != Some(stamp) => {
                let observed = MacNativeDocumentStamp(stamp);
                let lease = self.ivars().document.borrow().lease_for(stamp);
                let Some(lease) =
                    lease.filter(|lease| lease.belongs_to_source(&self.ivars().source))
                else {
                    self.record_native_fail(view, Some(native));
                    return;
                };
                let document = MacNativeDocumentLease(lease);
                if let Some(hook) = self.ivars().commit_hook.as_ref() {
                    if let Err(error) = hook(view, observed) {
                        eprintln!("{error}");
                        self.record_native_fail(view, Some(native));
                        return;
                    }
                    if !self.still_current(observed) {
                        self.record_native_fail(view, Some(native));
                        return;
                    }
                }
                if !document.is_current() || !self.still_current(observed) {
                    self.record_native_fail(view, Some(native));
                    return;
                }
                self.publish(
                    context,
                    MacOriginalNavigationFact::Committed {
                        stamp: MacNativeDocumentStamp(stamp),
                        native_url: actual_url,
                        document,
                    },
                );
            }
            Ok(_) => {}
            Err(_) => {
                if previous.is_some() && self.ivars().document.borrow().current_stamp() != previous
                {
                    self.publish(
                        context,
                        MacOriginalNavigationFact::Revoked {
                            previous: previous.map(MacNativeDocumentStamp),
                        },
                    );
                }
            }
        }
    }

    fn record_native_finish(&self, view: &WKWebView, navigation: &WKNavigation) {
        if !self.owns(view) {
            return;
        }
        let owned = self.ivars().actual_load.borrow();
        let Some(load) = owned
            .as_ref()
            .filter(|load| load.witness.matches(self.ivars().view, navigation))
        else {
            return;
        };
        let context = load.context;
        let attempt = load.witness.attempt;
        let identity = load.witness.identity();
        drop(owned);
        let committed = self.ivars().document.borrow().current_stamp();
        if committed
            .is_some_and(|stamp| stamp.attempt == attempt && Some(stamp.native_commit) == identity)
            && self.ivars().finished.get() != Some(attempt)
        {
            self.ivars().finished.set(Some(attempt));
            if let Some(stamp) = committed {
                self.publish(
                    context,
                    MacOriginalNavigationFact::Finished {
                        stamp: MacNativeDocumentStamp(stamp),
                    },
                );
            }
        }
    }

    fn record_native_fail(&self, view: &WKWebView, navigation: Option<&WKNavigation>) {
        if !self.owns(view) {
            return;
        }
        let owned = self.ivars().actual_load.borrow();
        let Some(witness) = owned.as_ref() else {
            return;
        };
        if navigation.is_some_and(|nav| witness.witness.matches(self.ivars().view, nav)) {
            let attempt = witness.witness.attempt;
            let context = witness.context;
            drop(owned);
            let previous = self.ivars().document.borrow().current_stamp();
            self.ivars().document.borrow_mut().fail_attempt(attempt);
            self.ivars().actual_load.borrow_mut().take();
            self.ivars().finished.set(None);
            self.publish(
                context,
                MacOriginalNavigationFact::Failed {
                    attempt: MacNativeAttempt(attempt),
                    revoked: previous.map(MacNativeDocumentStamp),
                },
            );
        }
        // An unrelated renderer provisional cancellation is not a matching
        // Host load failure; GH542 current committed B stays usable.
    }

    pub(super) fn current_committed(&self) -> Option<NativeCommittedStamp> {
        self.ivars().document.borrow().current_stamp()
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn finished_current_attempt(&self) -> Option<NativeAttempt> {
        let attempt = self.ivars().finished.get()?;
        self.ivars()
            .document
            .borrow()
            .current_stamp()
            .and_then(|stamp| (stamp.attempt == attempt).then_some(attempt))
    }

    #[allow(unsafe_code)] // Native Wry-delegate exact method forward only.
    fn forward_commit(&self, view: &WKWebView, navigation: &WKNavigation) {
        // SAFETY: Wry original implements this exact WKNavigationDelegate
        // selector and was retained from this same native WebView.
        unsafe {
            let _: () = msg_send![
                &*self.ivars().original,
                webView: view,
                didCommitNavigation: navigation
            ];
        }
    }

    #[allow(unsafe_code)] // Native Wry-delegate exact method forward only.
    fn forward_finish(&self, view: &WKWebView, navigation: &WKNavigation) {
        // SAFETY: Wry original implements exact didFinishNavigation; its native
        // page-load callbacks keep their existing behavior and ordering.
        unsafe {
            let _: () = msg_send![
                &*self.ivars().original,
                webView: view,
                didFinishNavigation: navigation
            ];
        }
    }

    /// Same-turn conditional effect cut for one Core-resolved original lease.
    /// No native call/callback occurs between validation, atomic claim and
    /// irreversible document retirement. Refusal changes no native owner.
    pub(super) fn try_admit_current_document_on_ui(
        &self,
        view: &WKWebView,
        selected_source: &Arc<AdmittedWebAppSource>,
        expected: &MacNativeDocumentLease,
        admission: &super::OriginalDocumentDestroyAdmission,
    ) -> bool {
        if !self.owns(view)
            || expected.stamp().view() != self.ivars().view
            || !Arc::ptr_eq(selected_source, &self.ivars().source)
            || !expected.is_current()
            || !expected.belongs_to_source(selected_source)
        {
            return false;
        }
        let stamp = expected.0.stamp();
        let document = self.ivars().document.borrow();
        if document.current_stamp() != Some(stamp)
            || document
                .lease_for(stamp)
                .is_none_or(|actual| !actual.same_binding(&expected.0))
            || document
                .source_binding()
                .is_none_or(|source| !Arc::ptr_eq(&source, selected_source))
        {
            return false;
        }
        let actual = self.ivars().actual_load.borrow();
        if actual.as_ref().is_none_or(|load| {
            load.witness.view != stamp.view
                || load.witness.attempt != stamp.attempt
                || load.witness.identity() != Some(stamp.native_commit)
        }) {
            return false;
        }
        drop(actual);
        drop(document);
        if !admission.try_accept_on_ui() {
            return false;
        }
        // All document mutation is UI-only; no callback or native operation
        // can interleave with these checks and the existing irreversible cut.
        self.destroy();
        true
    }

    /// Retire this exact operation before native destruction/source retirement.
    pub(super) fn destroy(&self) {
        let previous = self.current_committed();
        let context = self
            .ivars()
            .actual_load
            .borrow_mut()
            .take()
            .map(|load| load.context);
        self.ivars().finished.set(None);
        self.ivars().document.borrow_mut().destroy();
        if previous.is_some()
            && let Some(context) = context
        {
            self.publish(
                context,
                MacOriginalNavigationFact::Revoked {
                    previous: previous.map(MacNativeDocumentStamp),
                },
            );
        }
    }
}

// Type checking is the oracle at the FFI entry: the SDK permits a missing
// navigation argument before any Rust reference may be formed. These fail
// to compile if either entry regresses to a non-null &WKNavigation.
#[cfg(test)]
const _: for<'owner, 'view, 'navigation> extern "C-unwind" fn(
    &'owner KeldNavigationOwner,
    objc2::runtime::Sel,
    &'view WKWebView,
    Option<&'navigation WKNavigation>,
) = KeldNavigationOwner::did_commit;

#[cfg(test)]
const _: for<'owner, 'view, 'navigation> extern "C-unwind" fn(
    &'owner KeldNavigationOwner,
    objc2::runtime::Sel,
    &'view WKWebView,
    Option<&'navigation WKNavigation>,
) = KeldNavigationOwner::did_finish;

#[cfg(test)]
mod native_entry_request_tests {
    use super::super::macos_resource_intake::{
        NativeRequestShape, ORIGINAL_WEBAPP_LIMITS, check_shape,
    };
    use super::*;

    const ENTRY: &str = "keld-resource://0123456789abcdef0123456789abcdef/index.html";

    fn native_absolute(entry: &str) -> Option<Retained<NSString>> {
        NSURL::URLWithString(&NSString::from_str(entry))?.absoluteString()
    }

    // Independent oracle: the actual Foundation serialization and the existing
    // native intake owner, not the preparation function's implementation.
    fn intake_accepts(actual: &NSString) -> bool {
        check_shape(
            NativeRequestShape {
                is_get: true,
                has_body: false,
                has_body_stream: false,
                url_utf16_units: actual.length(),
                header_fields: 0,
                header_utf16_units: 0,
            },
            ORIGINAL_WEBAPP_LIMITS,
        )
        .is_ok()
    }

    #[test]
    fn native_entry_request_accepts_full_url_limit_and_refuses_plus_one() {
        let limit = ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units;
        let remaining = limit - ENTRY.encode_utf16().count() - 1;
        let exact = format!("{ENTRY}?{}", "x".repeat(remaining));
        let actual = native_absolute(&exact).expect("exact fixture is a native URL");
        assert_eq!(actual.length(), limit);
        assert!(intake_accepts(&actual));
        let request = native_entry_request(&exact).expect("exact full URI fits");
        assert_eq!(
            request
                .URL()
                .and_then(|url| url.absoluteString())
                .expect("URL")
                .to_string(),
            exact
        );
        let over = exact + "x";
        assert!(!intake_accepts(
            &native_absolute(&over).expect("oversize native URL")
        ));
        assert!(
            native_entry_request(&over).is_err(),
            "reject before begin_host_load/revocation"
        );
        // A query-only limit omits the actual nonempty scheme/authority/path.
        let formerly_admitted = format!("{ENTRY}?{}", "x".repeat(limit));
        assert!(native_entry_request(&formerly_admitted).is_err());
    }

    #[test]
    fn native_entry_request_matches_normalized_utf16_intake_and_preserves_encoding() {
        let serialized = "appLang=fr&queryObj=%257b&literal=%23%00&path=%252f";
        let expected = format!("{ENTRY}?{serialized}");
        let request = native_entry_request(&expected).expect("encoded query");
        assert_eq!(
            request
                .URL()
                .and_then(|url| url.absoluteString())
                .expect("URL")
                .to_string(),
            expected
        );
        let empty = native_entry_request(ENTRY).expect("no query separator for empty query");
        assert_eq!(
            empty
                .URL()
                .and_then(|url| url.absoluteString())
                .expect("URL")
                .to_string(),
            ENTRY
        );

        let mut native_denials = 0;
        for query in [
            "é".repeat(512),
            "😀".repeat(256),
            " ".repeat(1024),
            String::from("q=%C3%A9"),
        ] {
            let entry = format!("{ENTRY}?{query}");
            let Some(actual) = native_absolute(&entry) else {
                assert!(native_entry_request(&entry).is_err());
                continue;
            };
            let accepted = intake_accepts(&actual);
            if !accepted {
                native_denials += 1;
            }
            assert_eq!(
                native_entry_request(&entry).is_ok(),
                accepted,
                "use the actual native absoluteString UTF-16 length: {}",
                actual.length()
            );
        }
        assert!(
            native_denials > 0,
            "qualify at least one real normalization expansion; no assumed SDK result"
        );
    }
}

#[cfg(all(test, feature = "profile-test-hooks", debug_assertions))]
mod native_failure_census_tests {
    use super::*;
    #[test]
    fn callback_census_is_bounded_chronological_and_preserves_signed_scalar_codes() {
        let mut census = FixtureNativeFailures::default();
        assert_eq!(census.ordered().count(), 0);
        for code in -10..0 {
            census.record(FixtureNativeFailure {
                phase: if code % 2 == 0 {
                    FixtureNativeFailurePhase::Provisional
                } else {
                    FixtureNativeFailurePhase::Committed
                },
                code,
                navigation_present: code % 2 == 0,
                receiving_view_matches: true,
                retained_navigation_matches: false,
            });
        }
        assert_eq!(census.observed, 10);
        assert_eq!(census.overwritten, 2);
        let entries = census.ordered().collect::<Vec<_>>();
        assert_eq!(entries.len(), 8);
        assert_eq!(
            entries.iter().map(|entry| entry.code).collect::<Vec<_>>(),
            [-8, -7, -6, -5, -4, -3, -2, -1]
        );
        assert_eq!(entries[0].phase.as_str(), "provisional");
        assert_eq!(entries[1].phase.as_str(), "committed");
        assert!(entries[0].navigation_present);
        assert!(!entries[1].navigation_present);
        assert!(
            entries
                .iter()
                .all(|entry| entry.receiving_view_matches && !entry.retained_navigation_matches)
        );
    }
}
