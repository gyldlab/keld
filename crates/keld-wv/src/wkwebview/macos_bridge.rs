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
use serde::Serialize;
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
  const post = window.postMessage.bind(window);
  const freeze = Object.freeze.bind(Object);
  const define = Object.defineProperty.bind(Object);
  const U8 = Uint8Array;
  let nextRequest = 1;
  let pending = null;

  const fail = (detail) => Promise.reject(new Error("KELD-WV-011: " + detail));

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
      waiter.reject(new Error(detail.startsWith(code) ? detail : code + ": " + detail));
    }
  };
  window.addEventListener("message", onResult, false);
  window.addEventListener("pagehide", () => {
    if (pending === null) return;
    const waiter = pending;
    pending = null;
    waiter.reject(new Error("KELD-WV-011: renderer document navigated before reply"));
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
  let inflight = false;

  const sendNative = (call) => {
    if (documentNonce === null || inflight) return;
    inflight = true;
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

  globalThis.__keldBridgeNativeResult = (result) => {
    if (!result || !Number.isInteger(result.request)) return;
    inflight = false;
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
    if (queued !== null || inflight) {
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

    fn navigation_started(&mut self) {
        self.navigation_generation = self.navigation_generation.wrapping_add(1).max(1);
        self.document_nonce = None;
        self.pending = None;
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
        });
        Ok(RendererBridgeRequest {
            webview: self.webview,
            navigation: self.navigation_generation,
            request,
            channel,
            payload,
        })
    }

    fn settle(&mut self, navigation: u64, request: u32) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.navigation == navigation && pending.request == request)
        {
            self.pending = None;
            true
        } else {
            false
        }
    }
}

#[derive(Clone)]
pub(super) struct BridgeNavigationHandle(Arc<Mutex<BridgeState>>);

impl BridgeNavigationHandle {
    pub(super) fn navigation_started(&self) {
        lock_state(&self.0).navigation_started();
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

    pub(super) fn deliver(
        &self,
        webview: &wry::WryWebView,
        outcome: RendererBridgeOutcome,
    ) -> Result<(), WvError> {
        let request = outcome.request();
        let navigation = outcome.navigation();
        if !lock_state(&self.state).settle(navigation, request) {
            return Ok(());
        }
        let payload = PageOutcome::from(outcome);
        let json = serde_json::to_string(&payload)
            .map_err(|error| bridge_error(format!("failed to encode renderer reply: {error}")))?;
        let script = format!("globalThis.__keldBridgeNativeResult({json});");
        evaluate_in_world(webview, &self.world, &script);
        Ok(())
    }
}

#[derive(Serialize)]
struct PageOutcome {
    request: u32,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl From<RendererBridgeOutcome> for PageOutcome {
    fn from(outcome: RendererBridgeOutcome) -> Self {
        match outcome {
            RendererBridgeOutcome::Reply {
                request, payload, ..
            } => Self {
                request,
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
                ok: false,
                payload: None,
                code: Some(code),
                detail: Some(detail),
            },
        }
    }
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
                    reply_immediate_error(message, &ivars.world, request_id, detail);
                    return;
                }
            };
            report_admission(&call);
            match ivars.requests.try_send(call) {
                Ok(()) => {}
                Err(TrySendError::Full(call) | TrySendError::Disconnected(call)) => {
                    let _ = lock_state(&ivars.state).settle(call.navigation, call.request);
                    reply_immediate_error(
                        message,
                        &ivars.world,
                        call.request,
                        "host renderer dispatch queue is unavailable",
                    );
                }
            }
        }
    }
}

fn reply_immediate_error(
    message: &WKScriptMessage,
    world: &WKContentWorld,
    request: u32,
    detail: &str,
) {
    // SAFETY: callback WebView is live on the main thread.
    let Some(webview) = (unsafe { message.webView() }) else {
        return;
    };
    let outcome = PageOutcome {
        request,
        ok: false,
        payload: None,
        code: Some(String::from("KELD-WV-011")),
        detail: Some(detail.to_owned()),
    };
    let Ok(json) = serde_json::to_string(&outcome) else {
        return;
    };
    let script = format!("globalThis.__keldBridgeNativeResult({json});");
    evaluate_in_world(&webview, world, &script);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The echo id `keld-core` passes today; any host-chosen id works the same.
    const ADMITTED: u16 = 1;

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
        assert!(state.settle(navigation, 2));
        assert!(
            !state.settle(navigation, 2),
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
        assert!(
            !state.settle(old.navigation(), old.request()),
            "late old outcome must not consume the new document's waiter"
        );
        assert!(
            state.settle(new.navigation(), new.request()),
            "current document still settles exactly once"
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
        assert!(PAGE_FACADE_SCRIPT.contains("new Error(\"KELD-WV-011: \" + detail)"));
    }
}
