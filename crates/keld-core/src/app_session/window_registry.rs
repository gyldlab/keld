//! Private app-session registry: identities, facts, adoption and two-phase close.
//!
//! This state belongs to the app session, not a Bun generation. The parent owns
//! authenticated primary/subscription admission, the one ordered writer and UI
//! command queue. This module owns no handles, principal minter, I/O or output
//! queue. Close state and deferred call metadata have this one owner.

use std::collections::BTreeMap;
use std::fmt;

use keld_ipc::CorrelationId;
use keld_ipc::window::{
    CloseVerdict, Rect, WindowEvent, WindowGeneration, WindowId, WindowRef, WindowResponse,
    WindowState,
};
use keld_wv::WebviewId;

use super::PrimaryLinkIdentity;

/// Trusted caller metadata captured by the existing authenticated primary reader.
/// Runtime identity and Core attempt fencing remain distinct fields.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct WindowOwner {
    identity: PrimaryLinkIdentity,
    attempt: u32,
}
impl WindowOwner {
    pub(super) const fn new(identity: PrimaryLinkIdentity, attempt: u32) -> Self {
        Self { identity, attempt }
    }
}
impl fmt::Debug for WindowOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowOwner")
            .field("attempt", &self.attempt)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowCounter {
    Id,
    Incarnation,
    CloseSequence,
    #[cfg(any(target_os = "macos", test))]
    OriginalLoadCorrelation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowError {
    StaleWindow,
    Exhausted(WindowCounter),
    NotReady,
    WrongOwner,
    PendingDestroyCapacity,
    SessionWindowCapacity,
    #[cfg(target_os = "macos")]
    OriginalLoadBusy,
    #[cfg(target_os = "macos")]
    OriginalSourceUnavailable,
    #[cfg(target_os = "macos")]
    OriginalDocumentCloseRefused,
}
impl WindowError {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::StaleWindow => "KELD-CORE-038",
            Self::Exhausted(_) => "KELD-CORE-039",
            Self::NotReady => "KELD-CORE-040",
            Self::WrongOwner => "KELD-CORE-043",
            Self::PendingDestroyCapacity => "KELD-CORE-046",
            Self::SessionWindowCapacity => "KELD-CORE-047",
            #[cfg(target_os = "macos")]
            Self::OriginalLoadBusy => "KELD-CORE-050",
            #[cfg(target_os = "macos")]
            Self::OriginalSourceUnavailable => "KELD-CORE-051",
            #[cfg(target_os = "macos")]
            Self::OriginalDocumentCloseRefused => "KELD-CORE-054",
        }
    }
}
impl fmt::Display for WindowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self {
            Self::StaleWindow => {
                "stale or unavailable window; drop the reference in the 'closed' listener"
            }
            Self::Exhausted(WindowCounter::Id) => "WindowId exhausted; start a new app session",
            Self::Exhausted(WindowCounter::Incarnation) => {
                "WindowGeneration exhausted; start a new app session"
            }
            Self::Exhausted(WindowCounter::CloseSequence) => {
                "close_seq exhausted; destroy the window or start a new app session"
            }
            #[cfg(any(target_os = "macos", test))]
            Self::Exhausted(WindowCounter::OriginalLoadCorrelation) => {
                "original-load operation correlation exhausted; start a new app session"
            }
            #[cfg(target_os = "macos")]
            Self::OriginalDocumentCloseRefused => {
                "original-document close was not admitted; ignore the old result, preserve the current window/document and wait for a new explicit current-document action"
            }
            #[cfg(target_os = "macos")]
            Self::OriginalSourceUnavailable => {
                "the window has no matching admitted original source/preload configuration; create it through the verified original preload selection"
            }
            #[cfg(target_os = "macos")]
            Self::OriginalLoadBusy => {
                "an original load is still pending for this window; wait for its real finish or failure"
            }
            Self::NotReady => {
                "window registry is not serviceable; wait for Ready on a live app session"
            }
            Self::WrongOwner => {
                "window belongs to another or retired role generation; use the successor's subscribed replay"
            }
            Self::PendingDestroyCapacity => {
                "this window has too many Destroy calls waiting for physical release; wait for Released or drop a timed-out call"
            }
            Self::SessionWindowCapacity => {
                "this Host session has reached its cumulative distinct-window admission limit; end the session to mint another identity"
            }
        };
        write!(formatter, "{}: {detail}", self.code())
    }
}
impl std::error::Error for WindowError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingWindowCall {
    pub(super) owner: WindowOwner,
    pub(super) correlation: CorrelationId,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct OwnedWindowEvent {
    pub(super) owner: WindowOwner,
    pub(super) event: WindowEvent,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WindowReply {
    pub(super) owner: WindowOwner,
    pub(super) correlation: CorrelationId,
    pub(super) response: WindowResponse,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CreateOutcome {
    Build {
        build: u64,
        window: WindowRef,
        width: u32,
        height: u32,
        x: Option<i32>,
        y: Option<i32>,
    },
    Adopted(WindowRef),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Teardown {
    pub(super) window: WindowRef,
    pub(super) view: WebviewId,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) enum BuildOutcome {
    /// Parent writes the fact before the correlated reply on the same writer.
    Created {
        fact: OwnedWindowEvent,
        reply: WindowReply,
    },
    Cleanup(Teardown),
    Ignored,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BuildFailureOutcome {
    /// Parent answers this with041 including the UI-reported failed stage first.
    pub(super) call: Option<PendingWindowCall>,
    pub(super) last_window: Option<WindowOwner>,
    pub(super) diagnostic: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeSetter {
    Maximized { view: WebviewId, maximized: bool },
    FullScreen { view: WebviewId, full_screen: bool },
}
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FactOutcome {
    Event(OwnedWindowEvent),
    Coalesced,
    Suppressed,
    Unmapped,
    Dropped,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) enum CloseOutcome {
    Requested {
        close_seq: u64,
        fact: Option<OwnedWindowEvent>,
    },
    Merged {
        close_seq: u64,
    },
    Teardown(Teardown),
    Vetoed,
    Stale,
    AlreadyClosing,
    Unmapped,
    Exhausted,
}
#[derive(Debug, PartialEq)]
pub(super) struct ReleaseOutcome {
    /// Parent writes this before `last_window`, if both are present.
    pub(super) fact: Option<OwnedWindowEvent>,
    pub(super) last_window: Option<WindowOwner>,
    pub(super) dropped_facts: u64,
    pub(super) diagnostic: bool,
    /// Correlations only: the existing router writes them after both facts.
    pub(super) destroy_calls: Vec<PendingWindowCall>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AbandonedClose {
    pub(super) window: WindowRef,
    pub(super) close_seq: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionEffect {
    Teardown(Teardown),
    /// The Create was admitted: its effect is unknown, never proof for024/replay.
    CreateInterrupted(PendingWindowCall),
    CloseAbandoned(AbandonedClose),
    #[cfg(any(target_os = "macos", test))]
    OriginalLoadInterrupted(PendingWindowCall),
}

/// Immutable association captured before the UI command can be observed.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OriginalLoadContext {
    pub(super) operation: u64,
    pub(super) call: PendingWindowCall,
    pub(super) window: WindowRef,
    pub(super) view: WebviewId,
}
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OriginalLoadAdmission {
    Dispatch {
        operation: u64,
        window: WindowRef,
        view: WebviewId,
    },
    Busy,
}
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy)]
struct PendingOriginalLoad {
    context: OriginalLoadContext,
    #[cfg(target_os = "macos")]
    attempt: Option<keld_wv::wkwebview::MacNativeAttempt>,
    #[cfg(target_os = "macos")]
    committed: Option<keld_wv::wkwebview::MacNativeDocumentStamp>,
}
#[cfg(target_os = "macos")]
pub(super) struct CommittedOriginalLoad<'document> {
    pub(super) owner: WindowOwner,
    pub(super) window: WindowRef,
    pub(super) document_generation: u32,
    pub(super) native_url: &'document keld_wv::wkwebview::MacNavigationUrl,
}
#[cfg(target_os = "macos")]
pub(super) struct FinishedOriginalLoad {
    pub(super) fact: OwnedWindowEvent,
    pub(super) reply: WindowReply,
}

#[cfg(target_os = "macos")]
struct PendingOriginalDocumentDestroy {
    call: Option<PendingWindowCall>,
    admission: std::sync::Arc<keld_wv::wkwebview::OriginalDocumentDestroyAdmission>,
}
#[cfg(target_os = "macos")]
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct OriginalDocumentDestroyResolution {
    pub(super) refused: Option<PendingWindowCall>,
    pub(super) abandoned: Option<AbandonedClose>,
    pub(super) interrupted: Option<PendingWindowCall>,
}

#[derive(Clone, Copy)]
struct NativeWindow {
    view: WebviewId,
    state: WindowState,
}
#[derive(Clone, Copy)]
enum Phase {
    Opening(PendingWindowCall),
    Open(NativeWindow),
    ClosePending(NativeWindow, u64),
    Closing(Option<NativeWindow>),
    Destroyed(Option<WebviewId>),
}
/// A bounded snapshot of the existing authenticated Entry and native lease.
/// Native revocation is rechecked by each receiver; no snapshot grants a later
/// native side effect without that UI owner's final same-turn admission.
#[cfg(target_os = "macos")]
pub(super) struct RendererRoute {
    pub(super) window: WindowRef,
    pub(super) document: keld_wv::wkwebview::MacNativeDocumentLease,
    selection: std::sync::Arc<super::original_webapp::OriginalLoadSelection>,
}
#[cfg(target_os = "macos")]
impl RendererRoute {
    pub(super) fn window_key(&self) -> &str {
        self.selection.window_key()
    }
}
#[cfg(target_os = "macos")]
struct OriginalWindowSelection {
    selection: std::sync::Arc<super::original_webapp::OriginalLoadSelection>,
    preload_image: std::sync::Arc<keld_wv::wkwebview::AppPreloadImage>,
}
#[cfg(target_os = "macos")]
struct RendererDocumentState {
    committed: Option<keld_wv::wkwebview::MacNativeDocumentStamp>,
    document: Option<keld_wv::wkwebview::MacNativeDocumentLease>,
    // Physical observations survive only a fully qualified role-loss cut.
    native_url: Option<keld_wv::wkwebview::MacNavigationUrl>,
    registered: bool,
    finished: bool,
    // A physical observation never implies permission for a successor role.
    bound_owner: Option<WindowOwner>,
}
struct Entry {
    window: WindowRef,
    owner: WindowOwner,
    owner_live: bool,
    adoptable: bool,
    // Native creation committed. The parent must retire the owner synchronously
    // if publishing Created fails, before any later fact can use that writer.
    created: bool,
    dropped: u64,
    last_close_seq: u64,
    // Only admitted cold Destroy calls allocate this metadata. It carries no
    // stream or handle, and native Released is the sole completion input.
    destroy_calls: Vec<PendingWindowCall>,
    #[cfg(target_os = "macos")]
    original_destroy: Option<PendingOriginalDocumentDestroy>,
    #[cfg(any(target_os = "macos", test))]
    original_load: Option<PendingOriginalLoad>,
    #[cfg(target_os = "macos")]
    original_selection: Option<OriginalWindowSelection>,
    #[cfg(target_os = "macos")]
    renderer: RendererDocumentState,
    phase: Phase,
}

/// One physical native release can be delayed indefinitely. Do not permit an
/// authenticated primary to retain arbitrarily many distinct deferred reply
/// correlations for that one window. This is a new Host-owned admission budget
/// (not gh527's abandoned-Worker limit or the Auth call budget). 64 pending
/// calls need under 2 KiB of plain metadata per window and bound the linear
/// correlation check to a small, deterministic cost during a stalled release.
pub(super) const MAX_PENDING_DESTROYS_PER_WINDOW: usize = 64;
/// GH1149 review-only candidate: distinct Host identities over one session,
/// including failed and destroyed Create attempts. It is NOT a reviewed heap
/// byte budget or a stand-in for GH1142 source/callback/alias qualification.
/// Never turn the functional proposal into merge approval without GH1149 AC5.
pub(super) const PROPOSED_MAX_CUMULATIVE_WINDOW_IDS: usize = 1024;
impl Entry {
    fn pending_destroy_count(&self) -> usize {
        #[cfg(target_os = "macos")]
        let scoped = usize::from(
            self.original_destroy
                .as_ref()
                .is_some_and(|pending| pending.call.is_some()),
        );
        #[cfg(not(target_os = "macos"))]
        let scoped = 0;
        self.destroy_calls.len() + scoped
    }
    #[cfg(target_os = "macos")]
    fn reconcile_original_destroy(&mut self, receipt: bool) -> OriginalDocumentDestroyResolution {
        use keld_wv::wkwebview::OriginalDocumentDestroyDecision;
        let Some(pending) = self.original_destroy.as_mut() else {
            return OriginalDocumentDestroyResolution::default();
        };
        let decision = pending.admission.decision();
        if decision == OriginalDocumentDestroyDecision::Pending {
            return OriginalDocumentDestroyResolution::default();
        }
        let call = pending.call.take();
        let mut outcome = OriginalDocumentDestroyResolution::default();
        match decision {
            OriginalDocumentDestroyDecision::Accepted => {
                match self.phase {
                    Phase::Open(native) => self.phase = Phase::Closing(Some(native)),
                    Phase::ClosePending(native, close_seq) => {
                        self.phase = Phase::Closing(Some(native));
                        outcome.abandoned = Some(AbandonedClose {
                            window: self.window,
                            close_seq,
                        });
                    }
                    _ => {}
                }
                if matches!(self.phase, Phase::Closing(_)) {
                    self.adoptable = false;
                    self.retire_renderer_document();
                    outcome.interrupted = self.original_load.take().map(|load| load.context.call);
                    if let Some(call) =
                        call.filter(|call| self.owner_live && call.owner == self.owner)
                    {
                        self.destroy_calls.push(call);
                    }
                }
            }
            OriginalDocumentDestroyDecision::Refused
            | OriginalDocumentDestroyDecision::Cancelled => {
                outcome.refused = call.filter(|call| self.owner_live && call.owner == self.owner);
            }
            OriginalDocumentDestroyDecision::Pending => {}
        }
        if receipt {
            self.original_destroy = None;
        }
        outcome
    }
    #[cfg(target_os = "macos")]
    fn retire_renderer_document(&mut self) {
        self.renderer.committed = None;
        self.renderer.document = None;
        self.renderer.native_url = None;
        self.renderer.registered = false;
        self.renderer.finished = false;
        self.renderer.bound_owner = None;
    }
    /// Pure observation of the SAME Entry/source/native lease. It grants no
    /// role authority; publication and every later route recheck native liveness.
    #[cfg(target_os = "macos")]
    fn recoverable_original_document(
        &self,
    ) -> Option<(
        &keld_wv::wkwebview::MacNativeDocumentLease,
        &keld_wv::wkwebview::MacNavigationUrl,
    )> {
        let Phase::Open(native) = self.phase else {
            return None;
        };
        if !self.created
            || self.original_load.is_some()
            || self.original_destroy.is_some()
            || !self.destroy_calls.is_empty()
            || !self.renderer.registered
            || !self.renderer.finished
        {
            return None;
        }
        let document = self.renderer.document.as_ref()?;
        let binding = self.original_selection.as_ref()?;
        let native_url = self.renderer.native_url.as_ref()?;
        (document.stamp().view() == native.view
            && self.renderer.committed == Some(document.stamp())
            && document.belongs_to_source(binding.selection.source())
            && document.is_current())
        .then_some((document, native_url))
    }
    fn adoptable_for(&self, owner: WindowOwner) -> bool {
        self.owner_live
            && self.owner == owner
            && self.adoptable
            && matches!(self.phase, Phase::Open(_) | Phase::ClosePending(_, _))
    }
    fn view(&self) -> Option<WebviewId> {
        match self.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) | Phase::Closing(Some(native)) => {
                Some(native.view)
            }
            Phase::Destroyed(view) => view,
            Phase::Opening(_) | Phase::Closing(None) => None,
        }
    }
}

pub(super) struct WindowRegistry {
    ready: bool,
    ending: bool,
    last_id: u32,
    last_incarnation: u64,
    #[cfg(any(target_os = "macos", test))]
    last_original_load_operation: u64,
    entries: BTreeMap<u32, Entry>,
    deferred_last_window: bool,
    destroyed_fact_drops: u64,
}
impl WindowRegistry {
    pub(super) fn new() -> Self {
        Self {
            ready: false,
            ending: false,
            last_id: 0,
            last_incarnation: 0,
            #[cfg(any(target_os = "macos", test))]
            last_original_load_operation: 0,
            entries: BTreeMap::new(),
            deferred_last_window: false,
            destroyed_fact_drops: 0,
        }
    }
    /// False means a prior session end won; the parent must not publish Ready.
    pub(super) fn mark_ready(&mut self) -> bool {
        if self.ending {
            return false;
        }
        self.ready = true;
        true
    }
    /// Parent checks subscription045 before this method's registry-ready040.
    pub(super) fn create(
        &mut self,
        owner: WindowOwner,
        correlation: CorrelationId,
        width: u32,
        height: u32,
        x: Option<i32>,
        y: Option<i32>,
    ) -> Result<CreateOutcome, WindowError> {
        self.require_ready()?;
        if let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.adoptable_for(owner))
        {
            entry.adoptable = false;
            return Ok(CreateOutcome::Adopted(entry.window));
        }
        // Validate both counters before committing either; a failed pair is never reused.
        let id = self
            .last_id
            .checked_add(1)
            .ok_or(WindowError::Exhausted(WindowCounter::Id))?;
        let incarnation = self
            .last_incarnation
            .checked_add(1)
            .ok_or(WindowError::Exhausted(WindowCounter::Incarnation))?;
        // A Host-approved admission must precede the new native window. The
        // registry retains destroyed tombstones for the entire Host session;
        // this ceiling bounds distinct identities without recycling a pair.
        // Preserve original counter-exhaustion error precedence when both
        // limits are reached, and never mutate either half on refusal.
        if self.entries.len() >= PROPOSED_MAX_CUMULATIVE_WINDOW_IDS {
            return Err(WindowError::SessionWindowCapacity);
        }
        let window = WindowRef {
            id: WindowId(id),
            incarnation: WindowGeneration(incarnation),
        };
        self.last_id = id;
        self.last_incarnation = incarnation;
        self.entries.insert(
            id,
            Entry {
                window,
                owner,
                owner_live: true,
                adoptable: false,
                created: false,
                dropped: 0,
                last_close_seq: 0,
                destroy_calls: Vec::new(),
                #[cfg(target_os = "macos")]
                original_destroy: None,
                #[cfg(any(target_os = "macos", test))]
                original_load: None,
                #[cfg(target_os = "macos")]
                original_selection: None,
                #[cfg(target_os = "macos")]
                renderer: RendererDocumentState {
                    committed: None,
                    document: None,
                    native_url: None,
                    registered: false,
                    finished: false,
                    bound_owner: None,
                },
                phase: Phase::Opening(PendingWindowCall { owner, correlation }),
            },
        );
        // The checked incarnation is already unique for this session: reuse it as
        // the opaque native-build token instead of introducing another counter.
        Ok(CreateOutcome::Build {
            build: incarnation,
            window,
            width,
            height,
            x,
            y,
        })
    }
    /// Exact image/source association for the same entry the existing Create
    /// owner admitted. The caller prepared immutable bytes before this method.
    #[cfg(target_os = "macos")]
    pub(super) fn create_with_original_preload(
        &mut self,
        call: PendingWindowCall,
        size: (u32, u32),
        position: (Option<i32>, Option<i32>),
        selection: std::sync::Arc<super::original_webapp::OriginalLoadSelection>,
        preload_image: std::sync::Arc<keld_wv::wkwebview::AppPreloadImage>,
    ) -> Result<CreateOutcome, WindowError> {
        self.require_ready()?;
        let PendingWindowCall { owner, correlation } = call;
        let (width, height) = size;
        let (x, y) = position;
        // Match the existing Create owner's first-adoptable rule before it can
        // consume a mark. Never relabel an unrelated surviving native window.
        if let Some(entry) = self
            .entries
            .values()
            .find(|entry| entry.adoptable_for(owner))
        {
            let Some(existing) = entry.original_selection.as_ref() else {
                return Err(WindowError::OriginalSourceUnavailable);
            };
            if !std::sync::Arc::ptr_eq(&existing.selection, &selection)
                || existing.selection.window_key() != selection.window_key()
                || !existing.preload_image.same_configuration(&preload_image)
            {
                return Err(WindowError::OriginalSourceUnavailable);
            }
        }
        let outcome = self.create(owner, correlation, width, height, x, y)?;
        if let CreateOutcome::Build { window, .. } = outcome {
            let entry = self
                .entries
                .get_mut(&window.id.0)
                .ok_or(WindowError::StaleWindow)?;
            entry.original_selection = Some(OriginalWindowSelection {
                selection,
                preload_image,
            });
        }
        Ok(outcome)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn original_window_source(
        &self,
        owner: WindowOwner,
        window: WindowRef,
    ) -> Result<&std::sync::Arc<super::original_webapp::OriginalLoadSelection>, WindowError> {
        let entry = self.checked_entry(owner, window)?;
        if !matches!(entry.phase, Phase::Open(_) | Phase::ClosePending(_, _)) {
            return Err(WindowError::StaleWindow);
        }
        entry
            .original_selection
            .as_ref()
            .map(|original| &original.selection)
            .ok_or(WindowError::OriginalSourceUnavailable)
    }
    /// The parent supplies only the UI thread's actual native snapshot here.
    /// Create geometry is not stored and cannot fabricate Created.state.
    pub(super) fn native_built(
        &mut self,
        build: u64,
        view: WebviewId,
        state: WindowState,
    ) -> BuildOutcome {
        let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.window.incarnation.0 == build)
        else {
            return BuildOutcome::Ignored;
        };
        let native = NativeWindow { view, state };
        match entry.phase {
            Phase::Opening(call) => {
                entry.phase = Phase::Open(native);
                entry.created = true;
                self.deferred_last_window = false;
                BuildOutcome::Created {
                    fact: OwnedWindowEvent {
                        owner: call.owner,
                        event: WindowEvent::Created {
                            window: entry.window,
                            state,
                            pending_close: None,
                        },
                    },
                    reply: WindowReply {
                        owner: call.owner,
                        correlation: call.correlation,
                        response: WindowResponse::Created {
                            window: entry.window,
                        },
                    },
                }
            }
            Phase::Closing(None) => {
                entry.phase = Phase::Closing(Some(native));
                BuildOutcome::Cleanup(Teardown {
                    window: entry.window,
                    view,
                })
            }
            Phase::Open(_)
            | Phase::ClosePending(_, _)
            | Phase::Closing(Some(_))
            | Phase::Destroyed(_) => BuildOutcome::Ignored,
        }
    }
    pub(super) fn native_failed(&mut self, build: u64) -> BuildFailureOutcome {
        let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.window.incarnation.0 == build)
        else {
            return BuildFailureOutcome {
                call: None,
                last_window: None,
                diagnostic: true,
            };
        };
        let call = match entry.phase {
            Phase::Opening(call) => Some(call),
            Phase::Closing(None) => None,
            Phase::Open(_)
            | Phase::ClosePending(_, _)
            | Phase::Closing(Some(_))
            | Phase::Destroyed(_) => {
                return BuildFailureOutcome {
                    call: None,
                    last_window: None,
                    diagnostic: true,
                };
            }
        };
        #[cfg(target_os = "macos")]
        {
            entry.retire_renderer_document();
            entry.original_selection = None;
        }
        entry.phase = Phase::Destroyed(None);
        let live_call = call.filter(|_| entry.owner_live && !self.ending);
        let last_window =
            if self.deferred_last_window && live_call.is_some() && !self.has_live_windows() {
                self.deferred_last_window = false;
                live_call.map(|pending| pending.owner)
            } else {
                None
            };
        BuildFailureOutcome {
            call: live_call,
            last_window,
            diagnostic: false,
        }
    }
    /// The existing entry owns both admitted Create and deferred Destroy calls.
    pub(super) fn has_pending_call(&self, owner: WindowOwner, correlation: CorrelationId) -> bool {
        self.entries.values().any(|entry| {
            matches!(entry.phase, Phase::Opening(call) if call.owner == owner && call.correlation == correlation)
                || entry.destroy_calls.iter().any(|call| call.owner == owner && call.correlation == correlation)
                || {
                    #[cfg(target_os = "macos")]
                    { entry.original_destroy.as_ref().and_then(|pending| pending.call).is_some_and(|call| call.owner == owner && call.correlation == correlation) }
                    #[cfg(not(target_os = "macos"))]
                    { false }
                }
                || {
                    #[cfg(any(target_os = "macos", test))]
                    { entry.original_load.is_some_and(|load| load.context.call.owner == owner && load.context.call.correlation == correlation) }
                    #[cfg(not(any(target_os = "macos", test)))]
                    { false }
                }
        })
    }
    fn require_ready(&self) -> Result<(), WindowError> {
        if self.ready && !self.ending {
            Ok(())
        } else {
            Err(WindowError::NotReady)
        }
    }
    fn checked_entry(&self, owner: WindowOwner, window: WindowRef) -> Result<&Entry, WindowError> {
        self.require_ready()?;
        let entry = self
            .entries
            .get(&window.id.0)
            .filter(|entry| entry.window == window && !matches!(entry.phase, Phase::Destroyed(_)))
            .ok_or(WindowError::StaleWindow)?;
        if !entry.owner_live || entry.owner != owner {
            return Err(WindowError::WrongOwner);
        }
        Ok(entry)
    }
    fn admitted_entry(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
    ) -> Result<&mut Entry, WindowError> {
        self.checked_entry(owner, window)?;
        let entry = self
            .entries
            .get_mut(&window.id.0)
            .ok_or(WindowError::StaleWindow)?;
        entry.adoptable = false;
        Ok(entry)
    }
    fn admitted_view(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
    ) -> Result<WebviewId, WindowError> {
        match self.admitted_entry(owner, window)?.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) => Ok(native.view),
            _ => Err(WindowError::StaleWindow),
        }
    }
    /// The same checked owner/ready/incarnation resolver as native setters,
    /// but without consuming the adoption mark. Busy/nonce refusal cannot
    /// alter existing window state just by inspecting its potential parent.
    #[cfg(target_os = "macos")]
    pub(super) fn admitted_dialog_view(
        &self,
        owner: WindowOwner,
        window: WindowRef,
    ) -> Result<WebviewId, WindowError> {
        match self.checked_entry(owner, window)?.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) => Ok(native.view),
            _ => Err(WindowError::StaleWindow),
        }
    }
    /// Resolve the authentic current native view to the existing Host pair.
    /// Inspecting an IPC sender never consumes the recovery adoption mark.
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn admitted_renderer_window(
        &self,
        owner: WindowOwner,
        view: WebviewId,
    ) -> Result<WindowRef, WindowError> {
        self.require_ready()?;
        let entry = self.entries.values().find(|entry| {
            matches!(entry.phase, Phase::Open(native) | Phase::ClosePending(native, _) if native.view == view)
        }).ok_or(WindowError::StaleWindow)?;
        self.checked_entry(owner, entry.window)?;
        Ok(entry.window)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn admit_renderer_route(
        &mut self,
        owner: WindowOwner,
        view: WebviewId,
        document: keld_wv::wkwebview::MacNativeDocumentLease,
    ) -> Result<RendererRoute, WindowError> {
        let window = self.admitted_renderer_window(owner, view)?;
        let entry = self
            .entries
            .get_mut(&window.id.0)
            .ok_or(WindowError::StaleWindow)?;
        let binding = entry
            .original_selection
            .as_ref()
            .ok_or(WindowError::OriginalSourceUnavailable)?;
        if document.stamp().view() != view
            || entry.renderer.bound_owner != Some(owner)
            || !entry.renderer.registered
            || entry
                .renderer
                .document
                .as_ref()
                .is_none_or(|registered| !registered.same_binding(&document))
            || entry.renderer.committed != Some(document.stamp())
            || !document.belongs_to_source(binding.selection.source())
            || !document.is_current()
        {
            return Err(WindowError::StaleWindow);
        }
        entry.renderer.document = Some(document.clone());
        Ok(RendererRoute {
            window,
            document,
            selection: std::sync::Arc::clone(&binding.selection),
        })
    }
    #[cfg(target_os = "macos")]
    pub(super) fn renderer_route(
        &self,
        owner: WindowOwner,
        view: WebviewId,
        navigation: Option<u32>,
    ) -> Result<RendererRoute, WindowError> {
        let window = self.admitted_renderer_window(owner, view)?;
        let entry = self.checked_entry(owner, window)?;
        let binding = entry
            .original_selection
            .as_ref()
            .ok_or(WindowError::OriginalSourceUnavailable)?;
        let document = entry
            .renderer
            .document
            .as_ref()
            .ok_or(WindowError::StaleWindow)?;
        if document.stamp().view() != view
            || entry.renderer.bound_owner != Some(owner)
            || !entry.renderer.registered
            || entry
                .renderer
                .document
                .as_ref()
                .is_none_or(|registered| !registered.same_binding(document))
            || entry.renderer.committed != Some(document.stamp())
            || navigation.is_some_and(|expected| expected != document.stamp().observed_generation())
            || !document.belongs_to_source(binding.selection.source())
            || !document.is_current()
        {
            return Err(WindowError::StaleWindow);
        }
        Ok(RendererRoute {
            window,
            document: document.clone(),
            selection: std::sync::Arc::clone(&binding.selection),
        })
    }
    /// An actual native Registered control is metadata, not caller authority.
    /// A bounded mark may precede the observer; routing still requires the real
    /// matching accepted Core commit and the current authenticated owner.
    /// In particular a delayed old Registered can never bind the new Entry owner.
    #[cfg(target_os = "macos")]
    pub(super) fn note_renderer_registered(
        &mut self,
        view: WebviewId,
        document: keld_wv::wkwebview::MacNativeDocumentLease,
    ) -> Result<(), WindowError> {
        self.require_ready()?;
        let entry = self.entries.values_mut().find(|entry| {
            matches!(entry.phase, Phase::Open(native) | Phase::ClosePending(native, _) if native.view == view)
        }).ok_or(WindowError::StaleWindow)?;
        let binding = entry
            .original_selection
            .as_ref()
            .ok_or(WindowError::OriginalSourceUnavailable)?;
        let accepted_load = entry
            .original_load
            .is_some_and(|load| load.context.call.owner == entry.owner);
        if !entry.owner_live
            || (!accepted_load && entry.renderer.committed != Some(document.stamp()))
            || document.stamp().view() != view
            || !document.belongs_to_source(binding.selection.source())
            || !document.is_current()
        {
            return Err(WindowError::StaleWindow);
        }
        entry.renderer.document = Some(document);
        entry.renderer.registered = true;
        Ok(())
    }
    /// Iterate the existing Entry owner only. Ready publication separately
    /// requires COMPAT subscription and native liveness in its current fence;
    /// a false retirement may still name a just-revoked registered document.
    #[cfg(target_os = "macos")]
    pub(super) fn for_each_registered_renderer_route(
        &self,
        owner: WindowOwner,
        mut callback: impl FnMut(RendererRoute) -> Result<(), super::HostAppError>,
    ) -> Result<(), super::HostAppError> {
        for entry in self.entries.values().filter(|entry| {
            entry.owner_live
                && entry.owner == owner
                && entry.renderer.bound_owner == Some(owner)
                && entry.renderer.registered
        }) {
            let Some(document) = entry.renderer.document.as_ref() else {
                continue;
            };
            let Some(binding) = entry.original_selection.as_ref() else {
                continue;
            };
            if !matches!(entry.phase, Phase::Open(_) | Phase::ClosePending(_, _))
                || entry.view() != Some(document.stamp().view())
                || entry.renderer.committed != Some(document.stamp())
                || !document.belongs_to_source(binding.selection.source())
            {
                continue;
            }
            callback(RendererRoute {
                window: entry.window,
                document: document.clone(),
                selection: std::sync::Arc::clone(&binding.selection),
            })?;
        }
        Ok(())
    }
    /// Window Subscribe may observe a retained document, but cannot bind it.
    /// Borrowing keeps the native URL's existing source-wide credit attached.
    #[cfg(target_os = "macos")]
    pub(super) fn for_each_recovered_document(
        &self,
        owner: WindowOwner,
        mut callback: impl FnMut(
            WindowRef,
            &keld_wv::wkwebview::MacNativeDocumentLease,
            &keld_wv::wkwebview::MacNavigationUrl,
        ) -> Result<(), super::HostAppError>,
    ) -> Result<(), super::HostAppError> {
        if self.require_ready().is_err() {
            return Ok(());
        }
        for entry in self.entries.values().filter(|entry| {
            entry.owner_live && entry.owner == owner && entry.renderer.bound_owner.is_none()
        }) {
            if let Some((document, native_url)) = entry.recoverable_original_document() {
                callback(entry.window, document, native_url)?;
            }
        }
        Ok(())
    }
    /// Only the authenticated fresh COMPAT Subscribe, after Window Subscribe,
    /// invokes this transition. Created/replay/native Registered never do so.
    #[cfg(target_os = "macos")]
    pub(super) fn rebind_recovered_renderer_documents(&mut self, owner: WindowOwner) -> bool {
        if self.require_ready().is_err() {
            return false;
        }
        let mut rebound = false;
        for entry in self.entries.values_mut().filter(|entry| {
            entry.owner_live && entry.owner == owner && entry.renderer.bound_owner.is_none()
        }) {
            if entry.recoverable_original_document().is_some() {
                entry.renderer.bound_owner = Some(owner);
                rebound = true;
            }
        }
        rebound
    }
    /// Actual native failure/revocation may arrive after its `LoadUrl` settled.
    /// Retire only that same stamp/attempt, never a later B on the same view.
    #[cfg(target_os = "macos")]
    pub(super) fn renderer_document_revoked(
        &mut self,
        view: WebviewId,
        previous: keld_wv::wkwebview::MacNativeDocumentStamp,
    ) {
        if let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.view() == Some(view))
            && entry.renderer.committed == Some(previous)
        {
            entry.retire_renderer_document();
        }
    }
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn admit_original_load(
        &mut self,
        owner: WindowOwner,
        correlation: CorrelationId,
        window: WindowRef,
    ) -> Result<OriginalLoadAdmission, WindowError> {
        let entry = self.checked_entry(owner, window)?;
        let view = match entry.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) => native.view,
            _ => return Err(WindowError::StaleWindow),
        };
        if entry.original_load.is_some() {
            return Ok(OriginalLoadAdmission::Busy);
        }
        let operation =
            self.last_original_load_operation
                .checked_add(1)
                .ok_or(WindowError::Exhausted(
                    WindowCounter::OriginalLoadCorrelation,
                ))?;
        let entry = self.admitted_entry(owner, window)?;
        entry.original_load = Some(PendingOriginalLoad {
            context: OriginalLoadContext {
                operation,
                call: PendingWindowCall { owner, correlation },
                window,
                view,
            },
            #[cfg(target_os = "macos")]
            attempt: None,
            #[cfg(target_os = "macos")]
            committed: None,
        });
        self.last_original_load_operation = operation;
        Ok(OriginalLoadAdmission::Dispatch {
            operation,
            window,
            view,
        })
    }
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn original_load_context(&self, operation: u64) -> Option<OriginalLoadContext> {
        self.entries.values().find_map(|entry| {
            let pending = entry.original_load?;
            (entry.owner_live && !self.ending
                && entry.owner == pending.context.call.owner
                && entry.window == pending.context.window
                && matches!(entry.phase, Phase::Open(native) | Phase::ClosePending(native, _) if native.view == pending.context.view)
                && pending.context.operation == operation).then_some(pending.context)
        })
    }
    #[cfg(target_os = "macos")]
    fn original_load_mut(
        &mut self,
        context: keld_wv::NativeNavigationContext,
    ) -> Option<(&mut Entry, PendingOriginalLoad)> {
        let captured = self.original_load_context(context.operation().get())?;
        if captured.view != context.view() {
            return None;
        }
        let entry = self.entries.get_mut(&captured.window.id.0)?;
        let pending = entry.original_load?;
        Some((entry, pending))
    }
    #[cfg(target_os = "macos")]
    pub(super) fn original_load_started(
        &mut self,
        context: keld_wv::NativeNavigationContext,
        attempt: keld_wv::wkwebview::MacNativeAttempt,
    ) -> bool {
        if attempt.view() != context.view() {
            return false;
        }
        let Some((entry, mut pending)) = self.original_load_mut(context) else {
            return false;
        };
        if pending.attempt.is_some() {
            return false;
        }
        if entry
            .renderer
            .document
            .as_ref()
            .is_none_or(|document| document.stamp().attempt() != attempt)
        {
            entry.retire_renderer_document();
        }
        pending.attempt = Some(attempt);
        entry.original_load = Some(pending);
        true
    }
    #[cfg(target_os = "macos")]
    pub(super) fn original_load_committed(
        &mut self,
        context: keld_wv::NativeNavigationContext,
        stamp: keld_wv::wkwebview::MacNativeDocumentStamp,
        document: keld_wv::wkwebview::MacNativeDocumentLease,
        native_url: keld_wv::wkwebview::MacNavigationUrl,
    ) -> Option<CommittedOriginalLoad<'_>> {
        if stamp.view() != context.view() {
            return None;
        }
        let (entry, mut pending) = self.original_load_mut(context)?;
        if pending.attempt != Some(stamp.attempt()) || pending.committed.is_some() {
            return None;
        }
        let binding = entry.original_selection.as_ref()?;
        if document.stamp() != stamp
            || !document.belongs_to_source(binding.selection.source())
            || !document.is_current()
        {
            return None;
        }
        let registered = entry.renderer.registered
            && entry
                .renderer
                .document
                .as_ref()
                .is_some_and(|registered| registered.same_binding(&document));
        pending.committed = Some(stamp);
        entry.renderer.committed = Some(stamp);
        entry.renderer.document = Some(document);
        entry.renderer.native_url = Some(native_url);
        entry.renderer.registered = registered;
        entry.renderer.finished = false;
        // The captured admitted load, never a native Registered callback, owns
        // ordinary initial routing even when registration arrives after commit.
        entry.renderer.bound_owner = Some(pending.context.call.owner);
        entry.original_load = Some(pending);
        Some(CommittedOriginalLoad {
            owner: pending.context.call.owner,
            window: pending.context.window,
            document_generation: stamp.observed_generation(),
            native_url: entry.renderer.native_url.as_ref()?,
        })
    }
    #[cfg(target_os = "macos")]
    pub(super) fn original_load_finished(
        &mut self,
        context: keld_wv::NativeNavigationContext,
        stamp: keld_wv::wkwebview::MacNativeDocumentStamp,
    ) -> Option<FinishedOriginalLoad> {
        let (entry, pending) = self.original_load_mut(context)?;
        if pending.committed != Some(stamp)
            || pending.attempt != Some(stamp.attempt())
            || entry.renderer.committed != Some(stamp)
            || entry
                .renderer
                .document
                .as_ref()
                .is_none_or(|document| document.stamp() != stamp || !document.is_current())
        {
            return None;
        }
        entry.renderer.finished = true;
        entry.original_load = None;
        Some(FinishedOriginalLoad {
            fact: OwnedWindowEvent {
                owner: pending.context.call.owner,
                event: WindowEvent::NavigationFinished {
                    window: pending.context.window,
                    document_generation: stamp.observed_generation(),
                },
            },
            reply: WindowReply {
                owner: pending.context.call.owner,
                correlation: pending.context.call.correlation,
                response: WindowResponse::Loaded,
            },
        })
    }
    /// A pre-attempt refusal can consume only an unstarted captured call.
    /// Actual failures carry their own native attempt, including loadRequest
    /// failures that precede Started. Neither path can consume another context.
    #[cfg(target_os = "macos")]
    pub(super) fn original_load_failed(
        &mut self,
        context: keld_wv::NativeNavigationContext,
        attempt: Option<keld_wv::wkwebview::MacNativeAttempt>,
    ) -> Option<PendingWindowCall> {
        let (entry, pending) = self.original_load_mut(context)?;
        match (pending.attempt, attempt) {
            (None, None) => {}
            (None, Some(actual)) if actual.view() == context.view() => {}
            (Some(expected), Some(actual)) if expected == actual => {}
            _ => return None,
        }
        entry.original_load = None;
        // Refused before native admission leaves the old real document intact.
        // Once an actual attempt exists, the native owner has superseded it.
        if attempt.is_some() {
            entry.retire_renderer_document();
        }
        Some(pending.context.call)
    }
    /// Consume the closing entry's original-call interruption, under the same
    /// registry lock as the admitted close/destroy transition. No callback can
    /// use the retained metadata once the entry stopped being serviceable.
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn take_interrupted_original_load(
        &mut self,
        window: WindowRef,
    ) -> Option<PendingWindowCall> {
        let entry = self.entries.get_mut(&window.id.0)?;
        if entry.window != window || !matches!(entry.phase, Phase::Closing(_)) {
            return None;
        }
        entry
            .original_load
            .take()
            .map(|pending| pending.context.call)
    }
    /// Sending failed before native admission; remove only this exact context.
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn abandon_original_dispatch(&mut self, operation: u64) {
        if let Some(entry) = self.entries.values_mut().find(|entry| {
            entry
                .original_load
                .is_some_and(|pending| pending.context.operation == operation)
        }) {
            entry.original_load = None;
        }
    }
    pub(super) fn set_maximized(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
        maximized: bool,
    ) -> Result<NativeSetter, WindowError> {
        Ok(NativeSetter::Maximized {
            view: self.admitted_view(owner, window)?,
            maximized,
        })
    }
    pub(super) fn set_full_screen(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
        full_screen: bool,
    ) -> Result<NativeSetter, WindowError> {
        Ok(NativeSetter::FullScreen {
            view: self.admitted_view(owner, window)?,
            full_screen,
        })
    }
    fn native_fact(
        &mut self,
        view: WebviewId,
        update: impl FnOnce(WindowRef, &mut WindowState) -> Option<WindowEvent>,
    ) -> FactOutcome {
        let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.view() == Some(view))
        else {
            return FactOutcome::Unmapped;
        };
        match entry.phase {
            Phase::Open(mut native) | Phase::ClosePending(mut native, _) => {
                let event = update(entry.window, &mut native.state);
                entry.phase = match entry.phase {
                    Phase::ClosePending(_, seq) => Phase::ClosePending(native, seq),
                    _ => Phase::Open(native),
                };
                match event {
                    Some(event) if entry.owner_live => FactOutcome::Event(OwnedWindowEvent {
                        owner: entry.owner,
                        event,
                    }),
                    Some(_) => FactOutcome::Suppressed,
                    None => FactOutcome::Coalesced,
                }
            }
            Phase::Closing(_) => {
                entry.dropped = entry.dropped.saturating_add(1);
                FactOutcome::Dropped
            }
            Phase::Destroyed(_) => {
                self.destroyed_fact_drops = self.destroyed_fact_drops.saturating_add(1);
                FactOutcome::Dropped
            }
            Phase::Opening(_) => FactOutcome::Unmapped,
        }
    }
    pub(super) fn native_bounds(&mut self, view: WebviewId, bounds: Rect) -> FactOutcome {
        self.native_fact(view, |window, state| {
            if state.bounds == bounds {
                None
            } else {
                state.bounds = bounds;
                Some(WindowEvent::Bounds { window, bounds })
            }
        })
    }
    pub(super) fn native_focus(&mut self, view: WebviewId, focused: bool) -> FactOutcome {
        self.native_fact(view, |window, state| {
            if state.focused == focused {
                None
            } else {
                state.focused = focused;
                Some(WindowEvent::Focus { window, focused })
            }
        })
    }
    pub(super) fn native_maximized(&mut self, view: WebviewId, maximized: bool) -> FactOutcome {
        self.native_fact(view, |window, state| {
            if state.maximized == maximized {
                None
            } else {
                state.maximized = maximized;
                Some(WindowEvent::Maximized { window, maximized })
            }
        })
    }
    pub(super) fn native_full_screen(&mut self, view: WebviewId, full_screen: bool) -> FactOutcome {
        self.native_fact(view, |window, state| {
            if state.full_screen == full_screen {
                None
            } else {
                state.full_screen = full_screen;
                Some(WindowEvent::FullScreen {
                    window,
                    full_screen,
                })
            }
        })
    }
    pub(super) fn native_close(&mut self, view: WebviewId) -> CloseOutcome {
        let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.view() == Some(view))
        else {
            return CloseOutcome::Unmapped;
        };
        Self::begin_close(entry)
    }
    fn begin_close(entry: &mut Entry) -> CloseOutcome {
        match entry.phase {
            Phase::Open(native) => {
                let Some(close_seq) = entry.last_close_seq.checked_add(1) else {
                    return CloseOutcome::Exhausted;
                };
                entry.last_close_seq = close_seq;
                entry.phase = Phase::ClosePending(native, close_seq);
                CloseOutcome::Requested {
                    close_seq,
                    fact: entry.owner_live.then_some(OwnedWindowEvent {
                        owner: entry.owner,
                        event: WindowEvent::CloseRequested {
                            window: entry.window,
                            close_seq,
                        },
                    }),
                }
            }
            Phase::ClosePending(_, close_seq) => CloseOutcome::Merged { close_seq },
            Phase::Closing(_) | Phase::Destroyed(_) => CloseOutcome::AlreadyClosing,
            Phase::Opening(_) => CloseOutcome::Unmapped,
        }
    }
    pub(super) fn request_close(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
    ) -> Result<CloseOutcome, WindowError> {
        let entry = self.admitted_entry(owner, window)?;
        if matches!(entry.phase, Phase::Opening(_)) {
            return Err(WindowError::StaleWindow);
        }
        match Self::begin_close(entry) {
            CloseOutcome::Exhausted => Err(WindowError::Exhausted(WindowCounter::CloseSequence)),
            outcome => Ok(outcome),
        }
    }
    // Existing state-table fixtures need no endpoint. Production always supplies
    // the owning router's before-clear retirement callback below.
    #[cfg(all(test, not(keld_core_native_renderer_main)))]
    pub(super) fn close_reply(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
        close_seq: u64,
        verdict: CloseVerdict,
    ) -> Result<CloseOutcome, WindowError> {
        self.close_reply_with_retirement(owner, window, close_seq, verdict, |_| {})
    }
    pub(super) fn close_reply_with_retirement(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
        close_seq: u64,
        verdict: CloseVerdict,
        before_clear: impl FnOnce(&Self),
    ) -> Result<CloseOutcome, WindowError> {
        let entry = self.admitted_entry(owner, window)?;
        let Phase::ClosePending(native, pending) = entry.phase else {
            return Ok(CloseOutcome::Stale);
        };
        if close_seq != pending {
            return Ok(CloseOutcome::Stale);
        }
        match verdict {
            CloseVerdict::Veto => {
                entry.phase = Phase::Open(native);
                Ok(CloseOutcome::Vetoed)
            }
            CloseVerdict::Allow => {
                // All admission predicates passed. The synchronous callback can
                // still resolve this exact registered Entry before its clear.
                before_clear(self);
                let entry = self
                    .entries
                    .get_mut(&window.id.0)
                    .ok_or(WindowError::StaleWindow)?;
                #[cfg(target_os = "macos")]
                entry.retire_renderer_document();
                entry.phase = Phase::Closing(Some(native));
                Ok(CloseOutcome::Teardown(Teardown {
                    window,
                    view: native.view,
                }))
            }
        }
    }
    #[cfg(target_os = "macos")]
    pub(super) fn reserve_original_document_destroy(
        &mut self,
        owner: WindowOwner,
        correlation: CorrelationId,
        window: WindowRef,
        admission: std::sync::Arc<keld_wv::wkwebview::OriginalDocumentDestroyAdmission>,
    ) -> Result<WebviewId, WindowError> {
        use keld_wv::wkwebview::OriginalDocumentDestroyDecision;
        let entry = self.checked_entry(owner, window)?;
        let view = match entry.phase {
            Phase::Open(native) | Phase::ClosePending(native, _) => native.view,
            _ => return Err(WindowError::OriginalDocumentCloseRefused),
        };
        if entry.original_destroy.is_some()
            || entry.pending_destroy_count() >= MAX_PENDING_DESTROYS_PER_WINDOW
            || admission.decision() != OriginalDocumentDestroyDecision::Pending
        {
            return Err(WindowError::OriginalDocumentCloseRefused);
        }
        let entry = self
            .entries
            .get_mut(&window.id.0)
            .ok_or(WindowError::StaleWindow)?;
        entry.original_destroy = Some(PendingOriginalDocumentDestroy {
            call: Some(PendingWindowCall { owner, correlation }),
            admission,
        });
        Ok(view)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn original_document_destroy_decision(
        &mut self,
        view: WebviewId,
        admission: &std::sync::Arc<keld_wv::wkwebview::OriginalDocumentDestroyAdmission>,
    ) -> OriginalDocumentDestroyResolution {
        let Some(entry) = self.entries.values_mut().find(|entry| {
            entry.view() == Some(view)
                && entry
                    .original_destroy
                    .as_ref()
                    .is_some_and(|pending| std::sync::Arc::ptr_eq(&pending.admission, admission))
        }) else {
            return OriginalDocumentDestroyResolution::default();
        };
        entry.reconcile_original_destroy(true)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn cancel_original_document_destroy(
        &mut self,
        window: WindowRef,
    ) -> OriginalDocumentDestroyResolution {
        let Some(entry) = self
            .entries
            .get_mut(&window.id.0)
            .filter(|entry| entry.window == window)
        else {
            return OriginalDocumentDestroyResolution::default();
        };
        if let Some(pending) = &entry.original_destroy {
            // Cancellation may lose to native acceptance; immediately reconcile
            // the actual decision below instead of treating the bool as success.
            let _ = pending.admission.cancel_before_admission();
        }
        entry.reconcile_original_destroy(false)
    }
    /// Caller holds transition/current. Cancel the SAME UI admission before
    /// removing the role; Accepted synchronously becomes nonadoptable Closing.
    #[cfg(target_os = "macos")]
    pub(super) fn cancel_native_admissions(
        &mut self,
        owner: WindowOwner,
        mut abandoned: impl FnMut(AbandonedClose),
    ) {
        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.owner == owner)
        {
            if let Some(pending) = &entry.original_destroy {
                // Native may already have accepted. The immediate reconciliation
                // below, rather than cancellation's bool, decides Closing.
                let _ = pending.admission.cancel_before_admission();
            }
            let outcome = entry.reconcile_original_destroy(false);
            if let Some(close) = outcome.abandoned {
                abandoned(close);
            }
            // Role retirement owns link loss, not a new reply to a successor.
        }
    }
    #[cfg(all(test, not(keld_core_native_renderer_main)))]
    pub(super) fn destroy(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
        correlation: CorrelationId,
        abandoned: impl FnMut(AbandonedClose),
    ) -> Result<CloseOutcome, WindowError> {
        self.destroy_with_retirement(owner, window, correlation, abandoned, |_| {})
    }
    pub(super) fn destroy_with_retirement(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
        correlation: CorrelationId,
        mut abandoned: impl FnMut(AbandonedClose),
        before_clear: impl FnOnce(&Self),
    ) -> Result<CloseOutcome, WindowError> {
        let entry = self.admitted_entry(owner, window)?;
        // Refuse before abandoning an existing close, enqueuing a correlation,
        // or sending an additional UI command. Prior admitted calls retain
        // their eventual correlated replies; this fresh call receives a typed
        // nonterminal ERR over the same authenticated writer.
        if entry.pending_destroy_count() >= MAX_PENDING_DESTROYS_PER_WINDOW {
            return Err(WindowError::PendingDestroyCapacity);
        }
        let native = match entry.phase {
            Phase::Open(native) => Some(native),
            Phase::ClosePending(native, close_seq) => {
                abandoned(AbandonedClose { window, close_seq });
                Some(native)
            }
            Phase::Closing(_) => None,
            Phase::Opening(_) | Phase::Destroyed(_) => return Err(WindowError::StaleWindow),
        };
        if native.is_some() {
            // Refused and already-closing requests cannot retire a route. Keep
            // the old Entry available until its owning producer publishes false.
            before_clear(self);
        }
        let entry = self
            .entries
            .get_mut(&window.id.0)
            .ok_or(WindowError::StaleWindow)?;
        entry
            .destroy_calls
            .push(PendingWindowCall { owner, correlation });
        if let Some(native) = native {
            #[cfg(target_os = "macos")]
            entry.retire_renderer_document();
            entry.phase = Phase::Closing(Some(native));
            Ok(CloseOutcome::Teardown(Teardown {
                window,
                view: native.view,
            }))
        } else {
            Ok(CloseOutcome::AlreadyClosing)
        }
    }
    pub(super) fn native_released(&mut self, view: WebviewId) -> ReleaseOutcome {
        let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.view() == Some(view))
        else {
            return ReleaseOutcome {
                fact: None,
                last_window: None,
                dropped_facts: 0,
                diagnostic: true,
                destroy_calls: Vec::new(),
            };
        };
        if !matches!(entry.phase, Phase::Closing(Some(_))) {
            return ReleaseOutcome {
                fact: None,
                last_window: None,
                dropped_facts: 0,
                diagnostic: true,
                destroy_calls: Vec::new(),
            };
        }
        #[cfg(any(target_os = "macos", test))]
        {
            entry.original_load = None;
        }
        #[cfg(target_os = "macos")]
        {
            entry.retire_renderer_document();
            entry.original_selection = None;
        }
        #[cfg(target_os = "macos")]
        {
            entry.original_destroy = None;
        }
        entry.phase = Phase::Destroyed(Some(view));
        entry.adoptable = false;
        let dropped_facts = entry.dropped;
        entry.dropped = 0;
        let destroy_calls = std::mem::take(&mut entry.destroy_calls);
        let fact = if entry.created && entry.owner_live {
            Some(OwnedWindowEvent {
                owner: entry.owner,
                event: WindowEvent::Destroyed {
                    window: entry.window,
                },
            })
        } else {
            None
        };
        let recipient = fact
            .as_ref()
            .map(|event| event.owner)
            .filter(|_| !self.ending);
        let last_window = if let Some(owner) = recipient {
            if self.has_live_windows() {
                if self.only_opening_windows() {
                    self.deferred_last_window = true;
                }
                None
            } else {
                self.deferred_last_window = false;
                Some(owner)
            }
        } else {
            None
        };
        ReleaseOutcome {
            fact,
            last_window,
            dropped_facts,
            diagnostic: false,
            destroy_calls,
        }
    }
    fn has_live_windows(&self) -> bool {
        self.entries
            .values()
            .any(|entry| !matches!(entry.phase, Phase::Destroyed(_)))
    }
    fn only_opening_windows(&self) -> bool {
        self.entries
            .values()
            .any(|entry| matches!(entry.phase, Phase::Opening(_)))
            && self
                .entries
                .values()
                .all(|entry| matches!(entry.phase, Phase::Opening(_) | Phase::Destroyed(_)))
    }
    pub(super) fn role_lost(
        &mut self,
        owner: WindowOwner,
        mut abandoned: impl FnMut(AbandonedClose),
    ) {
        #[cfg(target_os = "macos")]
        {
            // Qualify before cancellation, pending-call erasure or ClosePending
            // normalization can make an unfinished/closing page appear eligible.
            for entry in self
                .entries
                .values_mut()
                .filter(|entry| entry.owner == owner && entry.owner_live)
            {
                if entry.recoverable_original_document().is_none() {
                    entry.retire_renderer_document();
                }
            }
            self.cancel_native_admissions(owner, &mut abandoned);
        }
        let mut affected = false;
        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.owner == owner && entry.owner_live)
        {
            affected = true;
            entry.owner_live = false;
            entry.adoptable = false;
            entry.destroy_calls.clear();
            #[cfg(target_os = "macos")]
            {
                entry.renderer.bound_owner = None;
            }
            #[cfg(any(target_os = "macos", test))]
            {
                entry.original_load = None;
            }
            match entry.phase {
                Phase::Opening(_) => entry.phase = Phase::Closing(None),
                Phase::ClosePending(native, close_seq) => {
                    abandoned(AbandonedClose {
                        window: entry.window,
                        close_seq,
                    });
                    entry.phase = Phase::Open(native);
                }
                _ => {}
            }
        }
        if affected {
            self.deferred_last_window = false;
        }
    }
    /// Parent invokes this once on an accepted primary Subscribe, before replay.
    /// Same-owner repeats cannot replay/adopt again; the parent owns subscription state.
    pub(super) fn subscribe_successor(&mut self, owner: WindowOwner) {
        for entry in self.entries.values_mut().filter(|entry| {
            !entry.owner_live && matches!(entry.phase, Phase::Open(_) | Phase::ClosePending(_, _))
        }) {
            entry.owner = owner;
            entry.owner_live = true;
            // The approved mark-at-Subscribe rule applies to Open survivors.
            // A gap close still transfers/replays, without gaining this mark.
            entry.adoptable = matches!(entry.phase, Phase::Open(_));
        }
    }
    pub(super) fn replay(&self, owner: WindowOwner) -> impl Iterator<Item = WindowEvent> + '_ {
        self.entries
            .values()
            .filter_map(move |entry| match entry.phase {
                Phase::Open(native) | Phase::ClosePending(native, _)
                    if entry.owner_live && entry.owner == owner =>
                {
                    Some(WindowEvent::Created {
                        window: entry.window,
                        state: native.state,
                        pending_close: match entry.phase {
                            Phase::ClosePending(_, close_seq) => Some(close_seq),
                            _ => None,
                        },
                    })
                }
                _ => None,
            })
    }
    /// Parent reports this once after the UI fact source has physically drained.
    pub(super) fn take_destroyed_fact_drops(&mut self) -> u64 {
        std::mem::take(&mut self.destroyed_fact_drops)
    }
    /// Existing writer/command owners consume each fixed effect synchronously.
    pub(super) fn session_end(&mut self, mut emit: impl FnMut(SessionEffect)) {
        self.ending = true;
        self.ready = false;
        self.deferred_last_window = false;
        for entry in self.entries.values_mut() {
            #[cfg(target_os = "macos")]
            {
                if let Some(pending) = &entry.original_destroy {
                    // Preserve an already-accepted native close by immediately
                    // reconciling its decision even when cancellation loses.
                    let _ = pending.admission.cancel_before_admission();
                }
                let outcome = entry.reconcile_original_destroy(false);
                if let Some(close) = outcome.abandoned {
                    emit(SessionEffect::CloseAbandoned(close));
                }
            }
            entry.adoptable = false;
            #[cfg(target_os = "macos")]
            entry.retire_renderer_document();
            #[cfg(any(target_os = "macos", test))]
            if let Some(pending) = entry.original_load.take() {
                emit(SessionEffect::OriginalLoadInterrupted(pending.context.call));
            }
            match entry.phase {
                Phase::Opening(call) => {
                    entry.phase = Phase::Closing(None);
                    emit(SessionEffect::CreateInterrupted(call));
                }
                Phase::Open(native) | Phase::ClosePending(native, _) => {
                    if let Phase::ClosePending(_, close_seq) = entry.phase {
                        emit(SessionEffect::CloseAbandoned(AbandonedClose {
                            window: entry.window,
                            close_seq,
                        }));
                    }
                    entry.phase = Phase::Closing(Some(native));
                    emit(SessionEffect::Teardown(Teardown {
                        window: entry.window,
                        view: native.view,
                    }));
                }
                Phase::Closing(_) | Phase::Destroyed(_) => {}
            }
        }
    }
}

#[cfg(all(test, not(keld_core_native_renderer_main)))]
mod tests;
