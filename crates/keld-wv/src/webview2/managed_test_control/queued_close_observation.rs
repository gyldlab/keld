//! X10 passive observations at existing owner boundaries; never native credit.
#![forbid(unsafe_code)]

use crate::renderer_bridge::RendererBridgeOutcome;
use crate::webview2::app_wake_bridge::AppWakeDelivery;
use std::sync::Mutex;

fn emit(value: &serde_json::Value) {
    static COUNT: Mutex<usize> = Mutex::new(0);
    if !super::super::t2_test_observation::enabled() {
        return;
    }
    {
        let Ok(mut count) = COUNT.lock() else { return };
        if *count >= 128 {
            return;
        }
        *count += 1;
    }
    eprintln!("KELD_X10_NATIVE_OWNER {value}");
}

#[derive(Clone, Copy)]
pub(in crate::webview2) struct EventSnapshot {
    pub(in crate::webview2) view: u32,
    pub(in crate::webview2) navigation: u32,
    pub(in crate::webview2) message: u32,
    pub(in crate::webview2) post_phase: &'static str,
    pub(in crate::webview2) retirement: Option<&'static str>,
    pub(in crate::webview2) retirement_sent: bool,
    pub(in crate::webview2) released: bool,
    pub(in crate::webview2) actual_closed: Option<bool>,
}
impl EventSnapshot {
    pub(in crate::webview2) fn emit(self, phase: &'static str, native_ok: Option<bool>) {
        emit(&serde_json::json!({
            "phase": phase, "view": self.view, "navigation": self.navigation,
            "message": self.message, "post_phase": self.post_phase,
            "retirement": self.retirement, "retirement_sent": self.retirement_sent,
            "released": self.released, "actual_closed": self.actual_closed,
            "native_ok": native_ok,
        }));
    }
}

pub(in crate::webview2) struct Incoming {
    kind: &'static str,
    view: u32,
    navigation: u64,
    message: u32,
    ready: Option<bool>,
    hold_active: bool,
}
impl Incoming {
    /// Scalars only under `BridgeInput`; emit only after the guard is dropped.
    pub(in crate::webview2) fn capture(
        delivery: &AppWakeDelivery,
        hold_active: bool,
    ) -> Option<Self> {
        let AppWakeDelivery::Renderer(Some(outcome)) = delivery else {
            return None;
        };
        let (kind, webview, navigation, message, ready) = match outcome {
            RendererBridgeOutcome::Event {
                webview,
                navigation,
                request,
                ..
            } => ("event", *webview, *navigation, *request, None),
            RendererBridgeOutcome::EventReleased {
                webview,
                navigation,
                request,
            } => ("event-released", *webview, *navigation, *request, None),
            RendererBridgeOutcome::RouteState {
                webview,
                navigation,
                ready,
                ..
            } => ("route-state", *webview, *navigation, 0, Some(*ready)),
            _ => return None,
        };
        Some(Self {
            kind,
            view: webview.0,
            navigation,
            message,
            ready,
            hold_active,
        })
    }

    pub(in crate::webview2) fn emit(self) {
        emit(&serde_json::json!({
            "phase": "ui-delivery", "kind": self.kind, "view": self.view,
            "navigation": self.navigation, "message": self.message,
            "ready": self.ready, "hold_active": self.hold_active,
        }));
    }
}

pub(in crate::webview2) fn retirement_wake_attempt(view: u32) {
    emit(&serde_json::json!({
        "phase": "bridge-retire-wake-attempt", "view": view,
    }));
}

pub(in crate::webview2) fn controller_close_wake_attempt(view: u32) {
    emit(&serde_json::json!({
        "phase": "bridge-close-wake-attempt", "view": view,
    }));
}

pub(in crate::webview2) fn controller_closed(view: u32, native_quiescent: bool) {
    emit(&serde_json::json!({
        "phase": "controller-closed", "view": view, "actual_closed": true,
        "native_storage_quiescent": native_quiescent,
    }));
}

pub(in crate::webview2) fn route_dropped(view: u32, navigation: u64, ready: bool) {
    emit(&serde_json::json!({
        "phase": "route-storage-dropped", "view": view, "navigation": navigation,
        "ready": ready, "reason": "exact-document-no-longer-current",
    }));
}

pub(in crate::webview2) fn view_drain(view: u32, drained: bool) {
    static LAST: Mutex<Option<(u32, bool)>> = Mutex::new(None);
    {
        let Ok(mut last) = LAST.lock() else { return };
        if *last == Some((view, drained)) {
            return;
        }
        *last = Some((view, drained));
    }
    emit(&serde_json::json!({
        "phase": "view-drain", "view": view, "drained": drained,
    }));
}
