use super::*;
use crate::app_session::primary_writer::{PrimaryWriter, WriterLink};
use keld_wv::renderer_bridge::NativeDataReservation;
use keld_wv::renderer_contract::NativeDocumentFixture;
use std::sync::mpsc::Receiver;

pub(in crate::app_session::compat_router) struct EventFixture<'a> {
    pub router: &'a PrimaryRouterHandle,
    pub caller: &'a PrimaryCallerContext,
    pub target: &'a Arc<WriterLink>,
    pub peer: &'a mut crate::app_session::BootstrapStream,
    pub writer: &'a PrimaryWriter,
    pub owner: &'a Arc<Mutex<CompatSession>>,
    pub outcomes: &'a Receiver<RendererBridgeOutcome>,
    pub commands: &'a Receiver<keld_wv::AppWindowCommand>,
    pub native: &'a mut NativeDocumentFixture,
    pub key: NativeRouteKey,
    pub storage: Option<NativeDataReservation>,
}
impl EventFixture<'_> {
    pub fn role(&self) -> RendererRoleWitness {
        RendererRoleWitness::new(Arc::clone(&self.caller.reader_stop))
    }
    pub fn model_retirement(&mut self, certainty: compat_control::DispatchCertainty) {
        self.native.revoke(); // actual shared document-owner revocation, modeled native callback
        let document = self.native.document();
        let role = self.role();
        let mut state = self.owner.lock().expect("same DATA");
        assert!(
            state
                .ledger
                .apply_native_event_retirement(&RetirementInput {
                    delivery: self.key.message,
                    document: &document,
                    role: &role,
                    certainty,
                })
        );
    }
    pub fn wait_writes(&self) {
        for observation in self
            .writer
            .snapshot_target_writes(self.target)
            .expect("same five positions")
            .into_iter()
            .flatten()
        {
            observation.wait_quiesced().expect("actual write ended");
        }
    }
    pub fn no_fatal(&self) -> bool {
        let mut fatal = false;
        while let Ok(command) = self.commands.try_recv() {
            fatal |= matches!(command, keld_wv::AppWindowCommand::Fatal);
        }
        !fatal
    }
}
pub(in crate::app_session::compat_router) fn fixture(run: impl FnOnce(EventFixture<'_>)) {
    crate::app_session::compat_router::owned_tests::fixture(
        |router, caller, target, peer, writer, outcomes, commands| {
            let mut windows = crate::app_session::window_router::WindowSession::new(true);
            let (root, mut native, route) = windows
                .fixture_original_retirement_route(WindowOwner::new(
                    caller.identity,
                    caller.attempt,
                ))
                .expect("real retained source fixture");
            let key = NativeRouteKey {
                webview: route.document.stamp().view().0,
                navigation: route.document.stamp().observed_generation(),
                message: 77,
            };
            let owner = router.compat.as_ref().expect("existing Compat");
            let storage = {
                let mut state = owner.lock().expect("actual record");
                state
                    .ledger
                    .reserve(Direction::MainToRenderer, key, caller, 512)
                    .expect("one DATA");
                let row = state.ledger.data[Direction::MainToRenderer.index()]
                    .as_mut()
                    .expect("same row");
                row.document = Some(route.document.clone());
                row.native_window = Some(route.window);
                row.pending_reply = Some(CorrelationId(77));
                state
                    .ledger
                    .lease_native_data(caller, key, &route.document, Arc::clone(owner))
                    .expect("same prepaid native storage")
            };
            run(EventFixture {
                router,
                caller,
                target,
                peer,
                writer,
                owner,
                outcomes,
                commands,
                native: &mut native,
                key,
                storage: Some(storage),
            });
            drop(native);
            drop(root);
        },
    );
}
