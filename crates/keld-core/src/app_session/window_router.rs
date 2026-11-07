//! Private window service composed by the existing primary app-link owner.
//!
//! Lock order is shutdown transition -> current generation/writer -> windows.
//! In particular the windows guard is dropped before failed-write retirement,
//! because retirement re-enters it to fence the old owner. No native handle,
//! frame parser, output queue, principal or subscription policy is minted here.

use std::io::Write as _;

use keld_ipc::channel_table::{Authority, ReceiveClass};
use keld_ipc::window::{
    DESKTOP_EVENT_STORAGE_BYTES, Rect, WindowEvent, WindowRequest, WindowResponse, WindowState,
};
#[cfg(target_os = "macos")]
use keld_ipc::window::{DesktopFact, DesktopFacts};
use keld_ipc::{ChannelId, SubscribeRefusal};
use keld_wv::{BuildStage, WindowSnapshot};

#[cfg(target_os = "macos")]
use super::window_registry::OriginalLoadAdmission;
use super::window_registry::{
    AbandonedClose, BuildFailureOutcome, BuildOutcome, CloseOutcome, CreateOutcome, FactOutcome,
    NativeSetter, OwnedWindowEvent, ReleaseOutcome, SessionEffect, WindowError, WindowOwner,
    WindowRegistry, WindowReply,
};
use super::{
    ActivePrimaryGeneration, AppWindowCommand, AppWindowEvent, CallError, CorrelationId, FrameKind,
    HostAppError, IpcError, LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleResponse, Ordering,
    PrimaryCallerContext, PrimaryRouterHandle, app_detail, app_ipc, append_app_cleanup,
    collapse_app_results, decode, encode, with_primary_caller_locked, write_frame,
};

// Current maximum payloads: WindowEvent Created = 50 bytes, WindowResponse
// Created/Adopted = 16, LifecycleResponse SubscribeRefused = 2. The literal
// codec vectors and maximum Created socket test qualify the current schema.
const WINDOW_PAYLOAD_STORAGE_BYTES: usize = 64;
/// GH481's event-specific body cap; the existing control frame limit is unchanged.
#[cfg(target_os = "macos")]
const MAX_DESKTOP_EVENT_BYTES: usize = 32 * 1024;

/// Core alone increments native observation epochs. Epoch zero has no authority.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DesktopEpoch(pub(super) u64);
#[cfg(target_os = "macos")]
impl DesktopEpoch {
    pub(super) fn advance(&mut self) -> Result<u64, HostAppError> {
        let next = self.0.checked_add(1).ok_or_else(|| {
            app_detail(
                "desktop observation",
                "desktop epoch exhausted; end the Host session",
            )
        })?;
        self.0 = next;
        Ok(next)
    }
}
/// Check the complete encoded postcard event body, not its outer KIPC header.
#[cfg(target_os = "macos")]
pub(super) fn admit_desktop_payload_bytes(bytes: usize) -> Result<(), HostAppError> {
    if bytes > MAX_DESKTOP_EVENT_BYTES {
        return Err(app_detail(
            "desktop observation",
            "desktop event exceeds 32 KiB",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn desktop_rect(native: keld_wv::AppDesktopRect) -> Rect {
    Rect {
        x: native.x,
        y: native.y,
        width: native.width,
        height: native.height,
    }
}

/// A pure validation/conversion boundary. The macOS UI source owns physical capture.
#[cfg(target_os = "macos")]
fn qualified_desktop(facts: keld_wv::AppDesktopFacts) -> Option<DesktopFacts> {
    if facts.primary_id == 0 || facts.displays.is_empty() || facts.displays.len() > 32 {
        return None;
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut primary_count = 0;
    let mut displays = Vec::with_capacity(facts.displays.len());
    for display in facts.displays {
        if display.id == 0
            || !ids.insert(display.id)
            || display.bounds.width == 0
            || display.bounds.height == 0
            || display.work_area.width == 0
            || display.work_area.height == 0
            || !display.scale_factor.is_finite()
            || display.scale_factor <= 0.0
        {
            return None;
        }
        if display.id == facts.primary_id {
            primary_count += 1;
        }
        displays.push(DesktopFact {
            id: display.id,
            bounds: desktop_rect(display.bounds),
            work_area: desktop_rect(display.work_area),
            scale_factor: display.scale_factor,
        });
    }
    if primary_count != 1 {
        return None;
    }
    Some(DesktopFacts {
        primary_id: facts.primary_id,
        displays,
        should_use_dark_colors: facts.should_use_dark_colors,
    })
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingDesktopSubscribe {
    owner: WindowOwner,
    correlation: CorrelationId,
}

#[derive(Default)]
struct WindowBuildResources {
    #[cfg(target_os = "macos")]
    preload_image: Option<std::sync::Arc<keld_wv::wkwebview::AppPreloadImage>>,
    #[cfg(target_os = "macos")]
    original_source: Option<std::sync::Arc<keld_wv::wkwebview::AdmittedWebAppSource>>,
}

#[cfg(target_os = "macos")]
fn original_refusal_detail(error: keld_wv::NativeNavigationRefusal) -> &'static str {
    match error {
        keld_wv::NativeNavigationRefusal::UnknownView => "native view is unavailable",
        keld_wv::NativeNavigationRefusal::MissingSource => "verified native source is unavailable",
        keld_wv::NativeNavigationRefusal::InvalidQuery => "original query was rejected",
        keld_wv::NativeNavigationRefusal::NativeUrlRejected => "native entry URL was rejected",
        keld_wv::NativeNavigationRefusal::RetainedByteLimit => {
            "native retained-byte capacity is exhausted"
        }
        keld_wv::NativeNavigationRefusal::Unavailable => "native navigation owner is unavailable",
    }
}

/// One optional facade service for the whole app session. None in the parent is
/// the permanent renderer mode; current production boot constructors pass None.
pub(super) struct WindowSession {
    registry: WindowRegistry,
    native_supported: bool,
    #[cfg(target_os = "macos")]
    original_selection: Option<std::sync::Arc<super::original_webapp::OriginalLoadSelection>>,
    // The one subscription owner is this live link, not the window instances.
    subscribed: Option<WindowOwner>,
    #[cfg(target_os = "macos")]
    desktop_required: bool,
    #[cfg(target_os = "macos")]
    desktop_seen: bool,
    #[cfg(target_os = "macos")]
    desktop_epoch: DesktopEpoch,
    #[cfg(target_os = "macos")]
    desktop_latest: Option<DesktopFacts>,
    #[cfg(target_os = "macos")]
    pending_desktop: Option<PendingDesktopSubscribe>,
    // One cold allocation, reused under the existing windows lock. The IPC
    // owner supplies the exact cap; no per-event allocation or extra queue.
    navigation_payload: Box<[u8]>,
}
impl WindowSession {
    pub(super) fn new(native_supported: bool) -> Self {
        Self {
            registry: WindowRegistry::new(),
            native_supported,
            #[cfg(target_os = "macos")]
            original_selection: None,
            subscribed: None,
            #[cfg(target_os = "macos")]
            desktop_required: false,
            #[cfg(target_os = "macos")]
            desktop_seen: false,
            #[cfg(target_os = "macos")]
            desktop_epoch: DesktopEpoch(0),
            #[cfg(target_os = "macos")]
            desktop_latest: None,
            #[cfg(target_os = "macos")]
            pending_desktop: None,
            navigation_payload: vec![0_u8; keld_ipc::window::NAVIGATION_EVENT_STORAGE_BYTES]
                .into_boxed_slice(),
        }
    }
    /// Actual macOS facade boot selects S1; old component and other OS paths
    /// keep the existing baseline subscription semantics.
    #[cfg(target_os = "macos")]
    pub(super) fn with_desktop(native_supported: bool) -> Self {
        let mut session = Self::new(native_supported);
        session.desktop_required = true;
        session
    }
    /// Cold boot supplies only the existing verified graph/source/policy result.
    #[cfg(target_os = "macos")]
    pub(super) fn install_original_selection(
        &mut self,
        selection: super::original_webapp::OriginalLoadSelection,
    ) -> Result<(), HostAppError> {
        if self.original_selection.is_some() {
            return Err(app_detail(
                "original source install",
                "the immutable source was already selected",
            ));
        }
        self.original_selection = Some(std::sync::Arc::new(selection));
        Ok(())
    }
    /// Read-only selection on the same subscribed owner; the parent separately
    /// holds its current-role/Ready/transition fence through intent publication.
    #[cfg(target_os = "macos")]
    pub(super) fn original_menu_quit_selected(&self, owner: WindowOwner) -> bool {
        self.native_supported && self.original_selection.is_some() && self.subscribed == Some(owner)
    }
    /// A later generation cannot silently change the canonical versions already
    /// captured in the original immutable preload image/source selection.
    #[cfg(target_os = "macos")]
    pub(super) fn validate_runtime_versions(
        &self,
        versions: Option<&std::collections::BTreeMap<String, String>>,
    ) -> Result<(), HostAppError> {
        if let Some(selection) = &self.original_selection
            && versions != Some(selection.runtime_versions())
        {
            return Err(app_detail(
                "original runtime versions",
                "the new generation is missing or changed the immutable original preload runtime versions; restore the verified runtime facts before installing the successor",
            ));
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    pub(super) fn admitted_renderer_window(
        &self,
        owner: WindowOwner,
        view: keld_wv::WebviewId,
    ) -> Result<keld_ipc::window::WindowRef, WindowError> {
        if self.subscribed != Some(owner) {
            return Err(WindowError::WrongOwner);
        }
        if !self.native_supported {
            return Err(WindowError::NotReady);
        }
        self.registry.admitted_renderer_window(owner, view)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn admit_renderer_route(
        &mut self,
        owner: WindowOwner,
        view: keld_wv::WebviewId,
        document: keld_wv::wkwebview::MacNativeDocumentLease,
    ) -> Result<super::window_registry::RendererRoute, WindowError> {
        self.admitted_renderer_window(owner, view)?;
        self.registry.admit_renderer_route(owner, view, document)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn renderer_route(
        &self,
        owner: WindowOwner,
        view: keld_wv::WebviewId,
        navigation: Option<u32>,
    ) -> Result<super::window_registry::RendererRoute, WindowError> {
        self.admitted_renderer_window(owner, view)?;
        self.registry.renderer_route(owner, view, navigation)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn note_renderer_registered(
        &mut self,
        view: keld_wv::WebviewId,
        document: keld_wv::wkwebview::MacNativeDocumentLease,
    ) -> Result<(), WindowError> {
        self.registry.note_renderer_registered(view, document)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn for_each_registered_renderer_route(
        &self,
        owner: WindowOwner,
        callback: impl FnMut(super::window_registry::RendererRoute) -> Result<(), HostAppError>,
    ) -> Result<(), HostAppError> {
        self.registry
            .for_each_registered_renderer_route(owner, callback)
    }
    /// The existing current/shutdown fence authenticates this fresh Subscribe.
    /// Native Registered and window enumeration never call this role transition.
    #[cfg(target_os = "macos")]
    pub(super) fn rebind_recovered_renderer_documents(&mut self, owner: WindowOwner) -> bool {
        self.native_supported
            && self.subscribed == Some(owner)
            && self.registry.rebind_recovered_renderer_documents(owner)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn admitted_dialog_view(
        &mut self,
        owner: WindowOwner,
        window: keld_ipc::window::WindowRef,
    ) -> Result<keld_wv::WebviewId, WindowError> {
        if self.subscribed != Some(owner) {
            return Err(WindowError::WrongOwner);
        }
        if !self.native_supported {
            return Err(WindowError::NotReady);
        }
        self.registry.admitted_dialog_view(owner, window)
    }
}

/// Resolution precedes this classification. Metadata comes from the one table;
/// only component tests can provide otherwise unallocated metadata combinations.
fn subscription_refusal(
    class: ReceiveClass,
    authority: Authority,
    lifecycle: bool,
    primary: bool,
) -> Option<SubscribeRefusal> {
    if !class.carries_host_events() || lifecycle {
        Some(SubscribeRefusal::NotSubscribable)
    } else if !matches!(authority, Authority::HostInternal) {
        Some(SubscribeRefusal::GuardedNotSupported)
    } else if !primary {
        Some(SubscribeRefusal::RoleNotAdmitted)
    } else {
        None
    }
}
fn owner(active: &ActivePrimaryGeneration) -> WindowOwner {
    WindowOwner::new(active.identity, active.attempt)
}
fn active_owner(
    current: &mut Option<ActivePrimaryGeneration>,
    expected: WindowOwner,
) -> Option<&mut ActivePrimaryGeneration> {
    current
        .as_mut()
        .filter(|active| owner(active) == expected && !active.reader_stop.load(Ordering::Acquire))
}
fn write_payload(
    active: &mut ActivePrimaryGeneration,
    kind: FrameKind,
    channel: ChannelId,
    correlation: CorrelationId,
    payload: &[u8],
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    write_frame(&mut active.writer, kind, 0, channel, correlation, payload).map_err(|source| {
        *failed = Some(active.attempt);
        app_ipc("window ordered writer", &source)
    })
}
fn window_reply(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    reply: WindowResponse,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let mut scratch = [0_u8; WINDOW_PAYLOAD_STORAGE_BYTES];
    let bytes = keld_ipc::codec::encode_into(&reply, &mut scratch)
        .map_err(|source| app_ipc("window reply encode", &source))?;
    write_payload(
        active,
        FrameKind::Reply,
        keld_ipc::channel_table::WINDOW.id(),
        correlation,
        bytes,
        failed,
    )
}
fn lifecycle_reply(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    reply: LifecycleResponse,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let mut scratch = [0_u8; WINDOW_PAYLOAD_STORAGE_BYTES];
    let bytes = keld_ipc::codec::encode_into(&reply, &mut scratch)
        .map_err(|source| app_ipc("window Subscribe reply encode", &source))?;
    write_payload(
        active,
        FrameKind::Reply,
        LIFECYCLE_CHANNEL,
        correlation,
        bytes,
        failed,
    )
}
fn call_error(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    code: &'static str,
    message: String,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let bytes = encode(&CallError {
        code: code.to_owned(),
        message,
    })
    .map_err(|source| app_ipc("window error encode", &source))?;
    // The actual registered refusal survives lossy/untrusted Bun crash tails.
    // Fixed owner fields only: never format the app-derived error message.
    // Diagnostic failure cannot retry or replace the canonical terminal frame.
    let _ = writeln!(
        std::io::stderr().lock(),
        "{code} phase=window-request correlation={}",
        correlation.0,
    );
    write_payload(
        active,
        FrameKind::Err,
        keld_ipc::channel_table::WINDOW.id(),
        correlation,
        &bytes,
        failed,
    )
}

#[cfg(target_os = "macos")]
fn lifecycle_call_error(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    code: &'static str,
    message: &'static str,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let bytes = encode(&CallError {
        code: code.to_owned(),
        message: format!("{code}: {message}"),
    })
    .map_err(|source| app_ipc("desktop Subscribe ERR encode", &source))?;
    write_payload(
        active,
        FrameKind::Err,
        LIFECYCLE_CHANNEL,
        correlation,
        &bytes,
        failed,
    )
}

#[cfg(target_os = "macos")]
fn desktop_unavailable(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    lifecycle_call_error(
        active,
        correlation,
        "KELD-CORE-049",
        "The native desktop snapshot is unavailable. Correct the Host's native display/appearance error and relaunch.",
        failed,
    )
}
fn navigation_committed_event(
    active: &mut ActivePrimaryGeneration,
    window: keld_ipc::window::WindowRef,
    document_generation: u32,
    native_url: &str,
    scratch: &mut [u8],
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let bytes = keld_ipc::window::encode_navigation_committed_into(
        window,
        document_generation,
        native_url,
        scratch,
    )
    .map_err(|error| app_ipc("original native commit encode", &error))?;
    write_payload(
        active,
        FrameKind::Event,
        keld_ipc::channel_table::WINDOW.id(),
        CorrelationId(0),
        bytes,
        failed,
    )
}
#[cfg(target_os = "macos")]
fn recovered_document_snapshots(
    registry: &WindowRegistry,
    active: &mut ActivePrimaryGeneration,
    owner: WindowOwner,
    scratch: &mut [u8],
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    registry.for_each_recovered_document(owner, |window, document, native_url| {
        let bytes = keld_ipc::window::encode_document_recovered_into(
            window,
            document.stamp().observed_generation(),
            native_url.as_str(),
            scratch,
        )
        .map_err(|error| app_ipc("original recovery snapshot encode", &error))?;
        // UI revocation is independent of the registry lock. Observe the same
        // native lease again at publication; the fact still grants no route.
        if !document.is_current() {
            return Ok(());
        }
        write_payload(
            active,
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0),
            bytes,
            failed,
        )
    })
}
#[cfg(target_os = "macos")]
fn original_native_failure(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    failure: (
        Option<super::window_registry::PendingWindowCall>,
        &'static str,
        &'static str,
    ),
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if let (Some(call), code, detail) = failure
        && subscribed == Some(call.owner)
        && let Some(active) = active_owner(current, call.owner)
    {
        call_error(
            active,
            call.correlation,
            code,
            format!(
                "{code}: {detail}; inspect the native diagnostic, correct it and issue a new explicit load"
            ),
            failed,
        )?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn original_native_finished(
    session: &mut WindowSession,
    current: &mut Option<ActivePrimaryGeneration>,
    context: keld_wv::NativeNavigationContext,
    stamp: keld_wv::wkwebview::MacNativeDocumentStamp,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let Some(completed) = session.registry.original_load_finished(context, stamp) else {
        return Ok(());
    };
    owned_event(
        current,
        session.subscribed,
        &completed.fact,
        &mut session.navigation_payload,
        failed,
    )?;
    owned_reply(current, session.subscribed, completed.reply, failed)
}

#[cfg(target_os = "macos")]
fn original_close_refusal(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let error = WindowError::OriginalDocumentCloseRefused;
    call_error(active, correlation, error.code(), error.to_string(), failed)
}
fn event(
    active: &mut ActivePrimaryGeneration,
    fact: &keld_ipc::window::WindowEvent,
    scratch: &mut [u8],
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if let WindowEvent::DocumentRecovered {
        window,
        document_generation,
        native_url,
    } = fact
    {
        let bytes = keld_ipc::window::encode_document_recovered_into(
            *window,
            *document_generation,
            native_url,
            scratch,
        )
        .map_err(|error| app_ipc("original recovery snapshot encode", &error))?;
        return write_payload(
            active,
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0),
            bytes,
            failed,
        );
    }
    if let WindowEvent::NavigationCommitted {
        window,
        document_generation,
        native_url,
    } = fact
    {
        return navigation_committed_event(
            active,
            *window,
            *document_generation,
            native_url,
            scratch,
            failed,
        );
    }
    if matches!(fact, WindowEvent::DesktopSnapshot { .. }) {
        // The qualified fixed-shape 32-display value needs at most 1715 bytes,
        // proven by the IPC maximum-width vector. No per-event heap allocation.
        let mut scratch = [0_u8; DESKTOP_EVENT_STORAGE_BYTES];
        let bytes = keld_ipc::codec::encode_into(fact, &mut scratch)
            .map_err(|source| app_ipc("desktop fact encode", &source))?;
        #[cfg(target_os = "macos")]
        admit_desktop_payload_bytes(bytes.len())?;
        return write_payload(
            active,
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0),
            bytes,
            failed,
        );
    }
    // Remaining fixed-shape facts fit the Created maximum with both u64 maxima;
    // the owning codec tests qualify this small-storage branch.
    let mut scratch = [0_u8; WINDOW_PAYLOAD_STORAGE_BYTES];
    let bytes = keld_ipc::codec::encode_into(fact, &mut scratch)
        .map_err(|source| app_ipc("window fact encode", &source))?;
    write_payload(
        active,
        FrameKind::Event,
        keld_ipc::channel_table::WINDOW.id(),
        CorrelationId(0),
        bytes,
        failed,
    )
}
fn owned_event(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    fact: &OwnedWindowEvent,
    scratch: &mut [u8],
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if subscribed == Some(fact.owner)
        && let Some(active) = active_owner(current, fact.owner)
    {
        event(active, &fact.event, scratch, failed)?;
    }
    Ok(())
}
fn owned_reply(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    reply: WindowReply,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if subscribed == Some(reply.owner)
        && let Some(active) = active_owner(current, reply.owner)
    {
        window_reply(active, reply.correlation, reply.response, failed)?;
    }
    Ok(())
}
fn last_window_locked(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    recipient: Option<WindowOwner>,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if let Some(recipient) = recipient
        && subscribed == Some(recipient)
        && let Some(active) = active_owner(current, recipient)
    {
        let mut scratch = [0_u8; WINDOW_PAYLOAD_STORAGE_BYTES];
        let bytes = keld_ipc::codec::encode_into(&LifecycleEvent::LastWindowClosed, &mut scratch)
            .map_err(|source| app_ipc("window last-window encode", &source))?;
        write_payload(
            active,
            FrameKind::Event,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            bytes,
            failed,
        )?;
        // Registry transitions own managed counts. The parent's retained
        // legacy flag must never synthesize a LastWindowClosed on recovery.
    }
    Ok(())
}

fn window_build_failed_locked(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    outcome: BuildFailureOutcome,
    stage: BuildStage,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if outcome.diagnostic {
        eprintln!(
            "KELD-CORE-041: ignored unexpected native build failure; inspect the UI build correlation"
        );
    }
    if let Some(call) = outcome.call
        && subscribed == Some(call.owner)
        && let Some(active) = active_owner(current, call.owner)
    {
        let stage = match stage {
            BuildStage::Window => "window",
            BuildStage::Webview => "webview",
        };
        call_error(
            active,
            call.correlation,
            "KELD-CORE-041",
            format!(
                "KELD-CORE-041: native {stage} creation failed. Inspect the UI owner's typed error and correct the requested window resources."
            ),
            failed,
        )?;
    }
    last_window_locked(current, subscribed, outcome.last_window, failed)
}
fn window_released_locked(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    outcome: ReleaseOutcome,
    scratch: &mut [u8],
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if outcome.diagnostic {
        eprintln!("KELD-CORE-038: unexpected native release; inspect UI teardown ordering");
    }
    if outcome.dropped_facts != 0 {
        eprintln!(
            "window teardown dropped {} native facts",
            outcome.dropped_facts
        );
    }
    if let Some(fact) = outcome.fact {
        owned_event(current, subscribed, &fact, scratch, failed)?;
    }
    last_window_locked(current, subscribed, outcome.last_window, failed)?;
    for call in outcome.destroy_calls {
        owned_reply(
            current,
            subscribed,
            WindowReply {
                owner: call.owner,
                correlation: call.correlation,
                response: WindowResponse::Destroyed,
            },
            failed,
        )?;
    }
    Ok(())
}

fn report_abandoned(close: AbandonedClose) {
    eprintln!(
        "window {} close_seq {} Abandoned; the pending request will not be replayed",
        close.window.id.0, close.close_seq
    );
}

fn native_state(snapshot: WindowSnapshot) -> WindowState {
    WindowState {
        bounds: Rect {
            x: snapshot.x,
            y: snapshot.y,
            width: snapshot.width,
            height: snapshot.height,
        },
        focused: snapshot.focused,
        maximized: snapshot.maximized,
        full_screen: snapshot.full_screen,
    }
}

/// Admit or reserve the one desktop Subscribe under the existing writer/registry locks.
#[cfg(target_os = "macos")]
fn subscribe_desktop_locked(
    session: &mut WindowSession,
    active: &mut ActivePrimaryGeneration,
    caller_owner: WindowOwner,
    correlation: CorrelationId,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if session.subscribed == Some(caller_owner) {
        return lifecycle_reply(active, correlation, LifecycleResponse::Subscribed, failed);
    }
    if let Some(pending) = session.pending_desktop {
        if pending.owner == caller_owner && pending.correlation == correlation {
            // A forged duplicate using the original pending
            // correlation could settle it with ERR and then
            // settle it again with Subscribed after Init.
            // This is a fatal protocol violation, matching
            // the established pending-window-CALL policy.
            *failed = Some(active.attempt);
            return Err(app_ipc(
                "desktop Subscribe correlation admission",
                &IpcError::Protocol {
                    detail: "desktop Subscribe reuses its pending correlation",
                },
            ));
        }
        // A different second correlation gets only its own
        // ERR; it cannot replace the reserved first call or
        // receive a desktop/window replay.
        return lifecycle_call_error(
            active,
            correlation,
            "KELD-IPC-005",
            "a desktop Subscribe is already pending on this role",
            failed,
        );
    }
    if !session.desktop_seen {
        session.pending_desktop = Some(PendingDesktopSubscribe {
            owner: caller_owner,
            correlation,
        });
        return Ok(());
    }
    complete_desktop_subscribe_locked(session, active, caller_owner, correlation, failed)
}

/// The one writer holds current + window registry locks for this entire batch.
/// Successor adoption marks and subscription eligibility become visible only
/// after the complete desktop/Created replay and correlated accepted reply.
#[cfg(target_os = "macos")]
fn complete_desktop_subscribe_locked(
    session: &mut WindowSession,
    active: &mut ActivePrimaryGeneration,
    owner: WindowOwner,
    correlation: CorrelationId,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let Some(facts) = session.desktop_latest.as_ref() else {
        return desktop_unavailable(active, correlation, failed);
    };
    event(
        active,
        &WindowEvent::DesktopSnapshot {
            epoch: session.desktop_epoch.0,
            snapshot: Some(facts.clone()),
        },
        &mut session.navigation_payload,
        failed,
    )?;
    session.registry.subscribe_successor(owner);
    for window in session.registry.replay(owner) {
        event(active, &window, &mut session.navigation_payload, failed)?;
    }
    recovered_document_snapshots(
        &session.registry,
        active,
        owner,
        &mut session.navigation_payload,
        failed,
    )?;
    lifecycle_reply(active, correlation, LifecycleResponse::Subscribed, failed)?;
    session.subscribed = Some(owner);
    Ok(())
}

#[cfg(target_os = "macos")]
fn desktop_observed_locked(
    session: &mut WindowSession,
    current: &mut Option<ActivePrimaryGeneration>,
    facts: Option<keld_wv::AppDesktopFacts>,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if !session.desktop_required {
        return Err(app_detail(
            "desktop observation",
            "native desktop sample entered an unselected facade session",
        ));
    }
    let validated = facts.and_then(qualified_desktop);
    if session.desktop_seen && session.desktop_latest == validated {
        // Identical native capture is idempotent and consumes no new epoch.
        return Ok(());
    }
    let next = session.desktop_epoch.advance()?;
    session.desktop_seen = true;
    session.desktop_latest = validated;
    // Enforce the full encoded size even when no subscriber has yet bound.
    // Existing 16-MiB frame bounds do not substitute for this one.
    let payload = encode(&WindowEvent::DesktopSnapshot {
        epoch: next,
        snapshot: session.desktop_latest.clone(),
    })
    .map_err(|error| app_ipc("desktop snapshot qualification", &error))?;
    admit_desktop_payload_bytes(payload.len())?;
    if let Some(pending) = session.pending_desktop.take() {
        if let Some(active) = active_owner(current, pending.owner) {
            return complete_desktop_subscribe_locked(
                session,
                active,
                pending.owner,
                pending.correlation,
                failed,
            );
        }
        // Old-generation timeout or link death consumed its authority. A late
        // successful sample has no legal recipient and creates no replay.
        return Ok(());
    }
    if let Some(owner) = session.subscribed
        && let Some(active) = active_owner(current, owner)
    {
        event(
            active,
            &WindowEvent::DesktopSnapshot {
                epoch: next,
                snapshot: session.desktop_latest.clone(),
            },
            &mut session.navigation_payload,
            failed,
        )?;
    }
    Ok(())
}

impl PrimaryRouterHandle {
    /// Caller already holds the shutdown transition. Ready publication cannot
    /// overtake terminal end or the registry's serviceability transition.
    pub(super) fn mark_window_registry_ready_guarded(&self) -> Result<bool, HostAppError> {
        let _current = self
            .current
            .lock()
            .map_err(|_| app_detail("window Ready", "generation lock poisoned"))?;
        self.windows.as_ref().map_or(Ok(true), |windows| {
            Ok(windows
                .lock()
                .map_err(|_| app_detail("window Ready", "registry lock poisoned"))?
                .registry
                .mark_ready())
        })
    }
    /// Called with the current writer lock held; never holds a windows guard on entry.
    pub(super) fn retire_window_owner(&self, retired: WindowOwner) -> Result<(), HostAppError> {
        #[cfg(target_os = "macos")]
        let menu_retirement = self
            .menu
            .as_ref()
            .map_or(Ok(()), |menu| menu.retire(retired));
        let window_retirement = self.windows.as_ref().map_or(Ok(()), |windows| {
            let mut session = windows
                .lock()
                .map_err(|_| app_detail("window role retirement", "registry lock poisoned"))?;
            if session.subscribed == Some(retired) {
                session.subscribed = None;
            }
            #[cfg(target_os = "macos")]
            if session
                .pending_desktop
                .is_some_and(|pending| pending.owner == retired)
            {
                session.pending_desktop = None;
            }
            session.registry.role_lost(retired, report_abandoned);
            Ok(())
        });
        #[cfg(target_os = "macos")]
        {
            super::collapse_app_results([menu_retirement, window_retirement])
        }
        #[cfg(not(target_os = "macos"))]
        {
            window_retirement
        }
    }
    fn finish_window_write(
        &self,
        current: &mut Option<ActivePrimaryGeneration>,
        failed: Option<u32>,
        result: Result<(), HostAppError>,
    ) -> Result<(), HostAppError> {
        match (failed, result) {
            (Some(attempt), Err(primary)) => {
                // No windows guard survives the operation closure. Writer loss or
                // an owned-correlation violation retires this link before unlock.
                eprintln!("{primary}");
                let retirement = self.retire_current_generation_locked(
                    current,
                    attempt,
                    "window session failed",
                    "failed window session close",
                );
                Err(append_app_cleanup(primary, [retirement]))
            }
            (_, result) => result,
        }
    }
    pub(super) fn subscribe_window(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
    ) -> Result<(), HostAppError> {
        let mut failed = None;
        let result = with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let result = (|| {
                let active = current.as_mut().ok_or_else(|| {
                    app_detail("window Subscribe", "validated generation disappeared")
                })?;
                let entry = keld_ipc::channel_table::CHANNEL_TABLE
                    .iter()
                    .find(|entry| entry.wire_id() == channel);
                let refusal = entry.map_or(Some(SubscribeRefusal::UnknownChannel), |entry| {
                    subscription_refusal(
                        entry.class(),
                        entry.authority(),
                        entry.id() == LIFECYCLE_CHANNEL,
                        caller.identity.is_primary(),
                    )
                });
                if let Some(reason) = refusal {
                    return lifecycle_reply(
                        active,
                        correlation,
                        LifecycleResponse::SubscribeRefused { reason },
                        &mut failed,
                    );
                }
                if channel != keld_ipc::channel_table::WINDOW.wire_id() || self.windows.is_none() {
                    return lifecycle_reply(
                        active,
                        correlation,
                        LifecycleResponse::SubscribeRefused {
                            reason: SubscribeRefusal::NotSubscribable,
                        },
                        &mut failed,
                    );
                }
                let Some(windows) = &self.windows else {
                    return Err(app_detail(
                        "window Subscribe",
                        "resolved window service disappeared",
                    ));
                };
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("window Subscribe", "registry lock poisoned"))?;
                let caller_owner = WindowOwner::new(caller.identity, caller.attempt);
                #[cfg(target_os = "macos")]
                if session.desktop_required {
                    return subscribe_desktop_locked(
                        &mut session,
                        active,
                        caller_owner,
                        correlation,
                        &mut failed,
                    );
                }
                if session.subscribed != Some(caller_owner) {
                    session.registry.subscribe_successor(caller_owner);
                    session.subscribed = Some(caller_owner);
                    // The existing current writer lock makes this complete replay
                    // and the correlated acceptance one ordered stream operation.
                    let WindowSession {
                        registry,
                        navigation_payload,
                        ..
                    } = &mut *session;
                    for fact in registry.replay(caller_owner) {
                        event(active, &fact, navigation_payload, &mut failed)?;
                    }
                    #[cfg(target_os = "macos")]
                    recovered_document_snapshots(
                        registry,
                        active,
                        caller_owner,
                        navigation_payload,
                        &mut failed,
                    )?;
                }
                lifecycle_reply(
                    active,
                    correlation,
                    LifecycleResponse::Subscribed,
                    &mut failed,
                )
            })();
            self.finish_window_write(current, failed, result)
        });
        if let Some(attempt) = failed {
            // The owner can call back into generation state: release transition,
            // current and windows before asking Runtime to retire/recover the role.
            let owner_result = self.link_failed(attempt);
            collapse_app_results([result, owner_result])
        } else {
            result
        }
    }
    pub(super) fn dispatch_window(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        let mut failed = None;
        let result = with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let result = (|| {
                let active = current.as_mut().ok_or_else(|| {
                    app_detail("window request", "validated generation disappeared")
                })?;
                let caller_owner = WindowOwner::new(caller.identity, caller.attempt);
                let unavailable = || {
                    "KELD-CORE-045: window link is not subscribed. Complete the accepted window subscription and replay before issuing window calls.".to_owned()
                };
                let Some(windows) = &self.windows else {
                    return call_error(
                        active,
                        correlation,
                        "KELD-CORE-045",
                        unavailable(),
                        &mut failed,
                    );
                };
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("window request", "registry lock poisoned"))?;
                //045 precedes readiness, platform and even body decoding. A refused
                //call cannot allocate a pair, enqueue work or mutate adoption marks.
                if session.subscribed != Some(caller_owner) {
                    return call_error(
                        active,
                        correlation,
                        "KELD-CORE-045",
                        unavailable(),
                        &mut failed,
                    );
                }
                if !self.window_ready.load(Ordering::Acquire) {
                    return call_error(active, correlation, "KELD-CORE-040", "KELD-CORE-040: registry is not ready. Wait for host Ready and accepted subscription/replay.".to_owned(), &mut failed);
                }
                if !session.native_supported {
                    return call_error(active, correlation, "KELD-CORE-042", "KELD-CORE-042: native window calls are unsupported by this platform adapter. Use a qualified window backend.".to_owned(), &mut failed);
                }
                if session.registry.has_pending_call(caller_owner, correlation) {
                    failed = Some(active.attempt);
                    return Err(app_ipc(
                        "window correlation admission",
                        &IpcError::Protocol {
                            detail: "window CALL reuses an already-pending correlation",
                        },
                    ));
                }
                if payload.len() > keld_ipc::window::MAX_ORIGINAL_LOAD_REQUEST_BYTES {
                    return call_error(active, correlation, "KELD-CORE-051", "KELD-CORE-051: window call exceeds the bounded original-load request size; send the exact selected original URI.".to_owned(), &mut failed);
                }
                let request: WindowRequest =
                    decode(payload).map_err(|source| app_ipc("window request decode", &source))?;
                self.window_request_locked(&mut session, active, correlation, request, &mut failed)
            })();
            self.finish_window_write(current, failed, result)
        });
        if let Some(attempt) = failed {
            collapse_app_results([result, self.link_failed(attempt)])
        } else {
            result
        }
    }
    fn window_request_locked(
        &self,
        session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        request: WindowRequest,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let caller = owner(active);
        match request {
            WindowRequest::Create {
                width,
                height,
                x,
                y,
            } => {
                let outcome = session
                    .registry
                    .create(caller, correlation, width, height, x, y);
                self.window_create_result(
                    active,
                    correlation,
                    outcome,
                    WindowBuildResources::default(),
                    failed,
                )
            }
            #[cfg(target_os = "macos")]
            request @ WindowRequest::CreateWithPreload { .. } => self
                .window_create_with_original_locked(session, active, correlation, request, failed),
            #[cfg(not(target_os = "macos"))]
            request @ WindowRequest::CreateWithPreload { .. } => {
                Self::window_create_with_original_locked(
                    session,
                    active,
                    correlation,
                    request,
                    failed,
                )
            }
            #[cfg(target_os = "macos")]
            WindowRequest::LoadUrl { window, uri } => {
                self.window_original_load_locked(session, active, correlation, window, &uri, failed)
            }
            #[cfg(not(target_os = "macos"))]
            WindowRequest::LoadUrl { window, uri } => Self::window_original_load_locked(
                session,
                active,
                correlation,
                window,
                &uri,
                failed,
            ),
            WindowRequest::SetMaximized { window, maximized } => {
                let outcome = session.registry.set_maximized(caller, window, maximized);
                self.window_setter_result(active, correlation, outcome, failed)
            }
            WindowRequest::SetFullScreen {
                window,
                full_screen,
            } => {
                let outcome = session
                    .registry
                    .set_full_screen(caller, window, full_screen);
                self.window_setter_result(active, correlation, outcome, failed)
            }
            WindowRequest::RequestClose { window } => {
                let outcome = session.registry.request_close(caller, window);
                self.window_close_result(
                    active,
                    correlation,
                    outcome,
                    &mut session.navigation_payload,
                    failed,
                )
            }
            request @ WindowRequest::CloseReply { .. } => {
                self.window_close_reply_locked(session, active, correlation, &request, failed)
            }
            WindowRequest::DestroyOriginalDocument {
                window,
                document_generation,
            } => {
                #[cfg(target_os = "macos")]
                {
                    self.window_destroy_original_document_locked(
                        session,
                        active,
                        correlation,
                        window,
                        document_generation,
                        failed,
                    )
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = (window, document_generation);
                    call_error(active, correlation, "KELD-CORE-042", "KELD-CORE-042: original-document teardown requires its qualified native adapter.".to_owned(), failed)
                }
            }
            WindowRequest::Destroy { window } => {
                self.window_destroy_locked(session, active, correlation, window, failed)
            }
        }
    }
    fn window_destroy_locked(
        &self,
        session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        window: keld_ipc::window::WindowRef,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let caller = owner(active);
        #[cfg(target_os = "macos")]
        let mut availability = Ok(());
        let outcome = session.registry.destroy_with_retirement(
            caller,
            window,
            correlation,
            report_abandoned,
            |registry| {
                #[cfg(target_os = "macos")]
                {
                    availability =
                        self.window_registered_route_retirement_locked(registry, active, window);
                }
                #[cfg(not(target_os = "macos"))]
                let _ = registry;
            },
        );
        #[cfg(not(target_os = "macos"))]
        let availability = Ok(());
        let destruction = match outcome {
            Ok(CloseOutcome::Teardown(teardown)) => {
                #[cfg(target_os = "macos")]
                let scoped = {
                    let outcome = session.registry.cancel_original_document_destroy(window);
                    Self::window_original_destroy_resolution(active, &outcome, failed)
                };
                #[cfg(not(target_os = "macos"))]
                let scoped = Ok(());
                let interrupted = {
                    #[cfg(target_os = "macos")]
                    {
                        session.registry.take_interrupted_original_load(window)
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        None
                    }
                };
                let interruption = Self::window_original_interruption(active, interrupted, failed);
                let teardown = self.send_window_teardown(teardown, "window teardown");
                collapse_app_results([scoped, interruption, teardown])
            }
            Ok(CloseOutcome::AlreadyClosing) => Ok(()),
            Ok(_) => Err(app_detail(
                "window Destroy",
                "registry returned an invalid destruction effect",
            )),
            Err(error) => call_error(active, correlation, error.code(), error.to_string(), failed),
        };
        collapse_app_results([availability, destruction])
    }
    /// Caller holds transition/current/windows. The Registry invokes this only
    /// for its admitted logical closing cut, before clearing that exact Entry.
    #[cfg(target_os = "macos")]
    fn window_registered_route_retirement_locked(
        &self,
        registry: &WindowRegistry,
        active: &ActivePrimaryGeneration,
        window: keld_ipc::window::WindowRef,
    ) -> Result<(), HostAppError> {
        let caller = PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: std::sync::Arc::clone(&active.reader_stop),
        };
        registry.for_each_registered_renderer_route(owner(active), |route| {
            if route.window == window {
                self.compat_registered_route_retired_locked(&caller, route)?;
            }
            Ok(())
        })
    }
    fn window_close_reply_locked(
        &self,
        session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        request: &WindowRequest,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let &WindowRequest::CloseReply {
            window,
            close_seq,
            verdict,
        } = request
        else {
            return Err(app_detail(
                "window close dispatch",
                "shared decoder routed a non-close request to close admission",
            ));
        };
        let caller = owner(active);
        #[cfg(target_os = "macos")]
        let mut availability = Ok(());
        let outcome = session.registry.close_reply_with_retirement(
            caller,
            window,
            close_seq,
            verdict,
            |registry| {
                #[cfg(target_os = "macos")]
                {
                    availability =
                        self.window_registered_route_retirement_locked(registry, active, window);
                }
                #[cfg(not(target_os = "macos"))]
                let _ = registry;
            },
        );
        #[cfg(not(target_os = "macos"))]
        let availability = Ok(());
        #[cfg(target_os = "macos")]
        let scoped = if matches!(outcome, Ok(CloseOutcome::Teardown(_))) {
            let resolution = session.registry.cancel_original_document_destroy(window);
            Self::window_original_destroy_resolution(active, &resolution, failed)
        } else {
            Ok(())
        };
        #[cfg(not(target_os = "macos"))]
        let scoped = Ok(());
        let interrupted = if matches!(outcome, Ok(CloseOutcome::Teardown(_))) {
            {
                #[cfg(target_os = "macos")]
                {
                    session.registry.take_interrupted_original_load(window)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    None
                }
            }
        } else {
            None
        };
        let interruption = Self::window_original_interruption(active, interrupted, failed);
        let closing = self.window_close_result(
            active,
            correlation,
            outcome,
            &mut session.navigation_payload,
            failed,
        );
        collapse_app_results([availability, scoped, interruption, closing])
    }
    #[cfg(target_os = "macos")]
    fn window_destroy_original_document_locked(
        &self,
        session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        window: keld_ipc::window::WindowRef,
        document_generation: u32,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let caller = owner(active);
        let view = match session.registry.admitted_dialog_view(caller, window) {
            Ok(view) => view,
            Err(error) => {
                return call_error(active, correlation, error.code(), error.to_string(), failed);
            }
        };
        let expected = if document_generation == 0 {
            None
        } else {
            session
                .registry
                .renderer_route(caller, view, Some(document_generation))
                .ok()
                .map(|route| route.document)
        };
        let Some(expected) = expected else {
            return original_close_refusal(active, correlation, failed);
        };
        let admission =
            std::sync::Arc::new(keld_wv::wkwebview::OriginalDocumentDestroyAdmission::new());
        if let Err(error) = session.registry.reserve_original_document_destroy(
            caller,
            correlation,
            window,
            std::sync::Arc::clone(&admission),
        ) {
            return call_error(active, correlation, error.code(), error.to_string(), failed);
        }
        if self
            .window_commands
            .send(AppWindowCommand::DestroyOriginalDocument {
                view,
                expected,
                admission: std::sync::Arc::clone(&admission),
            })
            .is_err()
        {
            // A lost cancellation can mean native already accepted; the next
            // call reconciles that exact retained object's actual decision.
            let _ = admission.cancel_before_admission();
            let outcome = session
                .registry
                .original_document_destroy_decision(view, &admission);
            return Self::window_original_destroy_resolution(active, &outcome, failed);
        }
        // A queued conditional request changes no phase/document/Ready state.
        // Native owns admission; actual Released remains the sole success input.
        Ok(())
    }
    #[cfg(target_os = "macos")]
    fn window_original_destroy_resolution(
        active: &mut ActivePrimaryGeneration,
        outcome: &super::window_registry::OriginalDocumentDestroyResolution,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        if let Some(close) = outcome.abandoned {
            report_abandoned(close);
        }
        let refused = if let Some(call) = outcome.refused.filter(|call| call.owner == owner(active))
        {
            original_close_refusal(active, call.correlation, failed)
        } else {
            Ok(())
        };
        let interrupted = Self::window_original_interruption(active, outcome.interrupted, failed);
        collapse_app_results([refused, interrupted])
    }
    #[cfg(target_os = "macos")]
    fn window_original_destroy_native_locked(
        session: &mut WindowSession,
        current: &mut Option<ActivePrimaryGeneration>,
        view: keld_wv::WebviewId,
        admission: std::sync::Arc<keld_wv::wkwebview::OriginalDocumentDestroyAdmission>,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let outcome = session
            .registry
            .original_document_destroy_decision(view, &admission);
        if let Some(close) = outcome.abandoned {
            report_abandoned(close);
        }
        let refused = if let Some(call) = outcome.refused
            && session.subscribed == Some(call.owner)
            && let Some(active) = active_owner(current, call.owner)
        {
            original_close_refusal(active, call.correlation, failed)
        } else {
            Ok(())
        };
        let interrupted = if let Some(call) = outcome.interrupted
            && session.subscribed == Some(call.owner)
            && let Some(active) = active_owner(current, call.owner)
        {
            Self::window_original_interruption(active, Some(call), failed)
        } else {
            Ok(())
        };
        let result = collapse_app_results([refused, interrupted]);
        // The native receipt transfers this exact ticket. Keep it through both
        // reply paths, then release at the existing end-of-event ownership cut.
        drop(admission);
        result
    }
    /// Caller holds transition/current. This common cut precedes current.take,
    /// reader-stop and any successor publication; it touches no native handle.
    #[cfg(target_os = "macos")]
    pub(super) fn cancel_window_native_admissions(
        &self,
        owner: WindowOwner,
    ) -> Result<(), HostAppError> {
        let Some(windows) = &self.windows else {
            return Ok(());
        };
        let (mut session, locked) = match windows.lock() {
            Ok(session) => (session, Ok(())),
            Err(poisoned) => (
                poisoned.into_inner(),
                Err(app_detail(
                    "window native admission retirement",
                    "registry lock poisoned",
                )),
            ),
        };
        // Poison permits only this existing revocation: cancel Pending or
        // reconcile native Accepted into nonadoptable Closing before handover.
        // Keep poison latched and return its error; ordinary admission/rebind
        // must continue refusing this Registry, never adopt from this guard.
        session
            .registry
            .cancel_native_admissions(owner, report_abandoned);
        locked
    }
    fn window_original_interruption(
        active: &mut ActivePrimaryGeneration,
        pending: Option<super::window_registry::PendingWindowCall>,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        if let Some(call) = pending.filter(|call| call.owner == owner(active)) {
            call_error(active, call.correlation, "KELD-CORE-053", "KELD-CORE-053: the window closed before its admitted original load finished; use a current live window for a new explicit load.".to_owned(), failed)?;
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    fn window_original_load_locked(
        _session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        _window: keld_ipc::window::WindowRef,
        _uri: &str,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        call_error(
            active,
            correlation,
            "KELD-CORE-042",
            "KELD-CORE-042: original editor loading requires its qualified macOS native adapter."
                .to_owned(),
            failed,
        )
    }
    #[cfg(target_os = "macos")]
    fn window_original_load_locked(
        &self,
        session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        window: keld_ipc::window::WindowRef,
        uri: &str,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        // Validate the real pair/owner before source information or native work.
        if let Err(error) = session.registry.admitted_dialog_view(owner(active), window) {
            return call_error(active, correlation, error.code(), error.to_string(), failed);
        }
        let selection = match session
            .registry
            .original_window_source(owner(active), window)
        {
            Ok(selection) => selection,
            Err(error) => {
                return call_error(active, correlation, error.code(), error.to_string(), failed);
            }
        };
        let query = match selection.qualify_uri(uri) {
            Ok(query) => query,
            Err(error) => {
                return call_error(
                    active,
                    correlation,
                    "KELD-CORE-051",
                    format!(
                        "KELD-CORE-051: {error}; prepare the verified original source and send its exact selected entry URI"
                    ),
                    failed,
                );
            }
        };
        let (operation, view) =
            match session
                .registry
                .admit_original_load(owner(active), correlation, window)
            {
                Ok(OriginalLoadAdmission::Dispatch {
                    operation, view, ..
                }) => (operation, view),
                Ok(OriginalLoadAdmission::Busy) => {
                    let error = WindowError::OriginalLoadBusy;
                    return call_error(
                        active,
                        correlation,
                        error.code(),
                        error.to_string(),
                        failed,
                    );
                }
                Err(error) => {
                    return call_error(
                        active,
                        correlation,
                        error.code(),
                        error.to_string(),
                        failed,
                    );
                }
            };
        let Some(operation_id) = std::num::NonZeroU64::new(operation) else {
            session.registry.abandon_original_dispatch(operation);
            return Err(app_detail(
                "original load correlation",
                "checked operation was zero",
            ));
        };
        let context = keld_wv::NativeNavigationContext::new(operation_id, view);
        if self
            .window_commands
            .send(AppWindowCommand::LoadAdmittedOriginal { context, query })
            .is_err()
        {
            session.registry.abandon_original_dispatch(operation);
            return call_error(active, correlation, "KELD-CORE-051", "KELD-CORE-051: native UI admission is unavailable; restart the Host before another original load.".to_owned(), failed);
        }
        // A queued command is no native milestone. Actual Finished owns Loaded.
        Ok(())
    }
    #[cfg(target_os = "macos")]
    fn window_original_native_locked(
        &self,
        session: &mut WindowSession,
        current: &mut Option<ActivePrimaryGeneration>,
        context: keld_wv::NativeNavigationContext,
        fact: keld_wv::MacOriginalNavigationFact,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        use keld_wv::MacOriginalNavigationFact;
        let subscribed = session.subscribed;
        let failure = match fact {
            MacOriginalNavigationFact::Started { attempt } => {
                session.registry.original_load_started(context, attempt);
                return Ok(());
            }
            MacOriginalNavigationFact::Committed {
                stamp,
                native_url,
                document,
            } => {
                let Some(super::window_registry::CommittedOriginalLoad {
                    owner,
                    window,
                    document_generation,
                    native_url,
                }) = session
                    .registry
                    .original_load_committed(context, stamp, document, native_url)
                else {
                    return Ok(());
                };
                if subscribed == Some(owner)
                    && let Some(active) = active_owner(current, owner)
                {
                    // The same Entry owns this credited native observation.
                    // Borrow through the writer without detaching its bytes.
                    navigation_committed_event(
                        active,
                        window,
                        document_generation,
                        native_url.as_str(),
                        &mut session.navigation_payload,
                        failed,
                    )?;
                    let caller = PrimaryCallerContext {
                        identity: active.identity,
                        attempt: active.attempt,
                        reader_stop: std::sync::Arc::clone(&active.reader_stop),
                    };
                    session
                        .registry
                        .for_each_registered_renderer_route(owner, |route| {
                            if route.window == window {
                                self.compat_registered_route_ready_locked(&caller, active, route)?;
                            }
                            Ok(())
                        })?;
                }
                return Ok(());
            }
            MacOriginalNavigationFact::Finished { stamp } => {
                return original_native_finished(session, current, context, stamp, failed);
            }
            MacOriginalNavigationFact::Refused { error } => {
                let detail = original_refusal_detail(error);
                (
                    session.registry.original_load_failed(context, None),
                    "KELD-CORE-051",
                    detail,
                )
            }
            MacOriginalNavigationFact::Failed { attempt, revoked } => {
                if let Some(previous) = revoked {
                    session
                        .registry
                        .renderer_document_revoked(context.view(), previous);
                }
                (
                    session
                        .registry
                        .original_load_failed(context, Some(attempt)),
                    "KELD-CORE-052",
                    "the actual admitted native navigation failed",
                )
            }
            MacOriginalNavigationFact::Revoked { previous } => {
                let Some(previous) = previous else {
                    return Ok(());
                };
                session
                    .registry
                    .renderer_document_revoked(context.view(), previous);
                (
                    session
                        .registry
                        .original_load_failed(context, Some(previous.attempt())),
                    "KELD-CORE-052",
                    "the admitted native document was revoked",
                )
            }
        };
        original_native_failure(current, subscribed, failure, failed)
    }
    #[cfg(not(target_os = "macos"))]
    fn window_create_with_original_locked(
        _session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        _request: WindowRequest,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        call_error(
            active,
            correlation,
            "KELD-CORE-042",
            "KELD-CORE-042: original preload creation requires its qualified macOS adapter."
                .to_owned(),
            failed,
        )
    }
    #[cfg(target_os = "macos")]
    fn window_create_with_original_locked(
        &self,
        session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        request: WindowRequest,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let WindowRequest::CreateWithPreload {
            width,
            height,
            x,
            y,
            preload_absolute,
            additional_arguments,
        } = request
        else {
            return Err(app_detail(
                "original Create dispatch",
                "shared decoder routed a non-preload request to preload admission",
            ));
        };
        let Some(selection) = session
            .original_selection
            .as_ref()
            .map(std::sync::Arc::clone)
        else {
            return call_error(active, correlation, "KELD-CORE-051", "KELD-CORE-051: verified source and original preload selection are unavailable; prepare the graph and resolved window policy before CreateWithPreload.".to_owned(), failed);
        };
        let preload_image = match selection.prepare_preload(&preload_absolute, additional_arguments)
        {
            Ok(image) => image,
            Err(error) => {
                return call_error(
                    active,
                    correlation,
                    "KELD-CORE-051",
                    format!(
                        "KELD-CORE-051: {error}; use the exact original preload and bounded captured arguments"
                    ),
                    failed,
                );
            }
        };
        let outcome = session.registry.create_with_original_preload(
            super::window_registry::PendingWindowCall {
                owner: owner(active),
                correlation,
            },
            (width, height),
            (x, y),
            std::sync::Arc::clone(&selection),
            std::sync::Arc::clone(&preload_image),
        );
        let resources = WindowBuildResources {
            preload_image: Some(preload_image),
            original_source: Some(std::sync::Arc::clone(selection.source())),
        };
        self.window_create_result(active, correlation, outcome, resources, failed)
    }
    fn window_create_result(
        &self,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        outcome: Result<CreateOutcome, WindowError>,
        #[cfg(target_os = "macos")] resources: WindowBuildResources,
        #[cfg(not(target_os = "macos"))] _resources: WindowBuildResources,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match outcome {
            Ok(CreateOutcome::Build {
                build,
                width,
                height,
                x,
                y,
                ..
            }) => self
                .window_commands
                .send(AppWindowCommand::CreateWindow {
                    build,
                    width,
                    height,
                    x,
                    y,
                    #[cfg(target_os = "macos")]
                    preload_image: resources.preload_image,
                    #[cfg(target_os = "macos")]
                    original_source: resources.original_source,
                })
                .map_err(|_| app_detail("window native build", "UI command owner is unavailable")),
            Ok(CreateOutcome::Adopted(window)) => window_reply(
                active,
                correlation,
                WindowResponse::Adopted { window },
                failed,
            ),
            Err(error) => call_error(active, correlation, error.code(), error.to_string(), failed),
        }
    }
    fn window_setter_result(
        &self,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        outcome: Result<NativeSetter, WindowError>,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match outcome {
            Ok(setter) => self.send_window_setter(active, correlation, setter, failed),
            Err(error) => call_error(active, correlation, error.code(), error.to_string(), failed),
        }
    }
    fn window_close_result(
        &self,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        outcome: Result<CloseOutcome, WindowError>,
        scratch: &mut [u8],
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match outcome {
            Ok(close) => self.window_close_call_locked(active, correlation, close, scratch, failed),
            Err(error) => call_error(active, correlation, error.code(), error.to_string(), failed),
        }
    }
    fn send_window_teardown(
        &self,
        teardown: super::window_registry::Teardown,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        self.window_commands
            .send(AppWindowCommand::DestroyWindow {
                view: teardown.view,
            })
            .map_err(|_| app_detail(phase, "UI command owner is unavailable"))
    }
    fn window_close_call_locked(
        &self,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        outcome: CloseOutcome,
        scratch: &mut [u8],
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let response = match outcome {
            CloseOutcome::Requested {
                close_seq,
                fact: Some(fact),
            } => {
                event(active, &fact.event, scratch, failed)?;
                WindowResponse::CloseRequested { close_seq }
            }
            CloseOutcome::Merged { close_seq } => WindowResponse::Merged { close_seq },
            CloseOutcome::AlreadyClosing => WindowResponse::AlreadyClosing,
            CloseOutcome::Vetoed => WindowResponse::ReplyApplied,
            CloseOutcome::Stale => {
                eprintln!(
                    "KELD-CORE-038: stale close reply ignored; answer the current close_seq once"
                );
                WindowResponse::ReplyStale
            }
            CloseOutcome::Teardown(teardown) => {
                // T6 acknowledges the verdict before issuing physical teardown.
                // Closing already committed: reply loss must not strand its view.
                let acknowledgement =
                    window_reply(active, correlation, WindowResponse::ReplyApplied, failed);
                let release = self.send_window_teardown(teardown, "window teardown");
                return collapse_app_results([acknowledgement, release]);
            }
            CloseOutcome::Requested { fact: None, .. }
            | CloseOutcome::Unmapped
            | CloseOutcome::Exhausted => {
                return Err(app_detail(
                    "window close reply",
                    "admitted call returned a native-only close outcome",
                ));
            }
        };
        window_reply(active, correlation, response, failed)
    }
    fn send_window_setter(
        &self,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        setter: NativeSetter,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let command = match setter {
            NativeSetter::Maximized { view, maximized } => {
                AppWindowCommand::SetMaximized { view, maximized }
            }
            NativeSetter::FullScreen { view, full_screen } => {
                AppWindowCommand::SetFullScreen { view, full_screen }
            }
        };
        self.window_commands
            .send(command)
            .map_err(|_| app_detail("window setter", "UI command owner is unavailable"))?;
        window_reply(active, correlation, WindowResponse::Accepted, failed)
    }
    fn window_build_outcome_locked(
        &self,
        current: &mut Option<ActivePrimaryGeneration>,
        subscribed: Option<WindowOwner>,
        outcome: BuildOutcome,
        scratch: &mut [u8],
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match outcome {
            BuildOutcome::Created { fact, reply } => {
                owned_event(current, subscribed, &fact, scratch, failed)?;
                owned_reply(current, subscribed, reply, failed)
            }
            BuildOutcome::Cleanup(teardown) => {
                self.send_window_teardown(teardown, "window late-build cleanup")
            }
            BuildOutcome::Ignored => {
                eprintln!(
                    "KELD-CORE-041: ignored unexpected native build result; inspect the UI build correlation"
                );
                Ok(())
            }
        }
    }
    fn window_native_close_locked(
        current: &mut Option<ActivePrimaryGeneration>,
        subscribed: Option<WindowOwner>,
        outcome: CloseOutcome,
        scratch: &mut [u8],
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match outcome {
            CloseOutcome::Requested {
                fact: Some(fact), ..
            } => owned_event(current, subscribed, &fact, scratch, failed),
            CloseOutcome::Unmapped => {
                eprintln!(
                    "KELD-CORE-038: native close references an unmapped view; inspect UI event ordering"
                );
                Ok(())
            }
            CloseOutcome::Exhausted => {
                eprintln!(
                    "KELD-CORE-039: native close_seq exhausted; destroy the window or end the session"
                );
                Ok(())
            }
            CloseOutcome::AlreadyClosing
            | CloseOutcome::Merged { .. }
            | CloseOutcome::Requested { fact: None, .. } => Ok(()),
            CloseOutcome::Teardown(_) | CloseOutcome::Vetoed | CloseOutcome::Stale => {
                Err(app_detail(
                    "window native close",
                    "registry returned a call-only close outcome",
                ))
            }
        }
    }
    // The caller retains transition/current/windows and the same subscribed
    // snapshot. Only scalar physical build/release completion facts enter here.
    fn window_completion_locked(
        &self,
        session: &mut WindowSession,
        current: &mut Option<ActivePrimaryGeneration>,
        subscribed: Option<WindowOwner>,
        event: &AppWindowEvent,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match *event {
            AppWindowEvent::WindowBuilt {
                build,
                view,
                snapshot,
            } => {
                let outcome = session
                    .registry
                    .native_built(build, view, native_state(snapshot));
                self.window_build_outcome_locked(
                    current,
                    subscribed,
                    outcome,
                    &mut session.navigation_payload,
                    failed,
                )
            }
            AppWindowEvent::WindowBuildFailed { build, stage } => {
                let outcome = session.registry.native_failed(build);
                window_build_failed_locked(current, subscribed, outcome, stage, failed)
            }
            AppWindowEvent::WindowReleased { view } => {
                let outcome = session.registry.native_released(view);
                window_released_locked(
                    current,
                    subscribed,
                    outcome,
                    &mut session.navigation_payload,
                    failed,
                )
            }
            _ => Err(app_detail(
                "window native completion",
                "shared dispatcher routed a non-completion native fact",
            )),
        }
    }

    // Caller owns transition -> current -> windows. This dispatch performs the
    // registry transition and its ordered effects without changing lock custody.
    fn window_native_event_locked(
        &self,
        session: &mut WindowSession,
        current: &mut Option<ActivePrimaryGeneration>,
        native_event: AppWindowEvent,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let subscribed = session.subscribed;
        let fact = match native_event {
            #[cfg(target_os = "macos")]
            AppWindowEvent::DesktopObserved { facts } => {
                return desktop_observed_locked(session, current, facts, failed);
            }
            #[cfg(target_os = "macos")]
            AppWindowEvent::OriginalDocumentDestroyDecision { view, admission } => {
                return Self::window_original_destroy_native_locked(
                    session, current, view, admission, failed,
                );
            }
            #[cfg(target_os = "macos")]
            AppWindowEvent::MacOriginalNavigation { context, fact } => {
                return self.window_original_native_locked(session, current, context, fact, failed);
            }
            event @ (AppWindowEvent::WindowBuilt { .. }
            | AppWindowEvent::WindowBuildFailed { .. }
            | AppWindowEvent::WindowReleased { .. }) => {
                return self.window_completion_locked(session, current, subscribed, &event, failed);
            }
            AppWindowEvent::WindowCloseRequested { view } => {
                return Self::window_native_close_locked(
                    current,
                    subscribed,
                    session.registry.native_close(view),
                    &mut session.navigation_payload,
                    failed,
                );
            }
            AppWindowEvent::WindowBounds {
                view,
                x,
                y,
                width,
                height,
            } => session.registry.native_bounds(
                view,
                Rect {
                    x,
                    y,
                    width,
                    height,
                },
            ),
            AppWindowEvent::WindowFocus { view, focused } => {
                session.registry.native_focus(view, focused)
            }
            AppWindowEvent::WindowMaximized { view, maximized } => {
                session.registry.native_maximized(view, maximized)
            }
            AppWindowEvent::WindowFullScreen { view, full_screen } => {
                session.registry.native_full_screen(view, full_screen)
            }
            AppWindowEvent::NavigationReady | AppWindowEvent::LastWindowClosed => {
                return Err(app_detail(
                    "window native event",
                    "legacy lifecycle fact entered managed registry dispatch",
                ));
            }
            #[cfg(any(target_os = "macos", windows))]
            AppWindowEvent::HostInitialized { .. } => {
                return Err(app_detail(
                    "window native event",
                    "the first host Init milestone belongs only to the lifecycle coordinator",
                ));
            }
            #[cfg(target_os = "macos")]
            AppWindowEvent::MacDialogFinished { .. } => {
                return Err(app_detail(
                    "window native event",
                    "dialog completion must be dispatched by the event coordinator",
                ));
            }
        };
        match fact {
            FactOutcome::Event(fact) => owned_event(
                current,
                subscribed,
                &fact,
                &mut session.navigation_payload,
                failed,
            ),
            FactOutcome::Unmapped => {
                eprintln!(
                    "KELD-CORE-038: native state references an unmapped view; inspect UI event ordering"
                );
                Ok(())
            }
            FactOutcome::Coalesced | FactOutcome::Suppressed | FactOutcome::Dropped => Ok(()),
        }
    }
    pub(super) fn window_native_event(
        &self,
        native_event: AppWindowEvent,
    ) -> Result<(), HostAppError> {
        let mut failed = None;
        let result = {
            let _transition = self.shutdown.transition_guard();
            let mut current = self
                .current
                .lock()
                .map_err(|_| app_detail("window native event", "generation lock poisoned"))?;
            let result = (|| {
                let windows = self.windows.as_ref().ok_or_else(|| {
                    app_detail(
                        "window native event",
                        "managed native fact arrived without a window registry",
                    )
                })?;
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("window native event", "registry lock poisoned"))?;
                self.window_native_event_locked(
                    &mut session,
                    &mut current,
                    native_event,
                    &mut failed,
                )
            })();
            // The closure releases windows before retirement can re-enter it.
            self.finish_window_write(&mut current, failed, result)
        };
        // Recovery belongs to the existing Runtime owner, after releasing every lock.
        if let Some(attempt) = failed {
            let owner_result = self.link_failed(attempt);
            if owner_result.is_ok() {
                return Ok(());
            }
            collapse_app_results([result, owner_result])
        } else {
            result
        }
    }
    #[cfg(target_os = "macos")]
    pub(super) fn cancel_current_window_native_admissions_guarded(
        &self,
    ) -> Result<(), HostAppError> {
        let current = self.current.lock().map_err(|_| {
            app_detail(
                "window native admission retirement",
                "generation lock poisoned",
            )
        })?;
        current.as_ref().map_or(Ok(()), |active| {
            self.cancel_window_native_admissions(owner(active))
        })
    }
    /// The existing WV main-owner service invokes this before terminal physical
    /// release. Some is facade-only (GH-446's no-legacy-anchor factory); the
    /// permanent None renderer path does not reinterpret an anchor close.
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn window_main_owner_stopping(&self, stopping: bool) -> Result<(), HostAppError> {
        if !stopping || self.windows.is_none() {
            return Ok(());
        }
        let _transition = self.shutdown.transition_guard();
        // Retire the actual authenticated current caller before stop_reader
        // removes its authority to publish terminal renderer dispositions.
        #[cfg(target_os = "macos")]
        let retirement = self
            .current
            .lock()
            .map_err(|_| app_detail("window owner stopping", "generation lock poisoned"))
            .and_then(|mut current| {
                current.as_mut().map_or(Ok(()), |active| {
                    let admission = self.cancel_window_native_admissions(owner(active));
                    let compat = self.retire_compat_locked(active);
                    collapse_app_results([admission, compat])
                })
            });
        #[cfg(not(target_os = "macos"))]
        let retirement = Ok(());
        // Ending and physical cleanup are still attempted if retirement failed.
        self.shutdown.stop_reader();
        self.guardian.deny_recovery();
        let ending = self.end_window_session_guarded();
        collapse_app_results([retirement, ending])
    }
    /// Caller holds the shutdown transition; existing command queue owns effects.
    pub(super) fn end_window_session_guarded(&self) -> Result<(), HostAppError> {
        let _current = self
            .current
            .lock()
            .map_err(|_| app_detail("window session end", "generation lock poisoned"))?;
        let Some(windows) = &self.windows else {
            return Ok(());
        };
        let mut session = windows
            .lock()
            .map_err(|_| app_detail("window session end", "registry lock poisoned"))?;
        #[cfg(target_os = "macos")]
        {
            // Host termination discards an outstanding pre-Init Subscribe
            // without inventing ERR or Subscribed on a retired writer.
            session.pending_desktop = None;
        }
        let mut result = Ok(());
        session.registry.session_end(|effect| match effect {
            SessionEffect::Teardown(teardown) => {
                let next = self.send_window_teardown(teardown, "window session teardown");
                result = collapse_app_results([std::mem::replace(&mut result, Ok(())), next]);
            }
            SessionEffect::CreateInterrupted(call) => {
                // Already admitted. There is no no-effect024 reply; closing its
                // actual role link gives022 with unknown effect and no replay.
                eprintln!(
                    "admitted window Create {} interrupted by session end; outcome unknown",
                    call.correlation.0
                );
            }
            #[cfg(any(target_os = "macos", test))]
            SessionEffect::OriginalLoadInterrupted(call) => {
                eprintln!("admitted original load {} interrupted by session end; native custody drains under its existing owner", call.correlation.0);
            }
            SessionEffect::CloseAbandoned(close) => report_abandoned(close),
        });
        result
    }
    /// Only after the actual UI event source has drained, never on a timer.
    pub(super) fn finish_window_facts(&self) -> Result<(), HostAppError> {
        if let Some(windows) = &self.windows {
            let dropped = windows
                .lock()
                .map_err(|_| app_detail("window fact drain", "registry lock poisoned"))?
                .registry
                .take_destroyed_fact_drops();
            if dropped != 0 {
                eprintln!("window session dropped {dropped} retired native facts");
            }
        }
        Ok(())
    }
}

#[cfg(all(test, not(keld_core_native_renderer_main)))]
mod tests;
