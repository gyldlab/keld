//! One exact accepted load and retained call across the final Core admission cut.
#![forbid(unsafe_code)]

use super::super::original_load::{LoadDispatch, PreparedNativeLoad};
use super::{
    ICoreWebView2, ICoreWebView2Environment, InstallStatus, Instant, NativeNavigationContext,
    OriginalNavigationFact, OriginalView, RendererRoleWitness,
};
use crate::engine::{NativeNavigationRefusal, OriginalEntryQuery};
use crate::renderer_contract::{NativeAttemptObservation, NativeDocumentStamp};

impl OriginalView {
    #[expect(
        clippy::too_many_lines,
        clippy::needless_pass_by_value,
        reason = "Own the accepted role through the complete single admission and native-return turn"
    )]
    pub(in crate::webview2) fn load_on_ui(
        &mut self,
        core: &ICoreWebView2,
        environment: &ICoreWebView2Environment,
        context: NativeNavigationContext,
        query: &OriginalEntryQuery,
        role: RendererRoleWitness,
    ) -> OriginalNavigationFact {
        let refuse = |error| OriginalNavigationFact::Refused { error };
        if context.view() != self.id {
            return refuse(NativeNavigationRefusal::UnknownView);
        }
        if self.status(Instant::now()) != InstallStatus::Ready
            || self
                .call
                .as_ref()
                .is_some_and(|call| !call.native_quiesced())
        {
            return refuse(NativeNavigationRefusal::Unavailable);
        }
        let prepared = match PreparedNativeLoad::prepare(core, environment, &self.navigation, query)
        {
            Ok(prepared) => prepared,
            Err(error) => return refuse(error),
        };
        let (prepared, call) = prepared.into_parts();
        // Store the exact new call BEFORE dropping the already-quiescent old one.
        let previous_call = self.call.replace(call);
        drop(previous_call);
        let Some(call) = self.call.as_ref() else {
            self.session
                .latch_failure("retained native load call disappeared", self.id);
            return refuse(NativeNavigationRefusal::Unavailable);
        };
        let mut prepared = Some(prepared);
        let mut accepted = None;
        let mut revoked: Option<NativeDocumentStamp> = None;
        let mut refusal = NativeNavigationRefusal::Unavailable;
        let captured_role = role.clone();
        let mut enter = || {
            let Some(prepared) = prepared.take() else {
                return false;
            };
            let Ok(mut navigation) = self.navigation.try_borrow_mut() else {
                return false;
            };
            revoked = navigation.current_document().map(|lease| lease.stamp());
            match navigation.begin_host_load(
                prepared,
                context,
                captured_role.clone(),
                Instant::now(),
            ) {
                Ok(attempt) => {
                    accepted = Some(attempt);
                    true
                }
                Err(error) => {
                    refusal = error;
                    false
                }
            }
        };
        let admitted = role.try_admit_native(&mut enter);
        // Only local scalar branching separates the released canonical fence
        // from invocation on the already-retained native target and arguments.
        let (attempt, dispatch) = match (admitted, accepted) {
            (true, Some(attempt)) => (attempt, call.invoke()),
            (_, Some(attempt)) => {
                self.session.latch_failure(
                    "native load fence disagreed with its actual admission",
                    self.id,
                );
                if let Ok(mut navigation) = self.navigation.try_borrow_mut() {
                    navigation.fail_attempt(attempt);
                }
                return OriginalNavigationFact::Failed {
                    attempt: NativeAttemptObservation(attempt),
                    revoked,
                };
            }
            (admitted, None) => {
                if admitted {
                    self.session.latch_failure(
                        "native load fence returned without an admitted attempt",
                        self.id,
                    );
                }
                return refuse(refusal);
            }
        };
        let matched = self
            .navigation
            .try_borrow()
            .is_ok_and(|navigation| navigation.active_attempt() == Some(attempt));
        if matches!(&dispatch, LoadDispatch::Returned(Ok(()))) && matched {
            return OriginalNavigationFact::Started {
                attempt: NativeAttemptObservation(attempt),
            };
        }
        if !matches!(&dispatch, LoadDispatch::Returned(_)) {
            self.session
                .latch_failure("admitted native load has no proved return", self.id);
        }
        if let Ok(mut navigation) = self.navigation.try_borrow_mut() {
            navigation.fail_attempt(attempt);
        } else {
            self.session
                .latch_failure("native load failure met a held document borrow", self.id);
        }
        OriginalNavigationFact::Failed {
            attempt: NativeAttemptObservation(attempt),
            revoked,
        }
    }
}
