//! Actual Core caller fence, ordered socket writer and inherited-stderr capture.
//! Component fixture identities prove these local contracts, not Runtime or UI.
use super::{
    HostAppError, MenuSession, MenuWork, PrimaryCallerContext, PrimaryRouterHandle,
    publish_quit_requested, retire_quit_intent_write_failure, same_caller, window_owner,
    with_primary_caller_locked,
};
use crate::app_session as app;
use keld_ipc::codec::{decode, encode};
use keld_ipc::link::{AppLinkDeadlines as _, GuardedCallAdmission, read_frame};
use keld_ipc::{CallError, CorrelationId, FrameKind, LIFECYCLE_CHANNEL, LifecycleEvent};
use keld_native::menu::{MenuAction, MenuKind, MenuNodeV1, MenuRequestV1, MenuRole};
use keld_wv::wkwebview::AppWindowCommand;
use sha2::{Digest as _, Sha256};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

fn session(
    source: &str,
) -> (
    Arc<MenuSession>,
    mpsc::Receiver<AppWindowCommand>,
    tempfile::TempDir,
) {
    let temp = tempfile::tempdir().expect("immutable menu policy");
    let path = temp.path().join("keld.permissions.jsonc");
    std::fs::write(&path, source).expect("policy bytes");
    let digest = Sha256::digest(source.as_bytes()).into();
    let verified = keld_guard::verified_manifest::load_verified_manifest(
        std::fs::File::open(&path).expect("retained source"),
        path,
        digest,
    )
    .expect("verified policy");
    let (wake, receive) = mpsc::channel();
    (Arc::new(MenuSession::new(verified, wake)), receive, temp)
}
fn caller(handle: &PrimaryRouterHandle) -> PrimaryCallerContext {
    let current = handle.current.lock().expect("current owner");
    let active = current.as_ref().expect("active owner");
    PrimaryCallerContext {
        identity: active.identity,
        attempt: active.attempt,
        reader_stop: Arc::clone(&active.reader_stop),
    }
}
fn request() -> MenuRequestV1 {
    MenuRequestV1 {
        nodes: vec![MenuNodeV1 {
            depth: 0,
            kind: MenuKind::Normal,
            label: "PRIVATE-LABEL-MUST-NOT-LOG".to_owned(),
            enabled: true,
            checked: None,
            action: MenuAction::NativeRole(MenuRole::Close),
        }],
    }
}
fn assert_err(peer: &mut app::BootstrapStream, correlation: u32, code: &str) {
    let (header, body) = read_frame(peer).expect("actual terminal frame");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Err,
            keld_ipc::channel_table::MENU.id(),
            CorrelationId(correlation)
        )
    );
    assert_eq!(
        decode::<CallError>(&body)
            .expect("one canonical typed ERR")
            .code,
        code
    );
}

#[test]
fn exact_menu_grant_precedes_decode_and_cannot_be_replaced_by_dialog_grant() {
    for source in [
        r#"{"app":{"system":[]}}"#,
        r#"{"app":{"system":["dialog"]}}"#,
    ] {
        let (mut handle, mut peer, _guardian, _ui) =
            app::tests::initial_ready_router_with_window_loop();
        let (menu, wake, _temp) = session(source);
        handle.menu = Some(Arc::clone(&menu));
        peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
            .expect("bounded wire read");
        handle
            .dispatch_menu(&caller(&handle), CorrelationId(41), &[255])
            .expect("denial writes typed ERR");
        assert_err(&mut peer, 41, "KELD-GUARD001");
        assert!(menu.state.lock().expect("state").pending.is_none());
        assert!(
            wake.try_recv().is_err(),
            "denial has zero UI wakes/native admission"
        );
    }
}

#[test]
fn allowed_install_has_one_slot_and_one_wake_without_early_receipt() {
    let (mut handle, mut peer, _guardian, _ui) =
        app::tests::initial_ready_router_with_window_loop();
    let (menu, wake, _temp) = session(r#"{"app":{"system":["menu","dialog"]}}"#);
    handle.menu = Some(Arc::clone(&menu));
    let caller = caller(&handle);
    let payload = encode(&request()).expect("bounded typed tree");
    handle
        .dispatch_menu(&caller, CorrelationId(11), &payload)
        .expect("reserve");
    assert!(matches!(wake.try_recv(), Ok(AppWindowCommand::MacAuthWake)));
    assert!(wake.try_recv().is_err());
    let pending = menu.state.lock().expect("one pending");
    assert_eq!(
        pending
            .pending
            .as_ref()
            .expect("retained request")
            .correlation,
        CorrelationId(11)
    );
    assert_eq!(
        pending.pending.as_ref().expect("retained tree").request,
        request()
    );
    drop(pending);
    assert_eq!(
        menu.admission(1, CorrelationId(11)),
        GuardedCallAdmission::RejectSession
    );
    assert_eq!(
        menu.admission(1, CorrelationId(12)),
        GuardedCallAdmission::Admit
    );
    // A distinct excess CALL gets ERR12; if admission incorrectly replies11
    // before native install, this independent ordered-wire assertion fails.
    handle
        .dispatch_menu(&caller, CorrelationId(12), &payload)
        .expect("bounded refusal");
    assert_err(&mut peer, 12, "KELD-IPC-005");
    assert_eq!(
        menu.state
            .lock()
            .expect("pending retained")
            .pending
            .as_ref()
            .expect("first call still owns native slot")
            .correlation,
        CorrelationId(11)
    );
    assert!(wake.try_recv().is_err());
}

#[test]
fn stale_link_arc_and_stopped_reader_cannot_reserve_menu_even_at_equal_attempt() {
    let (mut handle, _peer, _guardian, _ui) = app::tests::initial_ready_router_with_window_loop();
    let (menu, wake, _temp) = session(r#"{"app":{"system":["menu"]}}"#);
    handle.menu = Some(Arc::clone(&menu));
    let current = caller(&handle);
    let mut wrong = current.clone();
    wrong.reader_stop = Arc::new(AtomicBool::new(false));
    let payload = encode(&request()).expect("tree");
    handle
        .dispatch_menu(&wrong, CorrelationId(21), &payload)
        .expect("foreign arc fenced");
    current.reader_stop.store(true, Ordering::Release);
    handle
        .dispatch_menu(&current, CorrelationId(22), &payload)
        .expect("retired reader fenced");
    assert!(menu.state.lock().expect("state").pending.is_none());
    assert!(wake.try_recv().is_err());
}

#[test]
fn retirement_takes_correlation_once_and_never_drops_successor_request() {
    let (mut handle, _peer, _guardian, _ui) = app::tests::initial_ready_router_with_window_loop();
    let (menu, wake, _temp) = session(r#"{"app":{"system":["menu"]}}"#);
    handle.menu = Some(Arc::clone(&menu));
    let old = caller(&handle);
    handle
        .dispatch_menu(&old, CorrelationId(31), &encode(&request()).expect("tree"))
        .expect("reserve");
    wake.try_recv().expect("wake");
    let error = CallError::generation_retired();
    assert_eq!(
        menu.take_pending_for_retirement(window_owner(&old), &error)
            .expect("retire"),
        Some(CorrelationId(31))
    );
    assert_eq!(
        menu.take_pending_for_retirement(window_owner(&old), &error)
            .expect("duplicate retirement"),
        None
    );
    let mut next = old.clone();
    next.attempt += 1;
    menu.reserve(MenuWork {
        caller: next.clone(),
        correlation: CorrelationId(31),
        request: request(),
    })
    .expect("new role owns same transport correlation");
    menu.retire(window_owner(&old)).expect("old owner retired");
    assert!(same_caller(
        &menu
            .state
            .lock()
            .expect("state")
            .pending
            .as_ref()
            .expect("successor survives")
            .caller,
        &next
    ));
}

#[test]
fn received_guard_err_has_exactly_one_actual_stderr_record_and_no_private_label() {
    const CHILD: &str = "KELD_MENU_DIAGNOSTIC_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let (mut handle, mut peer, _guardian, _ui) =
            app::tests::initial_ready_router_with_window_loop();
        let (menu, _wake, _temp) = session(r#"{"app":{"system":[]}}"#);
        handle.menu = Some(menu);
        handle
            .dispatch_menu(
                &caller(&handle),
                CorrelationId(71),
                &encode(&request()).expect("tree"),
            )
            .expect("received denied install");
        assert_err(&mut peer, 71, "KELD-GUARD001");
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("unit test process"))
        .args(["--exact", "app_session::menu_router::tests::received_guard_err_has_exactly_one_actual_stderr_record_and_no_private_label", "--nocapture"])
        .env(CHILD, "1").output().expect("child stderr capture");
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).expect("diagnostic UTF8");
    assert_eq!(stderr.matches("phase=install").count(), 1, "{stderr}");
    assert!(
        stderr.contains("KELD-GUARD001 phase=install result=denied correlation=71 role=Some(1)"),
        "{stderr}"
    );
    assert!(stderr.contains("fix="));
    assert!(!stderr.contains("PRIVATE-LABEL-MUST-NOT-LOG"));
    assert!(!stderr.contains("0x"));
}

#[test]
fn native_quit_intent_unit_is_written_only_inside_existing_menu_guard() {
    for (policy, allowed) in [
        (r#"{"app":{"system":["menu"]}}"#, true),
        (r#"{"app":{"system":["dialog"]}}"#, false),
    ] {
        let (handle, mut peer, _guardian, _ui) =
            app::tests::initial_ready_router_with_window_loop();
        let (menu, _wake, _temp) = session(policy);
        let who = caller(&handle);
        let mut wrote = false;
        with_primary_caller_locked(&handle.current, &handle.shutdown, &who, |current| {
            let result = menu
                .authorize(|| publish_quit_requested(current.as_mut().expect("current writer")));
            if allowed {
                result.expect("menu grant").expect("actual socket write");
                wrote = true;
            } else {
                assert_eq!(
                    result.expect_err("dialog does not grant menu").code,
                    "KELD-GUARD001"
                );
            }
            Ok(())
        })
        .expect("same caller fence");
        assert_eq!(wrote, allowed);
        if allowed {
            let (header, bytes) = read_frame(&mut peer).expect("actual intent frame");
            assert_eq!(
                (header.kind, header.channel, header.corr),
                (FrameKind::Event, LIFECYCLE_CHANNEL, CorrelationId(0))
            );
            assert_eq!(bytes, [2]);
            assert_eq!(
                decode::<LifecycleEvent>(&bytes).expect("decode unit"),
                LifecycleEvent::QuitRequested
            );
        }
    }
}

#[test]
fn native_quit_intent_write_failure_keeps_actual_cause_and_retires_same_generation() {
    let (mut handle, peer, _guardian, _ui) = app::tests::initial_ready_router_with_window_loop();
    let (menu, _wake, _temp) = session(r#"{"app":{"system":["menu"]}}"#);
    handle.menu = Some(Arc::clone(&menu));
    let who = caller(&handle);
    let stop = Arc::clone(&who.reader_stop);
    peer.shutdown(std::net::Shutdown::Both)
        .expect("actual closed peer");
    drop(peer);
    let mut observed = None;
    with_primary_caller_locked(&handle.current, &handle.shutdown, &who, |current| {
        let source = publish_quit_requested(current.as_mut().expect("current writer"))
            .expect_err("closed socket cannot accept intent");
        let actual_cause = source.to_string();
        let state = menu
            .state
            .lock()
            .expect("same menu state held through attempted publication");
        let error = retire_quit_intent_write_failure(&handle, current, who.attempt, &source, state);
        assert!(
            error.to_string().contains(&actual_cause),
            "preserve actual failed write cause"
        );
        observed = Some(error);
        Ok(())
    })
    .expect("same role fence");
    assert!(
        observed
            .expect("actual write failure")
            .to_string()
            .contains("native menu QuitRequested write")
    );
    assert!(handle.current.lock().expect("current owner").is_none());
    assert!(
        stop.load(Ordering::Acquire),
        "same generation reader is retired"
    );
}

fn selected_original_menu_quit(
    handle: &PrimaryRouterHandle,
    candidate: &PrimaryCallerContext,
) -> Result<(bool, bool), HostAppError> {
    let mut entered = false;
    let mut eligible = false;
    with_primary_caller_locked(&handle.current, &handle.shutdown, candidate, |_| {
        entered = true;
        eligible = handle.original_menu_quit_selected(candidate)?;
        Ok(())
    })?;
    Ok((entered, eligible))
}

#[test]
fn original_menu_quit_producer_requires_actual_selection_subscribe_ready_and_current_caller() {
    let (mut handle, mut peer, owner_commands, _ui) =
        app::tests::initial_ready_router_with_window_loop();
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("finite component lifecycle reads");
    handle.windows = Some(Arc::new(std::sync::Mutex::new(
        app::window_router::WindowSession::new(true),
    )));
    let who = caller(&handle);
    let selected = |candidate: &PrimaryCallerContext| {
        selected_original_menu_quit(&handle, candidate).expect("actual caller gate")
    };
    assert_eq!(selected(&who), (true, false));
    let (_retained_stage, selection) = app::tests::gh499_graph::original_quit_selection();
    handle
        .windows
        .as_ref()
        .expect("same window session")
        .lock()
        .expect("window lock")
        .install_original_selection(selection)
        .expect("typed admitted source");
    assert_eq!(
        selected(&who),
        (true, false),
        "selection does not confer subscription"
    );
    handle
        .subscribe_window(
            &who,
            CorrelationId(79),
            keld_ipc::channel_table::WINDOW.wire_id(),
        )
        .expect("existing component Subscribe route");
    let (header, body) = read_frame(&mut peer).expect("actual correlated Subscribe reply");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(79))
    );
    assert_eq!(
        decode::<keld_ipc::LifecycleResponse>(&body).expect("decode"),
        keld_ipc::LifecycleResponse::Subscribed
    );
    assert_eq!(
        selected(&who),
        (true, false),
        "selected/subscribed original data does not manufacture Host Ready"
    );
    let owner = std::thread::spawn(move || {
        let app::GuardianOwnerCommand::ArmRecovery(_, reply) = owner_commands
            .recv_timeout(Duration::from_secs(3))
            .expect("actual initial recovery-arm command")
        else {
            panic!("Ready must use the existing initial recovery owner");
        };
        reply
            .send(Ok(()))
            .expect("owned recovery-arm acknowledgment");
    });
    handle
        .signal_ready()
        .expect("actual owned Ready publication");
    let (header, body) = read_frame(&mut peer).expect("actual Ready frame");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Event, LIFECYCLE_CHANNEL, CorrelationId(0))
    );
    assert_eq!(body, [0], "independent original Ready unit byte");
    owner.join().expect("bounded recovery owner joins");
    assert_eq!(
        selected(&who),
        (true, true),
        "qualified zero-window bootstrap remains selected"
    );
    handle.window_ready.store(false, Ordering::Release);
    assert_eq!(
        selected(&who),
        (true, false),
        "pre-Ready publication is held"
    );
    handle.window_ready.store(true, Ordering::Release);
    let mut foreign_link = who.clone();
    foreign_link.reader_stop = Arc::new(AtomicBool::new(false));
    assert_eq!(
        selected(&foreign_link),
        (false, false),
        "equal role numbers cannot replace its actual link"
    );
    who.reader_stop.store(true, Ordering::Release);
    assert_eq!(
        selected(&who),
        (false, false),
        "retired caller cannot publish"
    );
}
