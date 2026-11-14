//! Shared Core/native renderer endpoint data and bounded channel ownership.
//! Native platform adapters construct admitted requests; pages cannot mint them.

#[cfg(windows)]
mod control;
#[cfg(windows)]
pub use control::{NativeControlCharge, NativeControlReservation, RendererControlOwner};
#[cfg(windows)]
mod data;
#[cfg(windows)]
pub use data::{NativeDataCharge, NativeDataReservation};
#[cfg(windows)]
mod fixed_event_failure;
#[cfg(windows)]
pub use fixed_event_failure::RetainedRendererEventFailure;

mod native_facts;
#[cfg(windows)]
pub use crate::original_call::queue::numeric::NativeNumericResultReservation;
pub(crate) use native_facts::NativeMessageFacts;

#[cfg(windows)]
pub use crate::webview2::original_bridge::NativeEventRetirement;

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
        /// Original call captured from the native active Function owner.
        /// This is distinct from the one-way IPC message sequence.
        original_call: u32,
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
    /// Sealed Host-only retirement of this exact original native DATA event.
    /// Neither consumer completion nor physical storage release is implied.
    #[cfg(windows)]
    EventRetired {
        /// Actual irreversible no-entry cancellation or same-controller Close.
        retirement: crate::renderer_bridge::NativeEventRetirement,
    },
    /// Real WK event evaluation completion, independent of consumer disposition.
    EventQuiesced {
        /// True only when `WebKit` reported no evaluation error.
        completed: bool,
    },
}

/// The canonical Core generation owner supplies the final native admission cut.
/// Implementations must acquire shutdown -> current role/link, validate the exact
/// authenticated caller, then invoke `admit` at most once while that fence is held.
/// The callback may only validate and commit local native admission state: no
/// allocation, COM, application callback, wait or I/O is permitted inside it.
///
/// Returning true fixes the operation to its already retained original target and
/// charge. Retirement ordered afterward may revoke future admission, but cannot
/// redirect this accepted operation to a successor or release it before completion.
pub trait RendererRoleFence: Send + Sync {
    /// Attempt an irreversible local admission under the existing Core owner.
    /// False means the callback was not accepted; it conveys no delivery receipt.
    fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool;
}

/// Native-only observation of the existing authenticated role's stop witness.
/// This retains the Core-owned identity; it mints no principal or wire token.
/// Windows native publication additionally requires its canonical Core fence.
#[derive(Clone)]
pub struct RendererRoleWitness {
    stopped: Arc<AtomicBool>,
    fence: Option<Arc<dyn RendererRoleFence>>,
    #[cfg(windows)]
    controls: Option<Arc<dyn RendererControlOwner>>,
}
impl std::fmt::Debug for RendererRoleWitness {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RendererRoleWitness")
            .field("stopped", &self.stopped)
            .field("has_canonical_fence", &self.fence.is_some())
            .finish_non_exhaustive()
    }
}
impl RendererRoleWitness {
    /// Capture the actual primary caller's existing stop flag, never page data.
    /// This preserves the existing macOS observation API. It does not qualify a
    /// Windows native publication: `try_admit_native` refuses without a fence.
    #[must_use]
    pub const fn new(stopped: Arc<AtomicBool>) -> Self {
        Self {
            stopped,
            fence: None,
            #[cfg(windows)]
            controls: None,
        }
    }
    /// Retain the same actual caller together with its canonical Core owner.
    /// The fence must validate this exact stop allocation, identity and attempt;
    /// an independently copied Boolean is not an implementation of that contract.
    #[must_use]
    pub fn with_fence(stopped: Arc<AtomicBool>, fence: Arc<dyn RendererRoleFence>) -> Self {
        Self {
            stopped,
            fence: Some(fence),
            #[cfg(windows)]
            controls: None,
        }
    }
    /// Attach the same Core session's existing four CONTROL reservations.
    /// This is storage accounting only; final native publication still requires
    /// the canonical role fence and an independently checked native document.
    #[cfg(windows)]
    #[must_use]
    pub fn with_controls(mut self, owner: Arc<dyn RendererControlOwner>) -> Self {
        self.controls = Some(owner);
        self
    }
    /// Reserve exact native CONTROL storage before JSON/UTF-16 allocation.
    /// `accounted_bytes` includes all simultaneous representations and the
    /// existing 1024-byte record charge. No capacity is replenished by restart.
    #[cfg(windows)]
    #[must_use]
    pub fn try_reserve_native_control(
        &self,
        encoded_bytes: usize,
        accounted_bytes: usize,
    ) -> Option<NativeControlReservation> {
        self.controls
            .as_ref()?
            .try_reserve_native_control(encoded_bytes, accounted_bytes)
    }
    /// The physical View scope survives its creation role stopping or being
    /// replaced. These three storage methods deliberately do not test `is_live()`.
    #[cfg(windows)]
    #[must_use]
    pub fn has_native_view_scope(&self) -> bool {
        self.controls
            .as_ref()
            .is_some_and(|owner| owner.has_native_view_scope())
    }
    /// Call from the actual owning `OriginalView` outside native/interior guards.
    /// False retains the View; no native release may be inferred from it.
    #[cfg(windows)]
    #[must_use]
    pub fn close_native_offers(&self) -> bool {
        self.controls
            .as_ref()
            .is_some_and(|owner| owner.close_native_offers())
    }
    /// Read-only same-owner storage observation after producer close. Internal
    /// completion-only releases continue to be serviced while this is false.
    #[cfg(windows)]
    #[must_use]
    pub fn native_storage_is_quiescent(&self) -> bool {
        self.controls
            .as_ref()
            .is_some_and(|owner| owner.native_storage_is_quiescent())
    }

    /// Commit only local native admission under Core's actual retirement fence.
    /// Call the native API on the retained original target only after this method
    /// returns true and every Core/native interior guard has been released.
    #[must_use]
    pub fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool {
        self.fence
            .as_ref()
            .is_some_and(|fence| fence.try_admit_native(admit))
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

/// The canonical Core owner of one fixed route-unavailable control.
/// This is separate from a live role fence: retirement may already have stopped
/// that role. Implementations retain the exact old Registry owner and document,
/// acquire shutdown -> current -> Registry, and refuse if the Entry has been
/// adopted by a successor. The callback has the same local-only restrictions as
/// [`RendererRoleFence::try_admit_native`]. No other message may use this fence.
#[cfg(windows)]
pub trait RendererRouteRetirementFence: Send + Sync {
    /// Reserve only this fixed ready:false operation's storage using the same
    /// captured Registry owner, including when its live caller has stopped.
    fn try_reserve_route_unavailable(
        &self,
        document: &crate::renderer_contract::CheckedNativeDocumentLease,
        encoded_bytes: usize,
        accounted_bytes: usize,
    ) -> Option<NativeControlReservation>;

    /// Admit only ready:false for the captured original native document.
    fn try_admit_route_unavailable(
        &self,
        document: &crate::renderer_contract::CheckedNativeDocumentLease,
        admit: &mut dyn FnMut() -> bool,
    ) -> bool;
}

/// Opaque custody of the already ordered retirement of one registered route.
/// Only the canonical Core Registry owner supplies this operation permit. It
/// cannot authorize ready:true, data, replies, or settlement of another record.
#[cfg(windows)]
#[derive(Clone)]
pub struct RouteRetirementPermit {
    fence: Arc<dyn RendererRouteRetirementFence>,
}
#[cfg(windows)]
impl std::fmt::Debug for RouteRetirementPermit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RouteRetirementPermit")
            .finish_non_exhaustive()
    }
}
#[cfg(windows)]
impl RouteRetirementPermit {
    /// Retain the actual Core retirement owner, never an atomic observation.
    #[must_use]
    pub fn from_owner(fence: Arc<dyn RendererRouteRetirementFence>) -> Self {
        Self { fence }
    }
    /// Charge only the captured ready:false operation before encoding. This
    /// capacity lease cannot substitute for the final retirement admission.
    #[must_use]
    pub fn try_reserve_route_unavailable(
        &self,
        document: &crate::renderer_contract::CheckedNativeDocumentLease,
        encoded_bytes: usize,
        accounted_bytes: usize,
    ) -> Option<NativeControlReservation> {
        self.fence
            .try_reserve_route_unavailable(document, encoded_bytes, accounted_bytes)
    }
    /// Admit only the fixed ready:false control, then release every Core guard
    /// before calling the native API on the already retained original view.
    #[must_use]
    pub fn try_admit_route_unavailable(
        &self,
        document: &crate::renderer_contract::CheckedNativeDocumentLease,
        admit: &mut dyn FnMut() -> bool,
    ) -> bool {
        self.fence.try_admit_route_unavailable(document, admit)
    }
}

/// One request handed off by the native renderer adapter through the existing endpoint.
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
    /// Original numeric result prepayment; keep after all owned request buffers.
    #[cfg(windows)]
    pub(crate) numeric_result: Option<NativeNumericResultReservation>,
}

impl RendererBridgeRequest {
    /// Observe the exact fixed-only Host completion shape before moving its
    /// existing storage into the same Core DATA record. This grants no role,
    /// document, native completion, or retirement authority.
    #[cfg(windows)]
    #[must_use]
    pub fn has_fixed_event_failure_metadata(&self) -> bool {
        matches!(
            self.operation,
            RendererBridgeOperation::EventQuiesced { completed: false }
        ) && self.channel == 0
            && self.payload.capacity() == 0
            && self.native_facts.is_none()
            && self.numeric_result.is_none()
            && self.document_lease.is_some()
            && self.role_witness.is_some()
    }

    /// Move the same native numeric prepayment into the canonical pending call
    /// before Core enqueues or allocates its result. Taking supplies no grant or
    /// completion; all result storage must be disposed before this token.
    #[cfg(windows)]
    #[must_use]
    pub fn take_numeric_result(&mut self) -> Option<NativeNumericResultReservation> {
        self.numeric_result.take()
    }
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
    /// Actual native frame/request URL on Mac or WebMessageReceived.Source on
    /// Windows, qualified through that adapter's same admitted source.
    #[must_use]
    pub fn url(&self) -> Option<&str> {
        self.native_facts.as_ref().map(NativeMessageFacts::url)
    }
    /// Platform-described origin metadata, never route or principal identity.
    /// Mac supplies its qualified native tuple serialization; Windows supplies
    /// only the informational file-scheme serialization of its qualified URL.
    #[must_use]
    pub fn origin(&self) -> Option<&str> {
        self.native_facts
            .as_ref()
            .and_then(NativeMessageFacts::origin)
    }
    /// Actual same-message Mac native origin tuple. Windows returns None:
    /// its qualified Source URL does not supply a native `SecurityOrigin` tuple.
    #[must_use]
    pub fn native_origin_tuple(&self) -> Option<(&str, &str, i64)> {
        self.native_facts
            .as_ref()
            .and_then(NativeMessageFacts::native_origin_tuple)
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
        /// The same Core CONTROL slot, retained through actual native completion.
        #[cfg(windows)]
        control: Option<NativeControlReservation>,
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
        /// Same checked document retained by the original DATA admission.
        #[cfg(windows)]
        document: crate::renderer_contract::CheckedNativeDocumentLease,
        /// Same original caller retained for cleanup after logical revocation.
        #[cfg(windows)]
        role: RendererRoleWitness,
        /// SAME Core DATA record's native storage half. Owned payload precedes
        /// this field so disposal cannot report storage release before bytes drop.
        #[cfg(windows)]
        storage: Option<NativeDataReservation>,
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
        /// Only ready:false may use the captured original Registry retirement.
        /// A live-role fence remains mandatory for ready:true and all data.
        #[cfg(windows)]
        retirement: Option<RouteRetirementPermit>,
        /// Existing session CONTROL storage; never a second native reservation.
        #[cfg(windows)]
        control: Option<NativeControlReservation>,
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
        /// Same original numeric result prepayment; drops after owned result bytes.
        #[cfg(windows)]
        numeric_result: Option<NativeNumericResultReservation>,
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
        /// Reserved before retaining this terminal outcome and its native copies.
        #[cfg(windows)]
        control: Option<NativeControlReservation>,
        /// Same original numeric result storage; diagnostics must drop before it.
        #[cfg(windows)]
        numeric_result: Option<NativeNumericResultReservation>,
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
    #[cfg(target_os = "macos")]
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

/// Same 128-bit document credential generator used by native adapters.
pub(crate) fn credential() -> Result<String, crate::WvError> {
    use std::fmt::Write as _;
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| crate::WvError::RendererBridge {
        detail: String::from("Host credential randomness unavailable"),
    })?;
    let mut value = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(&mut value, "{byte:02x}");
    }
    Ok(value)
}

/// One existing Windows loop service turn, outside all native/Core guards.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RendererBridgeServiceTurn {
    /// Observe actual worker/source progress without waiting or renewing clocks.
    Poll,
    /// Drain actual Core protocol/source owners under one original I/O deadline.
    Drain,
}

/// The existing Core endpoint owner supplies this borrowed callback.
/// Poll must return None. Drain must return Some of its one original absolute
/// deadline after actual producer/protocol drainage; no subsequent turn may
/// renew it. A result is not a native callback or consumer completion receipt.
#[cfg(windows)]
pub type RendererBridgeService<'a> =
    dyn FnMut(RendererBridgeServiceTurn) -> Result<Option<std::time::Instant>, crate::WvError> + 'a;
