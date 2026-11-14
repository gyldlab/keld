//! The existing Host failure retains actual Windows owners through fatal exit.
//! No borrowed release witness, extra native map, or detached ownership is minted.
use super::{
    AdmittedStageGraph, AppWindowCommand, AppWindowEvent, Arc, DirectPrimaryOwner,
    DirectSessionEnd, GuardSnapshot, HostAppError, JoinHandle, PrimaryRouter, Receiver,
    RendererBridgeEndpoint, RendererDispatch, Sender, WebView2Engine, append_app_cleanup,
    dialog_session,
};

pub(super) struct RetainedWindowsSession {
    pub(super) _engine: WebView2Engine,
    pub(super) _dialog_main: Option<dialog_session::DialogMainOwner>,
    pub(super) _renderer_dispatch: Option<RendererDispatch>,
    pub(super) _renderer_endpoint: Option<RendererBridgeEndpoint>,
    pub(super) _router: Option<PrimaryRouter>,
    pub(super) _primary_owner: Option<DirectPrimaryOwner>,
    pub(super) _coordinator: Option<JoinHandle<Result<(), HostAppError>>>,
    pub(super) _commands: Option<Sender<AppWindowCommand>>,
    pub(super) _events: Option<Sender<AppWindowEvent>>,
    pub(super) _lease_errors: Option<Receiver<HostAppError>>,
    pub(super) _families: keld_runtime::windows_job::WindowsPrimaryFamilies,
    pub(super) installed_package: Option<Box<keld_update::ActivePackageSelection>>,
    pub(super) original_graph: Option<Arc<AdmittedStageGraph>>,
    pub(super) guard: Option<GuardSnapshot>,
}
impl RetainedWindowsSession {
    pub(super) fn unstarted(
        engine: WebView2Engine,
        families: keld_runtime::windows_job::WindowsPrimaryFamilies,
    ) -> Self {
        Self {
            _engine: engine,
            _dialog_main: None,
            _renderer_dispatch: None,
            _renderer_endpoint: None,
            _router: None,
            _primary_owner: None,
            _coordinator: None,
            _commands: None,
            _events: None,
            _lease_errors: None,
            _families: families,
            installed_package: None,
            original_graph: None,
            guard: None,
        }
    }
}

/// An unproved borrowed retirement returns its same actual owner to the caller.
/// The caller must move retained custody into `HostRunFailure` before returning.
#[must_use = "retain the unproved native owner through the fatal Host result"]
pub(super) struct PendingEngineFinish<T> {
    pub(super) result: Result<DirectSessionEnd, HostAppError>,
    pub(super) retained: Option<T>,
}

/// The existing pending preparation is retired only after ordinary startup
/// cleanup. Borrowing it prevents a failed witness from consuming/dropping it.
pub(super) fn finish_pending_direct_engine<T>(
    mut prepared: Option<T>,
    result: Result<DirectSessionEnd, HostAppError>,
    retire: impl FnOnce(&mut T) -> Result<(), HostAppError>,
) -> PendingEngineFinish<T> {
    let cleanup = prepared.as_mut().map_or(Ok(()), retire);
    let retained = if cleanup.is_err() {
        prepared.take()
    } else {
        None
    };
    let result = match (result, cleanup) {
        (Ok(end), Ok(())) => Ok(end),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(primary), cleanup) => Err(append_app_cleanup(primary, [cleanup])),
    };
    // A proved unstarted retirement may now drop its original owner. Failure's
    // exact same T moved to retained above, and is never reconstructed or leaked.
    drop(prepared);
    PendingEngineFinish { result, retained }
}
