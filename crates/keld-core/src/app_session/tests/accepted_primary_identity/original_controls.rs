//! Native CONTROL custody across authenticated Runtime primary replacement.

use super::{AcceptedRole, BoundPrimaryGeneration, PrimaryRoleConfig};
use crate::app_session::graph_contract::tests::prepared_original_window_source;
use crate::app_session::window_registry::{CreateOutcome, OriginalWindowResources};
use crate::app_session::{
    AppWindowCommand, DirectPrimaryOwnerCommand, PlatformPrimaryOwnerHandle, PrimaryCallerContext,
    PrimaryLinkIdentity, PrimaryRouter, PrimaryRouterHandle, SessionShutdownState,
};
use keld_ipc::CorrelationId;
use keld_ipc::window::{WindowGeneration, WindowRef};
use keld_wv::ApplicationNameForUserAgent;
use keld_wv::renderer_bridge::RendererRoleWitness;
use std::io::Write as _;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};

fn current_caller(handle: &PrimaryRouterHandle) -> PrimaryCallerContext {
    let current = handle.current.lock().expect("current owner");
    let active = current.as_ref().expect("authenticated primary");
    PrimaryCallerContext {
        identity: active.identity,
        attempt: active.attempt,
        reader_stop: Arc::clone(&active.reader_stop),
    }
}

fn witness(
    handle: &PrimaryRouterHandle,
    caller: &PrimaryCallerContext,
    window: WindowRef,
) -> RendererRoleWitness {
    handle
        .renderer_role_scope(caller)
        .expect("Runtime primary witness")
        .for_window(window)
}

fn original_window(
    handle: &PrimaryRouterHandle,
    caller: &PrimaryCallerContext,
    correlation: u32,
    resources: &OriginalWindowResources,
) -> (WindowRef, RendererRoleWitness) {
    let window = {
        let _transition = handle.shutdown.transition_guard();
        let created = handle
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("Registry")
            .create_original_for_caller(
                caller,
                CorrelationId(correlation),
                (640, 480),
                (None, None),
                OriginalWindowResources {
                    source: Arc::clone(&resources.source),
                    preload: Arc::clone(&resources.preload),
                },
            )
            .expect("original physical Window entry");
        let CreateOutcome::Build { window, .. } = created else {
            panic!("expected a new physical Window entry");
        };
        window
    };
    let witness = witness(handle, caller, window);
    assert!(witness.has_native_view_scope());
    (window, witness)
}

struct ControlRouter {
    router: PrimaryRouter,
    resources: OriginalWindowResources,
    events: mpsc::Receiver<AppWindowCommand>,
    _owner_commands: mpsc::Receiver<DirectPrimaryOwnerCommand>,
    _stage: tempfile::TempDir,
}

fn start_control_router(first: BoundPrimaryGeneration) -> ControlRouter {
    let (stage, prepared, preload) = prepared_original_window_source(
        ApplicationNameForUserAgent::new(String::from("control-fixture/1.0")).expect("boot field"),
    );
    let selection = prepared
        .bind_runtime(&first)
        .expect("authenticated original selection");
    let resources = selection
        .prepare_preload(
            preload.to_str().expect("UTF-8 selected preload"),
            Vec::new(),
        )
        .expect("retained selected source and preload");
    let (guardian, owner_commands) = mpsc::channel();
    let (window_commands, events) = mpsc::channel();
    let router = PrimaryRouter::start_bound(
        first,
        window_commands,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian,
        },
        SessionShutdownState::new(),
        None,
        Some(true),
        Some(Arc::new(selection)),
    )
    .expect("authenticated original router");
    {
        let handle = router.handle();
        let _transition = handle.shutdown.transition_guard();
        assert!(handle.mark_window_registry_ready_guarded().expect("ready"));
    }
    ControlRouter {
        router,
        resources,
        events,
        _owner_commands: owner_commands,
        _stage: stage,
    }
}

fn primary_role() -> AcceptedRole {
    AcceptedRole::start_with_config(
        PrimaryRoleConfig::primary("bun")
            .arg("--define=__KELD_RUNTIME_PROCESS_FACTS__:true")
            .require_runtime_process_facts(),
    )
}

fn install_successor(
    handle: &PrimaryRouterHandle,
    retired: u32,
    successor: BoundPrimaryGeneration,
) {
    let identity = PrimaryLinkIdentity::Role(successor.principal());
    handle.retire_generation(retired).expect("retire A");
    handle
        .install_generation_with_identity(successor.attempt(), successor.into_stream(), identity)
        .expect("install real Runtime successor");
}

#[test]
fn windows_original_control_is_shared_and_survives_primary_replacement() {
    let _custody = keld_runtime::windows_job::install_host_death_job()
        .expect("install process-lifetime custody");
    let mut role = primary_role();
    let (first, mut control) = role.bind();
    let fixture = start_control_router(first);
    let handle = fixture.router.handle();
    let first = current_caller(&handle);
    let (window, a) = original_window(&handle, &first, 1, &fixture.resources);
    let (_, a_other) = original_window(&handle, &first, 2, &fixture.resources);
    let wrong_incarnation = WindowRef {
        incarnation: WindowGeneration(window.incarnation.0 + 1),
        ..window
    };
    let wrong_entry = witness(&handle, &first, wrong_incarnation);
    assert!(!wrong_entry.has_native_view_scope());
    assert!(!wrong_entry.close_native_offers());
    assert!(wrong_entry.try_reserve_native_control(1, 1025).is_none());
    let foreign = PrimaryCallerContext {
        reader_stop: Arc::new(AtomicBool::new(false)),
        ..first.clone()
    };
    let wrong_allocation = witness(&handle, &foreign, window);
    assert!(
        wrong_allocation
            .try_reserve_native_control(1, 1025)
            .is_none()
    );

    let held = [&a, &a_other, &a, &a_other].map(|witness| {
        witness
            .try_reserve_native_control(4096, 12_000)
            .expect("one of four original CONTROL charges")
    });
    assert!(a.try_reserve_native_control(1, 1025).is_none());
    assert!(!held[0].try_extend_storage(4096, 17_537));
    assert!(held[0].try_extend_storage(4096, 17_536));
    assert!(!held[0].try_extend_storage(4096, 1));
    assert!(!held[0].try_extend(4097, 16_384));

    writeln!(control.get_mut(), "CRASH").expect("crash first real Runtime role");
    control.get_mut().flush().expect("flush crash");
    let (second, _control) = role.bind();
    install_successor(&handle, first.attempt, second);
    let (_, b) = original_window(&handle, &current_caller(&handle), 3, &fixture.resources);
    assert!(
        a.close_native_offers(),
        "retired A closes its physical entry"
    );
    assert!(a.close_native_offers(), "closure is idempotent");
    assert!(!a.native_storage_is_quiescent());
    assert!(b.try_reserve_native_control(1, 1025).is_none());

    let writer = handle.primary_writer.as_ref().expect("same primary writer");
    fixture.events.try_iter().for_each(drop);
    writer
        .acknowledge_progress()
        .expect("no older progress marker");
    let [a_first, a_other_first, a_last, a_other_last] = held;
    drop(a_first);
    assert!(matches!(
        fixture
            .events
            .recv_timeout(super::BOUND)
            .expect("physical release wake"),
        AppWindowCommand::HostWriterProgress
    ));
    assert!(
        !a.native_storage_is_quiescent(),
        "the second A allocation still owns backing"
    );
    assert!(a.try_reserve_native_control(1, 1025).is_none());
    assert!(a_other.try_reserve_native_control(1, 1025).is_none());
    let b_control = b
        .try_reserve_native_control(1, 1025)
        .expect("freed slot reaches live B");

    drop(a_last);
    assert!(
        a.native_storage_is_quiescent(),
        "the exact retired Entry is now drained"
    );
    assert!(
        !a_other.native_storage_is_quiescent(),
        "the other Entry has not closed"
    );
    assert!(a_other.close_native_offers());
    assert!(!a_other.native_storage_is_quiescent());
    drop((a_other_first, a_other_last, b_control));
    assert!(a_other.native_storage_is_quiescent());
    assert!(b.close_native_offers());
    assert!(
        b.try_reserve_native_control(1, 1025).is_none(),
        "closing a live Entry ends offers"
    );
    fixture.router.shutdown().expect("router cleanup");
    assert!(matches!(
        role.finish(),
        Some(keld_runtime::SupervisorOutcome::Stopped)
    ));
}
