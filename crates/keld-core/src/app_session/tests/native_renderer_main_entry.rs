//! Joint real-renderer/native recovery on the Core acceptance process main thread.
use super::native_generation_fixture::{
    ActorTurns, Inputs, NativeWorkerSetup, SENTINEL, assert_owned_groups, native_worker_setup,
    next_bound,
};
use super::native_observer_sensor as sensor;
use super::native_observer_setup::{NativeSignals, OBSERVATION_BOUND};
use super::{
    AppWindowCommand, GuardianOwner, GuardianOwnerHandle, NavTarget, PrimaryRouter,
    PrimaryRouterHandle, RendererBridgeEndpoint, RendererBridgeRequest, SessionShutdownState,
    WebviewSpec, WkWebViewEngine, coordinate_window_events, join_renderer_dispatch,
    start_renderer_dispatch,
};
use keld_ipc::codec::decode;
use keld_ipc::echo::EchoRequest;
use serde_json::{Value, json};
use std::fs;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Instant;

#[path = "native_renderer_observation.rs"]
mod observation;
use observation::{JointInputs, PageReports, assert_document};

use crate::NATIVE_RENDERER_CASE as CASE;

pub(super) fn run_main() {
    run_joint();
    println!("test {CASE} ... ok\ntest result: ok. 1 passed; 0 failed;");
}

/// The fixture receives a host-minted request and forwards that exact value.
/// It observes identity/correlation; the existing dispatch loop alone serves it.
#[derive(Debug)]
struct RequestFact {
    webview: u32,
    navigation: u64,
    request: u32,
    nonce: Value,
}

fn forward_request(
    incoming: &Receiver<RendererBridgeRequest>,
    dispatch: &SyncSender<RendererBridgeRequest>,
    expected_action: &str,
    expected_count: u32,
) -> RequestFact {
    let request = incoming
        .recv_timeout(super::GUARDIAN_OWNER_REPLY_DEADLINE)
        .expect("actual renderer-origin request");
    assert_eq!(request.channel(), keld_ipc::channel_table::ECHO.wire_id());
    let echo: EchoRequest = decode(request.payload()).expect("owning Echo codec");
    let control: Value =
        serde_json::from_str(&echo.message).expect("closed public fixture request");
    assert_eq!(control["action"], expected_action);
    assert_eq!(echo.count, expected_count);
    let nonce = control["nonce"]
        .as_str()
        .expect("actual page-created request nonce");
    assert_eq!(nonce.len(), 32);
    assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let fact = RequestFact {
        webview: request.webview().0,
        navigation: request.navigation(),
        request: request.request(),
        nonce: control["nonce"].clone(),
    };
    dispatch
        .send(request)
        .expect("forward exact admitted request to existing dispatcher");
    fact
}

struct WakeOnControllerUnwind(mpsc::Sender<AppWindowCommand>);
impl Drop for WakeOnControllerUnwind {
    fn drop(&mut self) {
        if thread::panicking() {
            let _ = self.0.send(AppWindowCommand::Fatal);
        }
    }
}

fn assert_native_held(
    signals: &NativeSignals,
    session: &super::FsDispatchSession,
    target: &std::path::Path,
) {
    assert!(
        matches!(signals.returned.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "Native returned before renderer terminal/release"
    );
    let state = session.state.lock().expect("Native accounting");
    assert_eq!(state.in_flight, 1);
    assert_eq!(state.prepared_attempt, Some(1));
    drop(state);
    assert_eq!(sensor::object_handle_count(target), 1);
    assert_eq!(fs::read(target).expect("held effect sentinel"), SENTINEL);
}

fn return_and_observe_baseline(
    signals: &mut NativeSignals,
    session: &super::FsDispatchSession,
    inputs: &Inputs,
) {
    signals
        .native_release
        .take()
        .expect("one actual Native release")
        .send(())
        .expect("held Native owner");
    let returned = signals
        .returned
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Native normal return");
    assert!(!returned.fixture_fault);
    assert_eq!((returned.request_entries, returned.request_returns), (1, 1));
    let deadline = Instant::now() + OBSERVATION_BOUND;
    loop {
        let state = session.state.lock().expect("retired worker metadata");
        let retired = state.prepared_attempt.is_none() && state.in_flight == 0;
        drop(state);
        if retired {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual old broker retirement absent"
        );
        thread::yield_now();
    }
    assert_eq!(sensor::object_handle_count(&inputs.target), 0);
    assert_eq!(
        sensor::directory_handle_count(&inputs.allowed_root),
        0,
        "old broker root survives native return/retirement"
    );
    assert_eq!(
        fs::read(&inputs.target).expect("retired effect sentinel"),
        SENTINEL
    );
    println!(
        "KELD_KEL143_JOINT_BROKER_BASELINE entries=1 returns=1 roots=0 target_handles=0 before_fresh_admission=true"
    );
}

fn assert_pending_terminal(initial: &Value, settled: &Value, fact: &RequestFact) {
    assert_document(settled, initial);
    assert_eq!(
        settled["requests"]
            .as_array()
            .expect("actual pending request")
            .len(),
        1
    );
    assert_eq!(settled["requests"][0]["nonce"], fact.nonce);
    assert_eq!(
        settled["outcomes"],
        json!([{"action":"native-hold","name":"Error",
        "error":"KELD-API-001: application channel call failed","code":null}])
    );
    assert_eq!(
        settled["terminals"],
        json!([{"nonce":fact.nonce,"ok":false,
        "code":"KELD-API-001","detail":"KELD-API-001: application channel call failed"}])
    );
}

struct JointResult {
    signals: NativeSignals,
    joint: JointInputs,
}

struct JointController {
    inputs: Arc<Inputs>,
    joint: JointInputs,
    reports: PageReports,
    signals: NativeSignals,
    taken: Receiver<()>,
    session: Arc<super::FsDispatchSession>,
    handle: PrimaryRouterHandle,
    owner: GuardianOwnerHandle,
    incoming: Receiver<RendererBridgeRequest>,
    dispatch: SyncSender<RendererBridgeRequest>,
    commands: mpsc::Sender<AppWindowCommand>,
}

impl JointController {
    fn held_renderer_terminal(&mut self) -> (Value, Value, RequestFact, Vec<u32>) {
        self.signals
            .initialized
            .recv_timeout(OBSERVATION_BOUND)
            .expect("actual worker observer scope");
        assert!(self.signals.pre_entry.is_none() && self.signals.pre_release.is_none());
        let first = forward_request(&self.incoming, &self.dispatch, "native-hold", 1);
        self.taken
            .recv_timeout(OBSERVATION_BOUND)
            .expect("actual native work self.taken");
        let held = self
            .signals
            .held
            .recv_timeout(OBSERVATION_BOUND)
            .expect("actual retained after-walk Native hold");
        assert!(!held.fixture_fault);
        assert_eq!((held.request_entries, held.request_returns), (1, 0));
        assert!(held.walk_entries > 0 && held.target_opens > 0);
        assert_eq!(
            held.truncate_calls + held.create_calls + held.content_writes,
            0,
            "pre-effect Native hold"
        );
        let initial = self.reports.wait("joint-page-ready");
        let started = self.reports.wait("joint-pending-start");
        assert_document(&started, &initial);
        let window = self.joint.window_ids(None);
        assert_eq!(window.len(), 1, "actual presented native window");
        let expired = self.reports.wait("joint-native-expiry");
        assert_eq!(expired["code"], "KELD-IPC-006");
        assert_eq!(expired["nonce"], first.nonce);
        let settled = self.reports.wait("joint-pending-settled");
        assert_pending_terminal(&initial, &settled, &first);
        assert_native_held(&self.signals, &self.session, &self.inputs.target);
        assert!(
            self.handle.is_current(1) && !self.handle.is_current(2),
            "actual G1 current while Native held"
        );
        assert_eq!(self.joint.window_ids(Some(&window)), window);
        assert!(
            self.handle.window_ready.load(Ordering::Acquire)
                && self.handle.recovery_armed.load(Ordering::Acquire),
            "real NavigationReady must complete its existing arm"
        );
        println!(
            "KELD_KEL143_JOINT_HELD_TERMINAL native_entries=1 native_returns=0 actual_renderer_terminal=1 same_document=true same_window=true"
        );
        (initial, settled, first, window)
    }

    fn recover_after_actual_return(&mut self) {
        self.owner
            .fail_generation(1)
            .expect("sole actual Runtime fault self.owner");
        let mut turns = ActorTurns::start(self.owner.clone());
        let deadline = Instant::now() + OBSERVATION_BOUND;
        while self.handle.last_revoked_attempt.load(Ordering::Acquire) != 1
            || self.handle.is_current(1)
        {
            assert!(Instant::now() < deadline, "actual G1 revocation absent");
            turns.advance();
        }
        assert_native_held(&self.signals, &self.session, &self.inputs.target);
        // The existing baton keeps one real actor receipt unconsumed at the
        // natural Retired-before-Prepare cut; it does not drive the FSM itself.
        return_and_observe_baseline(&mut self.signals, &self.session, &self.inputs);
        let deadline = Instant::now() + OBSERVATION_BOUND;
        while !self.handle.is_current(2) {
            assert!(Instant::now() < deadline, "actual fresh G2 absent");
            turns.advance();
        }
        drop(turns);
        assert_eq!(
            self.session
                .state
                .lock()
                .expect("G2 native state")
                .prepared_attempt,
            Some(2)
        );
        self.reports.wait_two_ready();
        self.joint.admit_after();
    }

    fn successor_public_calls(
        &mut self,
        initial: &Value,
        settled: &Value,
        first: &RequestFact,
        window: &[u32],
    ) {
        let second = forward_request(&self.incoming, &self.dispatch, "roundtrip", 1);
        let third = forward_request(&self.incoming, &self.dispatch, "deny", 2);
        for fact in [&second, &third] {
            assert_eq!(
                (fact.webview, fact.navigation),
                (first.webview, first.navigation)
            );
            assert_ne!(fact.request, first.request);
            assert_ne!(fact.nonce, first.nonce);
        }
        assert_ne!(second.request, third.request);
        assert_ne!(second.nonce, third.nonce);
        let final_page = self.reports.wait("joint-final");
        assert_document(&final_page, initial);
        assert_eq!(
            final_page["requests"]
                .as_array()
                .expect("three distinct actual page calls")
                .len(),
            3
        );
        assert_eq!(final_page["requests"][1]["nonce"], second.nonce);
        assert_eq!(final_page["requests"][2]["nonce"], third.nonce);
        assert_eq!(final_page["outcomes"][0], settled["outcomes"][0]);
        assert_eq!(
            final_page["outcomes"][1],
            json!({"action":"roundtrip","outcome":{"ok":true,"bytes":self.inputs.expected_content}})
        );
        assert_eq!(final_page["outcomes"][2]["action"], "deny");
        assert_eq!(
            final_page["outcomes"][2]["outcome"]["code"],
            "KELD-GUARD002"
        );
        assert_eq!(final_page["outcomes"][2]["outcome"]["ok"], false);
        assert_eq!(
            final_page["terminals"]
                .as_array()
                .expect("three actual bridge terminals")
                .len(),
            3
        );
        assert_eq!(
            fs::read(&self.inputs.target).expect("independent G2 exact bytes"),
            self.inputs.expected_content
        );
        assert_eq!(
            fs::read(&self.inputs.outside).expect("independent denied sentinel"),
            self.inputs.outside_content
        );
        assert_eq!(self.joint.window_ids(Some(window)), window);
        self.reports.assert_unique_page();
        println!(
            "KELD_KEL143_JOINT_G2 same_document=true same_navigation=true same_window=true allowed_exact_bytes=true denied_no_effect=true"
        );
    }

    fn run(mut self) -> JointResult {
        let _wake = WakeOnControllerUnwind(self.commands.clone());
        let (initial, settled, first, window) = self.held_renderer_terminal();
        self.recover_after_actual_return();
        self.successor_public_calls(&initial, &settled, &first, &window);
        JointResult {
            signals: self.signals,
            joint: self.joint,
        }
    }
}

fn assert_final_native_cleanup(signals: &NativeSignals, inputs: &Inputs) {
    let final_native = signals
        .final_snapshot
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Native final fence");
    assert!(!final_native.fixture_fault);
    assert_eq!(
        (final_native.request_entries, final_native.request_returns),
        (4, 4)
    );
    assert_eq!(sensor::directory_handle_count(&inputs.allowed_root), 0);
    assert_eq!(sensor::object_handle_count(&inputs.target), 0);
    assert!(
        !inputs.stage_root.exists(),
        "normal guardian stage cleanup before fixture fallback"
    );
}

fn run_joint() {
    let inputs = Arc::new(Inputs::supplied());
    let joint = JointInputs::supplied(&inputs);
    let report_reader = PageReports::open(&joint);
    let html = fs::read_to_string(&joint.page_identity.path).expect("source-bound joint page");
    let snapshot = inputs.prepare_policy();
    let NativeWorkerSetup {
        signals,
        gate,
        gate_release: _gate_release,
        gate_taken,
    } = native_worker_setup();
    let mut guardian = inputs.spawn_guardian();
    let initial = next_bound(&mut guardian, 1);
    assert_owned_groups(&guardian, guardian.group_pid().expect("actual G1 group"));
    let (commands, command_rx) = mpsc::channel();
    let shutdown = SessionShutdownState::new();
    let owner = GuardianOwner::start(guardian, commands.clone(), None, shutdown.clone())
        .expect("actual guardian actor");
    let router = PrimaryRouter::start_with_fs_test_gate(
        initial.into_stream(),
        commands.clone(),
        owner.handle(),
        shutdown,
        Some(&snapshot),
        Some(gate),
    )
    .expect("actual native router");
    let handle = router.handle();
    handle.recovery_armed.store(false, Ordering::Release);
    owner
        .attach_router(handle.clone())
        .expect("actual owner attachment");
    let (events, event_rx) = mpsc::channel();
    let event_handle = handle.clone();
    let event_commands = commands.clone();
    let coordinator = thread::spawn(move || {
        coordinate_window_events(
            &event_rx,
            &event_handle,
            &event_commands,
            super::AppReadyTrigger::NavigationReady,
            |_| Ok(()),
        )
    });
    let (requests, observed_requests) = mpsc::sync_channel(1);
    let (forwarded, dispatch_requests) = mpsc::sync_channel(1);
    let (outcomes, renderer_outcomes) = mpsc::channel();
    let dispatch = start_renderer_dispatch(dispatch_requests, outcomes, handle.clone())
        .expect("existing renderer dispatcher");
    let mut engine = WkWebViewEngine::new_dev_ephemeral().expect("real process-main WK engine");
    let spec = WebviewSpec {
        title: joint.title.clone(),
        initial: NavTarget::Html(html),
        ..WebviewSpec::default()
    };
    engine
        .create_app_with_renderer_bridge(
            &spec,
            events.clone(),
            RendererBridgeEndpoint::new(
                requests,
                renderer_outcomes,
                keld_ipc::channel_table::ECHO.wire_id(),
            ),
        )
        .expect("actual renderer bridge/window before first navigation");
    let control_inputs = Arc::clone(&inputs);
    let control_session = Arc::clone(&snapshot.fs);
    let control_owner = owner.handle();
    let control_commands = commands.clone();
    let controller = JointController {
        inputs: control_inputs,
        joint,
        reports: report_reader,
        signals,
        taken: gate_taken,
        session: control_session,
        handle,
        owner: control_owner,
        incoming: observed_requests,
        dispatch: forwarded,
        commands: control_commands,
    };
    let control = thread::spawn(move || controller.run());
    let window_result = engine.run_app_until_quit(command_rx, events);
    let control_result = control.join();
    drop(commands);
    let event_result = coordinator.join().expect("window coordinator join");
    let router_result = router.shutdown();
    let renderer_result = join_renderer_dispatch(dispatch);
    let owner_result = owner.shutdown();
    window_result.expect("actual window loop completes after public Quit");
    event_result.expect("actual NavigationReady/lifecycle events");
    router_result.expect("actual worker/readers join");
    renderer_result.expect("actual renderer dispatch join");
    owner_result.expect("actual persistent guardian successful reap");
    drop(engine);
    let result = control_result.expect("actual Native/renderer controller");
    println!(
        "KELD_KEL143_JOINT_PROCESS_EXIT_CENSUS_REQUIRED pid={} title={}",
        std::process::id(),
        result.joint.title
    );
    assert_final_native_cleanup(&result.signals, &inputs);
    println!(
        "KELD_KEL143_JOINT_DONE native_entries=4 native_returns=4 roots=0 stage_absent=true window_census=pending_normal_process_exit real_page_native_window=true"
    );
}
