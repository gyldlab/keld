//! Private window service composed by the existing primary app-link owner.
//!
//! Lock order is shutdown transition -> current generation/writer -> windows.
//! In particular the windows guard is dropped before failed-write retirement,
//! because retirement re-enters it to fence the old owner. No native handle,
//! frame parser, output queue, principal or subscription policy is minted here.

mod native_outcome;
#[cfg(windows)]
mod native_scope;
#[cfg(windows)]
mod publication;
mod request;
#[cfg(windows)]
pub(in crate::app_session) use publication::{
    NativePublicationDisposition, PublicationError, PublicationResult,
};
#[cfg(windows)]
mod publication_turn;
#[cfg(windows)]
pub(in crate::app_session) use publication_turn::WindowPublicationTurn;

#[cfg(any(not(windows), test))]
use std::io::Write as _;

#[cfg(any(not(windows), test))]
use super::{encode, write_frame};
use keld_ipc::SubscribeRefusal;
use keld_ipc::channel_table::{Authority, ReceiveClass};
#[cfg(target_os = "macos")]
use keld_ipc::window::{DesktopFact, DesktopFacts};
use keld_ipc::window::{Rect, WindowEvent, WindowRequest, WindowResponse, WindowState};
#[cfg(any(not(windows), test))]
use keld_ipc::{ChannelId, window::DESKTOP_EVENT_STORAGE_BYTES};
use keld_wv::{BuildStage, WindowSnapshot};

#[cfg(any(target_os = "macos", windows))]
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
    collapse_app_results, decode, with_primary_caller_locked,
};

// Current maximum payloads: WindowEvent Created = 50 bytes, WindowResponse
// Created/Adopted = 16, LifecycleResponse SubscribeRefused = 2. The literal
// codec vectors and maximum Created socket test qualify the current schema.
const WINDOW_PAYLOAD_STORAGE_BYTES: usize = 64;
/// GH481's event-specific body cap; the existing control frame limit is unchanged.
#[cfg(any(target_os = "macos", windows))]
const MAX_DESKTOP_EVENT_BYTES: usize = 32 * 1024;

/// Core alone increments native observation epochs. Epoch zero has no authority.
#[cfg(any(target_os = "macos", windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DesktopEpoch(pub(super) u64);
#[cfg(any(target_os = "macos", windows))]
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
#[cfg(any(target_os = "macos", windows))]
pub(super) fn admit_desktop_payload_bytes(bytes: usize) -> Result<(), HostAppError> {
    if bytes > MAX_DESKTOP_EVENT_BYTES {
        return Err(app_detail(
            "desktop observation",
            "desktop event exceeds 32 KiB",
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
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
    #[cfg(any(target_os = "macos", windows))]
    preload_image: Option<std::sync::Arc<keld_wv::preload_image::AppPreloadImage>>,
    #[cfg(any(target_os = "macos", windows))]
    original_source: Option<std::sync::Arc<keld_wv::resource_source::AdmittedWebAppSource>>,
    #[cfg(windows)]
    original_role: Option<keld_wv::renderer_bridge::RendererRoleWitness>,
}

#[cfg(any(target_os = "macos", windows))]
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
#[cfg_attr(
    windows,
    expect(
        clippy::struct_excessive_bools,
        reason = "Native support, desktop selection, sample observation, and initial publication closure are independent facts in this existing retained owner."
    )
)]
pub(super) struct WindowSession {
    registry: WindowRegistry,
    native_supported: bool,
    #[cfg(any(target_os = "macos", windows))]
    original_selection: Option<std::sync::Arc<super::original_webapp::OriginalLoadSelection>>,
    // The one subscription owner is this live link, not the window instances.
    subscribed: Option<WindowOwner>,
    #[cfg(windows)]
    publication: publication_turn::WindowPublicationState,
    #[cfg(any(target_os = "macos", windows))]
    desktop_required: bool,
    #[cfg(any(target_os = "macos", windows))]
    desktop_seen: bool,
    #[cfg(any(target_os = "macos", windows))]
    desktop_epoch: DesktopEpoch,
    #[cfg(target_os = "macos")]
    desktop_latest: Option<DesktopFacts>,
    #[cfg(windows)]
    desktop_latest: Option<std::sync::Arc<WindowEvent>>,
    #[cfg(windows)]
    initial_desktop_closed: bool,
    #[cfg(target_os = "macos")]
    pending_desktop: Option<PendingDesktopSubscribe>,
    #[cfg(windows)]
    pending_desktop: Option<publication::desktop::PendingDesktopSubscribe>,
    // One cold allocation, reused under the existing windows lock. The IPC
    // owner supplies the exact cap; no per-event allocation or extra queue.
    #[cfg(any(not(windows), test))]
    navigation_payload: Box<[u8]>,
}
impl WindowSession {
    pub(super) fn new(native_supported: bool) -> Self {
        Self {
            registry: WindowRegistry::new(),
            native_supported,
            #[cfg(any(target_os = "macos", windows))]
            original_selection: None,
            subscribed: None,
            #[cfg(windows)]
            publication: publication_turn::WindowPublicationState::default(),
            #[cfg(any(target_os = "macos", windows))]
            desktop_required: false,
            #[cfg(any(target_os = "macos", windows))]
            desktop_seen: false,
            #[cfg(any(target_os = "macos", windows))]
            desktop_epoch: DesktopEpoch(0),
            #[cfg(any(target_os = "macos", windows))]
            desktop_latest: None,
            #[cfg(windows)]
            initial_desktop_closed: false,
            #[cfg(any(target_os = "macos", windows))]
            pending_desktop: None,
            #[cfg(any(not(windows), test))]
            navigation_payload: vec![0_u8; keld_ipc::window::NAVIGATION_EVENT_STORAGE_BYTES]
                .into_boxed_slice(),
        }
    }
    /// Actual selected native facade boot enables S1; old component fixtures
    /// retain the baseline only when they explicitly use `new`.
    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn with_desktop(native_supported: bool) -> Self {
        let mut session = Self::new(native_supported);
        session.desktop_required = true;
        session
    }
    /// Cold boot supplies only the existing verified graph/source/policy result.
    #[cfg(any(target_os = "macos", windows))]
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
    #[cfg(any(target_os = "macos", windows))]
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
    #[cfg(any(target_os = "macos", windows))]
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
    #[cfg(any(target_os = "macos", all(windows, test)))]
    #[cfg_attr(
        all(windows, test),
        expect(
            dead_code,
            reason = "Keep the existing macOS/legacy route adapter type-checked in Windows tests; live Windows publication uses its captured-caller path."
        )
    )]
    pub(super) fn admit_renderer_route(
        &mut self,
        owner: WindowOwner,
        view: keld_wv::WebviewId,
        document: keld_wv::renderer_contract::CheckedNativeDocumentLease,
    ) -> Result<super::window_registry::RendererRoute, WindowError> {
        self.admitted_renderer_window(owner, view)?;
        self.registry.admit_renderer_route(owner, view, document)
    }
    #[cfg(any(target_os = "macos", all(windows, test)))]
    #[cfg_attr(
        all(windows, test),
        expect(
            dead_code,
            reason = "Keep the existing macOS/legacy route adapter type-checked in Windows tests; live Windows publication uses its captured-caller path."
        )
    )]
    pub(super) fn renderer_route(
        &self,
        owner: WindowOwner,
        view: keld_wv::WebviewId,
        navigation: Option<u32>,
    ) -> Result<super::window_registry::RendererRoute, WindowError> {
        self.admitted_renderer_window(owner, view)?;
        self.registry.renderer_route(owner, view, navigation)
    }
    #[cfg(windows)]
    pub(super) fn retains_retiring_renderer_route(
        &self,
        owner: WindowOwner,
        allocation: &std::sync::Arc<std::sync::atomic::AtomicBool>,
        window: keld_ipc::window::WindowRef,
        document: &keld_wv::renderer_contract::CheckedNativeDocumentLease,
    ) -> bool {
        self.registry
            .retains_retiring_renderer_route(owner, allocation, window, document)
    }
    #[cfg(any(target_os = "macos", windows))]
    #[cfg(windows)]
    pub(super) fn renderer_route_for_caller(
        &self,
        caller: &PrimaryCallerContext,
        view: keld_wv::WebviewId,
        navigation: Option<u32>,
    ) -> Result<super::window_registry::RendererRoute, WindowError> {
        self.admitted_renderer_window(WindowOwner::new(caller.identity, caller.attempt), view)?;
        self.registry
            .renderer_route_for_caller(caller, view, navigation)
    }
    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn note_renderer_registered(
        &mut self,
        view: keld_wv::WebviewId,
        document: keld_wv::renderer_contract::CheckedNativeDocumentLease,
    ) -> Result<(), WindowError> {
        self.registry.note_renderer_registered(view, document)
    }
    #[cfg(any(target_os = "macos", all(windows, test)))]
    #[cfg_attr(
        all(windows, test),
        expect(
            dead_code,
            reason = "Keep the existing macOS/legacy route adapter type-checked in Windows tests; live Windows publication uses its captured-caller path."
        )
    )]
    pub(super) fn for_each_registered_renderer_route(
        &self,
        owner: WindowOwner,
        callback: impl FnMut(super::window_registry::RendererRoute) -> Result<(), HostAppError>,
    ) -> Result<(), HostAppError> {
        self.registry
            .for_each_registered_renderer_route(owner, callback)
    }
    #[cfg(windows)]
    pub(in crate::app_session) fn observed_registered_renderer_route(
        &self,
        caller: &PrimaryCallerContext,
        view: keld_wv::WebviewId,
        document: &keld_wv::renderer_contract::CheckedNativeDocumentLease,
    ) -> Option<super::window_registry::RendererRoute> {
        self.admitted_renderer_window(WindowOwner::new(caller.identity, caller.attempt), view)
            .ok()?;
        self.registry
            .observed_registered_renderer_route(caller, view, document)
    }
    #[cfg(windows)]
    pub(in crate::app_session) fn next_retired_renderer_route_after(
        &self,
        caller: &PrimaryCallerContext,
        after: u32,
    ) -> Option<super::window_registry::RendererRoute> {
        self.registry
            .next_retired_renderer_route_after(caller, after)
    }
    /// One creation-ordered step under the existing Registry guard. This
    /// observation grants no native liveness or caller permission by itself.
    #[cfg(windows)]
    pub(super) fn next_registered_renderer_route_after(
        &self,
        owner: WindowOwner,
        after: u32,
    ) -> Option<super::window_registry::RendererRoute> {
        self.registry
            .next_registered_renderer_route_after(owner, after)
    }
    /// The existing current/shutdown fence authenticates this fresh Subscribe.
    /// Native Registered and window enumeration never call this role transition.
    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn rebind_recovered_renderer_documents(&mut self, owner: WindowOwner) -> bool {
        self.native_supported
            && self.subscribed == Some(owner)
            && self.registry.rebind_recovered_renderer_documents(owner)
    }
    #[cfg(any(target_os = "macos", windows))]
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
#[cfg(any(not(windows), test))]
fn active_owner(
    current: &mut Option<ActivePrimaryGeneration>,
    expected: WindowOwner,
) -> Option<&mut ActivePrimaryGeneration> {
    current
        .as_mut()
        .filter(|active| owner(active) == expected && !active.reader_stop.load(Ordering::Acquire))
}
#[cfg(any(not(windows), test))]
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
#[cfg(any(not(windows), test))]
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
#[cfg(any(not(windows), test))]
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
#[cfg(any(not(windows), test))]
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
#[cfg(any(not(windows), test))]
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
#[cfg(any(target_os = "macos", windows))]
#[cfg(any(not(windows), test))]
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
#[cfg(any(target_os = "macos", windows))]
#[cfg(any(not(windows), test))]
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

#[cfg(any(target_os = "macos", windows))]
#[cfg(any(not(windows), test))]
fn original_close_refusal(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let error = WindowError::OriginalDocumentCloseRefused;
    call_error(active, correlation, error.code(), error.to_string(), failed)
}
#[cfg(any(not(windows), test))]
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
#[cfg(any(not(windows), test))]
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
#[cfg(any(not(windows), test))]
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
#[cfg(any(not(windows), test))]
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

#[cfg(any(not(windows), test))]
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
#[cfg(any(not(windows), test))]
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
    #[cfg(not(windows))]
    pub(super) fn subscribe_window(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
    ) -> Result<(), HostAppError> {
        self.subscribe_window_legacy_policy(caller, correlation, channel)
    }
    #[cfg(not(windows))]
    pub(super) fn dispatch_window(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        self.dispatch_window_legacy_policy(caller, correlation, payload)
    }
    #[cfg(not(windows))]
    pub(super) fn window_native_event(&self, event: AppWindowEvent) -> Result<(), HostAppError> {
        self.window_native_event_legacy_policy(event)
    }
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
            #[cfg(windows)]
            if session.desktop_required {
                // The retained epoch belongs to the Host, but eligibility belongs
                // to the actual current reader. Replacement must resample.
                session.pending_desktop = None;
                session.initial_desktop_closed = true;
                session.desktop_seen = false;
                session.desktop_latest = None;
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
    #[cfg(any(not(windows), test))]
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
    #[cfg(any(not(windows), test))]
    pub(super) fn subscribe_window_legacy_policy(
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
                    #[cfg(any(target_os = "macos", windows))]
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
    #[cfg(any(not(windows), test))]
    pub(super) fn dispatch_window_legacy_policy(
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
                let mut session = self
                    .windows
                    .as_ref()
                    .map(|windows| {
                        windows
                            .lock()
                            .map_err(|_| app_detail("window request", "registry lock poisoned"))
                    })
                    .transpose()?;
                let input = self.prepare_window_request_input_locked(
                    session.as_deref(),
                    active,
                    correlation,
                    payload,
                )?;
                let request = match input {
                    request::WindowRequestInput::Request(request) => request,
                    request::WindowRequestInput::Refused(refusal) => {
                        let (code, message) = refusal.into_parts();
                        return call_error(active, correlation, code, message, &mut failed);
                    }
                    request::WindowRequestInput::DuplicateCorrelation => {
                        failed = Some(active.attempt);
                        return Err(request::duplicate_correlation());
                    }
                };
                let session = session.as_deref_mut().ok_or_else(|| {
                    app_detail("window request", "validated Registry disappeared")
                })?;
                self.window_request_locked(session, active, correlation, request, &mut failed)
            })();
            self.finish_window_write(current, failed, result)
        });
        if let Some(attempt) = failed {
            collapse_app_results([result, self.link_failed(attempt)])
        } else {
            result
        }
    }
    #[cfg(any(not(windows), test))]
    fn window_request_locked(
        &self,
        session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        request: WindowRequest,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let outcome = self.prepare_window_request_locked(
            session,
            active,
            correlation,
            request,
            #[cfg(windows)]
            &mut request::WindowCloseControl::LegacyPolicy,
        )?;
        self.publish_window_request_locked(session, active, correlation, outcome, failed)
    }
    /// Caller holds transition/current/windows. The Registry invokes this only
    /// for its admitted logical closing cut, before clearing that exact Entry.
    #[cfg(any(target_os = "macos", windows))]
    #[cfg(any(not(windows), test))]
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
    #[cfg(any(target_os = "macos", windows))]
    #[cfg(any(not(windows), test))]
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
    /// Caller holds transition/current. This common cut precedes current.take,
    /// reader-stop and any successor publication; it touches no native handle.
    #[cfg(any(target_os = "macos", windows))]
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
    #[cfg(any(not(windows), test))]
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
    fn send_window_teardown(
        &self,
        teardown: super::window_registry::Teardown,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        #[cfg(windows)]
        if let Some(dialog) = &self.dialog {
            dialog.parent_retiring(teardown.view)?;
        }
        self.window_commands
            .send(AppWindowCommand::DestroyWindow {
                view: teardown.view,
            })
            .map_err(|_| app_detail(phase, "UI command owner is unavailable"))
    }
    #[cfg(any(not(windows), test))]
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
    #[cfg(any(not(windows), test))]
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
    #[cfg(any(not(windows), test))]
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

    // Caller owns transition -> current -> windows. This dispatch performs the
    // registry transition and its ordered effects without changing lock custody.
    #[cfg(any(not(windows), test))]
    fn window_native_event_locked(
        &self,
        session: &mut WindowSession,
        current: &mut Option<ActivePrimaryGeneration>,
        native_event: AppWindowEvent,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        #[cfg(target_os = "macos")]
        let native_event = match native_event {
            AppWindowEvent::DesktopObserved { facts } => {
                return desktop_observed_locked(session, current, facts, failed);
            }
            other => other,
        };
        let outcome = Self::prepare_window_native_locked(session, native_event)?;
        self.publish_window_native_locked(session, current, outcome, failed)
    }
    #[cfg(any(not(windows), test))]
    pub(super) fn window_native_event_legacy_policy(
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
    #[cfg(any(target_os = "macos", windows))]
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
        #[cfg(any(target_os = "macos", windows))]
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
        #[cfg(not(any(target_os = "macos", windows)))]
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
        #[cfg(any(target_os = "macos", windows))]
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
            #[cfg(any(target_os = "macos", windows, test))]
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
