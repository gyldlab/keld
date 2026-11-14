//! One prepared observation and optional synthetic turn around the actual wrapper.
#![forbid(unsafe_code)]
use super::{NavigationState, OriginalNavigation, Probe};
use crate::renderer_contract::CheckedNativeDocumentLease;
use std::cell::RefCell;
use std::rc::Rc;

/// Fixed observation of the actual typed carrier, captured before admission.
#[derive(Clone, Copy)]
#[repr(u8)]
pub(in crate::webview2) enum PostCarrier {
    Event,
    Control,
    RouteStateReady,
    RouteStateNotReady,
    Invocation,
    Numeric,
}
impl PostCarrier {
    pub(in crate::webview2) const fn control(
        carrier: &crate::wv_link::HostRendererMessage<'_>,
    ) -> Self {
        match carrier {
            crate::wv_link::HostRendererMessage::RouteState { ready: true } => {
                Self::RouteStateReady
            }
            crate::wv_link::HostRendererMessage::RouteState { ready: false } => {
                Self::RouteStateNotReady
            }
            _ => Self::Control,
        }
    }
    const fn kind(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Control => "control",
            Self::RouteStateReady | Self::RouteStateNotReady => "route-state",
            Self::Invocation => "invocation",
            Self::Numeric => "numeric",
        }
    }
    const fn ready(self) -> Option<bool> {
        match self {
            Self::RouteStateReady => Some(true),
            Self::RouteStateNotReady => Some(false),
            _ => None,
        }
    }
}
const _: () = assert!(std::mem::size_of::<PostCarrier>() == 1);

#[derive(Clone, Copy)]
pub(in crate::webview2) struct EventStorage {
    pub(in crate::webview2) delivery: u32,
    pub(in crate::webview2) payload_id: usize,
    pub(in crate::webview2) payload_capacity: usize,
    pub(in crate::webview2) wide_id: usize,
    pub(in crate::webview2) wide_capacity: usize,
}
impl EventStorage {
    fn value(self) -> serde_json::Value {
        serde_json::json!({
            "delivery":self.delivery,"payload_id":self.payload_id,
            "payload_capacity":self.payload_capacity,"wide_id":self.wide_id,
            "wide_capacity":self.wide_capacity,
        })
    }
}

struct SyntheticTurn {
    state: Rc<RefCell<OriginalNavigation>>,
    probe: Rc<Probe>,
    active: bool,
}
impl SyntheticTurn {
    fn leave(&mut self) -> Option<(u64, NavigationState)> {
        if !self.active {
            return None;
        }
        self.active = false;
        let Ok(mut state) = self.state.try_borrow_mut() else {
            self.probe.invalid.set(true);
            return None; // Original callback hold remains; no manufactured quiescence.
        };
        state.leave_navigation_callback();
        let snapshot = state.post_observation();
        drop(state);
        Some((self.probe.next(), snapshot))
    }
}
impl Drop for SyntheticTurn {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}

pub(in crate::webview2) struct PostObservation {
    probe: Rc<Probe>,
    target: NavigationState,
    storage: Option<EventStorage>,
    carrier: PostCarrier,
    epoch: [u8; 32],
    role_live_before: Option<bool>,
    calls_before: u64,
    attempt_sequence: u64,
    synthetic: Option<SyntheticTurn>,
    synthetic_enter: Option<(u64, NavigationState)>,
    gate: Option<(u64, NavigationState)>,
    entry_sequence: Option<u64>,
    return_sequence: Option<u64>,
    unwind_sequence: Option<u64>,
}
impl PostObservation {
    /// All Rc/path/metadata preparation precedes the canonical final admission.
    pub(in crate::webview2) fn prepare(
        state: &Rc<RefCell<OriginalNavigation>>,
        document: &CheckedNativeDocumentLease,
        storage: Option<EventStorage>,
        carrier: PostCarrier,
        epoch: &[u8; 32],
        role_live_before: Option<bool>,
    ) -> Option<Self> {
        let navigation = state.try_borrow().ok()?;
        let probe = Rc::clone(navigation.post_observer());
        let mut target = navigation.post_observation();
        let stamp = document.stamp().0;
        target.view = stamp.view.0;
        target.generation = Some(stamp.generation.get());
        target.native_commit = Some(stamp.native_commit.0.get());
        drop(navigation);
        let mut result = Self {
            calls_before: probe.calls.get(),
            attempt_sequence: probe.next(),
            probe,
            target,
            storage,
            carrier,
            epoch: *epoch,
            role_live_before,
            synthetic: None,
            synthetic_enter: None,
            gate: None,
            entry_sequence: None,
            return_sequence: None,
            unwind_sequence: None,
        };
        if result.storage.is_some() && !result.probe.synthetic_used.get() {
            let armed = result
                .probe
                .synthetic_arm
                .get()
                .map(|path| super::super::native_hold::control_file_exists(path));
            match armed {
                Some(Ok(true)) => result.enter_synthetic(state),
                Some(Err(_)) => result.probe.invalid.set(true),
                Some(Ok(false)) | None => {}
            }
        }
        Some(result)
    }
    fn enter_synthetic(&mut self, state: &Rc<RefCell<OriginalNavigation>>) {
        let Ok(mut navigation) = state.try_borrow_mut() else {
            self.probe.invalid.set(true);
            return;
        };
        if !navigation.callbacks_inactive() || navigation.posts_held() {
            self.probe.invalid.set(true);
            return;
        }
        navigation.enter_navigation_callback();
        let snapshot = navigation.post_observation();
        drop(navigation);
        self.probe.synthetic_used.set(true);
        self.synthetic_enter = Some((self.probe.next(), snapshot));
        self.synthetic = Some(SyntheticTurn {
            state: Rc::clone(state),
            probe: Rc::clone(&self.probe),
            active: true,
        });
    }
    /// This scalar read is before `begin_post_with`, under its existing borrow.
    pub(in crate::webview2) fn final_gate(&mut self, state: NavigationState) {
        self.gate = Some((self.probe.next(), state));
    }
    /// Called immediately before the sole actual API; no allocation or borrow.
    pub(in crate::webview2) fn api_enter(&mut self) {
        self.entry_sequence = Some(self.probe.enter_call());
    }
    /// Called immediately after the actual API returns, before leaving its closure.
    pub(in crate::webview2) fn api_return(&mut self) {
        self.return_sequence = Some(self.probe.return_call());
    }
    pub(in crate::webview2) fn api_unwound(&mut self) {
        self.unwind_sequence = Some(self.probe.return_call());
    }
    pub(in crate::webview2) fn complete(mut self, result: &'static str, native_ok: Option<bool>) {
        let decision_sequence = self.probe.next();
        let synthetic_leave = self.synthetic.as_mut().and_then(SyntheticTurn::leave);
        drop(self.synthetic.take());
        let pair = |value: Option<(u64, NavigationState)>| {
            value.map(
                |(sequence, state)| serde_json::json!({"sequence":sequence,"state":state.value()}),
            )
        };
        self.probe.emit(&serde_json::json!({
            "phase":"post-attempt","attempt_sequence":self.attempt_sequence,
            "decision_sequence":decision_sequence,"target":self.target.value(),
            "event_storage":self.storage.map(EventStorage::value),"role_live_before":self.role_live_before,
            "carrier_kind":self.carrier.kind(),"route_ready":self.carrier.ready(),
            "document_epoch":std::str::from_utf8(&self.epoch).ok(),
            "calls_before":self.calls_before,"calls_after":self.probe.calls.get(),
            "synthetic_enter":pair(self.synthetic_enter),"synthetic_leave":pair(synthetic_leave),
            "final_gate":pair(self.gate),"entry_sequence":self.entry_sequence,
            "return_sequence":self.return_sequence,"unwind_sequence":self.unwind_sequence,
            "result":result,"native_ok":native_ok,
            "in_call_records":self.probe.in_call_records.get(),
            "in_call_max_depth":self.probe.in_call_max_depth.get(),
            "first_in_call_sequence":self.probe.first_in_call_sequence.get(),
            "last_in_call_sequence":self.probe.last_in_call_sequence.get(),
            "call_active":self.probe.active.get(),"invalid":self.probe.invalid.get(),
        }));
    }
}
