//! Retain the actual load in its guarded View across the one native invocation.
#![forbid(unsafe_code)]

use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::{NativeAttemptObservation, NativeDocumentStamp};
use crate::{
    NativeNavigationContext, NativeNavigationRefusal, OriginalEntryQuery, OriginalNavigationFact,
};

use super::super::original_load::{LoadDispatch, PreparedNativeLoad};
use super::{InstallStatus, Instant, OriginalView, WvError, failure};

impl OriginalView {
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the exact qualified request, current role fence, immediate entry and actual terminal observation in one turn."
    )]
    pub(in crate::webview2) fn load_on_ui(
        &mut self,
        context: NativeNavigationContext,
        query: &OriginalEntryQuery,
        role: RendererRoleWitness,
    ) -> Option<OriginalNavigationFact> {
        let refuse = |error| Some(OriginalNavigationFact::Refused { error });
        if context.view() != self.id {
            return refuse(NativeNavigationRefusal::UnknownView);
        }
        if self.installed_view_status(Instant::now()) != InstallStatus::Ready
            || !role.is_live()
            || !role.has_native_view_scope()
            || self
                .call
                .as_ref()
                .is_some_and(|call| !call.native_quiesced())
        {
            return refuse(NativeNavigationRefusal::Unavailable);
        }
        let Some(resource) = self.resource.as_ref() else {
            return refuse(NativeNavigationRefusal::MissingSource);
        };
        let prepared = match PreparedNativeLoad::prepare(resource, query) {
            Ok(prepared) => prepared,
            Err(error) => return refuse(error),
        };
        if self.installed_view_status(Instant::now()) != InstallStatus::Ready {
            return refuse(NativeNavigationRefusal::Unavailable);
        }
        let (qualification, call) = prepared.into_parts();
        let mut qualification = Some(qualification);
        let mut pending_call = Some(call);
        let mut previous_call = None;
        let mut accepted = None;
        let mut revoked = None;
        let mut refusal = NativeNavigationRefusal::Unavailable;
        self.observed_load_failure.set(None);
        let mut enter = || {
            if self.retired.get() || pending_call.is_none() {
                return false;
            }
            let Ok(mut navigation) = self.navigation.try_borrow_mut() else {
                return false;
            };
            let Some(qualification) = qualification.take() else {
                return false;
            };
            let previous_document = navigation.current_document().map(|lease| lease.stamp());
            match navigation.begin_host_load(qualification, context, role.clone(), Instant::now()) {
                Ok(attempt) => {
                    // No native destructor runs inside the Core fence or before
                    // the new call's immediate entry. A refusal leaves self.call untouched.
                    previous_call = std::mem::replace(&mut self.call, pending_call.take());
                    accepted = Some(attempt);
                    revoked = previous_document;
                    true
                }
                Err(error) => {
                    refusal = error;
                    false
                }
            }
        };
        let admitted = role.try_admit_native(&mut enter);
        let Some(attempt) = accepted else {
            if admitted {
                self.session.latch_failure(
                    "selected load fence accepted without a native attempt",
                    self.id,
                );
            }
            return refuse(refusal);
        };
        let observed = NativeAttemptObservation(attempt);
        let dispatch = match (admitted, self.call.as_ref()) {
            (true, Some(call)) => call.invoke(),
            _ => {
                self.session
                    .latch_failure("selected load fence and retained call disagree", self.id);
                LoadDispatch::OutcomeUnknown
            }
        };
        drop(previous_call);
        if matches!(&dispatch, LoadDispatch::Returned(Ok(())))
            && self
                .navigation
                .try_borrow()
                .is_ok_and(|nav| nav.active_attempt() == Some(attempt))
        {
            return Some(OriginalNavigationFact::Started { attempt: observed });
        }
        if !matches!(&dispatch, LoadDispatch::Returned(_)) {
            self.session
                .latch_failure("selected load has no proved native return", self.id);
        }
        if let Ok(mut navigation) = self.navigation.try_borrow_mut() {
            navigation.fail_attempt(attempt);
        } else {
            self.session.latch_failure(
                "selected Load failure met a borrowed document owner",
                self.id,
            );
        }
        failed_after_entry(
            self.observed_load_failure.take(),
            context,
            observed,
            revoked,
        )
    }

    pub(in crate::webview2) fn expire_original_navigation(
        &self,
        now: Instant,
    ) -> Result<(), WvError> {
        let observed = {
            let mut nav = self
                .navigation
                .try_borrow_mut()
                .map_err(|_| failure("selected navigation timeout met a borrowed owner"))?;
            let observed = (
                nav.host_context(),
                nav.active_attempt(),
                nav.current_document().map(|lease| lease.stamp()),
            );
            nav.expire_uncommitted(now).then_some(observed)
        };
        if let Some((Some(context), Some(attempt), revoked)) = observed {
            let observer = self.observer.as_ref().ok_or_else(|| {
                failure("selected navigation timeout lost its actual Host observer")
            })?;
            observer(
                Some(context),
                OriginalNavigationFact::Failed {
                    attempt: NativeAttemptObservation(attempt),
                    revoked,
                },
            );
        }
        Ok(())
    }

    pub(in crate::webview2) fn original_deadline(&self) -> Option<Instant> {
        self.navigation
            .try_borrow()
            .ok()
            .and_then(|nav| nav.next_deadline())
            .into_iter()
            .chain(self.renderer_deadline())
            .min()
    }
}

fn failed_after_entry(
    reported: Option<(NativeNavigationContext, NativeAttemptObservation)>,
    context: NativeNavigationContext,
    attempt: NativeAttemptObservation,
    revoked: Option<NativeDocumentStamp>,
) -> Option<OriginalNavigationFact> {
    // An inline native callback can already have delivered the terminal fact.
    // A callback for either a different Host context or a different native
    // attempt must not suppress this operation's actual failure.
    (reported != Some((context, attempt)))
        .then_some(OriginalNavigationFact::Failed { attempt, revoked })
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::{
        NativeAttemptObservation, NativeNavigationContext, OriginalNavigationFact,
        failed_after_entry,
    };
    use crate::WebviewId;
    use crate::native_document::NativeDocumentOwner;
    use crate::resource_source::windows::tests::Fixture;

    #[test]
    fn inline_failure_suppresses_only_the_exact_original_attempt() {
        let fixture = Fixture::new();
        let source = fixture.source_arc();
        let mut owner = NativeDocumentOwner::new(WebviewId(71));
        owner.bind_source(&source).unwrap();
        let first = owner.begin_host_load(source.source_id()).unwrap();
        owner.fail_attempt(first);
        let second = owner.begin_host_load(source.source_id()).unwrap();
        let actual = NativeAttemptObservation(second);
        let current = NativeNavigationContext::new(NonZeroU64::new(19).unwrap(), WebviewId(71));
        let older = NativeNavigationContext::new(NonZeroU64::new(18).unwrap(), WebviewId(71));

        assert!(failed_after_entry(Some((current, actual)), current, actual, None).is_none());
        for wrong in [
            None,
            Some((older, actual)),
            Some((current, NativeAttemptObservation(first))),
        ] {
            assert!(matches!(
                failed_after_entry(wrong, current, actual, None),
                Some(OriginalNavigationFact::Failed { attempt, .. }) if attempt == actual
            ));
        }
    }
}
