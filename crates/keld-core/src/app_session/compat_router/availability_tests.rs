//! Actual Core socket/subscription regressions and pure policy/receipt contracts.
//! Fixture callers do not qualify Runtime authentication or native registration.

use super::{
    CallError, CompatSession, ConsumerDisposition, CorrelationId, FrameKind, PrimaryCallerContext,
    PrimaryRouterHandle, Principal, RendererBridgeOutcome, RendererChannelQuery,
    WvConsumerDisposition, consumer, dispatch_privileged_query, encode, preflight_privileged_query,
    write_frame,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(debug_assertions)]
mod original_recovery;

// These state/socket cases prove Core subscription and pure-policy atoms.
// Fixture identities do not prove Runtime admission or native registration.
fn availability_session() -> (
    CompatSession,
    std::sync::mpsc::Receiver<RendererBridgeOutcome>,
) {
    use sha2::{Digest as _, Sha256};
    let temp = tempfile::tempdir().expect("availability manifest root");
    let path = temp.path().join("keld.permissions.jsonc");
    let text = r#"{"windows":{"editor":{"channels":["el:rendererReq"]}}}"#;
    std::fs::write(&path, text).expect("write actual manifest");
    let verified = super::super::load_verified_manifest(
        std::fs::File::open(&path).expect("open actual manifest"),
        path,
        Sha256::digest(text.as_bytes()).into(),
    )
    .expect("verify same immutable manifest");
    let (outcomes, received) = std::sync::mpsc::channel();
    (CompatSession::new(verified, outcomes), received)
}

fn active_caller(handle: &PrimaryRouterHandle) -> PrimaryCallerContext {
    let current = handle.current.lock().expect("component current");
    let active = current.as_ref().expect("component active link");
    PrimaryCallerContext {
        attempt: active.attempt,
        identity: active.identity,
        reader_stop: Arc::clone(&active.reader_stop),
    }
}

#[test]
fn availability_subscription_requires_live_exact_reader() {
    let (mut session, _outcomes) = availability_session();
    let (handle, _peer, _guardian, _window) =
        super::super::tests::initial_ready_router_with_window_loop();
    let owner = active_caller(&handle);
    assert!(
        !session.subscribed_to(&owner),
        "missing subscription is not readiness"
    );
    session.subscribed = Some(owner.clone());
    assert!(session.subscribed_to(&owner));
    let mut foreign = owner.clone();
    foreign.reader_stop = Arc::new(AtomicBool::new(false));
    assert!(
        !session.subscribed_to(&foreign),
        "same numeric attempt is another link"
    );
    owner.reader_stop.store(true, Ordering::Release);
    assert!(
        !session.subscribed_to(&owner),
        "retired reader cannot qualify readiness"
    );
}

#[test]
fn failed_actual_subscribed_reply_never_installs_availability() {
    use keld_ipc::link::AppLinkDeadlines as _;
    use std::io::Read as _;
    let (mut handle, mut peer, _guardian, _window) =
        super::super::tests::initial_ready_router_with_window_loop();
    let (session, outcomes) = availability_session();
    handle.compat = Some(Arc::new(Mutex::new(session)));
    let caller = active_caller(&handle);
    let mut reader = handle
        .current
        .lock()
        .expect("component current")
        .as_ref()
        .expect("component active")
        .writer
        .try_clone()
        .expect("actual reader clone");
    reader
        .set_app_link_deadlines(Some(std::time::Duration::from_secs(3)))
        .expect("reader deadline");
    peer.set_app_link_deadlines(Some(std::time::Duration::from_secs(3)))
        .expect("peer deadline");
    // Close the real owned writer half BEFORE admitting the CALL; the reader
    // half remains live. This cannot race a successful reply with peer teardown.
    handle
        .current
        .lock()
        .expect("component current")
        .as_ref()
        .expect("component active")
        .writer
        .shutdown(std::net::Shutdown::Write)
        .expect("actual writer half-close");
    assert_eq!(
        peer.read(&mut [0_u8; 1])
            .expect("observe actual writer EOF"),
        0
    );
    let payload = encode(&keld_ipc::LifecycleRequest::Subscribe {
        channel: keld_ipc::channel_table::COMPAT_CONTROL.wire_id(),
    })
    .expect("canonical Subscribe payload");
    write_frame(
        &mut peer,
        FrameKind::Call,
        0,
        keld_ipc::LIFECYCLE_CHANNEL,
        CorrelationId(81),
        &payload,
    )
    .expect("actual Subscribe CALL");
    let reader_handle = handle.clone();
    let worker = std::thread::spawn(move || {
        super::super::read_primary_frames(&mut reader, &reader_handle, &caller)
    });
    let result = worker.join().expect("actual primary reader join");
    let error = result.expect_err("closed owned writer must make genuine Subscribed write fail");
    assert!(
        error.to_string().contains("compat Subscribe writer"),
        "{error}"
    );
    let state = handle
        .compat
        .as_ref()
        .expect("compat")
        .lock()
        .expect("compat state");
    assert!(
        state.subscribed.is_none(),
        "failed reply must leave no qualifying subscription"
    );
    assert!(
        outcomes.try_recv().is_err(),
        "failed subscription cannot emit ready=true"
    );
}

#[test]
fn actual_subscribed_reply_uses_existing_writer_before_availability() {
    use keld_ipc::link::{AppLinkDeadlines as _, read_frame};
    let (mut handle, mut peer, _guardian, _window) =
        super::super::tests::initial_ready_router_with_window_loop();
    peer.set_app_link_deadlines(Some(std::time::Duration::from_secs(3)))
        .expect("socket deadline");
    let (session, outcomes) = availability_session();
    handle.compat = Some(Arc::new(Mutex::new(session)));
    let caller = active_caller(&handle);
    assert!(
        handle
            .subscribe_compat(
                &caller,
                CorrelationId(82),
                keld_ipc::channel_table::COMPAT_CONTROL.wire_id()
            )
            .expect("actual Subscribe")
    );
    let (header, bytes) = read_frame(&mut peer).expect("actual correlated reply");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Reply,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(82)
        )
    );
    assert_eq!(
        bytes,
        [1],
        "existing literal Subscribed postcard discriminant"
    );
    assert!(
        handle
            .compat
            .as_ref()
            .expect("compat")
            .lock()
            .expect("compat state")
            .subscribed_to(&caller)
    );
    assert!(
        outcomes.try_recv().is_err(),
        "a subscription without a native registered document is not readiness"
    );
}

#[test]
fn availability_and_preflight_never_cache_final_channel_permission() {
    use std::cell::Cell;
    let (mut session, _outcomes) = availability_session();
    let (handle, _peer, _guardian, _window) =
        super::super::tests::initial_ready_router_with_window_loop();
    let owner = active_caller(&handle);
    session.subscribed = Some(owner.clone());
    assert!(session.subscribed_to(&owner));
    let manifest = session.verified.manifest();
    // Numeric shape is sufficient only for this pure Guard policy atom.
    let principal = Principal::Webview {
        id: 9,
        generation: 3,
    };
    preflight_privileged_query(
        manifest,
        principal,
        RendererChannelQuery {
            channel: "rendererReq",
            current_window: manifest.window_channel_grants("editor"),
        },
    )
    .expect("permitted preflight");
    let effects = Cell::new(0);
    let denied = dispatch_privileged_query(
        manifest,
        principal,
        RendererChannelQuery {
            channel: "blocked",
            current_window: manifest.window_channel_grants("editor"),
        },
        |_| effects.set(effects.get() + 1),
    );
    assert!(denied.is_err());
    assert_eq!(
        effects.get(),
        0,
        "available route and earlier preflight grant no effect"
    );
    dispatch_privileged_query(
        manifest,
        principal,
        RendererChannelQuery {
            channel: "rendererReq",
            current_window: manifest.window_channel_grants("editor"),
        },
        |_| effects.set(effects.get() + 1),
    )
    .expect("fresh final exact grant");
    assert_eq!(effects.get(), 1);
}
#[test]
fn retirement_clears_exact_subscription_even_if_reader_already_stopped() {
    let (mut handle, _peer, _guardian, _window) =
        super::super::tests::initial_ready_router_with_window_loop();
    let (mut session, _outcomes) = availability_session();
    let caller = active_caller(&handle);
    session.subscribed = Some(caller.clone());
    handle.compat = Some(Arc::new(Mutex::new(session)));
    caller.reader_stop.store(true, Ordering::Release);
    let mut current = handle.current.lock().expect("component current");
    handle
        .retire_compat_locked(current.as_mut().expect("component active"))
        .expect("retire exact stopped owner");
    assert!(
        handle
            .compat
            .as_ref()
            .expect("compat")
            .lock()
            .expect("compat state")
            .subscribed
            .is_none()
    );
}
#[test]
fn native_refusal_never_becomes_a_consumed_receipt() {
    let observed = consumer(&WvConsumerDisposition::Refused {
        code: String::from("KELD-WV-011"),
        message: String::from("route unavailable"),
    });
    assert_eq!(
        observed,
        ConsumerDisposition::Refused {
            error: CallError {
                code: String::from("KELD-WV-011"),
                message: String::from("route unavailable"),
            }
        }
    );
    assert_ne!(observed, ConsumerDisposition::Consumed);
}
