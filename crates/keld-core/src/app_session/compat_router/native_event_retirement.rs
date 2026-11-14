//! A sealed native retirement settles impossible future protocol obligations
//! without manufacturing Consumed/FunctionFinished or dropping physical backing.
use super::{
    Arc, Charge, CompatLedger, CompatSession, Direction, HostAppError, MacNativeDocumentLease,
    Mutex, NativeRouteKey, PrimaryRouterHandle, RendererBridgeRequest, RendererRoleWitness,
    app_detail, compat_control,
};
#[cfg(all(test, debug_assertions))]
use super::{
    CompatControlResponse, ConsumerDisposition, CorrelationId, PrimaryCallerContext,
    RendererBridgeOutcome, TerminalDisposition, WindowOwner, WvConsumerDisposition,
};
use keld_wv::renderer_bridge::NativeEventRetirement;
use keld_wv::wv_link::WvDispatchCertainty;

pub(super) fn original_role_matches(role: &RendererRoleWitness, charge: &Charge) -> bool {
    role.same_role(&RendererRoleWitness::new(Arc::clone(
        &charge.owner.reader_stop,
    )))
}

pub(super) fn request_has_original_role(request: &RendererBridgeRequest, charge: &Charge) -> bool {
    request
        .role_witness()
        .is_some_and(|role| original_role_matches(role, charge))
}

// Production construction below borrows only the sealed Wv fact. Tests in this
// private module model native proof explicitly; they do not mint a public fact.
struct RetirementInput<'a> {
    delivery: u32,
    document: &'a MacNativeDocumentLease,
    role: &'a RendererRoleWitness,
    certainty: compat_control::DispatchCertainty,
}
impl<'a> From<&'a NativeEventRetirement> for RetirementInput<'a> {
    fn from(fact: &'a NativeEventRetirement) -> Self {
        Self {
            delivery: fact.delivery(),
            document: fact.document(),
            role: fact.role(),
            certainty: match fact.certainty() {
                WvDispatchCertainty::NotDispatched => {
                    compat_control::DispatchCertainty::NotDispatched
                }
                WvDispatchCertainty::OutcomeUnknown => {
                    compat_control::DispatchCertainty::OutcomeUnknown
                }
            },
        }
    }
}

impl CompatLedger {
    fn apply_native_event_retirement(&mut self, fact: &RetirementInput<'_>) -> bool {
        let Some(charge) = self.data[Direction::MainToRenderer.index()].as_mut() else {
            return false;
        };
        let stamp = fact.document.stamp();
        if charge.key
            != (NativeRouteKey {
                webview: stamp.view().0,
                navigation: stamp.observed_generation(),
                message: fact.delivery,
            })
            || !charge
                .document
                .as_ref()
                .is_some_and(|held| held.same_binding(fact.document))
            || !original_role_matches(fact.role, charge)
        {
            return false;
        }
        // Keep the first actual retirement proof. Existing consumer selection,
        // moved source strings, and any admitted/written response remain intact.
        if charge.native_retirement.is_none() {
            charge.native_retirement = Some(fact.certainty);
        }
        if charge.terminal_deadline.is_none() && charge.pending_reply.is_some() {
            charge.terminal_deadline =
                Some(std::time::Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE);
        }
        true
    }
}

impl PrimaryRouterHandle {
    pub(super) fn retire_native_event(
        &self,
        compat: &Arc<Mutex<CompatSession>>,
        fact: &NativeEventRetirement,
    ) -> Result<crate::app_session::window_router::NativePublicationDisposition, HostAppError> {
        let matched = compat
            .lock()
            .map_err(|_| {
                app_detail(
                    "compat native event retirement",
                    "same ledger lock poisoned",
                )
            })?
            .ledger
            .apply_native_event_retirement(&RetirementInput::from(fact));
        if matched {
            self.service_compat_writer_progress()?;
        }
        Ok(crate::app_session::window_router::NativePublicationDisposition::Handled)
    }
}

#[cfg(all(test, debug_assertions))]
pub(super) mod tests;
