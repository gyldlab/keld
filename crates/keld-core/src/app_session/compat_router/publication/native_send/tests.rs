//! Real Core publication/pipe controls with the existing modeled native lease.
//! This does not claim a `WebView` callback or application consumer executed.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use crate::app_session::compat_router::RendererRequestDisposition;
use crate::app_session::primary_writer::{ControlMessage, PrimaryWriter, WriterLink};
use crate::app_session::window_registry::WindowOwner;
use crate::app_session::window_router::WindowSession;
use keld_ipc::codec::decode;
use keld_ipc::compat_control::CompatControlEvent;
use keld_wv::renderer_bridge::{RendererBridgeOperation, RendererRoleWitness};
use keld_wv::renderer_contract::NativeDocumentFixture;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::time::Duration;

const CHANNEL: &str = "rendererReq";
const PAYLOAD: &[u8] = &[6, 0, 0]; // Canonical empty endpoint argument array.

struct Fixture<'a> {
    router: &'a PrimaryRouterHandle,
    caller: &'a PrimaryCallerContext,
    route: &'a RendererRoute,
    native: &'a NativeDocumentFixture,
    role: RendererRoleWitness,
    peer: &'a mut keld_ipc::BootstrapStream,
    writer: &'a PrimaryWriter,
    target: &'a Arc<WriterLink>,
    outcomes: &'a Receiver<RendererBridgeOutcome>,
}

fn fixture(run: impl FnOnce(Fixture<'_>)) {
    crate::app_session::compat_router::owned_tests::fixture(
        |base, caller, target, peer, writer, outcomes, _| {
            let mut router = base.clone();
            let mut windows = WindowSession::new(true);
            let (root, native, route) = windows
                .fixture_original_retirement_route(WindowOwner::new(
                    caller.identity,
                    caller.attempt,
                ))
                .expect("existing retained native lease fixture");
            {
                let _transition = router.shutdown.transition_guard();
                let _current = router.current.lock().expect("actual current caller");
                windows.fixture_bind_original_caller(caller);
            }
            router.windows = Some(Arc::new(Mutex::new(windows)));
            router
                .compat
                .as_ref()
                .expect("existing Compat")
                .lock()
                .expect("same modeled subscription")
                .subscribed = Some(caller.clone());
            let role = router.renderer_role_witness_for_window(caller, route.window);
            run(Fixture {
                router: &router,
                caller,
                route: &route,
                native: &native,
                role,
                peer,
                writer,
                target,
                outcomes,
            });
            drop(native);
            drop(root);
        },
    );
}

fn dispatch(router: &PrimaryRouterHandle, request: RendererBridgeRequest) {
    let mut retained = Some(request);
    assert!(matches!(
        router
            .dispatch_renderer_request_retained(&mut retained)
            .expect("existing retained dispatcher"),
        RendererRequestDisposition::Complete(NativePublicationDisposition::Handled)
    ));
    assert!(
        retained.is_none(),
        "successful handled input physically drops"
    );
}

impl Fixture<'_> {
    fn preflight(&self) {
        let request = self
            .native
            .original_call_request(
                2,
                self.role.clone(),
                RendererBridgeOperation::Preflight {
                    channel: CHANNEL.to_owned(),
                },
                Vec::new(),
            )
            .expect("same native source facts");
        dispatch(self.router, request);
        assert!(matches!(
            self.outcomes.try_recv(),
            Ok(RendererBridgeOutcome::Preflight {
                request: 2,
                accepted: true,
                ..
            })
        ));
    }

    fn assert_no_data_before_writer_fence(&mut self) {
        assert!(
            self.router
                .compat
                .as_ref()
                .expect("same Compat")
                .lock()
                .expect("same DATA lane")
                .ledger
                .data[Direction::RendererToMain.index()]
            .is_none()
        );
        // The actual same writer's next fixed reply is a FIFO observation fence.
        // A wrongly published DATA frame must appear before this reply.
        let prepared = self
            .writer
            .try_prepare_control(
                Arc::clone(self.target),
                ControlMessage::CompatReleased {
                    correlation: CorrelationId(900),
                },
                Instant::now() + Duration::from_secs(3),
            )
            .expect("same physical writer control position");
        let observation = {
            let _transition = self.router.shutdown.transition_guard();
            let current = self
                .router
                .current
                .lock()
                .expect("same actual caller fence");
            admit_locked(current.as_ref().expect("same active caller"), prepared)
                .expect("actual fixed reply admission")
                .observation()
        };
        let (header, _) = keld_ipc::link::read_frame(self.peer).expect("actual fixed reply");
        assert_eq!(
            (header.kind, header.corr),
            (FrameKind::Reply, CorrelationId(900)),
            "no DATA publication may precede the real writer fence"
        );
        observation
            .wait_quiesced()
            .expect("same write physically returned");
    }
}

#[test]
fn original_call_two_publishes_ipc_message_one_through_same_writer() {
    fixture(|f| {
        f.preflight();
        let request = f
            .native
            .original_call_request(
                1,
                f.role.clone(),
                RendererBridgeOperation::Send {
                    channel: CHANNEL.to_owned(),
                    original_call: 2,
                },
                PAYLOAD.to_vec(),
            )
            .expect("same original Send source");
        let url = request
            .url()
            .expect("qualified native source URL")
            .to_owned();
        dispatch(f.router, request);
        let (header, bytes) = keld_ipc::link::read_frame(f.peer).expect("actual renderer DATA");
        assert_eq!(
            (header.kind, header.corr),
            (FrameKind::Event, CorrelationId(0))
        );
        assert_eq!(
            decode::<CompatControlEvent>(&bytes).expect("canonical actual DATA frame"),
            CompatControlEvent::RendererMessage {
                webview: f.route.document.stamp().view().0,
                navigation: f.route.document.stamp().observed_generation(),
                message: 1,
                window_id: f.route.window.id.0,
                window_incarnation: f.route.window.incarnation.0,
                url,
                origin: "file://".to_owned(),
                topic: CHANNEL.to_owned(),
                payload: PAYLOAD.to_vec(),
            }
        );
        let state = f
            .router
            .compat
            .as_ref()
            .expect("same Compat")
            .lock()
            .expect("same ledger");
        assert!(state.ledger.preflight.is_none());
        let charge = state.ledger.data[Direction::RendererToMain.index()]
            .as_ref()
            .expect("same actual DATA source");
        assert_eq!(
            charge.key.message, 1,
            "message identity remains independent"
        );
        assert!(charge.owned_by(f.caller));
        assert!(
            charge
                .document
                .as_ref()
                .is_some_and(|document| { document.same_binding(&f.route.document) })
        );
        assert!(
            charge.native_done,
            "actual input backing dropped through dispatcher"
        );
        assert!(
            !charge.consumer_done,
            "pipe publication is not consumer execution"
        );
        assert!(
            f.outcomes.try_recv().is_err(),
            "no invented consumed result"
        );
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mismatch {
    Call,
    Role,
    Document,
    Channel,
}

#[test]
fn mismatched_original_call_role_document_or_channel_never_publishes_data() {
    for mismatch in [
        Mismatch::Call,
        Mismatch::Role,
        Mismatch::Document,
        Mismatch::Channel,
    ] {
        fixture(|mut f| {
            f.preflight();
            let foreign = (mismatch == Mismatch::Document).then(|| {
                WindowSession::new(true)
                    .fixture_original_retirement_route(WindowOwner::new(
                        f.caller.identity,
                        f.caller.attempt,
                    ))
                    .expect("existing foreign same-number native lease fixture")
            });
            let native = foreign.as_ref().map_or(f.native, |(_, native, route)| {
                assert_eq!(
                    (
                        route.document.stamp().view(),
                        route.document.stamp().observed_generation()
                    ),
                    (
                        f.route.document.stamp().view(),
                        f.route.document.stamp().observed_generation()
                    )
                );
                assert!(!route.document.same_binding(&f.route.document));
                native
            });
            let role = if mismatch == Mismatch::Role {
                RendererRoleWitness::new(Arc::new(AtomicBool::new(false)))
            } else {
                f.role.clone()
            };
            let request = native
                .original_call_request(
                    1, // Same IPC message for every case.
                    role,
                    RendererBridgeOperation::Send {
                        channel: if mismatch == Mismatch::Channel {
                            "rendererReq-mismatch".to_owned()
                        } else {
                            CHANNEL.to_owned()
                        },
                        original_call: if mismatch == Mismatch::Call { 1 } else { 2 },
                    },
                    PAYLOAD.to_vec(),
                )
                .expect("same retained test source facts");
            dispatch(f.router, request);
            if matches!(mismatch, Mismatch::Call | Mismatch::Channel) {
                assert!(matches!(
                    f.outcomes.try_recv(),
                    Ok(RendererBridgeOutcome::SendResult {
                        request: 1,
                        disposition: WvTerminalDisposition::Refused { code, .. },
                        ..
                    }) if code == "KELD-WV-011"
                ));
            }
            assert!(
                f.outcomes.try_recv().is_err(),
                "foreign role/document must not invent a terminal"
            );
            {
                let state = f
                    .router
                    .compat
                    .as_ref()
                    .expect("same Compat")
                    .lock()
                    .expect("same preflight");
                if mismatch == Mismatch::Channel {
                    assert!(
                        state.ledger.preflight.is_none(),
                        "refusal drains its own original call2"
                    );
                } else {
                    let pending = state
                        .ledger
                        .preflight
                        .as_ref()
                        .expect("original call2 remains owned");
                    assert_eq!(pending.call, 2);
                    assert!(same_caller(&pending.caller, f.caller));
                    assert!(pending.document.same_binding(&f.route.document));
                    assert_eq!(pending.channel, CHANNEL);
                }
            }
            f.assert_no_data_before_writer_fence();
        });
    }
}
