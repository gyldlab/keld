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
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowError {
    StaleWindow,
    Exhausted(WindowCounter),
    NotReady,
    WrongOwner,
}
impl WindowError {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::StaleWindow => "KELD-CORE-038",
            Self::Exhausted(_) => "KELD-CORE-039",
            Self::NotReady => "KELD-CORE-040",
            Self::WrongOwner => "KELD-CORE-043",
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
            Self::NotReady => {
                "window registry is not serviceable; wait for Ready on a live app session"
            }
            Self::WrongOwner => {
                "window belongs to another or retired role generation; use the successor's subscribed replay"
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FactOutcome {
    Event(OwnedWindowEvent),
    Coalesced,
    Suppressed,
    Unmapped,
    Dropped,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
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
    phase: Phase,
}
impl Entry {
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
        if let Some(entry) = self.entries.values_mut().find(|entry| {
            entry.owner_live
                && entry.owner == owner
                && entry.adoptable
                && matches!(entry.phase, Phase::Open(_) | Phase::ClosePending(_, _))
        }) {
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
        })
    }
    fn require_ready(&self) -> Result<(), WindowError> {
        if self.ready && !self.ending {
            Ok(())
        } else {
            Err(WindowError::NotReady)
        }
    }
    fn admitted_entry(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
    ) -> Result<&mut Entry, WindowError> {
        self.require_ready()?;
        let entry = self
            .entries
            .get_mut(&window.id.0)
            .filter(|entry| entry.window == window && !matches!(entry.phase, Phase::Destroyed(_)))
            .ok_or(WindowError::StaleWindow)?;
        if !entry.owner_live || entry.owner != owner {
            return Err(WindowError::WrongOwner);
        }
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
    pub(super) fn close_reply(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
        close_seq: u64,
        verdict: CloseVerdict,
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
                entry.phase = Phase::Closing(Some(native));
                Ok(CloseOutcome::Teardown(Teardown {
                    window,
                    view: native.view,
                }))
            }
        }
    }
    pub(super) fn destroy(
        &mut self,
        owner: WindowOwner,
        window: WindowRef,
        correlation: CorrelationId,
        mut abandoned: impl FnMut(AbandonedClose),
    ) -> Result<CloseOutcome, WindowError> {
        let entry = self.admitted_entry(owner, window)?;
        let native = match entry.phase {
            Phase::Open(native) => Some(native),
            Phase::ClosePending(native, close_seq) => {
                abandoned(AbandonedClose { window, close_seq });
                Some(native)
            }
            Phase::Closing(_) => None,
            Phase::Opening(_) | Phase::Destroyed(_) => return Err(WindowError::StaleWindow),
        };
        entry
            .destroy_calls
            .push(PendingWindowCall { owner, correlation });
        if let Some(native) = native {
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
        let recipient = fact.map(|event| event.owner).filter(|_| !self.ending);
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
            entry.adoptable = false;
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
