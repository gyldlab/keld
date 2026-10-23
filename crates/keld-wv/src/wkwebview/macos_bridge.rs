//! macOS KEL-142 renderer bridge: isolated `WebKit` world plus bounded host handoff.
//!
//! The page-visible window.keld facade is not the authority boundary.
//! Native admission happens only in a non-page content world handler installed
//! on one per-view user-content controller.

use std::ffi::CStr;
use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use objc2::rc::Retained;
use objc2::runtime::{NSObject, ProtocolObject};
use objc2::{DeclaredClass, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSProcessInfo, NSString};
use objc2_web_kit::{
    WKContentWorld, WKFrameInfo, WKScriptMessage, WKScriptMessageHandler, WKUserContentController,
    WKUserScript, WKUserScriptInjectionTime, WKWebView, WKWebViewConfiguration,
};
use sha2::{Digest as _, Sha256};

use crate::WebviewId;
use crate::error::WvError;
use crate::wv_link::{WvLinkEnvelope, WvLinkRejectReason, decode_wv_link};

const HANDLER_NAME: &str = "__keld_wv_link_v1";
/// Placeholder in both injected scripts for the host-admitted channel id.
///
/// `keld-wv` has no `keld-ipc` dependency (architecture 01 §3): the host passes
/// the admitted id to [`RendererBridgeEndpoint::new`] and
/// [`render_bridge_script`] writes it into each script at construction
/// (GH-508 spec §4.6).
const ADMITTED_CHANNEL_PLACEHOLDER: &str = "__KELD_ADMITTED_CHANNEL__";

const PAGE_FACADE_SCRIPT: &str = r#"
(() => {
  "use strict";
  // Captured at document start, before any page script can replace them.
  const Error = globalThis.Error;
  const post = window.postMessage.bind(window);
  const freeze = Object.freeze.bind(Object);
  const define = Object.defineProperty.bind(Object);
  const U8 = Uint8Array;
  let nextRequest = 1;
  let pending = null;

  // The one rejection constructor (GH-1095 §4.9): the message is built by its
  // caller exactly as before, and an own enumerable code equals its prefix.
  const coded = (code, message) => {
    const error = new Error(message);
    define(error, "code", { value: code, enumerable: true, writable: true, configurable: true });
    return error;
  };
  const fail = (detail) => Promise.reject(coded("KELD-WV-011", "KELD-WV-011: " + detail));

  const onResult = (event) => {
    if (event.source !== window || pending === null) return;
    const msg = event.data;
    if (!msg || msg.__keldRelay !== "result-v1" || msg.request !== pending.request) return;
    const waiter = pending;
    pending = null;
    if (msg.ok === true && Array.isArray(msg.payload)) {
      waiter.resolve(new U8(msg.payload));
    } else {
      const code = typeof msg.code === "string" ? msg.code : "KELD-WV-011";
      const detail = typeof msg.detail === "string" ? msg.detail : "renderer bridge request failed";
      // The one prefix rule (spec r2 §4.9), idempotent and applied only here.
      waiter.reject(coded(code, detail.startsWith(code + ": ") ? detail : code + ": " + detail));
    }
  };
  window.addEventListener("message", onResult, false);
  window.addEventListener("pagehide", () => {
    if (pending === null) return;
    const waiter = pending;
    pending = null;
    // The call may already have been dispatched: outcome unknown.
    waiter.reject(coded("KELD-WV-011", "KELD-WV-011: renderer document navigated before reply"));
  }, false);

  const invoke = function invoke(channel, payload, opts) {
    if (opts !== undefined) return fail("opts is not supported by this bridge");
    if (!Number.isInteger(channel) || channel <= 0 || channel > 0xffff) {
      return fail("channel must be a nonzero u16");
    }
    if (channel !== __KELD_ADMITTED_CHANNEL__) return fail("renderer channel is not declared by this build");
    if (!(payload instanceof U8)) return fail("payload must be a Uint8Array");
    if (payload.byteLength > 4096) return fail("payload exceeds the 4096-byte renderer bound");
    if (pending !== null) return fail("another renderer invoke is already pending");

    const snapshot = Array.from(payload);
    const request = nextRequest >>> 0 || 1;
    nextRequest = (request + 1) >>> 0 || 1;
    return new Promise((resolve, reject) => {
      pending = { request, resolve, reject };
      post({
        __keldRelay: "invoke-v1",
        request,
        channel,
        payload: snapshot,
      }, "*");
    });
  };

  freeze(invoke);
  const bridge = freeze({ invoke });
  define(window, "keld", {
    value: bridge,
    enumerable: true,
    writable: false,
    configurable: false,
  });
})();
"#;

const ISOLATED_BRIDGE_SCRIPT: &str = r#"
(() => {
  "use strict";
  const post = window.postMessage.bind(window);
  const handler = window.webkit.messageHandlers.__keld_wv_link_v1;
  let documentNonce = null;
  let queued = null;
  // The request id of the call in flight to native, or null.
  let inflight = null;

  const sendNative = (call) => {
    if (documentNonce === null || inflight !== null) return;
    inflight = call.request;
    handler.postMessage(JSON.stringify({
      v: 1,
      kind: "invoke",
      document: documentNonce,
      request: call.request,
      channel: call.channel,
      payload: call.payload,
    }));
  };

  globalThis.__keldBridgeNativeBind = (nonce) => {
    if (typeof nonce !== "string" || nonce.length === 0) return;
    documentNonce = nonce;
    if (queued !== null) {
      const call = queued;
      queued = null;
      sendNative(call);
    }
  };

  // Settles only a result for this document's in-flight request; any other
  // result is dropped and leaves inflight unchanged (GH-1095 §4.7).
  globalThis.__keldBridgeNativeResult = (result) => {
    if (!result || result.document !== documentNonce || result.request !== inflight) return;
    inflight = null;
    post({
      __keldRelay: "result-v1",
      request: result.request,
      ok: result.ok === true,
      payload: Array.isArray(result.payload) ? result.payload : undefined,
      code: typeof result.code === "string" ? result.code : undefined,
      detail: typeof result.detail === "string" ? result.detail : undefined,
    }, "*");
  };

  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const call = event.data;
    if (!call || call.__keldRelay !== "invoke-v1") return;
    if (!Number.isInteger(call.request) || call.request <= 0 || call.request > 0xffffffff) return;
    if (!Number.isInteger(call.channel) || call.channel <= 0 || call.channel > 0xffff) return;
    if (call.channel !== __KELD_ADMITTED_CHANNEL__ || !Array.isArray(call.payload) || call.payload.length > 4096) return;
    if (!call.payload.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) return;
    if (queued !== null || inflight !== null) {
      post({
        __keldRelay: "result-v1",
        request: call.request,
        ok: false,
        code: "KELD-WV-011",
        detail: "another renderer invoke is already pending",
      }, "*");
      return;
    }
    const snapshot = {
      request: call.request,
      channel: call.channel,
      payload: Array.from(call.payload),
    };
    if (documentNonce === null) queued = snapshot;
    else sendNative(snapshot);
  }, false);

  handler.postMessage(JSON.stringify({ v: 1, kind: "bind" }));
})();
"#;

/// Renders one injected script template with the host-admitted channel id.
fn render_bridge_script(template: &str, admitted_channel: u16) -> String {
    template.replace(ADMITTED_CHANNEL_PLACEHOLDER, &admitted_channel.to_string())
}

/// One renderer request that already passed `WebKit` world/frame/document admission.
#[derive(Debug)]
pub struct RendererBridgeRequest {
    webview: WebviewId,
    navigation: u64,
    request: u32,
    channel: u16,
    payload: Vec<u8>,
}

impl RendererBridgeRequest {
    /// Returns the host-minted `WebView` identity that admitted this request.
    #[must_use]
    pub const fn webview(&self) -> WebviewId {
        self.webview
    }

    /// Returns the renderer-local request id used only for promise settlement.
    #[must_use]
    pub const fn request(&self) -> u32 {
        self.request
    }

    /// Returns the host-owned navigation generation that admitted this call.
    ///
    /// This value never crosses the page/native envelope. It only prevents a
    /// late outcome from an old document settling a newer document that reused
    /// the same renderer-local request id.
    #[must_use]
    pub const fn navigation(&self) -> u64 {
        self.navigation
    }

    /// Returns the declared application channel selected by the facade.
    #[must_use]
    pub const fn channel(&self) -> u16 {
        self.channel
    }

    /// Returns the synchronously snapshotted application payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Terminal host result for one renderer-local request.
#[derive(Debug)]
pub enum RendererBridgeOutcome {
    /// Successful application reply for one renderer-local request.
    Reply {
        /// `WebView` that owns the renderer promise.
        webview: WebviewId,
        /// Host-owned navigation generation that admitted the request.
        navigation: u64,
        /// Renderer-local request id.
        request: u32,
        /// Application reply bytes.
        payload: Vec<u8>,
    },
    /// Typed terminal failure for one renderer-local request.
    Error {
        /// `WebView` that owns the renderer promise.
        webview: WebviewId,
        /// Host-owned navigation generation that admitted the request.
        navigation: u64,
        /// Renderer-local request id.
        request: u32,
        /// Stable KELD diagnostic code.
        code: String,
        /// Sanitized detail with no endpoint/token/authority material.
        detail: String,
    },
}

impl RendererBridgeOutcome {
    #[must_use]
    pub(super) const fn webview(&self) -> WebviewId {
        match self {
            Self::Reply { webview, .. } | Self::Error { webview, .. } => *webview,
        }
    }

    #[must_use]
    const fn request(&self) -> u32 {
        match self {
            Self::Reply { request, .. } | Self::Error { request, .. } => *request,
        }
    }

    #[must_use]
    const fn navigation(&self) -> u64 {
        match self {
            Self::Reply { navigation, .. } | Self::Error { navigation, .. } => *navigation,
        }
    }
}

/// Core-facing bounded request sender plus UI-facing terminal outcomes.
#[derive(Debug)]
pub struct RendererBridgeEndpoint {
    requests: SyncSender<RendererBridgeRequest>,
    outcomes: Receiver<RendererBridgeOutcome>,
    admitted_channel: u16,
}

impl RendererBridgeEndpoint {
    /// Creates the one-view host/renderer channel pair used by this slice.
    ///
    /// `admitted_channel` is the one kipc channel id the renderer may invoke;
    /// the host passes it from its channel table (GH-508 spec §4.6). Both the
    /// injected scripts and native admission reject every other id.
    #[must_use]
    pub fn new(
        requests: SyncSender<RendererBridgeRequest>,
        outcomes: Receiver<RendererBridgeOutcome>,
        admitted_channel: u16,
    ) -> Self {
        Self {
            requests,
            outcomes,
            admitted_channel,
        }
    }
}

#[derive(Debug)]
struct PendingRequest {
    navigation: u64,
    request: u32,
    /// Document nonce the request was admitted under. Every native result
    /// for it carries this nonce, so only that document can settle it
    /// (GH-1095 §4.7). It never leaves keld-wv.
    document: String,
}

#[derive(Debug)]
struct BridgeState {
    webview: WebviewId,
    admitted_channel: u16,
    expected_webview: Option<usize>,
    navigation_generation: u64,
    document_nonce: Option<String>,
    pending: Option<PendingRequest>,
    destroyed: bool,
}

impl BridgeState {
    fn new(webview: WebviewId, admitted_channel: u16) -> Self {
        Self {
            webview,
            admitted_channel,
            expected_webview: None,
            navigation_generation: 0,
            document_nonce: None,
            pending: None,
            destroyed: false,
        }
    }

    fn bind_webview(&mut self, address: usize) -> Result<(), &'static str> {
        match self.expected_webview {
            None => {
                self.expected_webview = Some(address);
                Ok(())
            }
            Some(expected) if expected == address => Ok(()),
            Some(_) => Err("renderer bridge WebView identity changed"),
        }
    }

    fn accepts_webview(&self, address: usize) -> bool {
        self.expected_webview == Some(address)
    }

    /// Starts a new navigation generation and hands back the pending request
    /// it clears, if any.
    fn navigation_started(&mut self) -> Option<PendingRequest> {
        self.navigation_generation = self.navigation_generation.wrapping_add(1).max(1);
        self.document_nonce = None;
        self.pending.take()
    }

    fn destroy(&mut self) {
        self.destroyed = true;
        self.document_nonce = None;
        self.pending = None;
    }

    fn bind(&mut self) -> Result<String, &'static str> {
        if self.destroyed {
            return Err("renderer bridge was destroyed");
        }
        if self.expected_webview.is_none() {
            return Err("renderer bridge WebView identity is not bound");
        }
        if self.navigation_generation == 0 {
            self.navigation_generation = 1;
        }
        if self.document_nonce.is_some() {
            return Err("document is already bound");
        }
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| "host randomness is unavailable")?;
        let mut encoded = String::with_capacity(32);
        for byte in nonce {
            use std::fmt::Write as _;
            let _ = write!(&mut encoded, "{byte:02x}");
        }
        self.document_nonce = Some(encoded.clone());
        Ok(encoded)
    }

    fn admit_invoke(
        &mut self,
        document: &str,
        request: u32,
        channel: u16,
        payload: Vec<u8>,
    ) -> Result<RendererBridgeRequest, &'static str> {
        if self.destroyed {
            return Err("renderer bridge was destroyed");
        }
        if self.document_nonce.as_deref() != Some(document) {
            return Err("renderer document is stale or forged");
        }
        if channel != self.admitted_channel {
            return Err("renderer channel is not declared by this build");
        }
        if self.pending.is_some() {
            return Err("another renderer invoke is already pending");
        }
        self.pending = Some(PendingRequest {
            navigation: self.navigation_generation,
            request,
            document: document.to_owned(),
        });
        Ok(RendererBridgeRequest {
            webview: self.webview,
            navigation: self.navigation_generation,
            request,
            channel,
            payload,
        })
    }

    /// Removes the pending request matching `(navigation, request)` and
    /// returns the document nonce it was admitted under; `None` when nothing
    /// matches, so a late or duplicate outcome settles nothing.
    fn settle(&mut self, navigation: u64, request: u32) -> Option<String> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.navigation == navigation && pending.request == request)
        {
            self.pending.take().map(|pending| pending.document)
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub(super) struct BridgeNavigationHandle(Arc<Mutex<BridgeState>>);

impl BridgeNavigationHandle {
    /// The committed-navigation reset (wry `PageLoadEvent::Started`). The old
    /// document is gone and its `pagehide` already settled its call, so the
    /// request this clears needs no delivery; its later host outcome finds no
    /// entry and is dropped.
    pub(super) fn navigation_started(&self) {
        let _committed = lock_state(&self.0).navigation_started();
    }
}

pub(super) struct MacRendererBridge {
    state: Arc<Mutex<BridgeState>>,
    world: Retained<WKContentWorld>,
    controller: Retained<WKUserContentController>,
    _handler: Retained<KeldScriptMessageHandler>,
}

pub(super) struct InstalledRendererBridge {
    pub(super) bridge: MacRendererBridge,
    pub(super) outcomes: Receiver<RendererBridgeOutcome>,
}

struct HandlerIvars {
    state: Arc<Mutex<BridgeState>>,
    requests: SyncSender<RendererBridgeRequest>,
    controller_address: usize,
    world: Retained<WKContentWorld>,
}

define_class!(
    // SAFETY: NSObject has no subclass invariants beyond Objective-C object
    // initialization. This delegate is main-thread-only because every WebKit
    // object passed to it is main-thread-bound.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = HandlerIvars]
    struct KeldScriptMessageHandler;

    // SAFETY: NSObjectProtocol is inherited from NSObject.
    unsafe impl NSObjectProtocol for KeldScriptMessageHandler {}

    // SAFETY: selector and signature exactly match WKScriptMessageHandler.
    unsafe impl WKScriptMessageHandler for KeldScriptMessageHandler {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive(
            this: &KeldScriptMessageHandler,
            controller: &WKUserContentController,
            message: &WKScriptMessage,
        ) {
            handle_message(this, controller, message);
        }
    }
);

impl KeldScriptMessageHandler {
    fn new(
        state: Arc<Mutex<BridgeState>>,
        requests: SyncSender<RendererBridgeRequest>,
        controller_address: usize,
        world: Retained<WKContentWorld>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let object = mtm.alloc::<Self>().set_ivars(HandlerIvars {
            state,
            requests,
            controller_address,
            world,
        });
        // SAFETY: object is freshly allocated with fully initialized ivars.
        unsafe { msg_send![super(object), init] }
    }
}

impl MacRendererBridge {
    pub(super) fn install(
        webview: WebviewId,
        configuration: &WKWebViewConfiguration,
        endpoint: RendererBridgeEndpoint,
    ) -> Result<InstalledRendererBridge, WvError> {
        if NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion
            < 11
        {
            return Err(bridge_error(
                "WKContentWorld requires macOS 11 or later; page-world fallback is forbidden",
            ));
        }
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            bridge_error("renderer bridge must be installed on the AppKit main thread")
        })?;
        let admitted_channel = endpoint.admitted_channel;
        let state = Arc::new(Mutex::new(BridgeState::new(webview, admitted_channel)));
        // SAFETY: created on the AppKit main thread before WKWebView exists.
        let controller = unsafe { WKUserContentController::new(mtm) };
        // SAFETY: macOS 11+ was checked and this is the UI thread.
        let isolated_world = unsafe { WKContentWorld::defaultClientWorld(mtm) };
        // SAFETY: same availability and thread proof.
        let page_world = unsafe { WKContentWorld::pageWorld(mtm) };
        let handler = KeldScriptMessageHandler::new(
            Arc::clone(&state),
            endpoint.requests,
            Retained::as_ptr(&controller).cast::<()>() as usize,
            isolated_world.clone(),
            mtm,
        );
        let handler_name = NSString::from_str(HANDLER_NAME);
        let isolated_source = NSString::from_str(&render_bridge_script(
            ISOLATED_BRIDGE_SCRIPT,
            admitted_channel,
        ));
        let page_source =
            NSString::from_str(&render_bridge_script(PAGE_FACADE_SCRIPT, admitted_channel));
        // SAFETY: initializer is macOS-11+ and runs on the main thread.
        let isolated_script = unsafe {
            WKUserScript::initWithSource_injectionTime_forMainFrameOnly_inContentWorld(
                mtm.alloc::<WKUserScript>(),
                &isolated_source,
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
                &isolated_world,
            )
        };
        // SAFETY: same proof. This intentionally exposes only the reviewed
        // facade in pageWorld.
        let page_script = unsafe {
            WKUserScript::initWithSource_injectionTime_forMainFrameOnly_inContentWorld(
                mtm.alloc::<WKUserScript>(),
                &page_source,
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
                &page_world,
            )
        };

        let proto = ProtocolObject::from_ref(&*handler);
        let install = objc2::exception::catch(AssertUnwindSafe(|| {
            // SAFETY: fresh per-view controller, one isolated handler name,
            // and two main-frame scripts before WebView construction.
            unsafe {
                controller.addScriptMessageHandler_contentWorld_name(
                    proto,
                    &isolated_world,
                    &handler_name,
                );
                controller.addUserScript(&isolated_script);
                controller.addUserScript(&page_script);
                configuration.setUserContentController(&controller);
            }
        }));
        if install.is_err() {
            return Err(bridge_error(
                "WebKit rejected the isolated renderer bridge configuration",
            ));
        }

        Ok(InstalledRendererBridge {
            bridge: Self {
                state,
                world: isolated_world,
                controller,
                _handler: handler,
            },
            outcomes: endpoint.outcomes,
        })
    }

    pub(super) fn navigation_handle(&self) -> BridgeNavigationHandle {
        BridgeNavigationHandle(Arc::clone(&self.state))
    }

    pub(super) fn verify_webview(&self, webview: &wry::WryWebView) -> Result<(), WvError> {
        // SAFETY: live WKWebView is read on the creating AppKit thread.
        let configuration = unsafe { webview.configuration() };
        // SAFETY: configuration remains live with the WebView.
        let observed = unsafe { configuration.userContentController() };
        if !std::ptr::eq(
            Retained::as_ptr(&observed),
            Retained::as_ptr(&self.controller),
        ) {
            return Err(bridge_error(
                "live WKWebView is not bound to its dedicated renderer controller",
            ));
        }
        let address = std::ptr::from_ref(webview).cast::<()>() as usize;
        lock_state(&self.state)
            .bind_webview(address)
            .map_err(bridge_error)?;
        Ok(())
    }

    pub(super) fn destroy(&self) {
        lock_state(&self.state).destroy();
    }

    /// A host `navigate()` resets the bridge before `WebKit` loads anything.
    /// If that load never commits, the current document stays live, so the
    /// call this reset clears settles here once, as outcome unknown (GH-1095
    /// §4.7). If it commits, the old document already settled the call on
    /// `pagehide`, and the successor drops this result by its document.
    /// Nothing here can fail after the call was cleared.
    ///
    /// Spec r2 §4.7: a host `navigate()` cannot run while the app runs today,
    /// so this path is not reachable end to end. It is covered only by the
    /// unit test that calls `navigation_reset_script` directly; its running-
    /// app owner is #459.
    pub(super) fn navigation_started(&self, webview: Option<&wry::WryWebView>) {
        if let Some(script) = navigation_reset_script(&self.state)
            && let Some(webview) = webview
        {
            evaluate_in_world(webview, &self.world, &script);
        }
    }

    pub(super) fn deliver(&self, webview: &wry::WryWebView, outcome: RendererBridgeOutcome) {
        let request = outcome.request();
        let navigation = outcome.navigation();
        let Some(document) = lock_state(&self.state).settle(navigation, request) else {
            return;
        };
        let script = page_result_script(&PageOutcome::settled(outcome, document));
        evaluate_in_world(webview, &self.world, &script);
    }
}

/// Resets the bridge for a host `navigate()` and returns the script that
/// settles the call the reset cleared, or `None` when no call was pending.
fn navigation_reset_script(state: &Arc<Mutex<BridgeState>>) -> Option<String> {
    let cleared = lock_state(state).navigation_started()?;
    Some(page_result_script(&PageOutcome::navigation_reset(cleared)))
}

/// One native→isolated result (`keld.wv-link/v1`). `document` names the
/// document nonce of the request it answers; the isolated world never relays
/// it to the page (GH-1095 §4.7).
#[derive(Debug)]
struct PageOutcome {
    request: u32,
    document: Option<String>,
    ok: bool,
    payload: Option<Vec<u8>>,
    code: Option<String>,
    detail: Option<String>,
}

impl PageOutcome {
    /// The host outcome for a request, addressed to the document it was
    /// admitted under.
    fn settled(outcome: RendererBridgeOutcome, document: String) -> Self {
        match outcome {
            RendererBridgeOutcome::Reply {
                request, payload, ..
            } => Self {
                request,
                document: Some(document),
                ok: true,
                payload: Some(payload),
                code: None,
                detail: None,
            },
            RendererBridgeOutcome::Error {
                request,
                code,
                detail,
                ..
            } => Self {
                request,
                document: Some(document),
                ok: false,
                payload: None,
                code: Some(code),
                detail: Some(detail),
            },
        }
    }

    /// The terminal for a call a host `navigate()` reset cleared before its
    /// outcome arrived. The call may already have been dispatched.
    fn navigation_reset(cleared: PendingRequest) -> Self {
        Self {
            request: cleared.request,
            document: Some(cleared.document),
            ok: false,
            payload: None,
            code: Some(String::from("KELD-WV-013")),
            detail: Some(String::from("renderer navigation started before reply")),
        }
    }

    /// A native refusal of an envelope, addressed to the document the
    /// envelope named (S3b): a stale document's refusal names the stale
    /// nonce, so a successor document drops it.
    fn refused(request: u32, document: &str, detail: &str) -> Self {
        Self {
            request,
            document: Some(document.to_owned()),
            ok: false,
            payload: None,
            code: Some(String::from("KELD-WV-011")),
            detail: Some(detail.to_owned()),
        }
    }
}

impl PageOutcome {
    /// The result as JSON. Every field is a number, a bool, a string or a
    /// byte list, so building it cannot fail; an absent field is omitted.
    fn to_json(&self) -> serde_json::Value {
        let mut fields = serde_json::Map::new();
        fields.insert(String::from("request"), self.request.into());
        if let Some(document) = &self.document {
            fields.insert(String::from("document"), document.as_str().into());
        }
        fields.insert(String::from("ok"), self.ok.into());
        if let Some(payload) = &self.payload {
            fields.insert(String::from("payload"), payload.clone().into());
        }
        if let Some(code) = &self.code {
            fields.insert(String::from("code"), code.as_str().into());
        }
        if let Some(detail) = &self.detail {
            fields.insert(String::from("detail"), detail.as_str().into());
        }
        serde_json::Value::Object(fields)
    }
}

/// The isolated-world script that delivers one native result. It has no
/// failure path, so a result whose pending entry was already removed is
/// always delivered.
fn page_result_script(outcome: &PageOutcome) -> String {
    format!(
        "globalThis.__keldBridgeNativeResult({});",
        outcome.to_json()
    )
}

fn handle_message(
    handler: &KeldScriptMessageHandler,
    controller: &WKUserContentController,
    message: &WKScriptMessage,
) {
    let ivars = handler.ivars();
    let controller_address = std::ptr::from_ref(controller).cast::<()>() as usize;
    if controller_address != ivars.controller_address {
        report_detail_rejection("renderer message used the wrong content controller");
        return;
    }
    // SAFETY: WebKit supplies a live content-world object to this UI-thread callback.
    let world = unsafe { message.world() };
    if !std::ptr::eq(Retained::as_ptr(&world), Retained::as_ptr(&ivars.world)) {
        report_detail_rejection("renderer message came from the wrong content world");
        return;
    }
    // SAFETY: WebKit supplies a live frame object to this UI-thread callback.
    let frame: Retained<WKFrameInfo> = unsafe { message.frameInfo() };
    // SAFETY: frame is live and queried on the WebKit UI thread.
    if !unsafe { frame.isMainFrame() } {
        report_detail_rejection("renderer message came from a subframe");
        return;
    }
    // SAFETY: callback WebView is live on the AppKit main thread.
    let Some(webview) = (unsafe { message.webView() }) else {
        report_detail_rejection("renderer message has no live WebView");
        return;
    };
    let webview_address = Retained::as_ptr(&webview).cast::<()>() as usize;
    if !lock_state(&ivars.state).accepts_webview(webview_address) {
        report_detail_rejection("renderer message came from the wrong WebView");
        return;
    }
    // SAFETY: message body is a live Objective-C object owned by WebKit.
    let body = unsafe { message.body() };
    let Ok(body) = body.downcast::<NSString>() else {
        return;
    };
    // SAFETY: NSString exposes a NUL-terminated UTF-8 pointer while retained.
    let raw = body.UTF8String();
    if raw.is_null() {
        return;
    }
    // SAFETY: raw is the valid pointer returned by NSString above.
    let Ok(text) = unsafe { CStr::from_ptr(raw) }.to_str() else {
        return;
    };
    let envelope = match decode_wv_link(text.as_bytes()) {
        Ok(envelope) => envelope,
        Err(reason) => {
            report_rejection(reason);
            return;
        }
    };

    match envelope {
        WvLinkEnvelope::Bind => {
            let nonce = match lock_state(&ivars.state).bind() {
                Ok(nonce) => nonce,
                Err(detail) => {
                    report_detail_rejection(detail);
                    return;
                }
            };
            report_bind(&ivars.state, &nonce);
            let Ok(json) = serde_json::to_string(&nonce) else {
                return;
            };
            let script = format!("globalThis.__keldBridgeNativeBind({json});");
            evaluate_in_world(&webview, &ivars.world, &script);
        }
        WvLinkEnvelope::Invoke {
            document,
            request,
            channel,
            payload,
        } => {
            let request_id = request.get();
            let admitted = lock_state(&ivars.state).admit_invoke(
                &document,
                request_id,
                channel.get(),
                payload,
            );
            let call = match admitted {
                Ok(call) => call,
                Err(detail) => {
                    let refused = PageOutcome::refused(request_id, &document, detail);
                    reply_immediate_error(message, &ivars.world, &refused);
                    return;
                }
            };
            report_admission(&call);
            match ivars.requests.try_send(call) {
                Ok(()) => {}
                Err(TrySendError::Full(call) | TrySendError::Disconnected(call)) => {
                    let _ = lock_state(&ivars.state).settle(call.navigation, call.request);
                    let refused = PageOutcome::refused(
                        call.request,
                        &document,
                        "host renderer dispatch queue is unavailable",
                    );
                    reply_immediate_error(message, &ivars.world, &refused);
                }
            }
        }
    }
}

fn reply_immediate_error(message: &WKScriptMessage, world: &WKContentWorld, refused: &PageOutcome) {
    // SAFETY: callback WebView is live on the main thread.
    let Some(webview) = (unsafe { message.webView() }) else {
        return;
    };
    evaluate_in_world(&webview, world, &page_result_script(refused));
}

fn evaluate_in_world(webview: &WKWebView, world: &WKContentWorld, script: &str) {
    let source = NSString::from_str(script);
    // SAFETY: live WKWebView and content world are used only on AppKit UI
    // thread. No completion block is retained or exported.
    unsafe {
        webview.evaluateJavaScript_inFrame_inContentWorld_completionHandler(
            &source, None, world, None,
        );
    }
}

fn lock_state(state: &Arc<Mutex<BridgeState>>) -> std::sync::MutexGuard<'_, BridgeState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn bridge_error(detail: impl Into<String>) -> WvError {
    WvError::RendererBridge {
        detail: detail.into(),
    }
}

fn reject_detail(reason: WvLinkRejectReason) -> &'static str {
    match reason {
        WvLinkRejectReason::MalformedEnvelope => "malformed renderer envelope",
        WvLinkRejectReason::UnsupportedVersion => "unsupported renderer envelope version",
        WvLinkRejectReason::PayloadTooLarge => "renderer payload exceeds 4096 bytes",
        WvLinkRejectReason::ZeroRequest => "renderer request id is zero",
        WvLinkRejectReason::ZeroChannel => "renderer channel id is zero",
    }
}

fn report_rejection(reason: WvLinkRejectReason) {
    report_detail_rejection(reject_detail(reason));
}

fn report_detail_rejection(detail: &str) {
    if std::env::var_os("KELD_KEL142_ACCEPTANCE_REPORT").is_some() {
        eprintln!("KELD_KEL142_REJECT detail={detail}");
    }
}

fn report_bind(state: &Arc<Mutex<BridgeState>>, nonce: &str) {
    if std::env::var_os("KELD_KEL142_ACCEPTANCE_REPORT").is_none() {
        return;
    }
    let state = lock_state(state);
    let hash = Sha256::digest(nonce.as_bytes());
    let mut digest = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write as _;
        let _ = write!(&mut digest, "{byte:02x}");
    }
    eprintln!(
        "KELD_KEL142_BIND webview={} navigation={} nonce_sha256={digest}",
        state.webview.0, state.navigation_generation
    );
}

fn report_admission(call: &RendererBridgeRequest) {
    if std::env::var_os("KELD_KEL142_ACCEPTANCE_REPORT").is_some() {
        eprintln!(
            "KELD_KEL142_RENDERER_ADMIT webview={} navigation={} request={} channel={} payload_len={}",
            call.webview.0,
            call.navigation,
            call.request,
            call.channel,
            call.payload.len()
        );
    }
}

/// Service one source on the same process main run loop before an original
/// Native absolute bound. This is a servicing result, never a drain receipt.
#[allow(
    unsafe_code,
    reason = "sanctioned CoreFoundation default-mode constant read on the main thread"
)]
pub(super) fn service_main_tail_before(
    _main_thread: MainThreadMarker,
    original_deadline: std::time::Instant,
) -> Result<(), WvError> {
    let Some(remaining) = original_deadline.checked_duration_since(std::time::Instant::now())
    else {
        return Err(WvError::EventLoop(String::from(
            "native main tail original bound expired",
        )));
    };
    if remaining.is_zero() {
        return Err(WvError::EventLoop(String::from(
            "native main tail original bound expired",
        )));
    }
    // SAFETY: exported CoreFoundation process-constant default mode; the marker
    // proves this service runs on the current process main thread. No raw handle
    // is adopted and no SDK object leaves its owner.
    let mode = unsafe { objc2_core_foundation::kCFRunLoopDefaultMode };
    let result = objc2_core_foundation::CFRunLoop::run_in_mode(mode, remaining.as_secs_f64(), true);
    if result == objc2_core_foundation::CFRunLoopRunResult::HandledSource
        || result == objc2_core_foundation::CFRunLoopRunResult::TimedOut
    {
        Ok(())
    } else {
        Err(WvError::EventLoop(String::from(
            "native main tail run-loop service unavailable",
        )))
    }
}

// DO NOT MERGE: gated off so hosted CI reaches the keld-host arms.
#[cfg(all(test, any()))]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// The echo id `keld-core` passes today; any host-chosen id works the same.
    const ADMITTED: u16 = 1;
    const NONCE_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const NONCE_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// Runs the shipped scripts, rendered exactly as `install` renders them,
    /// in the GH-1095 document model under Bun's `JavaScriptCore`.
    fn run_script_harness(input: &Value) -> Value {
        let program = format!(
            "const PAGE_SCRIPT = {};\nconst ISOLATED_SCRIPT = {};\nconst INPUT = {input};\n{}",
            serde_json::to_string(&render_bridge_script(PAGE_FACADE_SCRIPT, ADMITTED))
                .expect("encode page script"),
            serde_json::to_string(&render_bridge_script(ISOLATED_BRIDGE_SCRIPT, ADMITTED))
                .expect("encode isolated script"),
            include_str!("../../tests/fixtures/renderer_bridge_harness.js"),
        );
        let output = std::process::Command::new("bun")
            .args(["-e", &program])
            .output()
            .expect("bun must be on PATH (same contract as the KEL-142 host tests)");
        assert!(
            output.status.success(),
            "script harness failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("harness JSON")
    }

    /// The page-visible shape of one rejection, as the harness reports it.
    fn assert_coded_rejection(record: &Value, code: &str, message: &str) {
        assert_eq!(record["isError"], true, "{record}");
        assert_eq!(record["name"], "Error", "{record}");
        assert_eq!(record["message"], message, "message bytes are unchanged");
        assert_eq!(record["string"], format!("Error: {message}"));
        assert_eq!(record["code"], code, "{record}");
        assert!(
            message.starts_with(&format!("{code}: ")),
            "code is the message prefix"
        );
        for attribute in ["own", "enumerable", "writable", "configurable"] {
            assert_eq!(record[attribute], true, "code is {attribute}: {record}");
        }
        assert_eq!(record["keys"], json!(["code"]));
        assert_eq!(record["json"], json!({"code": code}).to_string());
    }

    /// GH-1095 criteria 4, 10, 13 (AT-EC unit): every `window.keld.invoke`
    /// rejection is an `Error` with an own, enumerable, string `code` equal
    /// to the prefix of its unchanged message, and `pagehide` rejects a
    /// pending call as outcome unknown.
    #[test]
    fn facade_rejections_carry_an_own_enumerable_code() {
        let out = run_script_harness(&json!({
            "scenario": "errors",
            "nonce": NONCE_A,
            "channel": ADMITTED,
            "results": [
                {"ok": false, "code": "KELD-WV-012", "detail": "no application generation is admitted; the call was not sent"},
                {"ok": false, "code": "KELD-WV-013", "detail": "application link ended before the renderer reply"},
                {"ok": false, "code": "KELD-API-001", "detail": "KELD-API-001: application channel call failed"},
                {"ok": false},
                {"ok": false, "code": "KELD-API-001", "detail": "KELD-API-001 failed"},
                {"ok": false, "code": "KELD-WV-013", "detail": "KELD-API-001: x"},
                {"ok": false, "code": "KELD-API-002", "detail": "KELD-API-001: x"},
                {"ok": false, "code": "KELD-WV-011", "detail": "KELD-WV-011: already prefixed"},
            ],
        }));
        for (local, message) in [
            ("opts", "KELD-WV-011: opts is not supported by this bridge"),
            ("channel", "KELD-WV-011: channel must be a nonzero u16"),
            ("payload", "KELD-WV-011: payload must be a Uint8Array"),
            (
                "busy",
                "KELD-WV-011: another renderer invoke is already pending",
            ),
        ] {
            assert_coded_rejection(&out[local], "KELD-WV-011", message);
        }
        let results = out["results"].as_array().expect("results");
        for (settled, (code, message)) in results.iter().zip([
            (
                "KELD-WV-012",
                "KELD-WV-012: no application generation is admitted; the call was not sent",
            ),
            (
                "KELD-WV-013",
                "KELD-WV-013: application link ended before the renderer reply",
            ),
            (
                "KELD-API-001",
                "KELD-API-001: application channel call failed",
            ),
            ("KELD-WV-011", "KELD-WV-011: renderer bridge request failed"),
            // r2 §4.9: a detail with the code but no ": " gains the prefix.
            ("KELD-API-001", "KELD-API-001: KELD-API-001 failed"),
            // A detail that starts with a different code is prefixed, never
            // re-coded: err.code comes only from the code field.
            ("KELD-WV-013", "KELD-WV-013: KELD-API-001: x"),
            ("KELD-API-002", "KELD-API-002: KELD-API-001: x"),
            // Idempotent: an already prefixed detail is not prefixed twice.
            ("KELD-WV-011", "KELD-WV-011: already prefixed"),
        ]) {
            assert_eq!(settled.as_array().expect("one call").len(), 1);
            assert_coded_rejection(&settled[0], code, message);
        }
        assert_eq!(results.len(), 8);
        assert_eq!(out["pagehide"].as_array().expect("pagehide").len(), 1);
        assert_coded_rejection(
            &out["pagehide"][0],
            "KELD-WV-013",
            "KELD-WV-013: renderer document navigated before reply",
        );
        // The oracle's control: a plain Error is reported without a code.
        assert_eq!(out["control"]["code"], Value::Null);
        assert_eq!(out["control"]["own"], false);
    }

    /// GH-1095 criterion 12 (AT-S arm c, unit): the isolated world settles a
    /// result only for its own document and its in-flight request. The
    /// predecessor's stale refusal and a result for another request are
    /// dropped without clearing `inflight`, so a relayed invoke is still
    /// refused as busy; the successor settles once, from its own result, and
    /// `document` never reaches the page. The payloads are the ones keld-wv
    /// builds.
    #[test]
    fn isolated_results_settle_only_their_own_document_and_request() {
        let stale = PageOutcome::refused(1, NONCE_A, "renderer document is stale or forged");
        let own = PageOutcome::settled(
            RendererBridgeOutcome::Reply {
                webview: WebviewId(1),
                navigation: 2,
                request: 1,
                payload: vec![7],
            },
            NONCE_B.to_owned(),
        );
        let out = run_script_harness(&json!({
            "scenario": "settlement",
            "predecessor": NONCE_A,
            "successor": NONCE_B,
            "channel": ADMITTED,
            "staleRefusal": stale.to_json(),
            "ownReply": own.to_json(),
        }));
        assert_eq!(out["sent"]["document"], NONCE_B);
        assert_eq!(
            out["sent"]["request"], 1,
            "the same request id as the stale one"
        );
        assert_eq!(
            out["afterStale"],
            json!([]),
            "the stale refusal crossed documents"
        );
        assert_eq!(
            out["afterWrongRequest"],
            json!([]),
            "another request settled the call"
        );
        assert_eq!(out["nativeInvokes"], 1, "a dropped result cleared inflight");
        assert_eq!(out["successor"], json!([{"ok": true, "value": [7]}]));
        let relays = out["relays"].as_array().expect("relays");
        assert_eq!(
            relays.len(),
            2,
            "busy refusal and the one own reply: {relays:?}"
        );
        assert_eq!(relays[0]["request"], 99);
        assert_eq!(relays[0]["code"], "KELD-WV-011");
        assert_eq!(relays[1]["request"], 1);
        for relay in relays {
            assert!(
                relay.get("document").is_none(),
                "document reached the page: {relay}"
            );
        }
        // The predecessor itself settled once, by its own pagehide.
        assert_eq!(out["predecessor"].as_array().expect("predecessor").len(), 1);
    }

    /// GH-1095 criterion 7 (AT-S arm d, unit): the reset result a host
    /// `navigate()` delivers settles the live document's call once as
    /// outcome unknown; a later `pagehide` finds nothing to settle, and a
    /// successor document drops the reset addressed to its predecessor.
    #[test]
    fn navigation_reset_result_settles_only_the_live_document_once() {
        let reset = PageOutcome {
            request: 0,
            document: Some(NONCE_A.to_owned()),
            ok: false,
            payload: None,
            code: Some(String::from("KELD-WV-013")),
            detail: Some(String::from("renderer navigation started before reply")),
        };
        let out = run_script_harness(&json!({
            "scenario": "reset",
            "nonce": NONCE_A,
            "successor": NONCE_B,
            "channel": ADMITTED,
            "reset": reset.to_json(),
        }));
        assert_eq!(out["afterReset"].as_array().expect("settled").len(), 1);
        assert_coded_rejection(
            &out["afterReset"][0],
            "KELD-WV-013",
            "KELD-WV-013: renderer navigation started before reply",
        );
        assert_eq!(
            out["settled"], out["afterReset"],
            "pagehide settled it again"
        );
        assert_eq!(
            out["successor"],
            json!([]),
            "the successor settled from a stale reset"
        );
    }

    /// GH-1095 criteria 7 and 13: a page script that replaces the global
    /// `Error` after the facade installed cannot stop a rejection from
    /// settling, and every rejection is still a genuine coded `Error`.
    #[test]
    fn rejections_settle_even_when_the_page_replaces_error() {
        let out = run_script_harness(&json!({
            "scenario": "tampered",
            "nonce": NONCE_A,
            "channel": ADMITTED,
            "result": {"ok": false, "code": "KELD-WV-013", "detail": "application link ended before the renderer reply"},
        }));
        for path in ["localThrew", "resultThrew", "pagehideThrew"] {
            assert_eq!(out[path], Value::Null, "{path}: {out}");
        }
        assert_coded_rejection(
            &out["local"],
            "KELD-WV-011",
            "KELD-WV-011: opts is not supported by this bridge",
        );
        assert_eq!(out["result"].as_array().expect("result").len(), 1, "{out}");
        assert_coded_rejection(
            &out["result"][0],
            "KELD-WV-013",
            "KELD-WV-013: application link ended before the renderer reply",
        );
        assert_eq!(
            out["pagehide"].as_array().expect("pagehide").len(),
            1,
            "{out}"
        );
        assert_coded_rejection(
            &out["pagehide"][0],
            "KELD-WV-013",
            "KELD-WV-013: renderer document navigated before reply",
        );
    }

    /// GH-1095 criterion 7: the script a host `navigate()` reset delivers
    /// for the call it clears is produced without a fallible step, exactly
    /// once, so no early return can leave that call unsettled; the later host
    /// outcome then finds no entry.
    #[test]
    fn navigation_reset_yields_one_settling_script() {
        let state = Arc::new(Mutex::new(BridgeState::new(WebviewId(2), ADMITTED)));
        let (nonce, call) = {
            let mut bridge = lock_state(&state);
            bridge.bind_webview(0x2000).expect("bind WebView");
            let nonce = bridge.bind().expect("bind");
            let call = bridge
                .admit_invoke(&nonce, 6, ADMITTED, vec![1])
                .expect("admit");
            (nonce, call)
        };
        let script: String = navigation_reset_script(&state).expect("the cleared call settles");
        let json = script
            .strip_prefix("globalThis.__keldBridgeNativeResult(")
            .and_then(|rest| rest.strip_suffix(");"))
            .expect("one native result call");
        assert_eq!(
            serde_json::from_str::<Value>(json).expect("result JSON"),
            json!({
                "request": 6,
                "document": nonce,
                "ok": false,
                "code": "KELD-WV-013",
                "detail": "renderer navigation started before reply",
            })
        );
        assert_eq!(
            navigation_reset_script(&state),
            None,
            "settled exactly once"
        );
        assert_eq!(
            lock_state(&state).settle(call.navigation(), call.request()),
            None,
            "the later host outcome finds no entry"
        );
    }

    /// GH-1095 AT-EC unit: the page facade constructs every rejection
    /// through its one coded-error helper.
    #[test]
    fn page_facade_has_one_error_constructor() {
        assert_eq!(PAGE_FACADE_SCRIPT.matches("new Error(").count(), 1);
    }

    /// GH-1095 criterion 12 (model): a request remembers the document nonce
    /// it was admitted under; settling it returns that nonce once, and a
    /// navigation reset hands the cleared request back with it.
    #[test]
    fn requests_carry_their_admitted_document_nonce() {
        let mut state = BridgeState::new(WebviewId(6), ADMITTED);
        state.bind_webview(0x6000).expect("bind WebView");
        let nonce = state.bind().expect("bind");
        let call = state
            .admit_invoke(&nonce, 3, ADMITTED, vec![1])
            .expect("admit");
        assert_eq!(
            state.settle(call.navigation(), call.request()),
            Some(nonce.clone())
        );
        assert_eq!(state.settle(call.navigation(), call.request()), None);

        let reset = state
            .admit_invoke(&nonce, 4, ADMITTED, vec![2])
            .expect("admit the call a reset clears");
        let cleared = state.navigation_started().expect("the reset clears it");
        assert_eq!(
            (
                cleared.navigation,
                cleared.request,
                cleared.document.as_str()
            ),
            (reset.navigation(), 4, nonce.as_str())
        );
        assert!(state.navigation_started().is_none(), "cleared only once");
    }

    /// GH-1095 criterion 12 (model): a native refusal names the document of
    /// the envelope it refuses, and a host outcome names the document its
    /// request was admitted under, in the JSON the isolated world receives.
    #[test]
    fn native_results_name_their_document() {
        let refused = page_result_script(&PageOutcome::refused(
            9,
            NONCE_A,
            "renderer document is stale or forged",
        ));
        let json = refused
            .strip_prefix("globalThis.__keldBridgeNativeResult(")
            .and_then(|rest| rest.strip_suffix(");"))
            .expect("one native result call");
        assert_eq!(
            serde_json::from_str::<Value>(json).expect("result JSON"),
            json!({
                "request": 9,
                "document": NONCE_A,
                "ok": false,
                "code": "KELD-WV-011",
                "detail": "renderer document is stale or forged",
            })
        );
        let settled = PageOutcome::settled(
            RendererBridgeOutcome::Reply {
                webview: WebviewId(1),
                navigation: 2,
                request: 5,
                payload: vec![7],
            },
            NONCE_B.to_owned(),
        );
        assert_eq!(
            settled.to_json(),
            json!({"request": 5, "document": NONCE_B, "ok": true, "payload": [7]})
        );
    }

    /// GH-1095 criterion 7 (AT-S arm d, model): a host `navigate()` reset
    /// hands its cleared call to the bridge wrapper, which delivers its one
    /// terminal; the later host outcome finds no entry and is dropped.
    #[test]
    fn navigation_reset_leaves_no_entry_for_the_later_host_outcome() {
        let mut state = BridgeState::new(WebviewId(8), ADMITTED);
        state.bind_webview(0x8000).expect("bind WebView");
        let nonce = state.bind().expect("bind");
        let call = state
            .admit_invoke(&nonce, 4, ADMITTED, vec![1])
            .expect("admit");
        let cleared = state
            .navigation_started()
            .expect("the reset clears the call");
        assert_eq!(
            PageOutcome::navigation_reset(cleared).to_json(),
            json!({
                "request": 4,
                "document": nonce,
                "ok": false,
                "code": "KELD-WV-013",
                "detail": "renderer navigation started before reply",
            })
        );
        assert!(state.pending.is_none());
        assert_eq!(
            state.settle(call.navigation(), call.request()),
            None,
            "the reset call cannot settle from its host outcome"
        );
    }

    #[test]
    fn navigation_invalidates_document_and_pending_renderer_call() {
        let mut state = BridgeState::new(WebviewId(7), ADMITTED);
        state.bind_webview(0x7000).expect("bind WebView");
        let nonce = state.bind().expect("bind");
        state
            .admit_invoke(&nonce, 11, ADMITTED, vec![1, 2])
            .expect("admit");
        state.navigation_started();
        assert!(state.document_nonce.is_none());
        assert!(state.pending.is_none());
        assert!(matches!(
            state.admit_invoke(&nonce, 12, ADMITTED, vec![]),
            Err("renderer document is stale or forged")
        ));
    }

    #[test]
    fn one_pending_call_and_declared_channel_are_independent_guards() {
        let mut state = BridgeState::new(WebviewId(3), ADMITTED);
        state.bind_webview(0x3000).expect("bind WebView");
        let nonce = state.bind().expect("bind");
        assert!(matches!(
            state.admit_invoke(&nonce, 1, 2, vec![]),
            Err("renderer channel is not declared by this build")
        ));
        state
            .admit_invoke(&nonce, 2, ADMITTED, vec![9])
            .expect("first call");
        assert!(matches!(
            state.admit_invoke(&nonce, 3, ADMITTED, vec![]),
            Err("another renderer invoke is already pending")
        ));
        let navigation = state.navigation_generation;
        assert_eq!(state.settle(navigation, 2), Some(nonce));
        assert_eq!(
            state.settle(navigation, 2),
            None,
            "one app reply can settle only once"
        );
    }

    #[test]
    fn wrong_webview_forged_nonce_and_destroyed_bridge_fail_closed() {
        let mut state = BridgeState::new(WebviewId(9), ADMITTED);
        state.bind_webview(0x9000).expect("bind expected WebView");
        assert!(!state.accepts_webview(0x9001));
        assert!(matches!(
            state.bind_webview(0x9001),
            Err("renderer bridge WebView identity changed")
        ));

        let nonce = state.bind().expect("bind document");
        assert!(matches!(
            state.admit_invoke("forged-document", 1, ADMITTED, vec![]),
            Err("renderer document is stale or forged")
        ));
        state.destroy();
        assert!(matches!(
            state.admit_invoke(&nonce, 2, ADMITTED, vec![]),
            Err("renderer bridge was destroyed")
        ));
        assert!(matches!(state.bind(), Err("renderer bridge was destroyed")));
    }

    #[test]
    fn old_navigation_result_cannot_settle_reused_request_id() {
        let mut state = BridgeState::new(WebviewId(5), ADMITTED);
        state.bind_webview(0x5000).expect("bind WebView");
        let old_nonce = state.bind().expect("old bind");
        let old = state
            .admit_invoke(&old_nonce, 1, ADMITTED, vec![1])
            .expect("old call");
        state.navigation_started();

        let new_nonce = state.bind().expect("new bind");
        let new = state
            .admit_invoke(&new_nonce, 1, ADMITTED, vec![2])
            .expect("new call reuses local request id");
        assert_ne!(old.navigation(), new.navigation());
        assert_eq!(
            state.settle(old.navigation(), old.request()),
            None,
            "late old outcome must not consume the new document's waiter"
        );
        assert_eq!(
            state.settle(new.navigation(), new.request()),
            Some(new_nonce),
            "current document still settles exactly once, under its own nonce"
        );
    }

    #[test]
    fn facade_and_isolated_scripts_keep_authority_out_of_page_messages() {
        for forbidden in [
            "endpoint",
            "token",
            "principal",
            "generation",
            "grant",
            "documentNonce",
        ] {
            assert!(
                !PAGE_FACADE_SCRIPT.contains(forbidden),
                "page facade leaked authority term {forbidden}"
            );
        }
        assert!(PAGE_FACADE_SCRIPT.contains("writable: false"));
        assert!(PAGE_FACADE_SCRIPT.contains("configurable: false"));
        assert!(PAGE_FACADE_SCRIPT.contains("pagehide"));
        assert!(
            !PAGE_FACADE_SCRIPT.contains(HANDLER_NAME),
            "page-world facade must not reveal the native handler name"
        );
        assert!(ISOLATED_BRIDGE_SCRIPT.contains("document: documentNonce"));
        assert!(ISOLATED_BRIDGE_SCRIPT.contains("__keld_wv_link_v1"));
    }

    /// GH-508 criterion 12: the admitted id is the one the host passes at
    /// construction. A bridge built with id 7 refuses the echo id 1 natively
    /// and in both rendered scripts, and admits 7, so no id is hard-coded.
    #[test]
    fn admitted_channel_is_the_host_supplied_id() {
        let mut state = BridgeState::new(WebviewId(4), 7);
        state.bind_webview(0x4000).expect("bind WebView");
        let nonce = state.bind().expect("bind");
        assert!(matches!(
            state.admit_invoke(&nonce, 1, 1, vec![]),
            Err("renderer channel is not declared by this build")
        ));
        let call = state
            .admit_invoke(&nonce, 2, 7, vec![5])
            .expect("the host-admitted id is admitted");
        assert_eq!(call.channel(), 7);

        for template in [PAGE_FACADE_SCRIPT, ISOLATED_BRIDGE_SCRIPT] {
            assert_eq!(template.matches(ADMITTED_CHANNEL_PLACEHOLDER).count(), 1);
            let rendered = render_bridge_script(template, 7);
            assert!(!rendered.contains(ADMITTED_CHANNEL_PLACEHOLDER));
            assert!(!rendered.contains("channel !== 1"));
        }
        assert!(render_bridge_script(PAGE_FACADE_SCRIPT, 7).contains(
            "if (channel !== 7) return fail(\"renderer channel is not declared by this build\");"
        ));
        assert!(
            render_bridge_script(ISOLATED_BRIDGE_SCRIPT, 7)
                .contains("if (call.channel !== 7 || !Array.isArray(call.payload)")
        );
        assert!(PAGE_FACADE_SCRIPT.contains("coded(\"KELD-WV-011\", \"KELD-WV-011: \" + detail)"));
    }
}
