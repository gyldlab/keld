//! macOS KEL-142 renderer bridge: isolated `WebKit` world plus bounded host handoff.
//!
//! The page-visible window.keld facade is not the authority boundary.
//! Native admission happens only in a non-page content world handler installed
//! on one per-view user-content controller.

use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::wv_link::{AppPageCall, AppWvLinkEnvelope, decode_app_wv_link};
use crate::wv_link::{
    WvConsumerDisposition, WvLinkEnvelope, WvLinkMode, WvLinkRejectReason, decode_wv_link,
    decode_wv_link_for_mode,
};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod boot_diagnostic;

/// Maximum source bytes admitted for one window before UTF-8 parsing.
pub const MAX_APP_PRELOAD_BYTES: usize = 512 * 1024;
const MAX_HOST_PRELOAD_BYTES: usize = 8 * 1024 * 1024;

/// One Host session's source-image retention account; share across all windows.
#[derive(Clone, Debug, Default)]
pub struct AppPreloadBudget(Arc<Mutex<usize>>);

impl AppPreloadBudget {
    /// Reserve the full per-window cap before opening/copying any source bytes.
    /// The successful image shrinks this reservation to its actual size.
    ///
    /// # Errors
    /// Refuses Host aggregate exhaustion; no bytes or descriptors are retained.
    pub fn reserve_window(&self) -> Result<AppPreloadReservation, WvError> {
        let mut used = self
            .0
            .lock()
            .map_err(|_| preload_error("source account is poisoned"))?;
        let next = used
            .checked_add(MAX_APP_PRELOAD_BYTES)
            .filter(|next| *next <= MAX_HOST_PRELOAD_BYTES)
            .ok_or_else(|| preload_error("8 MiB Host source-image limit exhausted"))?;
        *used = next;
        Ok(AppPreloadReservation {
            budget: self.clone(),
            charged: MAX_APP_PRELOAD_BYTES,
        })
    }

    /// Retained and currently reserved source bytes, for owner observations.
    #[must_use]
    pub fn retained_bytes(&self) -> Option<usize> {
        self.0.lock().ok().map(|used| *used)
    }
}

/// Capacity retained across one bounded descriptor read and exact image check.
#[derive(Debug)]
pub struct AppPreloadReservation {
    budget: AppPreloadBudget,
    charged: usize,
}

impl Drop for AppPreloadReservation {
    fn drop(&mut self) {
        // A poisoned account refuses future admission. Recover only for exact
        // resource release, never to authorize additional construction.
        let mut used = self
            .budget
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *used = used.saturating_sub(self.charged);
    }
}

impl AppPreloadReservation {
    /// Retain the exact complete image and digest selected by Core's installed,
    /// boot-bound, rechecked graph. No renderer URL or JavaScript argument may
    /// supply the expected digest or select an alternate preload.
    /// `versions` is supplied by the canonical Host session-facts owner.
    ///
    /// # Errors
    /// Refuses changed source, invalid UTF-8, missing/bounded facts or arguments.
    pub fn from_qualified_graph(
        mut self,
        bytes: Vec<u8>,
        selected_sha256: [u8; 32],
        additional_arguments: Vec<String>,
        versions: std::collections::BTreeMap<String, String>,
    ) -> Result<AppPreloadImage, WvError> {
        if bytes.is_empty()
            || bytes.len() > MAX_APP_PRELOAD_BYTES
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != selected_sha256
        {
            return Err(preload_error(
                "preload bytes differ from the Core-qualified boot-selected graph digest",
            ));
        }
        if additional_arguments.len() > 16 {
            return Err(preload_error("more than 16 preload arguments"));
        }
        let argument_bytes = additional_arguments
            .iter()
            .try_fold(0_usize, |total, value| {
                if value.len() > 256 || value.contains('\0') {
                    return None;
                }
                total.checked_add(value.len())
            })
            .filter(|total| *total <= 2_048)
            .ok_or_else(|| preload_error("preload arguments exceed their byte bound"))?;
        let _ = argument_bytes;
        let fact_bytes = versions.iter().try_fold(0_usize, |total, (key, value)| {
            if key.is_empty()
                || key.len() > 128
                || value.is_empty()
                || value.len() > 256
                || key.contains('\0')
                || value.contains('\0')
            {
                return None;
            }
            total.checked_add(key.len())?.checked_add(value.len())
        });
        if versions.is_empty() || versions.len() > 32 || fact_bytes.is_none_or(|size| size > 8_192)
        {
            return Err(preload_error(
                "canonical Host process versions are missing or exceed their bound",
            ));
        }
        let source =
            String::from_utf8(bytes).map_err(|_| preload_error("original preload is not UTF-8"))?;
        {
            let mut used = self
                .budget
                .0
                .lock()
                .map_err(|_| preload_error("source account is poisoned"))?;
            *used -= self.charged - source.len();
            self.charged = source.len();
        }
        Ok(AppPreloadImage(Arc::new(AppPreloadImageInner {
            source: source.into_boxed_str(),
            sha256: selected_sha256,
            additional_arguments,
            versions,
            _reservation: self,
        })))
    }
}

#[derive(Debug)]
struct AppPreloadImageInner {
    source: Box<str>,
    sha256: [u8; 32],
    additional_arguments: Vec<String>,
    versions: std::collections::BTreeMap<String, String>,
    _reservation: AppPreloadReservation,
}

/// Immutable Host-authenticated original image/configuration for exactly one window.
/// Clones share bytes and the one retention charge; navigation never reopens a path.
#[derive(Clone, Debug)]
pub struct AppPreloadImage(Arc<AppPreloadImageInner>);

impl AppPreloadImage {
    /// Number of immutable source bytes retained by this image.
    #[must_use]
    pub fn source_bytes(&self) -> usize {
        self.0.source.len()
    }
    /// Ordered original window arguments captured before native construction.
    #[must_use]
    pub fn additional_arguments(&self) -> &[String] {
        &self.0.additional_arguments
    }
    /// Immutable canonical Host session process facts used by the preload.
    #[must_use]
    pub fn versions(&self) -> &std::collections::BTreeMap<String, String> {
        &self.0.versions
    }
    /// Whether a surviving window may adopt this exact source/configuration.
    #[must_use]
    pub fn same_configuration(&self, other: &Self) -> bool {
        self.0.sha256 == other.0.sha256
            && self.0.additional_arguments == other.0.additional_arguments
            && self.0.versions == other.0.versions
    }
}

fn preload_error(detail: impl Into<String>) -> WvError {
    WvError::AppPreloadSource {
        detail: detail.into(),
    }
}

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

/// Operation on the single existing native/Core renderer endpoint.
#[derive(Debug)]
pub enum RendererBridgeOperation {
    /// Actual authenticated app registry installed for this native lease/source.
    /// Identity-only observation: request zero, no payload, topic or grant.
    Registered,
    /// Existing numeric KEL142 request.
    Invoke,
    /// Asynchronous actual Host admission before original Function entry.
    Preflight {
        /// Exact outbound literal, derived from copied action data.
        channel: String,
    },
    /// Actual original ipcRenderer.send; final Host Guard remains mandatory.
    Send {
        /// Exact literal channel.
        channel: String,
    },
    /// Original app-world consumer completed an addressed event.
    Consumed {
        /// Exact consumer result; transport release remains separate.
        disposition: WvConsumerDisposition,
    },
    /// Authority was physically retired; no successor replay.
    Retired,
    /// Native original Function turn quiesced without any Send entering Core.
    PreflightAborted,
    /// Real WK event evaluation completion, independent of consumer disposition.
    EventQuiesced {
        /// True only when `WebKit` reported no evaluation error.
        completed: bool,
    },
}

/// Native-only observation of the existing authenticated role's stop witness.
/// This retains the Core-owned identity; it mints no principal or wire token.
#[derive(Clone, Debug)]
pub struct RendererRoleWitness {
    stopped: Arc<AtomicBool>,
}
impl RendererRoleWitness {
    /// Capture the actual primary caller's existing stop flag, never page data.
    #[must_use]
    pub const fn new(stopped: Arc<AtomicBool>) -> Self {
        Self { stopped }
    }
    /// Whether the captured role has not been retired by its owner.
    #[must_use]
    pub fn is_live(&self) -> bool {
        !self.stopped.load(Ordering::Acquire)
    }
    /// Require the exact existing role owner, not equal Boolean values.
    #[must_use]
    pub fn matches(&self, stopped: &Arc<AtomicBool>) -> bool {
        self.is_live() && Arc::ptr_eq(&self.stopped, stopped)
    }
    /// Compare two native observations without minting another generation.
    #[must_use]
    pub fn same_role(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stopped, &other.stopped)
    }
}

/// One renderer request that already passed `WebKit` world/frame/document admission.
#[derive(Debug)]
pub struct RendererBridgeRequest {
    webview: WebviewId,
    navigation: u64,
    request: u32,
    channel: u16,
    payload: Vec<u8>,
    operation: RendererBridgeOperation,
    document_lease: Option<super::MacNativeDocumentLease>,
    native_facts: Option<NativeMessageFacts>,
    role_witness: Option<RendererRoleWitness>,
}

impl RendererBridgeRequest {
    /// Select operation before using the legacy numeric accessors.
    #[must_use]
    pub const fn operation(&self) -> &RendererBridgeOperation {
        &self.operation
    }
    /// Read the sole native document owner's revocation witness at final Host admission.
    #[must_use]
    pub fn document_lease(&self) -> Option<&super::MacNativeDocumentLease> {
        self.document_lease.as_ref()
    }
    /// Original role observed before retaining a queued effectful call.
    #[must_use]
    pub fn role_witness(&self) -> Option<&RendererRoleWitness> {
        self.role_witness.as_ref()
    }
    /// Actual native request URL captured in this WK callback, never caller data.
    #[must_use]
    pub fn url(&self) -> Option<&str> {
        self.native_facts.as_ref().map(NativeMessageFacts::url)
    }
    /// Qualified actual native origin serialization; absent means fail closed.
    #[must_use]
    pub fn origin(&self) -> Option<&str> {
        self.native_facts
            .as_ref()
            .and_then(NativeMessageFacts::origin)
    }
    /// Actual same-message native origin tuple; serialization is separately qualified.
    #[must_use]
    pub fn native_origin_tuple(&self) -> Option<(&str, &str, i64)> {
        self.native_facts
            .as_ref()
            .map(NativeMessageFacts::native_origin_tuple)
    }

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
    /// Existing bounded receiver removed a request; retry retained controls only.
    ControlCapacity,
    /// Actual Host preflight completed; late/stale outcomes are discarded natively.
    Preflight {
        /// Owning native view.
        webview: WebviewId,
        /// Original checked native document generation.
        navigation: u64,
        /// Original untrusted local call correlation only.
        request: u32,
        /// Whether actual current-role/grant/capacity admission succeeded.
        accepted: bool,
    },
    /// Canonical one-way send terminal disposition; never manufactures a page response.
    SendResult {
        /// Owning native view.
        webview: WebviewId,
        /// Original checked native document generation.
        navigation: u64,
        /// Original local one-way sequence.
        request: u32,
        /// Exact canonical terminal result after actual transport release.
        disposition: crate::wv_link::WvTerminalDisposition,
    },
    /// Core completed the actual canonical event-release receipt.
    EventReleased {
        /// Owning view.
        webview: WebviewId,
        /// Original checked native document generation.
        navigation: u64,
        /// Exact event delivery correlation.
        request: u32,
    },
    /// Addressed GH543 event carrying sole GH544 codec bytes.
    Event {
        /// Owning native view.
        webview: WebviewId,
        /// Exact original checked native generation; never current-successor lookup.
        navigation: u64,
        /// Nonzero delivery correlation.
        request: u32,
        /// Exact literal event topic.
        channel: String,
        /// Opaque bounded endpoint arguments.
        payload: Vec<u8>,
    },
    /// Current authenticated Bun route availability; retirement does not free JS maps.
    RouteState {
        /// Owning native view.
        webview: WebviewId,
        /// Exact checked native document generation.
        navigation: u64,
        /// Whether the actual route is admitted.
        ready: bool,
        /// The existing authenticated role owner; never exposed to JavaScript.
        role: RendererRoleWitness,
    },
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
    pub(super) const fn webview(&self) -> Option<WebviewId> {
        match self {
            Self::Reply { webview, .. }
            | Self::Error { webview, .. }
            | Self::Preflight { webview, .. }
            | Self::SendResult { webview, .. }
            | Self::Event { webview, .. }
            | Self::RouteState { webview, .. }
            | Self::EventReleased { webview, .. } => Some(*webview),
            Self::ControlCapacity => None,
        }
    }

    #[must_use]
    const fn request(&self) -> u32 {
        match self {
            Self::Reply { request, .. }
            | Self::Error { request, .. }
            | Self::Preflight { request, .. }
            | Self::SendResult { request, .. }
            | Self::Event { request, .. }
            | Self::EventReleased { request, .. } => *request,
            Self::RouteState { .. } | Self::ControlCapacity => 0,
        }
    }

    #[must_use]
    const fn navigation(&self) -> u64 {
        match self {
            Self::Reply { navigation, .. }
            | Self::Error { navigation, .. }
            | Self::Preflight { navigation, .. }
            | Self::SendResult { navigation, .. }
            | Self::Event { navigation, .. }
            | Self::RouteState { navigation, .. }
            | Self::EventReleased { navigation, .. } => *navigation,
            Self::ControlCapacity => 0,
        }
    }
}

/// Core-facing bounded request sender plus UI-facing terminal outcomes.
#[derive(Debug)]
pub struct RendererBridgeEndpoint {
    pub(super) requests: SyncSender<RendererBridgeRequest>,
    pub(super) outcomes: Option<Receiver<RendererBridgeOutcome>>,
    pub(super) admitted_channel: u16,
}

impl RendererBridgeEndpoint {
    pub(super) fn shared_view(
        requests: SyncSender<RendererBridgeRequest>,
        admitted_channel: u16,
    ) -> Self {
        Self {
            requests,
            outcomes: None,
            admitted_channel,
        }
    }

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
            outcomes: Some(outcomes),
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
        let app =
            original.map(|original| Self::original_state(original, endpoint.requests.clone()));
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
    ) -> SharedAppState {
        let OriginalBridgeInputs {
            image: _image,
            document,
            wake,
        } = original;
        std::rc::Rc::new(std::cell::RefCell::new(AppNativeState {
            document,
            stamp: None,
            prepared: None,
            bootstrap_scripts: Vec::new(),
            app_credential: None,
            trusted_credential: None,
            registered: false,
            registration_notice: None,
            route_role: None,
            pending_call: None,
            active_call: None,
            local_calls: VecDeque::new(),
            local_budget: LocalCallBudget::default(),
            pending_send: None,
            pending_event: None,
            evaluations: 0,
            retiring_at: None,
            control_fault: false,
            last_message: 0,
            wake,
            requests,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            fixture: PreloadFixtureState::default(),
        }))
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
            state.prepared = Some(PreparedAppBinding {
                attempt,
                app: app_token,
                trusted: trusted_token,
            });
            state.bootstrap_scripts = replacements;
            state.stamp = None;
            state.app_credential = None;
            state.trusted_credential = None;
            state.registered = false;
            state.cancel_queued_originals();
            state.flush_controls();
            Ok(())
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
            state.control_fault = true;
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
                    serde_json::to_string(state.app_credential.as_ref().ok_or_else(|| {
                        bridge_error("boot diagnostic has no current native app binding")
                    })?)
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
            state.app_credential = None;
            state.trusted_credential = None;
            state.registered = false;
            state.cancel_queued_originals();
            state.flush_controls();
        }
        lock_state(&self.state).destroy();
    }
    pub(super) fn flush_controls(&self) {
        if let Some(app) = &self.app {
            app.borrow_mut().flush_controls();
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
        let mut state = app.borrow_mut();
        state.flush_controls();
        if state.evaluations_drained() {
            return Ok(true);
        }
        if state.control_fault
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
        app.borrow_mut().flush_controls();
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
        if state.pending_call.as_ref().is_none_or(|pending| {
            pending.queued.invocation.id != request
                || u64::from(
                    pending
                        .queued
                        .invocation
                        .lease
                        .stamp()
                        .observed_generation(),
                ) != navigation
                || pending.admission != AppCallAdmission::Waiting
        }) {
            return Ok(());
        }
        if accepted {
            if let Some(pending) = state.pending_call.as_mut() {
                pending.admission = AppCallAdmission::Accepted;
            }
            // The original Function may enter only after its earlier local
            // registrations have both finished and physically left WebKit.
            return Ok(());
        }
        let Some(pending) = state.pending_call.take() else {
            return Ok(());
        };
        let queued = pending.queued;
        if state.original_current(&queued.invocation.lease, &queued.credential) {
            // Reuse the same charged record through the actual private refusal
            // evaluation. A denied call cannot turn its budget into an unbounded
            // sequence of outstanding control evaluations.
            state.retain_cancelled_original(queued, pending.locals_before)?;
        } else {
            state.drop_queued_call(queued);
        }
        Ok(())
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
            || state.control_fault
            || state.pending_event.is_some()
            || request == 0
            || channel.len() > 256
            || payload.len() > 4096
            || !state.registered
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
            .app_credential
            .clone()
            .ok_or_else(|| bridge_error("addressed event has no current app binding"))?;
        let json=serde_json::to_string(&serde_json::json!({"v":2,"kind":"event","document":token,"delivery":request,"channel":channel,"payload":payload})).map_err(|_|bridge_error("addressed event serialization failed"))?;
        state.pending_event = Some(PendingAppEvent {
            id: request,
            lease: lease.clone(),
            credential: token,
            disposition: None,
            consumed_sent: false,
            quiesced: None,
            quiesced_sent: false,
        });
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
                if state.pending_send.as_ref().is_none_or(|send| {
                    send.id != request
                        || u64::from(send.lease.stamp().observed_generation()) != navigation
                }) {
                    return Ok(());
                }
                let Some(send) = state.pending_send.take() else {
                    return Ok(());
                };
                if send.lease.is_current() && state.retiring_at.is_none() {
                    let token = state.app_credential.as_ref().ok_or_else(|| {
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
                if state
                    .current()
                    .is_none_or(|stamp| u64::from(stamp.generation.get()) != navigation)
                    || state.retiring_at.is_some()
                {
                    return Ok(());
                }
                if ready {
                    if !role.is_live() {
                        return Ok(());
                    }
                    state.route_role = Some(role);
                } else {
                    // Late retirement of A must not retire an observed successor B.
                    if !state
                        .route_role
                        .as_ref()
                        .is_some_and(|current| current.same_role(&role))
                    {
                        return Ok(());
                    }
                    state.route_role = None;
                }
                let token =
                    serde_json::to_string(state.app_credential.as_ref().ok_or_else(|| {
                        bridge_error("route state has no current private binding")
                    })?)
                    .map_err(|_| bridge_error("route state serialization failed"))?;
                if !ready && let Some(pending) = state.pending_call.as_mut() {
                    pending.admission = AppCallAdmission::Cancelled;
                }
                state.flush_controls();
                drop(state);
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
                .trusted_credential
                .as_ref()
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
        assert!(bounded_native_message_text(&NSString::from_str("é"), 1).is_err());
        assert_eq!(
            bounded_native_message_text(&NSString::from_str("é"), 2).expect("exact UTF8"),
            "é"
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
const MAX_LOCAL_CALL_RECORDS: usize = 64;
const MAX_LOCAL_CALL_BYTES: usize = 524_288;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandlerRole {
    Trusted,
    App,
}

type SharedAppState = std::rc::Rc<std::cell::RefCell<AppNativeState>>;
struct AppNativeState {
    document: std::rc::Rc<std::cell::RefCell<super::macos_document::NativeDocumentOwner>>,
    stamp: Option<super::macos_document::NativeCommittedStamp>,
    prepared: Option<PreparedAppBinding>,
    bootstrap_scripts: Vec<Retained<WKUserScript>>,
    app_credential: Option<String>,
    trusted_credential: Option<String>,
    registered: bool,
    registration_notice: Option<super::MacNativeDocumentLease>,
    route_role: Option<RendererRoleWitness>,
    pending_call: Option<PendingAppCall>,
    active_call: Option<ActiveAppCall>,
    local_calls: VecDeque<QueuedLocalCall>,
    local_budget: LocalCallBudget,
    pending_send: Option<PendingAppSend>,
    pending_event: Option<PendingAppEvent>,
    evaluations: usize,
    retiring_at: Option<std::time::Instant>,
    control_fault: bool,
    last_message: u32,
    wake: tao::event_loop::EventLoopProxy<super::WkUserEvent>,
    requests: SyncSender<RendererBridgeRequest>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fixture: PreloadFixtureState,
}

struct PreparedAppBinding {
    attempt: super::MacNativeAttempt,
    app: String,
    trusted: String,
}

impl AppNativeState {
    fn current(&self) -> Option<super::macos_document::NativeCommittedStamp> {
        let current = self.document.borrow().current_stamp()?;
        (Some(current) == self.stamp).then_some(current)
    }
    fn synchronize_commit(&mut self) -> Result<(), WvError> {
        let stamp = self
            .document
            .borrow()
            .current_stamp()
            .ok_or_else(|| bridge_error("no qualified committed application document"))?;
        if self.stamp == Some(stamp) {
            return Ok(());
        }
        let prepared = self
            .prepared
            .take()
            .filter(|prepared| prepared.attempt.0 == stamp.attempt)
            .ok_or_else(|| {
                bridge_error("actual commit has no matching native-prepared bootstrap")
            })?;
        self.cancel_queued_originals();
        self.stamp = Some(stamp);
        self.app_credential = Some(prepared.app);
        self.trusted_credential = Some(prepared.trusted);
        self.registered = false;
        self.registration_notice = None;
        self.route_role = None;
        self.last_message = 0;
        Ok(())
    }
    fn authentic(&self, supplied: &str, role: HandlerRole) -> bool {
        if self.current().is_none() {
            return false;
        }
        let stored = match role {
            HandlerRole::App => &self.app_credential,
            HandlerRole::Trusted => &self.trusted_credential,
        };
        stored.as_ref().is_some_and(|value| value == supplied)
    }
}

fn credential() -> Result<String, WvError> {
    use std::fmt::Write as _;
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| bridge_error("Host credential randomness unavailable"))?;
    let mut value = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(&mut value, "{byte:02x}");
    }
    Ok(value)
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
        image.0.source
    );
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let app = if boot_diagnostic::enabled() {
        format!("{}\n{app}", boot_diagnostic::app_script())
    } else {
        app
    };
    Ok((page, app))
}

fn local_action(value: &serde_json::Value) -> Option<&str> {
    value.as_str()
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
    let mut state = app.borrow_mut();
    state.flush_controls();
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    {
        state.fixture.call_ingress = state.fixture.call_ingress.saturating_add(1);
        state.fixture.last_call_cut = "received";
    }

    if state.retiring_at.is_some()
        || !state.registered
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
    if state.control_fault || (channel.is_some() && lock_state(&ivars.state).pending.is_some()) {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if std::env::var_os("KELD_KEL142_ACCEPTANCE_REPORT").is_some() {
            eprintln!(
                "KELD_KEL142_CALL_BUSY call={} method={} channel={:?} pending_call={} active_call={:?} active_forwarded={} pending_send={} pending_event={} control_fault={}",
                call.call,
                call.method,
                channel,
                state.pending_call.is_some(),
                state.active_call.as_ref().map(|active| active.id),
                state
                    .active_call
                    .as_ref()
                    .is_some_and(|active| active.send_forwarded),
                state.pending_send.is_some(),
                state.pending_event.is_some(),
                state.control_fault,
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
    queue_original(&mut state, call, lease, channel, facts)?;
    drop(state);
    pump_original_call(webview, &ivars.world, app)
}

fn original_call_channel(call: &AppPageCall) -> Result<Option<String>, WvError> {
    let channel = match call.method.as_str() {
        "request" => Some(String::from("rendererReq")),
        "sendMessage" => {
            let topic = call
                .args
                .first()
                .and_then(local_action)
                .ok_or_else(|| bridge_error("sendMessage needs a copied literal action"))?;
            if topic.len() > 256 {
                return Err(bridge_error("named action exceeds 256 bytes"));
            }
            Some(topic.to_owned())
        }
        _ => None,
    };
    Ok(channel)
}

struct PreparedOriginalInvocation {
    id: u32,
    lease: super::MacNativeDocumentLease,
    source: String,
}
fn pump_original_call(
    webview: &WKWebView,
    world: &WKContentWorld,
    app: &SharedAppState,
) -> Result<(), WvError> {
    let prepared = app.borrow_mut().prepare_next_original()?;
    if let Some(prepared) = prepared {
        evaluate_owned(
            webview,
            world,
            &prepared.source,
            app.clone(),
            EvaluationPurpose::Call {
                id: prepared.id,
                lease: prepared.lease,
            },
        );
    }
    Ok(())
}
fn queue_original(
    state: &mut AppNativeState,
    call: AppPageCall,
    lease: super::MacNativeDocumentLease,
    channel: Option<String>,
    facts: Option<NativeMessageFacts>,
) -> Result<(), WvError> {
    // Observe the existing native role BEFORE retaining an effectful call. A
    // delayed preflight cannot silently attach to a successor Bun generation.
    let role_witness = if channel.is_some() {
        Some(
            state
                .route_role
                .as_ref()
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
    let credential = call.document;
    let bytes = source
        .capacity()
        .checked_add(credential.capacity())
        .and_then(|value| value.checked_add(channel.as_ref().map_or(0, String::capacity)))
        .and_then(|value| value.checked_add(std::mem::size_of::<QueuedLocalCall>()))
        .ok_or_else(|| bridge_error("original call retained-byte count overflow"))?;
    // Native callback facts already carry their owning source-account charge.
    // Do not copy URL/origin strings or invent facts when the queue is serviced.
    state.local_budget.reserve(bytes)?;
    let queued = QueuedLocalCall {
        invocation: PreparedOriginalInvocation { id, lease, source },
        credential,
        bytes,
        channel,
        role_witness,
        facts,
        cancelled: false,
    };
    if queued.channel.is_some()
        && state.pending_call.is_none()
        && state.pending_send.is_none()
        && state
            .active_call
            .as_ref()
            .is_none_or(|active| active.channel.is_none())
        && !state
            .local_calls
            .iter()
            .any(|entry| entry.channel.is_some())
    {
        let locals_before = state.local_calls.len();
        if let Some(queued) = state.begin_original_preflight(queued, locals_before)? {
            state.local_calls.push_back(queued);
        }
    } else {
        state.local_calls.push_back(queued);
    }
    Ok(())
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
            let active = state
                .active_call
                .as_mut()
                .filter(|active| active.id == call && active.credential == document)
                .ok_or_else(|| {
                    bridge_error(
                        "original invocation completion does not match retained physical custody",
                    )
                })?;
            active.function_finished = true;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                state.fixture.finished_calls += 1;
            }
            state.flush_controls();
            drop(state);
            pump_original_call(webview, &ivars.world, app)?;
        }
        AppWvLinkEnvelope::Registered { document, .. } => {
            if !state.authentic(&document, HandlerRole::App)
                || state.retiring_at.is_some()
                || state.registered
            {
                return Err(bridge_error(
                    "private registry binding is stale or duplicate",
                ));
            }
            let stamp = state
                .current()
                .ok_or_else(|| bridge_error("registry notice lost actual native commit"))?;
            let lease = state
                .document
                .borrow()
                .lease_for(stamp)
                .map(super::MacNativeDocumentLease)
                .ok_or_else(|| bridge_error("registry notice has no current native lease"))?;
            state.registered = true;
            state.registration_notice = Some(lease);
            state.flush_controls();
            let token = serde_json::to_string(
                state
                    .app_credential
                    .as_ref()
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
            let event = state
                .pending_event
                .as_mut()
                .filter(|event| {
                    event.id == delivery.get()
                        && event.credential == document
                        && !event.consumed_sent
                        && event.disposition.is_none()
                })
                .ok_or_else(|| {
                    bridge_error("stale, unsolicited or duplicate consumer completion")
                })?;
            event.disposition = Some(disposition);
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                state.fixture.consumed_receipts += 1;
            }
            state.flush_controls();
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
            if state.pending_call.is_some()
                || state.active_call.is_some()
                || state.pending_send.is_some()
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
        HandlerRole::App => &mut state.app_credential,
        HandlerRole::Trusted => &mut state.trusted_credential,
    };
    let token = slot.clone().ok_or_else(|| {
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
        || !state.registered
    {
        return Err(bridge_error(
            "app send is not current and privately registered",
        ));
    }
    if message.get()
        != state
            .last_message
            .checked_add(1)
            .ok_or_else(|| bridge_error("app send sequence exhausted"))?
    {
        return Err(bridge_error("duplicate, skipped or replayed app message"));
    }
    state.last_message = message.get();
    let active = state
        .active_call
        .as_ref()
        .filter(|active| {
            active.credential == document
                && active.channel.as_deref() == Some(&channel)
                && !active.send_forwarded
                && active.lease.is_current()
                && active
                    .role_witness
                    .as_ref()
                    .is_some_and(|role| state.role_current(role))
        })
        .ok_or_else(|| bridge_error("app send lacks its current original-function admission"))?;
    if state.pending_send.is_some() {
        return Err(bridge_error("native send credit occupied"));
    }
    let lease = active.lease.clone();
    let mut request = request_for_lease(
        lease.clone(),
        message.get(),
        RendererBridgeOperation::Send { channel },
        payload,
    );
    request.native_facts = facts;
    state
        .requests
        .try_send(request)
        .map_err(|_| bridge_error("existing bounded renderer endpoint unavailable"))?;
    state.pending_send = Some(PendingAppSend {
        id: message.get(),
        lease,
    });
    if let Some(active) = state.active_call.as_mut() {
        active.send_forwarded = true;
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

#[derive(Default)]
struct LocalCallBudget {
    records: usize,
    bytes: usize,
}
impl LocalCallBudget {
    fn reserve(&mut self, bytes: usize) -> Result<(), WvError> {
        let records = self
            .records
            .checked_add(1)
            .filter(|value| *value <= MAX_LOCAL_CALL_RECORDS)
            .ok_or_else(|| bridge_error("post-bind local call record limit (64) exhausted"))?;
        let total = self
            .bytes
            .checked_add(bytes)
            .filter(|value| *value <= MAX_LOCAL_CALL_BYTES)
            .ok_or_else(|| bridge_error("post-bind local call byte limit (524288) exhausted"))?;
        self.records = records;
        self.bytes = total;
        Ok(())
    }
    fn release(&mut self, bytes: usize) -> Result<(), WvError> {
        let records = self
            .records
            .checked_sub(1)
            .ok_or_else(|| bridge_error("local call record release underflow"))?;
        let total = self
            .bytes
            .checked_sub(bytes)
            .ok_or_else(|| bridge_error("local call byte release underflow"))?;
        self.records = records;
        self.bytes = total;
        Ok(())
    }
}
struct QueuedLocalCall {
    invocation: PreparedOriginalInvocation,
    credential: String,
    bytes: usize,
    channel: Option<String>,
    role_witness: Option<RendererRoleWitness>,
    facts: Option<NativeMessageFacts>,
    cancelled: bool,
}

#[cfg(test)]
mod postbind_queue_limits {
    use super::{LocalCallBudget, MAX_LOCAL_CALL_BYTES, MAX_LOCAL_CALL_RECORDS};

    #[test]
    fn postbind_record_max_plus_one_refuses_without_changing_custody() {
        let mut budget = LocalCallBudget::default();
        for _ in 0..MAX_LOCAL_CALL_RECORDS {
            budget.reserve(1).expect("within bounded record census");
        }
        assert!(budget.reserve(1).is_err());
        assert_eq!((budget.records, budget.bytes), (64, 64));
        budget.release(1).expect("one actual record release");
        budget
            .reserve(1)
            .expect("released capacity permits successor");
        for _ in 0..MAX_LOCAL_CALL_RECORDS {
            budget
                .release(1)
                .expect("each original record released once");
        }
        assert_eq!((budget.records, budget.bytes), (0, 0));
        assert!(budget.release(1).is_err());
        assert_eq!((budget.records, budget.bytes), (0, 0));
    }

    #[test]
    fn postbind_byte_max_plus_one_and_overflow_refuse_atomically() {
        let mut budget = LocalCallBudget::default();
        assert!(budget.reserve(MAX_LOCAL_CALL_BYTES + 1).is_err());
        assert_eq!((budget.records, budget.bytes), (0, 0));
        budget
            .reserve(MAX_LOCAL_CALL_BYTES)
            .expect("exact byte maximum");
        assert!(budget.reserve(1).is_err());
        assert!(budget.reserve(usize::MAX).is_err());
        assert_eq!((budget.records, budget.bytes), (1, MAX_LOCAL_CALL_BYTES));
        assert!(budget.release(MAX_LOCAL_CALL_BYTES + 1).is_err());
        assert_eq!((budget.records, budget.bytes), (1, MAX_LOCAL_CALL_BYTES));
        budget
            .release(MAX_LOCAL_CALL_BYTES)
            .expect("actual charged bytes released");
        assert_eq!((budget.records, budget.bytes), (0, 0));
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppCallAdmission {
    Waiting,
    Accepted,
    Cancelled,
}
struct PendingAppCall {
    queued: QueuedLocalCall,
    locals_before: usize,
    admission: AppCallAdmission,
}
struct ActiveAppCall {
    id: u32,
    lease: super::MacNativeDocumentLease,
    credential: String,
    channel: Option<String>,
    send_forwarded: bool,
    function_finished: bool,
    evaluation_finished: bool,
    local_bytes: usize,
    role_witness: Option<RendererRoleWitness>,
}
struct PendingAppSend {
    id: u32,
    lease: super::MacNativeDocumentLease,
}
struct PendingAppEvent {
    id: u32,
    lease: super::MacNativeDocumentLease,
    credential: String,
    disposition: Option<WvConsumerDisposition>,
    consumed_sent: bool,
    quiesced: Option<bool>,
    quiesced_sent: bool,
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

fn request_for_lease(
    lease: super::MacNativeDocumentLease,
    sequence: u32,
    operation: RendererBridgeOperation,
    payload: Vec<u8>,
) -> RendererBridgeRequest {
    let stamp = lease.stamp();
    RendererBridgeRequest {
        webview: stamp.view(),
        navigation: u64::from(stamp.observed_generation()),
        request: sequence,
        channel: 0,
        payload,
        operation,
        document_lease: Some(lease),
        native_facts: None,
        role_witness: None,
    }
}

impl AppNativeState {
    fn release_event(&mut self, navigation: u64, request: u32) -> Result<(), WvError> {
        let Some(event) = self.pending_event.as_ref().filter(|event| {
            event.id == request
                && u64::from(event.lease.stamp().observed_generation()) == navigation
        }) else {
            return Ok(());
        };
        if event.quiesced.is_none() || !event.quiesced_sent {
            return Err(bridge_error(
                "Core attempted event release before actual WK quiescence",
            ));
        }
        self.pending_event = None;
        Ok(())
    }
    fn original_current(&self, lease: &super::MacNativeDocumentLease, credential: &str) -> bool {
        if self.retiring_at.is_some()
            || !self.registered
            || !lease.is_current()
            || !self.authentic(credential, HandlerRole::App)
        {
            return false;
        }
        let owner = self.document.borrow();
        let stamp = lease.0.stamp();
        self.stamp == Some(stamp)
            && owner
                .lease_for(stamp)
                .is_some_and(|current| current.same_binding(&lease.0))
            && owner
                .source_binding()
                .is_some_and(|source| lease.belongs_to_source(&source))
    }
    fn release_local(&mut self, bytes: usize) {
        if self.local_budget.release(bytes).is_err() {
            self.control_fault = true;
        }
    }
    fn role_current(&self, role: &RendererRoleWitness) -> bool {
        role.is_live()
            && self
                .route_role
                .as_ref()
                .is_some_and(|current| current.is_live() && current.same_role(role))
    }
    fn queued_call_current(&self, call: &QueuedLocalCall) -> bool {
        !call.cancelled
            && self.original_current(&call.invocation.lease, &call.credential)
            && (call.channel.is_none()
                || call
                    .role_witness
                    .as_ref()
                    .is_some_and(|role| self.role_current(role)))
    }
    fn drop_queued_call(&mut self, call: QueuedLocalCall) {
        let bytes = call.bytes;
        drop(call);
        self.release_local(bytes);
    }
    fn retain_cancelled_original(
        &mut self,
        mut call: QueuedLocalCall,
        older_locals: usize,
    ) -> Result<(), WvError> {
        call.cancelled = true;
        if older_locals > self.local_calls.len() {
            // Retain charged custody but forbid execution; never guess an
            // execution order when the private predecessor census is invalid.
            self.control_fault = true;
            self.local_calls.push_back(call);
            return Err(bridge_error(
                "cancelled original call predecessor census is invalid",
            ));
        }
        self.local_calls.insert(older_locals, call);
        let _ = self.wake.send_event(super::WkUserEvent::RendererCustody);
        Ok(())
    }
    /// Submit one preflight using the role observed at native call ingress.
    /// Full means zero handoff: retain the same charged record until the existing
    /// capacity wake. Never acquire a successor role at this later dispatch cut.
    fn begin_original_preflight(
        &mut self,
        mut queued: QueuedLocalCall,
        locals_before: usize,
    ) -> Result<Option<QueuedLocalCall>, WvError> {
        if !self.queued_call_current(&queued) {
            if self.original_current(&queued.invocation.lease, &queued.credential) {
                queued.cancelled = true;
                let _ = self.wake.send_event(super::WkUserEvent::RendererCustody);
                return Ok(Some(queued));
            }
            self.drop_queued_call(queued);
            return Ok(None);
        }
        let Some(channel) = queued.channel.as_ref() else {
            self.drop_queued_call(queued);
            return Err(bridge_error(
                "local registration cannot request effect preflight",
            ));
        };
        let mut request = request_for_lease(
            queued.invocation.lease.clone(),
            queued.invocation.id,
            RendererBridgeOperation::Preflight {
                channel: channel.clone(),
            },
            Vec::new(),
        );
        request.native_facts = queued.facts.take();
        request.role_witness.clone_from(&queued.role_witness);
        match self.requests.try_send(request) {
            Ok(()) => {
                self.pending_call = Some(PendingAppCall {
                    queued,
                    locals_before,
                    admission: AppCallAdmission::Waiting,
                });
                Ok(None)
            }
            Err(TrySendError::Full(request)) => {
                queued.facts = request.native_facts;
                Ok(Some(queued))
            }
            Err(TrySendError::Disconnected(_)) => {
                self.drop_queued_call(queued);
                self.control_fault = true;
                Err(bridge_error(
                    "existing bounded renderer endpoint disconnected",
                ))
            }
        }
    }
    fn clear_local_calls(&mut self) {
        while let Some(call) = self.local_calls.pop_front() {
            let bytes = call.bytes;
            drop(call);
            self.release_local(bytes);
        }
    }
    fn cancel_queued_originals(&mut self) {
        self.clear_local_calls();
        if let Some(pending) = self.pending_call.as_mut() {
            pending.admission = AppCallAdmission::Cancelled;
        }
    }
    fn clear_active_call(&mut self) {
        if let Some(call) = self.active_call.take() {
            let bytes = call.local_bytes;
            drop(call);
            if bytes != 0 {
                self.release_local(bytes);
            }
        }
    }
    fn prepare_next_original(&mut self) -> Result<Option<PreparedOriginalInvocation>, WvError> {
        self.flush_controls();
        if self.active_call.is_some() || self.control_fault || self.retiring_at.is_some() {
            return Ok(None);
        }
        if let Some(pending) = self.pending_call.as_ref()
            && pending.locals_before == 0
        {
            // An application callback can enqueue its next effect before the
            // containing main event physically releases. Keep the existing
            // charged call until Core's exact EventReleased; controls and local
            // registrations still progress, and currentness is rechecked below.
            if pending.admission != AppCallAdmission::Accepted
                || self.pending_send.is_some()
                || self.pending_event.is_some()
            {
                return Ok(None);
            }
            let pending = self
                .pending_call
                .take()
                .ok_or_else(|| bridge_error("admitted original call disappeared"))?;
            if !self.queued_call_current(&pending.queued) {
                self.pending_call = Some(PendingAppCall {
                    admission: AppCallAdmission::Cancelled,
                    ..pending
                });
                self.flush_controls();
                return Ok(None);
            }
            let queued = pending.queued;
            self.active_call = Some(ActiveAppCall {
                id: queued.invocation.id,
                lease: queued.invocation.lease.clone(),
                credential: queued.credential,
                channel: queued.channel,
                send_forwarded: false,
                function_finished: false,
                evaluation_finished: false,
                local_bytes: queued.bytes,
                role_witness: queued.role_witness,
            });
            return Ok(Some(queued.invocation));
        }
        if self
            .local_calls
            .front()
            .is_some_and(|call| call.channel.is_some() && !call.cancelled)
            && (self.pending_call.is_some() || self.pending_send.is_some())
        {
            return Ok(None);
        }
        let Some(local) = self.local_calls.pop_front() else {
            return Ok(None);
        };
        if !self.queued_call_current(&local) {
            return self.prepare_cancelled_original(local);
        }
        if local.channel.is_some() {
            if let Some(local) = self.begin_original_preflight(local, 0)? {
                self.local_calls.push_front(local);
            }
            return Ok(None);
        }
        if let Some(pending) = self.pending_call.as_mut() {
            pending.locals_before = pending
                .locals_before
                .checked_sub(1)
                .ok_or_else(|| bridge_error("original call order barrier underflow"))?;
        }
        self.active_call = Some(ActiveAppCall {
            id: local.invocation.id,
            lease: local.invocation.lease.clone(),
            credential: local.credential,
            channel: None,
            send_forwarded: false,
            function_finished: false,
            evaluation_finished: false,
            local_bytes: local.bytes,
            role_witness: None,
        });
        Ok(Some(local.invocation))
    }
    fn prepare_cancelled_original(
        &mut self,
        mut local: QueuedLocalCall,
    ) -> Result<Option<PreparedOriginalInvocation>, WvError> {
        if self.original_current(&local.invocation.lease, &local.credential) {
            // Only unentered page-copy slots are released. The original
            // Function is not invoked, and its bookkeeping is never faked.
            let token = serde_json::to_string(&local.credential)
                .map_err(|_| bridge_error("cancelled call credential serialization failed"))?;
            local.invocation.source = format!(
                "globalThis.__keldAppNativeRefusal({token},{});",
                local.invocation.id
            );
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                self.fixture.refused_before_entry =
                    self.fixture.refused_before_entry.saturating_add(1);
                self.fixture.last_refusal_call = local.invocation.id;
                self.fixture.last_refusal_after_calls = self.fixture.finished_calls;
            }
            self.active_call = Some(ActiveAppCall {
                id: local.invocation.id,
                lease: local.invocation.lease.clone(),
                credential: local.credential,
                channel: None,
                send_forwarded: false,
                function_finished: true,
                evaluation_finished: false,
                local_bytes: local.bytes,
                role_witness: None,
            });
            return Ok(Some(local.invocation));
        }
        self.drop_queued_call(local);
        let _ = self.wake.send_event(super::WkUserEvent::RendererCustody);
        Ok(None)
    }
    fn enqueue_control(&mut self, request: RendererBridgeRequest) -> bool {
        match self.requests.try_send(request) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => false,
            Err(TrySendError::Disconnected(_)) => {
                self.control_fault = true;
                false
            }
        }
    }
    fn reconcile_queued_originals(&mut self) {
        // At most the bounded admitted count is examined. Local registrations
        // remain role-independent; queued effects retain their original role.
        let count = self.local_calls.len();
        for _ in 0..count {
            let Some(mut call) = self.local_calls.pop_front() else {
                break;
            };
            if self.original_current(&call.invocation.lease, &call.credential) {
                if !self.queued_call_current(&call) {
                    call.cancelled = true;
                }
                self.local_calls.push_back(call);
            } else {
                self.drop_queued_call(call);
            }
        }
        if self
            .pending_call
            .as_ref()
            .is_some_and(|pending| !self.queued_call_current(&pending.queued))
            && let Some(pending) = self.pending_call.as_mut()
        {
            pending.admission = AppCallAdmission::Cancelled;
        }
    }
    fn flush_controls(&mut self) {
        self.reconcile_queued_originals();
        if let Some(event) = self.pending_event.as_ref()
            && !event.consumed_sent
            && let Some(disposition) = event.disposition.clone()
        {
            let request = request_for_lease(
                event.lease.clone(),
                event.id,
                RendererBridgeOperation::Consumed { disposition },
                Vec::new(),
            );
            if self.enqueue_control(request)
                && let Some(event) = self.pending_event.as_mut()
            {
                event.consumed_sent = true;
            }
        }
        if let Some(event) = self.pending_event.as_ref()
            && !event.quiesced_sent
            && let Some(completed) = event.quiesced
        {
            let request = request_for_lease(
                event.lease.clone(),
                event.id,
                RendererBridgeOperation::EventQuiesced { completed },
                Vec::new(),
            );
            if self.enqueue_control(request)
                && let Some(event) = self.pending_event.as_mut()
            {
                event.quiesced_sent = true;
            }
        }
        if let Some(call) = self.active_call.as_ref()
            && call.function_finished
            && call.evaluation_finished
        {
            if call.channel.is_some() && !call.send_forwarded {
                let request = request_for_lease(
                    call.lease.clone(),
                    call.id,
                    RendererBridgeOperation::PreflightAborted,
                    Vec::new(),
                );
                if self.enqueue_control(request) {
                    self.clear_active_call();
                }
            } else {
                self.clear_active_call();
            }
        }
        if let Some(pending) = self.pending_call.as_ref()
            && pending.admission == AppCallAdmission::Cancelled
        {
            let request = request_for_lease(
                pending.queued.invocation.lease.clone(),
                pending.queued.invocation.id,
                RendererBridgeOperation::PreflightAborted,
                Vec::new(),
            );
            if self.enqueue_control(request)
                && let Some(pending) = self.pending_call.take()
            {
                let queued = pending.queued;
                if self.original_current(&queued.invocation.lease, &queued.credential) {
                    if self
                        .retain_cancelled_original(queued, pending.locals_before)
                        .is_err()
                    {
                        self.control_fault = true;
                    }
                } else {
                    self.drop_queued_call(queued);
                }
            }
        }
        if let Some(lease) = self.registration_notice.as_ref() {
            if lease.is_current() {
                let request = request_for_lease(
                    lease.clone(),
                    0,
                    RendererBridgeOperation::Registered,
                    Vec::new(),
                );
                if self.enqueue_control(request) {
                    self.registration_notice = None;
                }
            } else {
                self.registration_notice = None;
            }
        }
    }
    fn evaluations_drained(&self) -> bool {
        let controls = self.pending_event.as_ref().is_none_or(|event| {
            event.quiesced_sent && (event.consumed_sent || !event.lease.is_current())
        });
        self.evaluations == 0
            && self.pending_call.is_none()
            && self.active_call.is_none()
            && self.local_calls.is_empty()
            && self.local_budget.records == 0
            && self.local_budget.bytes == 0
            && controls
            && !self.control_fault
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
    {
        let mut state = app.borrow_mut();
        if state.evaluations >= 64 {
            state.control_fault = true;
            state.document.borrow_mut().destroy();
            eprintln!(
                "KELD-WV-011: native evaluation record bound exhausted; view/image custody retained as unproved"
            );
            return;
        }
        state.evaluations += 1;
    }
    let source = NSString::from_str(script);
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let control_kind = control_observation_kind(script);
    let completion = block2::RcBlock::new(
        move |_value: *mut objc2::runtime::AnyObject, error: *mut objc2_foundation::NSError| {
            let receipt = NativeEvaluationReceipt {
                purpose: purpose.clone(),
                completed: error.is_null(),
            };
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                let mut state = app.borrow_mut();
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
                        state.control_fault = true;
                        return;
                    }
                    state.fixture.held_completion = Some(receipt);
                    let _ = state.wake.send_event(super::WkUserEvent::RendererCustody);
                    return;
                }
            }
            complete_evaluation(&app, receipt);
        },
    );
    // SAFETY: retained live native view/world on AppKit UI thread. WK copies
    // this owned completion block until actual success/failure. Same-view
    // release retains the concrete view/image until every completion arrives.
    unsafe {
        webview.evaluateJavaScript_inFrame_inContentWorld_completionHandler(
            &source,
            None,
            world,
            Some(&completion),
        );
    }
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
        state.control_fault = true;
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
                state.control_fault = true;
            }
        }
        EvaluationPurpose::Call { id, lease } => {
            if let Some(call) = state
                .active_call
                .as_mut()
                .filter(|call| call.id == id && call.lease.same_binding(&lease))
            {
                call.evaluation_finished = true;
                if !receipt.completed || !lease.is_current() {
                    call.function_finished = true;
                }
            }
        }
        EvaluationPurpose::Event { id, lease } => {
            if let Some(event) = state
                .pending_event
                .as_mut()
                .filter(|event| event.id == id && event.lease.same_binding(&lease))
            {
                event.quiesced = Some(receipt.completed);
            }
        }
    }
    state.flush_controls();
    let _ = state.wake.send_event(super::WkUserEvent::RendererCustody);
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
    let tuple = facts.native_origin_tuple();
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
        record["registry_registered_at_probe"] = serde_json::Value::Bool(state.registered);
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
        self.app.as_ref().is_some_and(|app| app.borrow().registered)
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
            "app_runtime":true,"current_owner":state.current().is_some(),"prepared_attempt":state.prepared.is_some(),
            "app_bound":state.app_credential.is_some(),"trusted_bound":state.trusted_credential.is_some(),"registered":state.registered,"registration_notice_pending":state.registration_notice.is_some(),
            "retiring":state.retiring_at.is_some(),"control_fault":state.control_fault,"evaluations":state.evaluations,
            "call_ingress":state.fixture.call_ingress,"last_call_cut":state.fixture.last_call_cut,
            "held_call":state.fixture.held_call.is_some(),"held_bindings":state.fixture.held_bindings.len(),
            "old_replay_rejections":state.fixture.old_replays_observed,"replay_wk_completed":state.fixture.replay_wk_completed,
            "binding_wk_completed":state.fixture.binding_wk_completed,"actual_wk_completions":state.fixture.actual_completions,"evaluation_failures_control_call_event_replay_binding":state.fixture.evaluation_failures,"actual_control_errors":state.fixture.control_errors,
            "function_finished_receipts":state.fixture.finished_calls,"consumed_receipts":state.fixture.consumed_receipts,
            "refused_before_entry":state.fixture.refused_before_entry,"last_refusal_call":state.fixture.last_refusal_call,"last_refusal_after_calls":state.fixture.last_refusal_after_calls,
            "held_event_completion":state.fixture.held_completion.as_ref().is_some_and(|receipt|matches!(&receipt.purpose,EvaluationPurpose::Event{..})),
            "held_call_completion":state.fixture.held_completion.as_ref().is_some_and(|receipt|matches!(&receipt.purpose,EvaluationPurpose::Call{..})),
            "local_queue_records":state.local_calls.len(),"local_retained_records":state.local_budget.records,"local_retained_bytes":state.local_budget.bytes,
            "pending_call":state.pending_call.as_ref().map(|call|serde_json::json!({"id":call.queued.invocation.id,"lease_current":call.queued.invocation.lease.is_current(),"locals_before":call.locals_before,"admission":format!("{:?}",call.admission)})),
            "active_call":state.active_call.as_ref().map(|call|serde_json::json!({"id":call.id,"lease_current":call.lease.is_current(),"function_finished":call.function_finished,"evaluation_finished":call.evaluation_finished,"send_forwarded":call.send_forwarded})),
            "pending_send":state.pending_send.as_ref().map(|send|serde_json::json!({"id":send.id,"lease_current":send.lease.is_current()})),
            "pending_event":state.pending_event.as_ref().map(|event|serde_json::json!({"id":event.id,"lease_current":event.lease.is_current(),"consumed_sent":event.consumed_sent,"quiesced":event.quiesced,"quiesced_sent":event.quiesced_sent}))
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
                && state.registered
                && state.app_credential.is_some()
                && state.active_call.is_none()
                && state.pending_call.is_none()
                && state.local_calls.is_empty()
                && state.local_budget.records == 0
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
            .app_credential
            .as_ref()
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

/// Actual same-message frame metadata. Only the exact source-qualified native
/// keld-resource tuple has the isolated-world serialization observed by the
/// inert WK probe. Unknown tuple semantics refuse capture, never guess null.
#[derive(Debug)]
pub(super) struct NativeMessageFacts {
    url: super::MacNavigationUrl,
    protocol: Option<super::MacNavigationUrl>,
    host: Option<super::MacNavigationUrl>,
    port: i64,
    serialized_origin: Option<super::MacNavigationUrl>,
}

impl NativeMessageFacts {
    pub(super) fn url(&self) -> &str {
        self.url.as_str()
    }
    pub(super) fn origin(&self) -> Option<&str> {
        self.serialized_origin
            .as_ref()
            .map(super::MacNavigationUrl::as_str)
    }
    pub(super) fn native_origin_tuple(&self) -> (&str, &str, i64) {
        (
            self.protocol
                .as_ref()
                .map_or("", super::MacNavigationUrl::as_str),
            self.host
                .as_ref()
                .map_or("", super::MacNavigationUrl::as_str),
            self.port,
        )
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
    Ok(NativeMessageFacts {
        url,
        protocol,
        host,
        port,
        serialized_origin: Some(serialized_origin),
    })
}
