//! Closed X11 test stimulus on the existing native CONTROL producer.
//! No new queue, payload copy, post fence, callback or grant.
#![forbid(unsafe_code)]

mod observation;
pub(super) use observation::PressureWatch;

use super::publication::NativeControlPublication;
use super::queued_close_observation::ScopeSnapshot;
use super::{
    Arc, CompatSession, Direction, HostAppError, NativeRouteKey, PrimaryCallerContext,
    PrimaryRouterHandle, RendererRoute, WebviewId, app_detail, with_primary_caller_locked,
};
use crate::app_session::primary_writer::RetainedWrite;
use crate::app_session::window_router::WindowSession;
use keld_ipc::CorrelationId;
use std::ffi::OsStr;
use std::sync::atomic::Ordering;
use std::time::Instant;

pub(in crate::app_session) const ENV: &str = "KELD_TEST_X11_NATIVE_PRESSURE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PressureControl {
    Disabled,
    Armed,
    Fired,
    Invalid,
}

impl PressureControl {
    pub(super) fn capture() -> Self {
        Self::parse(std::env::var_os(ENV).as_deref())
    }

    fn parse(value: Option<&OsStr>) -> Self {
        match value {
            None => Self::Disabled,
            Some(value) if value == OsStr::new("four-route-ready-v1") => Self::Armed,
            Some(_) => Self::Invalid,
        }
    }

    fn active(self) -> Result<bool, HostAppError> {
        match self {
            Self::Invalid => Err(failure("invalid closed pressure selector")),
            Self::Armed => Ok(true),
            Self::Disabled | Self::Fired => Ok(false),
        }
    }

    fn take(&mut self) -> Result<bool, HostAppError> {
        if !self.active()? {
            return Ok(false);
        }
        *self = Self::Fired;
        Ok(true)
    }
}

fn failure(detail: &'static str) -> HostAppError {
    app_detail("X11 existing native CONTROL pressure", detail)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct SourceKey {
    view: u32,
    navigation: u32,
    message: u32,
}

impl From<NativeRouteKey> for SourceKey {
    fn from(key: NativeRouteKey) -> Self {
        Self {
            view: key.webview,
            navigation: key.navigation,
            message: key.message,
        }
    }
}

struct Candidate {
    route: RendererRoute,
    main: SourceKey,
    renderer: SourceKey,
    main_native_id: usize,
    renderer_retained: usize,
}

fn candidate(
    state: &CompatSession,
    windows: &WindowSession,
    caller: &PrimaryCallerContext,
    correlation: CorrelationId,
) -> Result<Option<Candidate>, HostAppError> {
    let Some(main) = state.ledger.data[Direction::MainToRenderer.index()].as_ref() else {
        return Ok(None);
    };
    if main.key.message != correlation.0 || !main.owned_by(caller) {
        return Ok(None);
    }
    let route = windows
        .renderer_route_for_caller(
            caller,
            WebviewId(main.key.webview),
            Some(main.key.navigation),
        )
        .map_err(|_| failure("original admitted Event route disappeared"))?;
    if main.native_window != Some(route.window)
        || !main
            .document
            .as_ref()
            .is_some_and(|document| document.is_current() && document.same_binding(&route.document))
        || main.consumer_done
        || main.native_done
    {
        return Err(failure(
            "same original Event is no longer held before intake",
        ));
    }
    let storage = main
        .native_storage
        .as_ref()
        .filter(|storage| !storage.load(Ordering::Acquire))
        .ok_or_else(|| failure("original native Event backing is absent"))?;
    let renderer = state.ledger.data[Direction::RendererToMain.index()]
        .as_ref()
        .ok_or_else(|| failure("actual renderer VALUE source is absent"))?;
    if !renderer.owned_by(caller)
        || renderer.native_window != Some(route.window)
        || !renderer
            .document
            .as_ref()
            .is_some_and(|document| document.is_current() && document.same_binding(&route.document))
        || renderer.consumer_done
    {
        return Err(failure(
            "renderer VALUE is not the same original caller/document",
        ));
    }
    let Some(RetainedWrite::Pending(receipt)) = renderer.primary_write.as_ref() else {
        return Err(failure("actual original physical receipt is not pending"));
    };
    if receipt
        .poll()
        .map_err(|error| error.into_host_error("X11 original VALUE observation"))?
        .is_some()
    {
        return Err(failure("actual original VALUE already physically returned"));
    }
    if state.ledger.controls.iter().flatten().count() != 0 {
        return Err(failure(
            "temporary CALL or another CONTROL is still retained",
        ));
    }
    Ok(Some(Candidate {
        route,
        main: main.key.into(),
        renderer: renderer.key.into(),
        main_native_id: Arc::as_ptr(storage) as usize,
        renderer_retained: renderer.retained,
    }))
}

#[derive(serde::Serialize)]
struct PressureRecord {
    phase: &'static str,
    fifth_full: bool,
    window: u32,
    incarnation: u64,
    caller_allocation: usize,
    main: SourceKey,
    renderer: SourceKey,
    main_native_id: usize,
    renderer_retained: usize,
    control_ids: [usize; 4],
    scope: ScopeSnapshot,
}

fn record(
    state: &CompatSession,
    selected: &Candidate,
    caller: &PrimaryCallerContext,
) -> Result<PressureRecord, HostAppError> {
    let mut ids = [0; 4];
    for (slot, item) in state.ledger.controls.iter().enumerate() {
        let control = item
            .as_ref()
            .ok_or_else(|| failure("one of four original CONTROL slots disappeared"))?;
        let storage = control
            .native_release
            .as_ref()
            .ok_or_else(|| failure("CONTROL has no actual native backing token"))?;
        if !control.owned_by(caller)
            || control.native_window != Some(selected.route.window)
            || !control.application_done
            || control.primary_write.is_some()
            || storage.load(Ordering::Acquire)
        {
            return Err(failure(
                "CONTROL no longer belongs to its queued native original",
            ));
        }
        let id = Arc::as_ptr(storage) as usize;
        if ids[..slot].contains(&id) {
            return Err(failure(
                "CONTROL records unexpectedly alias one native token",
            ));
        }
        ids[slot] = id;
    }
    Ok(PressureRecord {
        phase: "filled",
        fifth_full: true,
        window: selected.route.window.id.0,
        incarnation: selected.route.window.incarnation.0,
        caller_allocation: Arc::as_ptr(&caller.reader_stop) as usize,
        main: selected.main,
        renderer: selected.renderer,
        main_native_id: selected.main_native_id,
        renderer_retained: selected.renderer_retained,
        control_ids: ids,
        scope: state.queued_close_snapshot(selected.route.window),
    })
}

impl PrimaryRouterHandle {
    /// Called only after the genuine CALL's temporary CONTROL release and after
    /// all earlier publication guards ended. No physical post is entered here.
    pub(in crate::app_session) fn x11_pressure_after_event_release(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        deadline: Instant,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(());
        };
        if !compat
            .lock()
            .map_err(|_| failure("pressure control owner is poisoned"))?
            .x11_pressure
            .active()?
        {
            return Ok(());
        }
        let mut observed = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let windows = self
                .windows
                .as_ref()
                .ok_or_else(|| failure("Registry is absent"))?
                .lock()
                .map_err(|_| failure("Registry is poisoned"))?;
            let mut state = compat
                .lock()
                .map_err(|_| failure("Compat owner is poisoned"))?;
            if !state.x11_pressure.active()? {
                return Ok(());
            }
            let Some(selected) = candidate(&state, &windows, caller, correlation)? else {
                return Ok(());
            };
            if !state.subscribed_to(caller) || !state.x11_pressure.take()? {
                return Err(failure(
                    "original subscription or once-only selection changed",
                ));
            }
            drop(state);
            for _ in 0..4 {
                if Instant::now() >= deadline {
                    return Err(failure("original CALL deadline expired"));
                }
                if !matches!(
                    self.compat_route_ready_after_bound_locked(caller, &selected.route)?,
                    NativeControlPublication::Enqueued
                ) {
                    return Err(failure(
                        "existing canonical route producer did not admit four records",
                    ));
                }
            }
            if Instant::now() >= deadline {
                return Err(failure(
                    "original CALL deadline expired before capacity challenge",
                ));
            }
            if !matches!(
                self.compat_route_ready_after_bound_locked(caller, &selected.route)?,
                NativeControlPublication::Full
            ) {
                return Err(failure("fifth original native CONTROL did not report Full"));
            }
            let mut state = compat
                .lock()
                .map_err(|_| failure("final census owner is poisoned"))?;
            let filled = record(&state, &selected, caller)?;
            state.x11_watch = Some(PressureWatch::new(&selected, &filled));
            observed = Some(filled);
            Ok(())
        })?;
        if let Some(observed) = observed {
            // Only fixed scalars leave the guards. This log grants no authority.
            eprintln!(
                "KELD_X11_NATIVE_PRESSURE {}",
                serde_json::to_string(&observed)
                    .map_err(|_| failure("fixed pressure census could not be serialized"))?
            );
        }
        self.observe_x11_pressure_progress("filled");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{OsStr, PressureControl};

    #[test]
    fn closed_selection_is_captured_without_a_live_environment_dependency() {
        assert_eq!(PressureControl::parse(None), PressureControl::Disabled);
        assert_eq!(
            PressureControl::parse(Some(OsStr::new(""))),
            PressureControl::Invalid
        );
        assert_eq!(
            PressureControl::parse(Some(OsStr::new("four-route-ready-v1"))),
            PressureControl::Armed
        );
        assert_eq!(
            PressureControl::parse(Some(OsStr::new("four-route-ready-v2"))),
            PressureControl::Invalid
        );
    }

    #[test]
    fn selected_pressure_has_one_attempt_and_invalid_never_silently_disables() {
        let mut selected = PressureControl::Armed;
        assert!(selected.take().expect("one selected attempt"));
        assert!(!selected.take().expect("no retry or extra records"));
        assert_eq!(selected, PressureControl::Fired);
        assert!(PressureControl::Invalid.take().is_err());
    }
}
