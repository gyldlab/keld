//! macOS KEL-142 renderer bridge: isolated `WebKit` world plus bounded host handoff.
//!
//! The page-visible window.keld facade is not the authority boundary.
//! Native admission happens only in a non-page content world handler installed
//! on one per-view user-content controller.

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
use crate::original_binding::{OriginalBinding, OriginalCredentialSlot};
use crate::original_call::active::{ActiveOriginalCall, PendingOriginalSend};
use crate::original_call::queue::{
    OriginalCallInput, OriginalCallQueue, OriginalControlFault, OriginalInvocationKind,
    OriginalQueueProgress, PreparingOriginalInvocation,
};
use crate::original_call::{original_call_channel, request_for_lease};
use crate::original_event::PendingOriginalEvent;
use crate::renderer_bridge::credential;
use crate::wv_link::{AppPageCall, AppWvLinkEnvelope, decode_app_wv_link};
use crate::wv_link::{
    WvLinkEnvelope, WvLinkMode, WvLinkRejectReason, decode_wv_link, decode_wv_link_for_mode,
};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod boot_diagnostic;

use crate::preload_image::preload_error;
pub use crate::preload_image::{
    AppPreloadBudget, AppPreloadImage, AppPreloadReservation, MAX_APP_PRELOAD_BYTES,
};

const HANDLER_NAME: &str = "__keld_wv_link_v1";
/// Placeholder in both injected scripts for the host-admitted channel id.
///
/// `keld-wv` has no `keld-ipc` dependency (architecture 01 Â§3): the host passes
/// the admitted id to [`RendererBridgeEndpoint::new`] and
/// [`render_bridge_script`] writes it into each script at construction
/// (GH-508 spec Â§4.6).
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

use crate::renderer_bridge::NativeMessageFacts;
pub use crate::renderer_bridge::{
    RendererBridgeEndpoint, RendererBridgeOperation, RendererBridgeOutcome, RendererBridgeRequest,
    RendererRoleWitness,
};

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
            operation: RendererBridgeOperation::Invoke,
            document_lease: None,
            native_facts: None,
            role_witness: None,
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
pub(super) struct BridgeNavigationHandle(Arc<Mutex<BridgeState>>, bool);

impl BridgeNavigationHandle {
    pub(super) fn navigation_started(&self) {
        if !self.1 {
            lock_state(&self.0).navigation_started();
        }
    }
}

pub(super) struct MacRendererBridge {
    state: Arc<Mutex<BridgeState>>,
    world: Retained<WKContentWorld>,
    controller: Retained<WKUserContentController>,
    handler: Retained<KeldScriptMessageHandler>,
    app_handlers: Vec<Retained<KeldScriptMessageHandler>>,
    app: Option<SharedAppState>,
    app_world: Option<Retained<WKContentWorld>>,
}

struct OriginalBridgeInputs {
    image: Arc<AppPreloadImage>,
    document: std::rc::Rc<std::cell::RefCell<super::macos_document::NativeDocumentOwner>>,
    wake: tao::event_loop::EventLoopProxy<super::WkUserEvent>,
}

pub(super) struct InstalledRendererBridge {
    pub(super) bridge: MacRendererBridge,
    pub(super) outcomes: Option<Receiver<RendererBridgeOutcome>>,
}

struct HandlerIvars {
    state: Arc<Mutex<BridgeState>>,
    requests: SyncSender<RendererBridgeRequest>,
    controller_address: usize,
    world: Retained<WKContentWorld>,
    role: HandlerRole,
    app: Option<SharedAppState>,
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
        role: HandlerRole,
        app: Option<SharedAppState>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let object = mtm.alloc::<Self>().set_ivars(HandlerIvars {
            state,
            requests,
            controller_address,
            world,
            role,
            app,
        });
        // SAFETY: object is freshly allocated with fully initialized ivars.
        unsafe { msg_send![super(object), init] }
    }
}

fn render_isolated_source(original: bool, admitted_channel: u16) -> String {
    let isolated_template = if original {
        ISOLATED_BRIDGE_SCRIPT.replace("v: 1", "v: 2").replace(
                "handler.postMessage(JSON.stringify({ v: 2, kind: \"bind\" }));",
                "globalThis.__keldBridgeNativeTryBind = () => handler.postMessage(JSON.stringify({v:2,kind:'bind'})); globalThis.__keldBridgeNativeTryBind();"
            )
    } else {
        String::from(ISOLATED_BRIDGE_SCRIPT)
    };
    let isolated_template = if original {
        isolated_template.replace("  globalThis.__keldBridgeNativeResult = (result) => {",r#"
  globalThis.__keldBridgeV2Bound = (message) => {
    if(!message||message.v!==2||message.kind!=="bound"||message.document!==globalThis.__keldHostDocumentCredential)return;
    globalThis.__keldBridgeNativeBind(message.document);
  };
  globalThis.__keldBridgeV2Result = (message) => {
    if(!message||message.v!==2||message.kind!=="invoke-result"||message.document!==documentNonce)return;
    const value=message.result;
    globalThis.__keldBridgeNativeResult({request:message.request,ok:value.status==="reply",payload:value.payload,code:value.code,detail:value.detail});
  };
  globalThis.__keldBridgeNativeResult = (result) => {
"#)
    } else {
        isolated_template
    };
    render_bridge_script(&isolated_template, admitted_channel)
}

impl MacRendererBridge {
    pub(super) fn install(
        webview: WebviewId,
        configuration: &WKWebViewConfiguration,
        endpoint: RendererBridgeEndpoint,
    ) -> Result<InstalledRendererBridge, WvError> {
        Self::install_inner(webview, configuration, endpoint, None)
    }
    pub(super) fn install_original(
        webview: WebviewId,
        configuration: &WKWebViewConfiguration,
        endpoint: RendererBridgeEndpoint,
        image: Arc<AppPreloadImage>,
        document: std::rc::Rc<std::cell::RefCell<super::macos_document::NativeDocumentOwner>>,
        wake: tao::event_loop::EventLoopProxy<super::WkUserEvent>,
    ) -> Result<InstalledRendererBridge, WvError> {
        Self::install_inner(
            webview,
            configuration,
            endpoint,
            Some(OriginalBridgeInputs {
                image,
                document,
                wake,
            }),
        )
    }
    fn install_inner(
        webview: WebviewId,
        configuration: &WKWebViewConfiguration,
        endpoint: RendererBridgeEndpoint,
        original: Option<OriginalBridgeInputs>,
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
        let app_world = if original.is_some() {
            // SAFETY: macOS 11+ checked, AppKit UI thread, separate named realm.
            Some(unsafe {
                WKContentWorld::worldWithName(&NSString::from_str("keld.app-preload"), mtm)
            })
        } else {
            None
        };
        let original_scripts = original
            .as_ref()
            .map(|original| render_original_scripts(&original.image))
            .transpose()?;
        let app = original
            .map(|original| Self::original_state(original, endpoint.requests.clone()))
            .transpose()?;
        let handler = KeldScriptMessageHandler::new(
            Arc::clone(&state),
            endpoint.requests,
            Retained::as_ptr(&controller).cast::<()>() as usize,
            isolated_world.clone(),
            HandlerRole::Trusted,
            app.clone(),
            mtm,
        );
        let isolated_source =
            NSString::from_str(&render_isolated_source(app.is_some(), admitted_channel));
        let page_source =
            NSString::from_str(&render_bridge_script(PAGE_FACADE_SCRIPT, admitted_channel));
        let isolated_script = Self::document_start_script(mtm, &isolated_source, &isolated_world);
        let page_script = Self::document_start_script(mtm, &page_source, &page_world);

        let mut app_handlers = Vec::new();
        let mut app_scripts = Vec::new();
        if let (Some(app_state), Some(app_world), Some((page_source, app_source))) =
            (&app, &app_world, original_scripts)
        {
            app_handlers.push(KeldScriptMessageHandler::new(
                Arc::clone(&state),
                handler.ivars().requests.clone(),
                Retained::as_ptr(&controller).cast::<()>() as usize,
                app_world.clone(),
                HandlerRole::App,
                Some(app_state.clone()),
                mtm,
            ));
            for (source, world) in [
                (app_source, app_world.clone()),
                (page_source, page_world.clone()),
            ] {
                app_scripts.push(Self::document_start_script(
                    mtm,
                    &NSString::from_str(&source),
                    &world,
                ));
            }
        }
        let bridge = Self {
            state,
            world: isolated_world,
            controller,
            handler,
            app_handlers,
            app,
            app_world,
        };
        bridge.configure_scripts(configuration, &isolated_script, &page_script, &app_scripts)?;
        Ok(InstalledRendererBridge {
            bridge,
            outcomes: endpoint.outcomes,
        })
    }

    fn original_state(
        original: OriginalBridgeInputs,
        requests: SyncSender<RendererBridgeRequest>,
    ) -> Result<SharedAppState, WvError> {
        let calls = OriginalCallQueue::new()?;
        let OriginalBridgeInputs {
            image: _image,
            document,
            wake,
        } = original;
        Ok(std::rc::Rc::new(std::cell::RefCell::new(AppNativeState {
            document,
            binding: OriginalBinding::default(),
            bootstrap_scripts: Vec::new(),
            calls,
            pending_send: None,
            pending_event: None,
            evaluations: 0,
            retiring_at: None,
            control_fault: OriginalControlFault::default(),
            wake,
            requests,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            fixture: PreloadFixtureState::default(),
        })))
    }

    fn document_start_script(
        mtm: MainThreadMarker,
        source: &NSString,
        world: &WKContentWorld,
    ) -> Retained<WKUserScript> {
        // SAFETY: caller checked macOS11+ and holds this main-thread marker;
        // fixed/admitted source, explicit world, main frame, document start.
        unsafe {
            WKUserScript::initWithSource_injectionTime_forMainFrameOnly_inContentWorld(
                mtm.alloc::<WKUserScript>(),
                source,
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
                world,
            )
        }
    }

    fn configure_scripts(
        &self,
        configuration: &WKWebViewConfiguration,
        isolated_script: &WKUserScript,
        page_script: &WKUserScript,
        app_scripts: &[Retained<WKUserScript>],
    ) -> Result<(), WvError> {
        let proto = ProtocolObject::from_ref(&*self.handler);
        let handler_name = NSString::from_str(HANDLER_NAME);
        let install = objc2::exception::catch(AssertUnwindSafe(|| {
            // SAFETY: fresh per-view controller, one isolated handler name,
            // and two main-frame scripts before WebView construction.
            unsafe {
                self.controller.addScriptMessageHandler_contentWorld_name(
                    proto,
                    &self.world,
                    &handler_name,
                );
                self.controller.addUserScript(isolated_script);
                self.controller.addUserScript(page_script);
                for (handler, name) in self.app_handlers.iter().zip([APP_HANDLER_NAME]) {
                    self.controller.addScriptMessageHandler_contentWorld_name(
                        ProtocolObject::from_ref(&**handler),
                        &handler.ivars().world,
                        &NSString::from_str(name),
                    );
                }
                for script in app_scripts {
                    self.controller.addUserScript(script);
                }
                configuration.setUserContentController(&self.controller);
            }
        }));
        if install.is_err() {
            return Err(bridge_error(
                "WebKit rejected the isolated renderer bridge configuration",
            ));
        }

        Ok(())
    }

    /// Prepare only native-issued bootstrap data for the actual pending attempt.
    /// No document/IPC grant exists until the same owner reports real commit.
    pub(super) fn attempt_hook(&self) -> Option<super::macos_navigation_owner::NativeAttemptHook> {
        let app = self.app.clone()?;
        let app_world = self.app_world.clone()?;
        let trusted_world = self.world.clone();
        let controller = self.controller.clone();
        let native_state = Arc::clone(&self.state);
        Some(std::rc::Rc::new(move |view, attempt| {
            if !lock_state(&native_state)
                .accepts_webview(std::ptr::from_ref(view).cast::<()>() as usize)
            {
                return Err(bridge_error("bootstrap hook used foreign native WebView"));
            }
            {
                let state = app.borrow();
                let owner = state.document.borrow();
                let source = owner
                    .source_binding()
                    .ok_or_else(|| bridge_error("pending bootstrap has no native bound source"))?;
                if attempt.view() != lock_state(&native_state).webview
                    || owner
                        .resource_lease(attempt.0, source.source_id())
                        .is_none()
                {
                    return Err(bridge_error(
                        "bootstrap does not match actual pending NativeAttempt",
                    ));
                }
            }
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                let state = app.borrow();
                match state.fixture.attempt_hook_fault {
                    1 => return Err(bridge_error("debug actual native attempt hook denial")),
                    2 => {
                        state.document.borrow_mut().destroy();
                        return Ok(());
                    }
                    _ => {}
                }
            }
            let app_token = credential()?;
            let trusted_token = credential()?;
            let mtm = MainThreadMarker::new()
                .ok_or_else(|| bridge_error("bootstrap must install on AppKit UI thread"))?;
            let mut replacements = Vec::with_capacity(2);
            for (world, token) in [(&app_world, &app_token), (&trusted_world, &trusted_token)] {
                let encoded = serde_json::to_string(token)
                    .map_err(|_| bridge_error("bootstrap credential serialization failed"))?;
                let text = NSString::from_str(&format!(
                    "Object.defineProperty(globalThis,'__keldHostDocumentCredential',{{value:{encoded},writable:false,configurable:false}});"
                ));
                replacements.push(Self::document_start_script(mtm, &text, world));
            }
            Self::replace_bootstrap(&controller, &app, &replacements)?;
            let mut state = app.borrow_mut();
            let document = state.document.clone();
            let prepared = state.binding.prepare_attempt(
                &document.borrow(),
                attempt,
                app_token,
                trusted_token,
            );
            // Native replacement already succeeded. Retain those exact objects
            // and clear/cancel old bindings even if reentry invalidated the attempt.
            state.bootstrap_scripts = replacements;
            if prepared.is_err() {
                state.binding.invalidate_prepared();
            }
            state.binding.clear_for_attempt();
            state.cancel_queued_originals();
            drop(state);
            flush_app_controls(&app);
            prepared.map_err(bridge_error)
        }))
    }

    fn replace_bootstrap(
        controller: &WKUserContentController,
        app: &SharedAppState,
        replacements: &[Retained<WKUserScript>],
    ) -> Result<(), WvError> {
        // Snapshot native script owners; retain all Wry/other script objects
        // in the original order. Only our prior bootstrap pointers are removed.
        // SAFETY: same live per-view controller on its creating AppKit thread.
        let actual = unsafe { controller.userScripts() };
        if actual.count() > 64 {
            return Err(bridge_error("native user-script owner bound exceeded"));
        }
        let prior = app.borrow().bootstrap_scripts.clone();
        let retained: Vec<_> = actual
            .iter()
            .filter(|script| {
                !prior
                    .iter()
                    .any(|owned| Retained::as_ptr(owned) == Retained::as_ptr(script))
            })
            .collect();
        if retained.len() + replacements.len() > 64 {
            return Err(bridge_error(
                "replacement native user-script owner bound exceeded",
            ));
        }
        let install = objc2::exception::catch(AssertUnwindSafe(|| {
            // SAFETY: replace only owned bootstrap entries while retaining
            // every actual other script object in its original order.
            unsafe {
                controller.removeAllUserScripts();
                for script in &retained {
                    controller.addUserScript(script);
                }
                for script in replacements {
                    controller.addUserScript(script);
                }
            }
        }));
        // Verify the actual installed objects and order before admitting load.
        // No API assumption that WebKit retained identity is authority.
        // SAFETY: same actual native UI-owned user-content controller.
        let observed = unsafe { controller.userScripts() };
        let identity_preserved = observed.count() == retained.len() + replacements.len()
            && observed
                .iter()
                .zip(retained.iter().chain(replacements.iter()))
                .all(|(actual, expected)| Retained::as_ptr(&actual) == Retained::as_ptr(expected));
        if install.is_err() || !identity_preserved {
            let mut state = app.borrow_mut();
            state.control_fault.latch();
            state.document.borrow_mut().destroy();
            return Err(bridge_error(
                "WebKit rejected native attempt bootstrap installation; close this poisoned view",
            ));
        }
        Ok(())
    }

    pub(super) fn commit_hook(&self) -> Option<super::macos_navigation_owner::NativeCommitHook> {
        let app = self.app.clone()?;
        let app_world = self.app_world.clone()?;
        let trusted_world = self.world.clone();
        let native_state = Arc::clone(&self.state);
        Some(std::rc::Rc::new(move |view, stamp| {
            if !lock_state(&native_state)
                .accepts_webview(std::ptr::from_ref(view).cast::<()>() as usize)
            {
                return Err(bridge_error("commit hook used a foreign native WebView"));
            }
            {
                let mut state = app.borrow_mut();
                if state.document.borrow().current_stamp() != Some(stamp.0) {
                    return Err(bridge_error(
                        "commit hook stamp differs from living native owner",
                    ));
                }
                state.synchronize_commit()?;
                let mut trusted = lock_state(&native_state);
                trusted.navigation_generation = u64::from(stamp.observed_generation());
                trusted.document_nonce = None;
                trusted.pending = None;
            }
            // Initial binds are admitted by actual world/controller/mainframe/view
            // callbacks. Hook asks installed scripts to request them; it does not
            // queue early page work or hand a page any credential.
            let app_bind: std::borrow::Cow<'_, str> =
                std::borrow::Cow::Borrowed("globalThis.__keldAppTryBind?.(); void 0;");
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            let app_bind = if boot_diagnostic::enabled() {
                let token = {
                    let state = app.borrow();
                    serde_json::to_string(
                        state
                            .binding
                            .credential(OriginalCredentialSlot::App)
                            .ok_or_else(|| {
                                bridge_error("boot diagnostic has no current native app binding")
                            })?,
                    )
                    .map_err(|_| bridge_error("boot diagnostic binding serialization refused"))?
                };
                std::borrow::Cow::Owned(format!(
                    "globalThis.__keldBootDiagnosticBind?.({token});{app_bind}"
                ))
            } else {
                app_bind
            };
            evaluate_owned(
                view,
                &app_world,
                &app_bind,
                app.clone(),
                EvaluationPurpose::Control,
            );
            evaluate_owned(
                view,
                &trusted_world,
                "globalThis.__keldBridgeNativeTryBind?.(); void 0;",
                app.clone(),
                EvaluationPurpose::Control,
            );
            Ok(())
        }))
    }

    pub(super) fn request_sender(&self) -> (SyncSender<RendererBridgeRequest>, u16) {
        (
            self.handler.ivars().requests.clone(),
            lock_state(&self.state).admitted_channel,
        )
    }
    pub(super) fn navigation_handle(&self) -> BridgeNavigationHandle {
        BridgeNavigationHandle(Arc::clone(&self.state), self.app.is_some())
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
        if let Some(app) = &self.app {
            let mut state = app.borrow_mut();
            state.document.borrow_mut().destroy();
            state
                .retiring_at
                .get_or_insert_with(std::time::Instant::now);
            state.binding.clear_credentials();
            state.cancel_queued_originals();
            drop(state);
            flush_app_controls(app);
        }
        lock_state(&self.state).destroy();
    }
    pub(super) fn flush_controls(&self) {
        if let Some(app) = &self.app {
            flush_app_controls(app);
        }
    }
    pub(super) fn service_original_calls(&self, webview: &wry::WryWebView) -> Result<(), WvError> {
        if let Some(app) = &self.app {
            let world = self
                .app_world
                .as_ref()
                .ok_or_else(|| bridge_error("original bridge lacks app world"))?;
            pump_original_call(webview, world, app)?;
        }
        Ok(())
    }
    pub(super) fn retirement_deadline(&self) -> Option<std::time::Instant> {
        self.app
            .as_ref()?
            .borrow()
            .retiring_at?
            .checked_add(std::time::Duration::from_secs(5))
    }
    pub(super) fn retirement_requested(&self) -> bool {
        self.app
            .as_ref()
            .is_some_and(|app| app.borrow().retiring_at.is_some())
    }
    pub(super) fn try_finish_retirement(&self) -> Result<bool, WvError> {
        let Some(app) = &self.app else {
            return Ok(true);
        };
        flush_app_controls(app);
        let state = app.borrow();
        if state.evaluations_drained() {
            return Ok(true);
        }
        if state.control_fault.is_faulted()
            || state
                .retiring_at
                .is_some_and(|started| started.elapsed() >= std::time::Duration::from_secs(5))
        {
            return Err(bridge_error(
                "original preload evaluation/control quiescence unproved; retain native view and image custody",
            ));
        }
        Ok(false)
    }

    pub(super) fn deliver(
        &self,
        webview: &wry::WryWebView,
        outcome: RendererBridgeOutcome,
    ) -> Result<(), WvError> {
        if matches!(outcome, RendererBridgeOutcome::ControlCapacity) {
            return self.service_original_calls(webview);
        }
        let Some(app) = &self.app else {
            return self.deliver_numeric(webview, outcome);
        };
        let Some(world) = &self.app_world else {
            return Err(bridge_error("original bridge lacks app world"));
        };
        flush_app_controls(app);
        let navigation = outcome.navigation();
        match outcome {
            RendererBridgeOutcome::Preflight {
                request, accepted, ..
            } => Self::deliver_preflight(app, navigation, request, accepted),
            RendererBridgeOutcome::Event {
                request,
                channel,
                payload,
                ..
            } => Self::deliver_event(webview, world, app, navigation, request, &channel, &payload),
            other => self.deliver_app_other(webview, world, app, other),
        }?;
        self.service_original_calls(webview)
    }

    fn deliver_preflight(
        app: &SharedAppState,
        navigation: u64,
        request: u32,
        accepted: bool,
    ) -> Result<(), WvError> {
        let mut state = app.borrow_mut();
        let AppNativeState {
            calls,
            binding,
            document,
            control_fault,
            ..
        } = &mut *state;
        let result =
            calls.deliver_preflight(navigation, request, accepted, binding, &document.borrow());
        if result.is_err() {
            control_fault.latch();
        }
        let needs_service_wake = result
            .as_ref()
            .is_ok_and(|progress| progress.needs_service_wake);
        drop(state);
        post_original_service_wake(app, needs_service_wake);
        result.map(|_| ())
    }

    fn deliver_event(
        webview: &wry::WryWebView,
        world: &WKContentWorld,
        app: &SharedAppState,
        navigation: u64,
        request: u32,
        channel: &str,
        payload: &[u8],
    ) -> Result<(), WvError> {
        let mut state = app.borrow_mut();
        if state.retiring_at.is_some()
            || state.control_fault.is_faulted()
            || state.pending_event.is_some()
            || request == 0
            || channel.len() > 256
            || payload.len() > 4096
            || !state.binding.registered()
        {
            return Err(bridge_error(
                "addressed event cannot enter bounded native delivery",
            ));
        }
        let stamp = state
            .current()
            .filter(|stamp| u64::from(stamp.generation.get()) == navigation)
            .ok_or_else(|| bridge_error("addressed event targets an old native document"))?;
        let lease = state
            .document
            .borrow()
            .lease_for(stamp)
            .map(super::MacNativeDocumentLease)
            .ok_or_else(|| bridge_error("addressed event native lease revoked"))?;
        let token = state
            .binding
            .credential(OriginalCredentialSlot::App)
            .map(str::to_owned)
            .ok_or_else(|| bridge_error("addressed event has no current app binding"))?;
        let json=serde_json::to_string(&serde_json::json!({"v":2,"kind":"event","document":token,"delivery":request,"channel":channel,"payload":payload})).map_err(|_|bridge_error("addressed event serialization failed"))?;
        let event =
            PendingOriginalEvent::begin(&state.document.borrow(), request, lease.clone(), token)
                .map_err(bridge_error)?;
        state.pending_event = Some(event);
        drop(state);
        evaluate_owned(
            webview,
            world,
            &format!("globalThis.__keldAppNativeEvent({json});"),
            app.clone(),
            EvaluationPurpose::Event { id: request, lease },
        );
        Ok(())
    }

    fn deliver_app_other(
        &self,
        webview: &wry::WryWebView,
        world: &WKContentWorld,
        app: &SharedAppState,
        outcome: RendererBridgeOutcome,
    ) -> Result<(), WvError> {
        let navigation = outcome.navigation();
        let mut state = app.borrow_mut();
        match outcome {
            RendererBridgeOutcome::SendResult {
                request,
                disposition,
                ..
            } => {
                if state
                    .pending_send
                    .as_ref()
                    .is_none_or(|send| !send.matches_result(navigation, request))
                {
                    return Ok(());
                }
                let Some(send) = state.pending_send.take() else {
                    return Ok(());
                };
                if send.lease().is_current() && state.retiring_at.is_none() {
                    let token = state
                        .binding
                        .credential(OriginalCredentialSlot::App)
                        .ok_or_else(|| {
                            bridge_error("terminal result has no current app binding")
                        })?;
                    let json=serde_json::to_string(&serde_json::json!({"v":2,"kind":"send-result","document":token,"message":request,"disposition":disposition})).map_err(|_|bridge_error("private terminal serialization failed"))?;
                    drop(state);
                    evaluate_owned(
                        webview,
                        world,
                        &format!("globalThis.__keldAppNativeTerminal({json});"),
                        app.clone(),
                        EvaluationPurpose::Control,
                    );
                }
                Ok(())
            }
            RendererBridgeOutcome::EventReleased { request, .. } => {
                state.release_event(navigation, request)
            }
            RendererBridgeOutcome::RouteState { ready, role, .. } => {
                if state.retiring_at.is_some() {
                    return Ok(());
                }
                let document = state.document.clone();
                if !state
                    .binding
                    .observe_route(&document.borrow(), navigation, ready, role)
                {
                    return Ok(());
                }
                let token = serde_json::to_string(
                    state
                        .binding
                        .credential(OriginalCredentialSlot::App)
                        .ok_or_else(|| {
                            bridge_error("route state has no current private binding")
                        })?,
                )
                .map_err(|_| bridge_error("route state serialization failed"))?;
                if !ready {
                    state.calls.cancel_pending();
                }
                drop(state);
                flush_app_controls(app);
                evaluate_owned(
                    webview,
                    world,
                    &format!("globalThis.__keldAppRouteState({token},{ready});"),
                    app.clone(),
                    EvaluationPurpose::Control,
                );
                Ok(())
            }
            RendererBridgeOutcome::ControlCapacity => Ok(()),
            numeric => {
                if state
                    .current()
                    .is_none_or(|stamp| u64::from(stamp.generation.get()) != navigation)
                    || state.retiring_at.is_some()
                {
                    return Ok(());
                }
                drop(state);
                self.deliver_numeric(webview, numeric)
            }
        }
    }
    fn deliver_numeric(
        &self,
        webview: &wry::WryWebView,
        outcome: RendererBridgeOutcome,
    ) -> Result<(), WvError> {
        let request = outcome.request();
        let navigation = outcome.navigation();
        if !lock_state(&self.state).settle(navigation, request) {
            return Ok(());
        }
        let Some(payload) = PageOutcome::from_numeric(outcome) else {
            return Ok(());
        };
        let json = serde_json::to_string(&payload)
            .map_err(|error| bridge_error(format!("failed to encode renderer reply: {error}")))?;
        if let Some(app) = &self.app {
            let state = app.borrow();
            let token = state
                .binding
                .credential(OriginalCredentialSlot::Trusted)
                .ok_or_else(|| bridge_error("numeric result has no current trusted binding"))?;
            let result = if payload.ok {
                serde_json::json!({"status":"reply","payload":payload.payload})
            } else {
                serde_json::json!({"status":"error","code":payload.code,"detail":payload.detail})
            };
            let private=serde_json::to_string(&serde_json::json!({"v":2,"kind":"invoke-result","document":token,"request":request,"result":result})).map_err(|_|bridge_error("private numeric result serialization failed"))?;
            drop(state);
            evaluate_owned(
                webview,
                &self.world,
                &format!("globalThis.__keldBridgeV2Result({private});"),
                app.clone(),
                EvaluationPurpose::Control,
            );
        } else {
            let script = format!("globalThis.__keldBridgeNativeResult({json});");
            evaluate_in_world(webview, &self.world, &script);
        }
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

impl PageOutcome {
    fn from_numeric(outcome: RendererBridgeOutcome) -> Option<Self> {
        match outcome {
            RendererBridgeOutcome::Reply {
                request, payload, ..
            } => Some(Self {
                request,
                ok: true,
                payload: Some(payload),
                code: None,
                detail: None,
            }),
            RendererBridgeOutcome::Error {
                request,
                code,
                detail,
                ..
            } => Some(Self {
                request,
                ok: false,
                payload: None,
                code: Some(code),
                detail: Some(detail),
            }),
            _ => None,
        }
    }
}

/// Preserve the complete native string, including any rejecting NUL/tail.
/// C-string prefix decoding would disagree with the closed wire parser.
fn bounded_native_message_text(body: &NSString, maximum: usize) -> Result<String, WvError> {
    // UTF-16 is checked before any UTF-8 representation request. Then the
    // owning Foundation UTF-8 length bounds the safe complete Rust copy.
    if body.len_utf16() > maximum {
        return Err(bridge_error(
            "native renderer envelope exceeds its string bound",
        ));
    }
    let bytes = body.len();
    if bytes == 0 || bytes > maximum {
        return Err(bridge_error(
            "native renderer envelope has invalid or excessive UTF-8 length",
        ));
    }
    let text = body.to_string();
    if text.len() != bytes || text.contains('\0') {
        return Err(bridge_error(
            "native renderer envelope contains NUL or changed encoding",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod native_message_text_tests {
    use super::*;
    #[test]
    fn native_nul_cannot_hide_foreign_tail_from_closed_decoder() {
        let body = NSString::from_str("{\"v\":2,\"kind\":\"bind\"}\0foreign");
        assert!(
            bounded_native_message_text(&body, crate::wv_link::MAX_WV_LINK_ENVELOPE_LEN).is_err()
        );
    }
    #[test]
    fn native_utf8_bound_is_checked_without_repair_or_clipping() {
        assert!(bounded_native_message_text(&NSString::from_str("Ã©"), 1).is_err());
        assert_eq!(
            bounded_native_message_text(&NSString::from_str("Ã©"), 2).expect("exact UTF8"),
            "Ã©"
        );
        assert!(bounded_native_message_text(&NSString::from_str("ab"), 1).is_err());
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
    let maximum = if ivars.app.is_some() && ivars.role == HandlerRole::App {
        crate::wv_link::MAX_APP_WV_LINK_ENVELOPE_LEN
    } else {
        crate::wv_link::MAX_WV_LINK_ENVELOPE_LEN
    };
    let text = match bounded_native_message_text(&body, maximum) {
        Ok(text) => text,
        Err(error) => {
            report_detail_rejection(&error.to_string());
            return;
        }
    };
    let text = text.as_str();
    if ivars.app.is_some() {
        if let Err(error) = handle_compat_message(handler, &webview, &frame, text) {
            report_detail_rejection(&error.to_string());
        }
    } else {
        handle_legacy_message(handler, &webview, message, text);
    }
}

fn handle_compat_message(
    handler: &KeldScriptMessageHandler,
    webview: &WKWebView,
    frame: &WKFrameInfo,
    text: &str,
) -> Result<(), WvError> {
    let ivars = handler.ivars();
    let facts = ivars
        .app
        .as_ref()
        .and_then(|app| app.borrow().document.borrow().source_binding())
        .map(|source| capture_native_message_facts(frame, webview, &source))
        .transpose();
    let facts = facts?;
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if ivars.role == HandlerRole::App
        && let Some(facts) = &facts
    {
        match fixture_origin_probe(handler, text, facts) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => return Err(error),
        }
    }
    if ivars.role == HandlerRole::App {
        decode_app_wv_link(text.as_bytes())
            .map_err(|reason| bridge_error(reject_detail(reason)))
            .and_then(|input| match input {
                AppWvLinkEnvelope::Transport(envelope) => {
                    handle_compat_envelope(handler, webview, envelope, facts)
                }
                AppWvLinkEnvelope::Call(call) => handle_app_call(handler, webview, call, facts),
                local => handle_app_control(handler, webview, local),
            })
    } else {
        decode_wv_link_for_mode(text.as_bytes(), WvLinkMode::CompatV2)
            .map_err(|reason| bridge_error(reject_detail(reason)))
            .and_then(|envelope| handle_compat_envelope(handler, webview, envelope, facts))
    }
}

fn handle_legacy_message(
    handler: &KeldScriptMessageHandler,
    webview: &WKWebView,
    message: &WKScriptMessage,
    text: &str,
) {
    let ivars = handler.ivars();
    let envelope = match decode_wv_link(text.as_bytes()) {
        Ok(envelope) => envelope,
        Err(reason) => {
            report_rejection(reason);
            return;
        }
    };

    match envelope {
        WvLinkEnvelope::Send { .. } | WvLinkEnvelope::Consumed { .. } => {
            report_detail_rejection("named envelope cannot use a legacy endpoint");
        }
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
            evaluate_in_world(webview, &ivars.world, &script);
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
        WvLinkRejectReason::InvalidNamedChannel => "invalid literal renderer topic",
        WvLinkRejectReason::InvalidDocument => "invalid private world credential",
        WvLinkRejectReason::ControlTooLarge => "renderer control envelope exceeds 4096 bytes",
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

const APP_HANDLER_NAME: &str = "__keld_app_world";
const PRELOAD_RUNTIME: &str = include_str!("macos_preload_runtime.generated.js");

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandlerRole {
    Trusted,
    App,
}

type SharedAppState = std::rc::Rc<std::cell::RefCell<AppNativeState>>;
struct AppNativeState {
    document: std::rc::Rc<std::cell::RefCell<super::macos_document::NativeDocumentOwner>>,
    // Pure binding metadata drops as one group. Native script installation,
    // retirement and source-image custody remain in this platform adapter.
    binding: OriginalBinding,
    bootstrap_scripts: Vec<Retained<WKUserScript>>,
    calls: OriginalCallQueue<String>,
    pending_send: Option<PendingOriginalSend>,
    pending_event: Option<PendingOriginalEvent>,
    evaluations: usize,
    retiring_at: Option<std::time::Instant>,
    control_fault: OriginalControlFault,
    wake: tao::event_loop::EventLoopProxy<super::WkUserEvent>,
    requests: SyncSender<RendererBridgeRequest>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fixture: PreloadFixtureState,
}

impl AppNativeState {
    fn current(&self) -> Option<super::macos_document::NativeCommittedStamp> {
        self.binding.current(&self.document.borrow())
    }
    fn synchronize_commit(&mut self) -> Result<(), WvError> {
        let candidate = self
            .binding
            .take_matching_commit(&self.document.borrow())
            .map_err(bridge_error)?;
        let Some(candidate) = candidate else {
            return Ok(());
        };
        // The owner borrow ended before existing cancellation. Finish rechecks
        // the exact same checked lease/source after that work.
        self.cancel_queued_originals();
        self.binding
            .finish_commit(&self.document.borrow(), candidate)
            .map_err(bridge_error)
    }
    fn authentic(&self, supplied: &str, role: HandlerRole) -> bool {
        let slot = match role {
            HandlerRole::App => OriginalCredentialSlot::App,
            HandlerRole::Trusted => OriginalCredentialSlot::Trusted,
        };
        self.binding
            .authentic(&self.document.borrow(), supplied, slot)
    }
}

fn render_original_scripts(image: &AppPreloadImage) -> Result<(String, String), WvError> {
    #[derive(Serialize)]
    struct Facts<'a> {
        argv: &'a [String],
        versions: &'a std::collections::BTreeMap<String, String>,
    }
    let facts = serde_json::to_string(&Facts {
        argv: image.additional_arguments(),
        versions: image.versions(),
    })
    .map_err(|_| preload_error("canonical process facts serialization refused"))?;
    let prefix = format!(
        "(() => {{\n{PRELOAD_RUNTIME}\nconst runtime=globalThis.__keldOriginalPreloadRuntime; delete globalThis.__keldOriginalPreloadRuntime;\n"
    );
    let page = format!("{prefix}runtime.installPage({facts});\n}})();");
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let page = if boot_diagnostic::enabled() {
        format!("{}\n{page}", boot_diagnostic::page_script())
    } else {
        page
    };
    // The exact independently authenticated body is inserted verbatim. No
    // caller-provided module text, evaluation or loader fallback is accepted.
    let app = format!(
        "{prefix}runtime.installApp({facts},function(require,process,exports,module,Buffer,global,setImmediate,clearImmediate){{\n{}\n}});\n}})();",
        image.source()
    );
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let app = if boot_diagnostic::enabled() {
        format!("{}\n{app}", boot_diagnostic::app_script())
    } else {
        app
    };
    Ok((page, app))
}

fn handle_app_call(
    handler: &KeldScriptMessageHandler,
    webview: &WKWebView,
    call: AppPageCall,
    facts: Option<NativeMessageFacts>,
) -> Result<(), WvError> {
    let ivars = handler.ivars();
    let app = ivars
        .app
        .as_ref()
        .ok_or_else(|| bridge_error("app handler has no source image"))?;
    flush_app_controls(app);
    let mut state = app.borrow_mut();
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    {
        state.fixture.call_ingress = state.fixture.call_ingress.saturating_add(1);
        state.fixture.last_call_cut = "received";
    }

    if state.retiring_at.is_some()
        || !state.binding.registered()
        || !state.authentic(&call.document, HandlerRole::App)
    {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if state.fixture.held_credential.as_deref() == Some(call.document.as_str())
            && !state.authentic(&call.document, HandlerRole::App)
        {
            state.fixture.old_replays_observed =
                state.fixture.old_replays_observed.saturating_add(1);
            let _ = state.wake.send_event(super::WkUserEvent::RendererCustody);
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        {
            state.fixture.last_call_cut = "credential-or-registration-rejected";
        }
        return Err(bridge_error(
            "early, retired, missing or swapped app credential; no work retained",
        ));
    }
    let channel = original_call_channel(&call)?;
    if state.control_fault.is_faulted()
        || (channel.is_some() && lock_state(&ivars.state).pending.is_some())
    {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if std::env::var_os("KELD_KEL142_ACCEPTANCE_REPORT").is_some() {
            eprintln!(
                "KELD_KEL142_CALL_BUSY call={} method={} channel={:?} pending_call={} active_call={:?} active_forwarded={} pending_send={} pending_event={} control_fault={}",
                call.call,
                call.method,
                channel,
                state.calls.has_pending(),
                state.calls.active().map(ActiveOriginalCall::id),
                state
                    .calls
                    .active()
                    .is_some_and(ActiveOriginalCall::send_forwarded),
                state.pending_send.is_some(),
                state.pending_event.is_some(),
                state.control_fault.is_faulted(),
            );
        }
        return Err(bridge_error("bounded renderer admission is occupied"));
    }
    let stamp = state
        .current()
        .ok_or_else(|| bridge_error("native document is retired"))?;
    let lease = state
        .document
        .borrow()
        .lease_for(stamp)
        .map(super::MacNativeDocumentLease)
        .ok_or_else(|| bridge_error("native document lease is revoked"))?;
    if !state.original_current(&lease, &call.document) {
        return Err(bridge_error(
            "original call lost its exact native source binding",
        ));
    }
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if state.fixture.hold_next_call {
        state.fixture.hold_next_call = false;
        state.fixture.last_call_cut = "held-native-ingress";
        if state.fixture.held_call.is_some() {
            return Err(bridge_error(
                "debug fixture already retains one real message",
            ));
        }
        state.fixture.held_credential = Some(call.document.clone());
        state.fixture.held_call=Some(serde_json::to_string(&serde_json::json!({"v":1,"kind":"call","document":call.document,"call":call.call,"method":call.method,"args":call.args})).map_err(|_|bridge_error("held message serialization refused"))?);
        let _ = state.wake.send_event(super::WkUserEvent::RendererCustody);
        return Ok(());
    }
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    {
        state.fixture.last_call_cut = if channel.is_some() {
            "preflight-forwarded"
        } else {
            "local-function-scheduled"
        };
    }
    let progress = queue_original(&mut state, call, lease, channel, facts)?;
    drop(state);
    post_original_service_wake(app, progress.needs_service_wake);
    pump_original_call(webview, &ivars.world, app)
}

/// Ephemeral service notification only. The proxy is cloned under the existing
/// owner, then every RefCell guard ends before the same original wake is posted.
fn post_original_service_wake(app: &SharedAppState, needs_service_wake: bool) {
    if needs_service_wake {
        let wake = app.borrow().wake.clone();
        let _ = wake.send_event(super::WkUserEvent::RendererCustody);
    }
}
fn flush_app_controls(app: &SharedAppState) {
    let needs_service_wake = app.borrow_mut().flush_controls();
    post_original_service_wake(app, needs_service_wake);
}

fn pump_original_call(
    webview: &WKWebView,
    world: &WKContentWorld,
    app: &SharedAppState,
) -> Result<(), WvError> {
    flush_app_controls(app);
    let progress = app.borrow_mut().prepare_next_original()?;
    post_original_service_wake(app, progress.needs_service_wake);
    let result = match progress.value {
        Some(prepared) => {
            let purpose = EvaluationPurpose::Call {
                id: prepared.identity().id(),
                lease: prepared.identity().lease().clone(),
            };
            evaluate_owned_inner(webview, world, "", app.clone(), purpose, Some(prepared))
        }
        None => Ok(()),
    };
    if result.is_err() {
        app.borrow_mut().control_fault.latch();
    }
    result
}
fn queue_original(
    state: &mut AppNativeState,
    call: AppPageCall,
    lease: super::MacNativeDocumentLease,
    channel: Option<String>,
    facts: Option<NativeMessageFacts>,
) -> Result<OriginalQueueProgress<()>, WvError> {
    // Capture only the actual existing ingress role. No later preflight or
    // native preparation can silently attach to a successor Bun generation.
    let role_witness = if channel.is_some() {
        Some(
            state
                .binding
                .route_role()
                .filter(|role| role.is_live())
                .ok_or_else(|| bridge_error("original effect has no live authenticated role"))?
                .clone(),
        )
    } else {
        None
    };
    let id = call.call;
    let json = serde_json::to_string(&call)
        .map_err(|_| bridge_error("original call serialization failed"))?;
    let token = serde_json::to_string(&call.document)
        .map_err(|_| bridge_error("private call credential serialization failed"))?;
    let source = if channel.is_some() {
        format!(
            "globalThis.__keldAppRouteState({token},true);globalThis.__keldAppNativeCall({json});"
        )
    } else {
        format!("globalThis.__keldAppNativeCall({json});")
    };
    // Native facts retain their existing source-account charge; move them
    // unchanged through this same queue and never reconstruct them on service.
    state.calls.enqueue(
        OriginalCallInput {
            id,
            lease,
            payload_bytes: source.capacity(),
            payload: source,
            credential: call.document,
            channel,
            role_witness,
            facts,
        },
        &state.binding,
        &state.document.borrow(),
        &state.requests,
        state.pending_send.as_ref(),
        &mut state.control_fault,
    )
}

/// Render only the existing private refusal in the original String allocation.
/// The canonical JSON serializer writes into a bounded adapter, without a new
/// UTF-8 rendering buffer. Any partial/error result stays retained as Unproved.
fn prepare_original_payload(
    source: &mut String,
    kind: OriginalInvocationKind,
    id: u32,
    credential: &str,
    charged_bytes: usize,
) -> Result<usize, WvError> {
    if source.capacity() > charged_bytes {
        return Err(bridge_error(
            "original source allocation exceeds its existing charge",
        ));
    }
    if kind == OriginalInvocationKind::Refusal {
        use std::io::Write as _;
        source.clear();
        let mut output = ExistingOriginalString(source);
        output
            .write_all(b"globalThis.__keldAppNativeRefusal(")
            .map_err(|_| bridge_error("private refusal exceeds its retained source allocation"))?;
        serde_json::to_writer(&mut output, credential)
            .map_err(|_| bridge_error("cancelled call credential serialization failed"))?;
        write!(&mut output, ",{id});")
            .map_err(|_| bridge_error("private refusal exceeds its retained source allocation"))?;
    }
    Ok(source.capacity())
}
struct ExistingOriginalString<'a>(&'a mut String);
impl std::io::Write for ExistingOriginalString<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
        if bytes.len() > self.0.capacity().saturating_sub(self.0.len()) {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        self.0.push_str(text);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod original_source_carrier_tests {
    use super::*;
    #[test]
    fn original_source_keeps_its_bytes_and_allocation() {
        let mut source = String::from("globalThis.__keldAppNativeCall({});");
        let before = source.clone();
        let pointer = source.as_ptr();
        let capacity = source.capacity();
        assert_eq!(
            prepare_original_payload(
                &mut source,
                OriginalInvocationKind::Original,
                7,
                "credential",
                capacity,
            )
            .unwrap(),
            capacity
        );
        assert_eq!(source, before);
        assert_eq!(source.as_ptr(), pointer);
    }
    #[test]
    fn refusal_uses_canonical_escaping_in_the_existing_allocation() {
        let mut source = String::with_capacity(256);
        source.push_str("original admitted invocation");
        let capacity = source.capacity();
        let pointer = source.as_ptr();
        let credential = "quoted\\\"\\ncredential";
        prepare_original_payload(
            &mut source,
            OriginalInvocationKind::Refusal,
            u32::MAX,
            credential,
            capacity,
        )
        .unwrap();
        assert_eq!(
            source,
            format!(
                "globalThis.__keldAppNativeRefusal({},{});",
                serde_json::to_string(credential).unwrap(),
                u32::MAX
            )
        );
        assert_eq!(source.capacity(), capacity);
        assert_eq!(source.as_ptr(), pointer);
    }
    #[test]
    fn refusal_cannot_grow_a_source_beyond_its_original_charge() {
        let mut source = String::from("x");
        let capacity = source.capacity();
        let pointer = source.as_ptr();
        assert!(
            prepare_original_payload(
                &mut source,
                OriginalInvocationKind::Refusal,
                1,
                "credential",
                capacity,
            )
            .is_err()
        );
        assert_eq!(source.capacity(), capacity);
        assert_eq!(source.as_ptr(), pointer);
    }
}

fn handle_app_control(
    handler: &KeldScriptMessageHandler,
    webview: &WKWebView,
    control: AppWvLinkEnvelope,
) -> Result<(), WvError> {
    let ivars = handler.ivars();
    let app = ivars
        .app
        .as_ref()
        .ok_or_else(|| bridge_error("missing app runtime"))?;
    let mut state = app.borrow_mut();
    match control {
        AppWvLinkEnvelope::CallFinished { document, call } => {
            state
                .calls
                .active_mut()
                .ok_or_else(|| {
                    bridge_error(
                        "original invocation completion does not match retained physical custody",
                    )
                })?
                .observe_function_finished(call, &document)
                .map_err(bridge_error)?;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                state.fixture.finished_calls += 1;
            }
            drop(state);
            flush_app_controls(app);
            pump_original_call(webview, &ivars.world, app)?;
        }
        AppWvLinkEnvelope::Registered { document, .. } => {
            if state.retiring_at.is_some() {
                return Err(bridge_error(
                    "private registry binding is stale or duplicate",
                ));
            }
            let owner = state.document.clone();
            state
                .binding
                .register(&owner.borrow(), &document)
                .map_err(bridge_error)?;
            drop(state);
            flush_app_controls(app);
            let state = app.borrow();
            let token = serde_json::to_string(
                state
                    .binding
                    .credential(OriginalCredentialSlot::App)
                    .ok_or_else(|| bridge_error("registration has no current private binding"))?,
            )
            .map_err(|_| bridge_error("registration token serialization failed"))?;
            drop(state);
            evaluate_owned(
                webview,
                &ivars.world,
                &format!("globalThis.__keldAppNativeRegistered({token});"),
                app.clone(),
                EvaluationPurpose::Control,
            );
        }
        AppWvLinkEnvelope::ReleaseCall { document, call } => {
            if !state.authentic(&document, HandlerRole::App) || state.retiring_at.is_some() {
                return Err(bridge_error(
                    "old original cleanup cannot affect a successor page",
                ));
            }
            let token = serde_json::to_string(&document)
                .map_err(|_| bridge_error("private cleanup serialization failed"))?;
            drop(state);
            evaluate_owned(
                webview,
                &ivars.world,
                &format!("globalThis.__keldAppNativeRefusal({token},{call});"),
                app.clone(),
                EvaluationPurpose::Control,
            );
        }
        _ => return Err(bridge_error("foreign app local operation")),
    }
    Ok(())
}

fn handle_compat_envelope(
    handler: &KeldScriptMessageHandler,
    webview: &WKWebView,
    envelope: WvLinkEnvelope,
    facts: Option<NativeMessageFacts>,
) -> Result<(), WvError> {
    let ivars = handler.ivars();
    let app = ivars
        .app
        .as_ref()
        .ok_or_else(|| bridge_error("compat handler has no immutable original image"))?;
    match envelope {
        WvLinkEnvelope::Bind => return bind_compat_document(handler, webview, app),
        WvLinkEnvelope::Send {
            document,
            message,
            channel,
            payload,
        } => return send_compat_message(handler, app, &document, message, channel, payload, facts),
        WvLinkEnvelope::Consumed {
            document,
            delivery,
            disposition,
        } => {
            if ivars.role != HandlerRole::App {
                return Err(bridge_error("wrong world for app consumer completion"));
            }
            let mut state = app.borrow_mut();
            // Terminal cleanup authenticates against the retained ORIGINAL event
            // record even after logical revocation; it grants no new delivery.
            state
                .pending_event
                .as_mut()
                .ok_or_else(|| bridge_error("stale, unsolicited or duplicate consumer completion"))?
                .observe_consumed(delivery.get(), &document, disposition)
                .map_err(bridge_error)?;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                state.fixture.consumed_receipts += 1;
            }
            drop(state);
            flush_app_controls(app);
        }
        WvLinkEnvelope::Invoke {
            document,
            request,
            channel,
            payload,
        } => {
            let state = app.borrow();
            if ivars.role != HandlerRole::Trusted
                || !state.authentic(&document, HandlerRole::Trusted)
            {
                return Err(bridge_error("numeric invoke used wrong world credential"));
            }
            if state.calls.has_pending() || state.calls.has_active() || state.pending_send.is_some()
            {
                return Err(bridge_error("shared native DATA floor is occupied"));
            }
            let mut call = lock_state(&ivars.state)
                .admit_invoke(&document, request.get(), channel.get(), payload)
                .map_err(bridge_error)?;
            let stamp = state
                .current()
                .ok_or_else(|| bridge_error("native document retired"))?;
            call.document_lease = state
                .document
                .borrow()
                .lease_for(stamp)
                .map(super::MacNativeDocumentLease);
            ivars
                .requests
                .try_send(call)
                .map_err(|_| bridge_error("existing renderer endpoint unavailable"))?;
        }
    }
    Ok(())
}

fn bind_compat_document(
    handler: &KeldScriptMessageHandler,
    webview: &WKWebView,
    app: &SharedAppState,
) -> Result<(), WvError> {
    let ivars = handler.ivars();
    let mut state = app.borrow_mut();
    state.synchronize_commit()?;
    let slot = match ivars.role {
        HandlerRole::App => OriginalCredentialSlot::App,
        HandlerRole::Trusted => OriginalCredentialSlot::Trusted,
    };
    let token = state
        .binding
        .credential(slot)
        .map(str::to_owned)
        .ok_or_else(|| {
            bridge_error("real committed document has no native-prepared world credential")
        })?;
    let bound=serde_json::to_string(&serde_json::json!({"v":2,"kind":"bound","document":token,"limits":{"payload_bytes":4096,"channel_utf8_bytes":256,"send_records":1,"event_records":1},"ready":false})).map_err(|_|bridge_error("private binding serialization failed"))?;
    let script = if ivars.role == HandlerRole::App {
        format!("globalThis.__keldAppNativeBind({bound});")
    } else {
        let stamp = state
            .current()
            .ok_or_else(|| bridge_error("native document retired while binding"))?;
        let mut trusted = lock_state(&ivars.state);
        trusted.navigation_generation = u64::from(stamp.generation.get());
        trusted.document_nonce = Some(token);
        format!("globalThis.__keldBridgeV2Bound({bound});")
    };
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if ivars.role == HandlerRole::App && state.fixture.hold_bindings {
        if !state.fixture.held_bindings.contains(&script) {
            if state.fixture.held_bindings.len() >= 2 {
                return Err(bridge_error(
                    "bounded bootstrap fixture already holds A and B",
                ));
            }
            state.fixture.held_bindings.push(script);
        }
        let _ = state.wake.send_event(super::WkUserEvent::RendererCustody);
        return Ok(());
    }
    drop(state);
    evaluate_owned(
        webview,
        &ivars.world,
        &script,
        app.clone(),
        EvaluationPurpose::Control,
    );
    Ok(())
}

fn send_compat_message(
    handler: &KeldScriptMessageHandler,
    app: &SharedAppState,
    document: &str,
    message: std::num::NonZeroU32,
    channel: String,
    payload: Vec<u8>,
    facts: Option<NativeMessageFacts>,
) -> Result<(), WvError> {
    let ivars = handler.ivars();
    if ivars.role != HandlerRole::App {
        return Err(bridge_error("trusted world cannot send app topics"));
    }
    let mut state = app.borrow_mut();
    if !state.authentic(document, HandlerRole::App)
        || state.retiring_at.is_some()
        || !state.binding.registered()
    {
        return Err(bridge_error(
            "app send is not current and privately registered",
        ));
    }
    state
        .binding
        .accept_message_sequence(message.get())
        .map_err(bridge_error)?;
    let active = state
        .calls
        .active()
        .filter(|active| {
            active.send_eligible(
                &state.document.borrow(),
                state.binding.route_role(),
                document,
                &channel,
            )
        })
        .ok_or_else(|| bridge_error("app send lacks its current original-function admission"))?;
    if state.pending_send.is_some() {
        return Err(bridge_error("native send credit occupied"));
    }
    let lease = active.lease().clone();
    let mut request = request_for_lease(
        lease.clone(),
        message.get(),
        RendererBridgeOperation::Send {
            channel,
            original_call: active.id(),
        },
        payload,
    );
    request.native_facts = facts;
    state
        .requests
        .try_send(request)
        .map_err(|_| bridge_error("existing bounded renderer endpoint unavailable"))?;
    state.pending_send = Some(PendingOriginalSend::after_handoff(message.get(), lease));
    if let Some(active) = state.calls.active_mut() {
        active.send_handed_off().map_err(bridge_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod original_image_tests {
    use super::*;
    fn source() -> Vec<u8> {
        b"globalThis.qualifiedLocalPreloadFixture = 'non-drawio';".to_vec()
    }
    fn facts() -> std::collections::BTreeMap<String, String> {
        [(String::from("keld"), String::from("fixture-real-facts-1"))].into()
    }
    fn selected_digest() -> [u8; 32] {
        Sha256::digest(source()).into()
    }
    #[test]
    fn original_image_is_exact_immutable_and_one_shared_charge() {
        let budget = AppPreloadBudget::default();
        let bytes = source();
        let digest = selected_digest();
        let image = budget
            .reserve_window()
            .expect("reserve")
            .from_qualified_graph(bytes, digest, vec![], facts())
            .expect("exact image");
        assert_eq!(budget.retained_bytes(), Some(source().len()));
        let clone = image.clone();
        drop(image);
        assert_eq!(budget.retained_bytes(), Some(source().len()));
        let (page, app) = render_original_scripts(&clone).expect("render");
        assert!(app.contains(std::str::from_utf8(&source()).expect("UTF8")));
        assert!(!page.contains("reqInfo"));
        drop(clone);
        assert_eq!(budget.retained_bytes(), Some(0));
    }
    #[test]
    fn same_length_mutation_cannot_self_pin_and_failure_releases_reservation() {
        let budget = AppPreloadBudget::default();
        let mut bad = source();
        bad[0] ^= 1;
        assert!(
            budget
                .reserve_window()
                .expect("reserve")
                .from_qualified_graph(bad, selected_digest(), vec![], facts())
                .is_err()
        );
        assert_eq!(budget.retained_bytes(), Some(0));
        assert!(
            budget
                .reserve_window()
                .expect("reserve")
                .from_qualified_graph(
                    source(),
                    selected_digest(),
                    vec![],
                    std::collections::BTreeMap::default(),
                )
                .is_err()
        );
        assert_eq!(budget.retained_bytes(), Some(0));
    }
    #[test]
    fn host_capacity_is_reserved_before_read_and_restored_on_drop() {
        let budget = AppPreloadBudget::default();
        let mut reservations = Vec::new();
        for _ in 0..16 {
            reservations.push(budget.reserve_window().expect("within 8 MiB"));
        }
        assert_eq!(budget.retained_bytes(), Some(8 * 1024 * 1024));
        assert!(budget.reserve_window().is_err());
        reservations.pop();
        assert!(budget.reserve_window().is_ok());
        drop(reservations);
        assert_eq!(budget.retained_bytes(), Some(0));
    }
    #[test]
    fn adoption_requires_same_ordered_arguments_and_canonical_facts() {
        let budget = AppPreloadBudget::default();
        let a = budget
            .reserve_window()
            .expect("reserve")
            .from_qualified_graph(
                source(),
                selected_digest(),
                vec![String::from("--initial-adaptive-colors=auto")],
                facts(),
            )
            .expect("A");
        let b = budget
            .reserve_window()
            .expect("reserve")
            .from_qualified_graph(source(), selected_digest(), vec![], facts())
            .expect("B");
        assert!(!a.same_configuration(&b));
        assert!(a.same_configuration(&a.clone()));
        assert_eq!(
            a.additional_arguments(),
            [String::from("--initial-adaptive-colors=auto")]
        );
    }

    #[test]
    fn different_non_drawio_preload_never_adopts_an_equal_argument_window() {
        let budget = AppPreloadBudget::default();
        let first = b"globalThis.nonDrawioSource = 'one'".to_vec();
        let second = b"globalThis.nonDrawioSource = 'two'".to_vec();
        let first_digest = Sha256::digest(&first).into();
        let second_digest = Sha256::digest(&second).into();
        let a = budget
            .reserve_window()
            .expect("reserve A")
            .from_qualified_graph(first, first_digest, vec![], facts())
            .expect("first qualified image");
        let b = budget
            .reserve_window()
            .expect("reserve B")
            .from_qualified_graph(second, second_digest, vec![], facts())
            .expect("second qualified image");
        assert!(
            !a.same_configuration(&b),
            "another selected preload is not adoptable"
        );
        assert_eq!(
            budget.retained_bytes(),
            Some(a.source_bytes() + b.source_bytes())
        );
    }
}

#[derive(Clone)]
enum EvaluationPurpose {
    Control,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    FixtureReplay,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    FixtureBinding {
        index: usize,
    },
    Call {
        id: u32,
        lease: super::MacNativeDocumentLease,
    },
    Event {
        id: u32,
        lease: super::MacNativeDocumentLease,
    },
}

impl AppNativeState {
    fn release_event(&mut self, navigation: u64, request: u32) -> Result<(), WvError> {
        let Some(event) = self.pending_event.as_ref() else {
            return Ok(());
        };
        if event
            .can_release(navigation, request)
            .map_err(bridge_error)?
        {
            self.pending_event = None;
        }
        Ok(())
    }
    fn original_current(&self, lease: &super::MacNativeDocumentLease, credential: &str) -> bool {
        self.retiring_at.is_none()
            && self
                .binding
                .original_current(&self.document.borrow(), lease, credential)
    }
    fn cancel_queued_originals(&mut self) {
        if self.calls.cancel_queued().is_err() {
            self.control_fault.latch();
        }
    }
    fn prepare_next_original(
        &mut self,
    ) -> Result<OriginalQueueProgress<Option<PreparingOriginalInvocation<String>>>, WvError> {
        if self.control_fault.is_faulted() || self.retiring_at.is_some() {
            return Ok(OriginalQueueProgress {
                value: None,
                needs_service_wake: false,
            });
        }
        let result = self.calls.take_next_preparation(
            &self.binding,
            &self.document.borrow(),
            &self.requests,
            (self.pending_send.as_ref(), self.pending_event.as_ref()),
            &self.control_fault,
            prepare_original_payload,
        );
        if result.is_err() {
            self.control_fault.latch();
        }
        result
    }

    /// The existing Mac observation cut remains on its actual living owner.
    /// It creates no Windows role fence and performs no native operation.
    fn enter_prepared_original(
        &mut self,
        prepared: &PreparingOriginalInvocation<String>,
    ) -> Result<bool, WvError> {
        if self.retiring_at.is_some() || self.control_fault.is_faulted() {
            return Ok(false);
        }
        if self.evaluations >= 64 {
            self.control_fault.latch();
            self.document.borrow_mut().destroy();
            return Err(bridge_error("native evaluation record bound exhausted"));
        }
        let admitted = self.calls.try_enter_prepared(
            prepared,
            &self.binding,
            &self.document.borrow(),
            (self.pending_send.as_ref(), self.pending_event.as_ref()),
            &self.control_fault,
        );
        if admitted {
            self.evaluations += 1;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if prepared.kind() == OriginalInvocationKind::Refusal {
                self.fixture.refused_before_entry =
                    self.fixture.refused_before_entry.saturating_add(1);
                self.fixture.last_refusal_call = prepared.identity().id();
                self.fixture.last_refusal_after_calls = self.fixture.finished_calls;
            }
        }
        Ok(admitted)
    }
    fn flush_controls(&mut self) -> bool {
        match self.calls.flush_controls(
            &mut self.binding,
            &self.document.borrow(),
            &mut self.pending_event,
            &self.requests,
        ) {
            Ok(progress) => progress.needs_service_wake,
            Err(_) => {
                self.control_fault.latch();
                false
            }
        }
    }
    fn evaluations_drained(&self) -> bool {
        let controls = self
            .pending_event
            .as_ref()
            .is_none_or(PendingOriginalEvent::retirement_controls_quiesced);
        self.evaluations == 0
            && self.calls.is_drained()
            && controls
            && !self.control_fault.is_faulted()
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn control_observation_kind(source: &str) -> &'static str {
    for (prefix, name) in [
        ("globalThis.__keldAppTryBind", "app-try-bind"),
        ("globalThis.__keldBridgeNativeTryBind", "trusted-try-bind"),
        ("globalThis.__keldAppNativeBind", "app-bound"),
        ("globalThis.__keldBridgeV2Bound", "trusted-bound"),
        ("globalThis.__keldAppNativeRegistered", "app-registered"),
        ("globalThis.__keldAppRouteState", "app-route-state"),
        ("globalThis.__keldAppNativeRefusal", "app-refusal"),
        ("globalThis.__keldAppNativeTerminal", "app-terminal"),
        ("globalThis.__keldBridgeV2Result", "trusted-result"),
    ] {
        if source.starts_with(prefix) {
            return name;
        }
    }
    "other-fixed-control"
}
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[derive(Serialize)]
struct NativeControlFailure {
    kind: &'static str,
    code: isize,
    count: u32,
}

/// Evaluate in this native `WebView`'s main frame and the explicit private world.
/// No `WKFrameInfo` is captured: original/control callback entry must compare its
/// private native document credential before work. The actual public WK
/// completion owns physical view custody independently of callback acceptance.
fn evaluate_owned(
    webview: &WKWebView,
    world: &WKContentWorld,
    script: &str,
    app: SharedAppState,
    purpose: EvaluationPurpose,
) {
    if let Err(error) = evaluate_owned_inner(webview, world, script, app.clone(), purpose, None) {
        app.borrow_mut().control_fault.latch();
        eprintln!("{error}");
    }
}

fn evaluate_owned_inner(
    webview: &WKWebView,
    world: &WKContentWorld,
    script: &str,
    app: SharedAppState,
    purpose: EvaluationPurpose,
    original: Option<PreparingOriginalInvocation<String>>,
) -> Result<(), WvError> {
    // Original invocation text can come only from its retained backing. The
    // ordinary script parameter is used solely by existing control/event paths.
    match (&original, &purpose) {
        (Some(prepared), EvaluationPurpose::Call { id, lease })
            if prepared.identity().id() == *id
                && prepared.identity().lease().same_binding(lease) => {}
        (None, EvaluationPurpose::Call { .. }) | (Some(_), _) => {
            return Err(bridge_error(
                "original native evaluation lacks its exact retained carrier",
            ));
        }
        (None, _) => {}
    }
    let script = match original.as_ref() {
        Some(prepared) => prepared.payload()?.as_str(),
        None => script,
    };
    if original.is_none() {
        let mut state = app.borrow_mut();
        if state.evaluations >= 64 {
            state.control_fault.latch();
            state.document.borrow_mut().destroy();
            return Err(bridge_error(
                "native evaluation record bound exhausted; view/image custody retained as unproved",
            ));
        }
        state.evaluations += 1;
    }
    let source = NSString::from_str(script);
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let control_kind = control_observation_kind(script);
    let callback_app = app.clone();
    let completion = block2::RcBlock::new(
        move |_value: *mut objc2::runtime::AnyObject, error: *mut objc2_foundation::NSError| {
            let receipt = NativeEvaluationReceipt {
                purpose: purpose.clone(),
                completed: error.is_null(),
            };
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                let mut state = callback_app.borrow_mut();
                state.fixture.actual_completions += 1;
                if !error.is_null() && matches!(&receipt.purpose, EvaluationPurpose::Control) {
                    // SAFETY: WebKit supplies this nonnull NSError for the actual
                    // completion block and keeps it live throughout the callback.
                    // Read only its scalar code here; no handle or data escapes.
                    let code = unsafe { (&*error).code() };
                    if let Some(found) = state
                        .fixture
                        .control_errors
                        .iter_mut()
                        .find(|found| found.kind == control_kind && found.code == code)
                    {
                        found.count = found.count.saturating_add(1);
                    } else if state.fixture.control_errors.len() < 16 {
                        state.fixture.control_errors.push(NativeControlFailure {
                            kind: control_kind,
                            code,
                            count: 1,
                        });
                    }
                }
                if !receipt.completed {
                    let index = match &receipt.purpose {
                        EvaluationPurpose::Control => 0,
                        EvaluationPurpose::Call { .. } => 1,
                        EvaluationPurpose::Event { .. } => 2,
                        EvaluationPurpose::FixtureReplay => 3,
                        EvaluationPurpose::FixtureBinding { .. } => 4,
                    };
                    state.fixture.evaluation_failures[index] =
                        state.fixture.evaluation_failures[index].saturating_add(1);
                }
                let hold_event = state.fixture.hold_next_completion.event
                    && matches!(&receipt.purpose, EvaluationPurpose::Event { .. });
                let hold_call = state.fixture.hold_next_completion.call
                    && matches!(&receipt.purpose, EvaluationPurpose::Call { .. });
                if hold_event || hold_call {
                    if hold_event {
                        state.fixture.hold_next_completion.event = false;
                    }
                    if hold_call {
                        state.fixture.hold_next_completion.call = false;
                    }
                    if state.fixture.held_completion.is_some() {
                        state.control_fault.latch();
                        return;
                    }
                    state.fixture.held_completion = Some(receipt);
                    let _ = state.wake.send_event(super::WkUserEvent::RendererCustody);
                    return;
                }
            }
            complete_evaluation(&callback_app, receipt);
        },
    );
    if let Some(prepared) = original.as_ref() {
        // The source and owned completion are already prepared. Release both
        // AppState/document borrows before the immediate existing native call.
        let admitted = app.borrow_mut().enter_prepared_original(prepared);
        if !admitted.as_ref().is_ok_and(|entered| *entered) {
            if let Some(prepared) = original {
                let progress = app.borrow_mut().calls.defer_preparation(prepared)?;
                post_original_service_wake(&app, progress.needs_service_wake);
            }
            flush_app_controls(&app);
            return admitted.map(|_| ());
        }
    }
    // SAFETY: retained live native view/world on AppKit UI thread. WK copies
    // this owned completion block until actual success/failure. Same-view
    // release retains the concrete view/image until every completion arrives.
    // Original entry just rechecked its same living lease/role with no guard
    // retained across this call; API return is separate from the WK callback.
    unsafe {
        webview.evaluateJavaScript_inFrame_inContentWorld_completionHandler(
            &source,
            None,
            world,
            Some(&completion),
        );
    }
    if let Some(prepared) = original {
        prepared.native_returned()?;
        // An inline callback may already have supplied both independent facts.
        // The same charged record can release only now, after real API return.
        flush_app_controls(&app);
    }
    Ok(())
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[derive(Default)]
struct PreloadFixtureState {
    refused_before_entry: u32,
    last_refusal_call: u32,
    last_refusal_after_calls: u32,
    control_errors: Vec<NativeControlFailure>,
    call_ingress: u32,
    evaluation_failures: [u32; 5],
    last_call_cut: &'static str,
    attempt_hook_fault: u8,
    hold_bindings: bool,
    held_bindings: Vec<String>,
    hold_next_call: bool,
    held_call: Option<String>,
    held_credential: Option<String>,
    old_replays_observed: u32,
    replay_wk_completed: Option<bool>,
    binding_wk_completed: [Option<bool>; 2],
    hold_next_completion: PreloadFixtureCompletionHolds,
    held_completion: Option<NativeEvaluationReceipt>,
    actual_completions: u32,
    consumed_receipts: u32,
    finished_calls: u32,
    origin_probe: Option<serde_json::Value>,
}
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[derive(Default)]
struct PreloadFixtureCompletionHolds {
    event: bool,
    call: bool,
}
struct NativeEvaluationReceipt {
    purpose: EvaluationPurpose,
    completed: bool,
}

fn complete_evaluation(app: &SharedAppState, receipt: NativeEvaluationReceipt) {
    let mut state = app.borrow_mut();
    let Some(remaining) = state.evaluations.checked_sub(1) else {
        state.control_fault.latch();
        state.document.borrow_mut().destroy();
        return;
    };
    state.evaluations = remaining;
    match receipt.purpose {
        EvaluationPurpose::Control => {}
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        EvaluationPurpose::FixtureReplay => {
            state.fixture.replay_wk_completed = Some(receipt.completed);
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        EvaluationPurpose::FixtureBinding { index } => {
            if let Some(slot) = state.fixture.binding_wk_completed.get_mut(index) {
                *slot = Some(receipt.completed);
            } else {
                state.control_fault.latch();
            }
        }
        EvaluationPurpose::Call { id, lease } => {
            if let Some(call) = state.calls.active_mut() {
                call.observe_native_quiescence(id, &lease, receipt.completed);
            }
        }
        EvaluationPurpose::Event { id, lease } => {
            if let Some(event) = state.pending_event.as_mut() {
                event.observe_native_quiescence(id, &lease, receipt.completed);
            }
        }
    }
    // This actual native-completion notification already covers any queue
    // transition in the same flush. Preserve one existing wake after the guard.
    state.flush_controls();
    let wake = state.wake.clone();
    drop(state);
    let _ = wake.send_event(super::WkUserEvent::RendererCustody);
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn fixture_origin_probe(
    handler: &KeldScriptMessageHandler,
    text: &str,
    facts: &NativeMessageFacts,
) -> Result<bool, WvError> {
    if text.len() > 16 * 1024 {
        return Ok(false);
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Ok(false);
    };
    if value.get("v").and_then(serde_json::Value::as_u64) != Some(0)
        || value.get("kind").and_then(serde_json::Value::as_str) != Some("origin-probe")
    {
        return Ok(false);
    }
    let app = handler
        .ivars()
        .app
        .as_ref()
        .ok_or_else(|| bridge_error("origin probe has no app world"))?;
    let mut state = app.borrow_mut();
    let supplied = value
        .get("document")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| bridge_error("origin probe missing current private binding"))?;
    if handler.ivars().role != HandlerRole::App || !state.authentic(supplied, HandlerRole::App) {
        return Err(bridge_error("origin probe came from wrong native binding"));
    }
    let tuple = facts
        .native_origin_tuple()
        .ok_or_else(|| bridge_error("Mac origin probe lacks its actual native tuple"))?;
    let mut isolated = serde_json::Map::new();
    for key in ["href", "origin", "protocol", "hostname", "port"] {
        let text = value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|text| text.len() <= 4096)
            .ok_or_else(|| bridge_error("bounded inert origin probe value missing"))?;
        isolated.insert(key.to_owned(), serde_json::Value::String(text.to_owned()));
    }
    let current = state
        .current()
        .ok_or_else(|| bridge_error("origin probe lost current native document"))?;
    let document = state.document.borrow();
    let source = document
        .source_binding()
        .ok_or_else(|| bridge_error("origin probe lost actual source binding"))?;
    let lease = document
        .lease_for(current)
        .ok_or_else(|| bridge_error("origin probe current native lease absent"))?;
    if !lease.is_current() || !lease.belongs_to_source(&source) {
        return Err(bridge_error("origin probe native lease/source mismatch"));
    }
    drop(document);
    let expected = source
        .qualify_native_origin(tuple.0, tuple.1, tuple.2)
        .map_err(|_| bridge_error("origin probe actual tuple is not source qualified"))?;
    if facts.origin() != Some(expected.as_str())
        || isolated.get("origin").and_then(serde_json::Value::as_str) != Some(expected.as_str())
        || isolated.get("href").and_then(serde_json::Value::as_str) != Some(facts.url())
        || isolated.get("hostname").and_then(serde_json::Value::as_str) != Some(tuple.1)
        || isolated.get("protocol").and_then(serde_json::Value::as_str)
            != Some(format!("{}:", tuple.0).as_str())
        || isolated.get("port").and_then(serde_json::Value::as_str) != Some("")
    {
        return Err(bridge_error(
            "qualified native origin disagrees with same-callback isolated origin components",
        ));
    }
    let foreign_host = if tuple.1 == "00000000000000000000000000000000" {
        "ffffffffffffffffffffffffffffffff"
    } else {
        "00000000000000000000000000000000"
    };
    if source
        .qualify_native_origin(tuple.0, foreign_host, tuple.2)
        .is_ok()
    {
        return Err(bridge_error(
            "origin probe mismatched source authority qualified",
        ));
    }
    if let Some(boot) = value.get("boot") {
        if !boot_diagnostic::enabled() {
            return Err(bridge_error("boot diagnostic was not Root-enabled"));
        }
        // The existing actual frame/source/credential/current-lease checks above
        // authenticate this metadata. Only fixed redacted fields reach logs.
        let mut record = boot_diagnostic::record(boot, current.view.0, current.generation.get())?;
        record["registry_registered_at_probe"] =
            serde_json::Value::Bool(state.binding.registered());
        eprintln!("KELD_BOOT_DIAGNOSTIC {record}");
    }
    state.fixture.origin_probe = Some(
        serde_json::json!({"native_url":facts.url(),"native_tuple":{"protocol":tuple.0,"host":tuple.1,"port":tuple.2},"qualified_serialized_origin":facts.origin(),"isolated":isolated,"qualified_matches_actual_isolated_origin":true,"mismatched_source_authority_rejected":true,"current_native_lease_matches_source":true,"authority_claim":false}),
    );
    let _ = state.wake.send_event(super::WkUserEvent::RendererCustody);
    Ok(true)
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
impl MacRendererBridge {
    pub(super) fn fixture_attempt_hook_fault(&self, mode: u8) {
        if let Some(app) = &self.app {
            app.borrow_mut().fixture.attempt_hook_fault = mode;
        }
    }
    pub(super) fn fixture_hold_bindings(&self) {
        if let Some(app) = &self.app {
            app.borrow_mut().fixture.hold_bindings = true;
        }
    }
    pub(super) fn fixture_held_bindings(&self) -> usize {
        self.app
            .as_ref()
            .map_or(0, |app| app.borrow().fixture.held_bindings.len())
    }
    pub(super) fn fixture_registered(&self) -> bool {
        self.app
            .as_ref()
            .is_some_and(|app| app.borrow().binding.registered())
    }
    pub(super) fn fixture_evaluations_idle(&self) -> bool {
        self.app
            .as_ref()
            .is_some_and(|app| app.borrow().evaluations == 0)
    }
    pub(super) fn fixture_replay_binding(
        &self,
        view: &WKWebView,
        index: usize,
    ) -> Result<(), WvError> {
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| bridge_error("no fixture app state"))?;
        let script = app
            .borrow()
            .fixture
            .held_bindings
            .get(index)
            .cloned()
            .ok_or_else(|| bridge_error("genuine held native Bound data absent"))?;
        let world = self
            .app_world
            .as_ref()
            .ok_or_else(|| bridge_error("no fixture app realm"))?;
        evaluate_owned(
            view,
            world,
            &script,
            app.clone(),
            EvaluationPurpose::FixtureBinding { index },
        );
        Ok(())
    }
    pub(super) fn fixture_binding_completed(&self, index: usize) -> Option<bool> {
        self.app
            .as_ref()?
            .borrow()
            .fixture
            .binding_wk_completed
            .get(index)
            .copied()
            .flatten()
    }
    pub(super) fn fixture_replay_completed(&self) -> Option<bool> {
        self.app.as_ref()?.borrow().fixture.replay_wk_completed
    }
    pub(super) fn fixture_old_replays(&self) -> u32 {
        self.app
            .as_ref()
            .map_or(0, |app| app.borrow().fixture.old_replays_observed)
    }
    pub(super) fn fixture_diagnostics(&self) -> serde_json::Value {
        let Some(app) = &self.app else {
            return serde_json::json!({"app_runtime":false});
        };
        let state = app.borrow();
        serde_json::json!({
            "app_runtime":true,"current_owner":state.current().is_some(),"prepared_attempt":state.binding.has_prepared_attempt(),
            "app_bound":state.binding.credential(OriginalCredentialSlot::App).is_some(),"trusted_bound":state.binding.credential(OriginalCredentialSlot::Trusted).is_some(),"registered":state.binding.registered(),"registration_notice_pending":state.binding.registration_notice().is_some(),
            "retiring":state.retiring_at.is_some(),"control_fault":state.control_fault.is_faulted(),"evaluations":state.evaluations,
            "call_ingress":state.fixture.call_ingress,"last_call_cut":state.fixture.last_call_cut,
            "held_call":state.fixture.held_call.is_some(),"held_bindings":state.fixture.held_bindings.len(),
            "old_replay_rejections":state.fixture.old_replays_observed,"replay_wk_completed":state.fixture.replay_wk_completed,
            "binding_wk_completed":state.fixture.binding_wk_completed,"actual_wk_completions":state.fixture.actual_completions,"evaluation_failures_control_call_event_replay_binding":state.fixture.evaluation_failures,"actual_control_errors":state.fixture.control_errors,
            "function_finished_receipts":state.fixture.finished_calls,"consumed_receipts":state.fixture.consumed_receipts,
            "refused_before_entry":state.fixture.refused_before_entry,"last_refusal_call":state.fixture.last_refusal_call,"last_refusal_after_calls":state.fixture.last_refusal_after_calls,
            "held_event_completion":state.fixture.held_completion.as_ref().is_some_and(|receipt|matches!(&receipt.purpose,EvaluationPurpose::Event{..})),
            "held_call_completion":state.fixture.held_completion.as_ref().is_some_and(|receipt|matches!(&receipt.purpose,EvaluationPurpose::Call{..})),
            "local_queue_records":state.calls.local_len(),"local_retained_records":state.calls.retained_records(),"local_retained_bytes":state.calls.retained_bytes(),
            "pending_call":state.calls.pending_observation().map(|(id,lease,locals_before,admission)|serde_json::json!({"id":id,"lease_current":lease.is_current(),"locals_before":locals_before,"admission":format!("{admission:?}")})),
            "active_call":state.calls.active().map(|call|serde_json::json!({"id":call.id(),"lease_current":call.lease().is_current(),"function_finished":call.function_finished(),"evaluation_finished":call.native_finished(),"send_forwarded":call.send_forwarded()})),
            "pending_send":state.pending_send.as_ref().map(|send|serde_json::json!({"id":send.id(),"lease_current":send.lease().is_current()})),
            "pending_event":state.pending_event.as_ref().map(|event|serde_json::json!({"id":event.id(),"lease_current":event.lease().is_current(),"consumed_sent":event.consumed_sent(),"quiesced":event.native_quiescence(),"quiesced_sent":event.quiescence_sent()}))
        })
    }
    pub(super) fn fixture_throw_control(&self, view: &WKWebView) -> Result<(), WvError> {
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| bridge_error("fixture has no app state"))?;
        let world = self
            .app_world
            .as_ref()
            .ok_or_else(|| bridge_error("fixture has no app world"))?;
        // Fixed negative control; appended void must never swallow this throw.
        evaluate_owned(
            view,
            world,
            "throw new Error('KELD fixture deliberate control exception'); void 0;",
            app.clone(),
            EvaluationPurpose::Control,
        );
        Ok(())
    }
    pub(super) fn fixture_ready(&self) -> bool {
        self.app.as_ref().is_some_and(|app| {
            let state = app.borrow();
            state.current().is_some()
                && state.binding.registered()
                && state
                    .binding
                    .credential(OriginalCredentialSlot::App)
                    .is_some()
                && !state.calls.has_active()
                && !state.calls.has_pending()
                && state.calls.local_len() == 0
                && state.calls.retained_records() == 0
                && state.evaluations == 0
        })
    }
    pub(super) fn fixture_finished_calls(&self) -> u32 {
        self.app
            .as_ref()
            .map_or(0, |app| app.borrow().fixture.finished_calls)
    }
    pub(super) fn fixture_hold_next_call(&self) {
        if let Some(app) = &self.app {
            app.borrow_mut().fixture.hold_next_call = true;
        }
    }
    pub(super) fn fixture_has_held_call(&self) -> bool {
        self.app
            .as_ref()
            .is_some_and(|app| app.borrow().fixture.held_call.is_some())
    }
    pub(super) fn fixture_replay_held(&self, view: &WKWebView) -> Result<(), WvError> {
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| bridge_error("no fixture app state"))?;
        let state = app.borrow();
        let text = state
            .fixture
            .held_call
            .as_ref()
            .ok_or_else(|| bridge_error("no actual old callback message was held"))?;
        let encoded = serde_json::to_string(text)
            .map_err(|_| bridge_error("held native data serialization refused"))?;
        let world = self
            .app_world
            .as_ref()
            .ok_or_else(|| bridge_error("no fixture app realm"))?;
        drop(state);
        evaluate_owned(
            view,
            world,
            &format!(
                "(()=>{{const old=JSON.parse({encoded});globalThis.__keldAppNativeCall(old);window.webkit.messageHandlers.__keld_app_world.postMessage({encoded});}})();"
            ),
            app.clone(),
            EvaluationPurpose::FixtureReplay,
        );
        Ok(())
    }
    pub(super) fn fixture_hold_event_completion(&self) {
        if let Some(app) = &self.app {
            app.borrow_mut().fixture.hold_next_completion.event = true;
        }
    }
    pub(super) fn fixture_hold_call_completion(&self) {
        if let Some(app) = &self.app {
            app.borrow_mut().fixture.hold_next_completion.call = true;
        }
    }
    pub(super) fn fixture_completion_held(&self) -> bool {
        self.app
            .as_ref()
            .is_some_and(|app| app.borrow().fixture.held_completion.is_some())
    }
    pub(super) fn fixture_release_actual_completion(&self) -> Result<(), WvError> {
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| bridge_error("no fixture app state"))?;
        let receipt = app
            .borrow_mut()
            .fixture
            .held_completion
            .take()
            .ok_or_else(|| bridge_error("no real WK completion receipt was held"))?;
        complete_evaluation(app, receipt);
        Ok(())
    }
    pub(super) fn fixture_origin_probe(&self, view: &WKWebView) -> Result<(), WvError> {
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| bridge_error("no fixture app state"))?;
        let state = app.borrow();
        let token = state
            .binding
            .credential(OriginalCredentialSlot::App)
            .ok_or_else(|| bridge_error("no current origin probe binding"))?;
        let token = serde_json::to_string(token)
            .map_err(|_| bridge_error("private probe binding serialization refused"))?;
        let script = format!(
            "window.webkit.messageHandlers.__keld_app_world.postMessage(JSON.stringify({{v:0,kind:'origin-probe',document:{token},href:location.href,origin:location.origin,protocol:location.protocol,hostname:location.hostname,port:location.port}}));"
        );
        let world = self
            .app_world
            .as_ref()
            .ok_or_else(|| bridge_error("no probe realm"))?;
        drop(state);
        evaluate_owned(
            view,
            world,
            &script,
            app.clone(),
            EvaluationPurpose::Control,
        );
        Ok(())
    }
    pub(super) fn fixture_origin_result(&self) -> Option<serde_json::Value> {
        self.app.as_ref()?.borrow().fixture.origin_probe.clone()
    }
    pub(super) fn fixture_counts(&self) -> (u32, u32, usize) {
        self.app.as_ref().map_or((0, 0, 0), |app| {
            let state = app.borrow();
            (
                state.fixture.actual_completions,
                state.fixture.consumed_receipts,
                state.evaluations,
            )
        })
    }
}

/// Capture only an actual callback's checked main frame on the `AppKit` UI owner.
/// The caller separately verifies receiving controller/content-world and live
/// document credentials. `WKFrameInfo` is transient, never frame identity.
#[allow(unsafe_code)] // Exact GH540 UI-only native frame/origin getters.
pub(super) fn capture_native_message_facts(
    frame: &WKFrameInfo,
    expected_view: &WKWebView,
    source: &Arc<super::AdmittedWebAppSource>,
) -> Result<NativeMessageFacts, WvError> {
    // SAFETY: live WKFrameInfo borrowed only for this actual UI callback.
    if !unsafe { frame.isMainFrame() } {
        return Err(bridge_error("native message frame is not main frame"));
    }
    // SAFETY: native frame and actual expected WKWebView are live on UI.
    let actual_view =
        unsafe { frame.webView() }.ok_or_else(|| bridge_error("native frame has no live view"))?;
    if !std::ptr::eq(actual_view.as_ref(), expected_view) {
        return Err(bridge_error("native frame belongs to another view"));
    }
    // SAFETY: actual same-frame request metadata, not a body/page-supplied URI.
    let request = unsafe { frame.request() };
    let url = request
        .URL()
        .and_then(|url| url.absoluteString())
        .ok_or_else(|| bridge_error("native frame has no actual absolute request URL"))?;
    let account = source.retained_credit_account();
    let url = super::MacNavigationUrl::capture(&url, &account).map_err(|_| {
        bridge_error("native frame URL exceeds shared source metadata/retained-byte limit")
    })?;
    source
        .qualify_document_url(url.as_str())
        .map_err(|_| bridge_error("native frame is not the selected source document"))?;
    // SAFETY: same live checked native frame; no page/Bun value chooses this object.
    let origin = unsafe { frame.securityOrigin() };
    // SAFETY: readonly getters of that retained native WKSecurityOrigin on UI.
    let protocol = unsafe { origin.protocol() };
    // SAFETY: same origin object and UI lifetime as protocol above.
    let host = unsafe { origin.host() };
    // SAFETY: readonly signed native port observation, never an authority selector.
    let port = unsafe { origin.port() };
    let port = i64::try_from(port)
        .map_err(|_| bridge_error("native origin port cannot be represented"))?;
    let capture_component = |value: &NSString| -> Result<Option<super::MacNavigationUrl>, WvError> {
        if value.length() == 0 {
            return Ok(None);
        }
        super::MacNavigationUrl::capture(value, &account)
            .map(Some)
            .map_err(|_| {
                bridge_error(
                    "native origin tuple exceeds shared source metadata/retained-byte limit",
                )
            })
    };
    let protocol = capture_component(&protocol)?;
    let host = capture_component(&host)?;
    let serialized_origin = source
        .qualify_native_origin(
            protocol
                .as_ref()
                .map_or("", super::MacNavigationUrl::as_str),
            host.as_ref().map_or("", super::MacNavigationUrl::as_str),
            port,
        )
        .map_err(|_| {
            bridge_error(
                "native origin tuple has no qualified serialization for this selected source",
            )
        })?;
    Ok(NativeMessageFacts::from_macos_native(
        url,
        protocol,
        host,
        port,
        Some(serialized_origin),
    ))
}
