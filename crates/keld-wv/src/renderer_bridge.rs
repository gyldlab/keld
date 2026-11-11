//! Shared Core/native renderer endpoint data and bounded channel ownership.
//! Native platform adapters construct admitted requests; pages cannot mint them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};

use crate::WebviewId;
use crate::wv_link::WvConsumerDisposition;

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
    pub(crate) webview: WebviewId,
    pub(crate) navigation: u64,
    pub(crate) request: u32,
    pub(crate) channel: u16,
    pub(crate) payload: Vec<u8>,
    pub(crate) operation: RendererBridgeOperation,
    pub(crate) document_lease: Option<crate::renderer_contract::CheckedNativeDocumentLease>,
    pub(crate) native_facts: Option<NativeMessageFacts>,
    pub(crate) role_witness: Option<RendererRoleWitness>,
}

impl RendererBridgeRequest {
    /// Select operation before using the legacy numeric accessors.
    #[must_use]
    pub const fn operation(&self) -> &RendererBridgeOperation {
        &self.operation
    }
    /// Read the sole native document owner's revocation witness at final Host admission.
    #[must_use]
    pub fn document_lease(&self) -> Option<&crate::renderer_contract::CheckedNativeDocumentLease> {
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
    pub(crate) const fn webview(&self) -> Option<WebviewId> {
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
    pub(crate) const fn request(&self) -> u32 {
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
    pub(crate) const fn navigation(&self) -> u64 {
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
    pub(crate) requests: SyncSender<RendererBridgeRequest>,
    pub(crate) outcomes: Option<Receiver<RendererBridgeOutcome>>,
    pub(crate) admitted_channel: u16,
}

impl RendererBridgeEndpoint {
    pub(crate) fn shared_view(
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
    /// the host passes it from its channel table (GH-508 spec Ã‚Â§4.6). Both the
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

/// Actual same-message frame metadata. Only the exact source-qualified native
/// keld-resource tuple has the isolated-world serialization observed by the
/// inert WK probe. Unknown tuple semantics refuse capture, never guess null.
#[derive(Debug)]
pub(crate) struct NativeMessageFacts {
    pub(crate) url: crate::resource_source::NativeNavigationUrl,
    pub(crate) protocol: Option<crate::resource_source::NativeNavigationUrl>,
    pub(crate) host: Option<crate::resource_source::NativeNavigationUrl>,
    pub(crate) port: i64,
    pub(crate) serialized_origin: Option<crate::resource_source::NativeNavigationUrl>,
}

impl NativeMessageFacts {
    pub(crate) fn url(&self) -> &str {
        self.url.as_str()
    }
    pub(crate) fn origin(&self) -> Option<&str> {
        self.serialized_origin
            .as_ref()
            .map(crate::resource_source::NativeNavigationUrl::as_str)
    }
    pub(crate) fn native_origin_tuple(&self) -> (&str, &str, i64) {
        (
            self.protocol
                .as_ref()
                .map_or("", crate::resource_source::NativeNavigationUrl::as_str),
            self.host
                .as_ref()
                .map_or("", crate::resource_source::NativeNavigationUrl::as_str),
            self.port,
        )
    }
}
