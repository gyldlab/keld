//! Retain the actual load in its guarded View across the one native invocation.
#![forbid(unsafe_code)]

use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::{NativeAttemptObservation, NativeDocumentStamp};
use crate::{
    NativeNavigationContext, NativeNavigationRefusal, OriginalEntryQuery, OriginalNavigationFact,
};

use super::super::original_load::{LoadDispatch, PreparedNativeLoad};
use super::{InstallStatus, Instant, OriginalView, WvError, failure};

use super::super::original_navigation::OriginalNavigation;

#[derive(Clone, Copy)]
pub(super) struct InFlightLoad {
    context: NativeNavigationContext,
    attempt: NativeAttemptObservation,
    previous: Option<NativeDocumentStamp>,
    failure_reported: bool,
}

impl InFlightLoad {
    pub(super) fn observe_failure(
        &mut self,
        context: Option<NativeNavigationContext>,
        fact: &mut OriginalNavigationFact,
    ) {
        if let OriginalNavigationFact::Failed { attempt, revoked } = fact
            && context == Some(self.context)
            && *attempt == self.attempt
        {
            if revoked.is_none() {
                *revoked = self.previous;
            }
            self.failure_reported = true;
        }
    }
}

#[derive(Clone, Copy)]
struct PriorDocument {
    context: Option<NativeNavigationContext>,
    stamp: NativeDocumentStamp,
}

impl PriorDocument {
    fn capture(nav: &OriginalNavigation) -> Option<Self> {
        Some(Self {
            context: nav.host_context(),
            stamp: NativeDocumentStamp(nav.document().current_stamp()?),
        })
    }

    fn revoked_by(&self, nav: &OriginalNavigation) -> bool {
        nav.document().current_stamp() != Some(self.stamp.0)
    }
}

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
        let mut refused_revocation = None;
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
            let previous_document = PriorDocument::capture(&navigation);
            match navigation.begin_host_load(qualification, context, role.clone(), Instant::now()) {
                Ok(attempt) => {
                    // No native destructor runs inside the Core fence or before
                    // the new call's immediate entry. A refusal leaves self.call untouched.
                    previous_call = std::mem::replace(&mut self.call, pending_call.take());
                    accepted = Some(attempt);
                    revoked = previous_document.map(|prior| prior.stamp);
                    self.inflight_load.set(Some(InFlightLoad {
                        context,
                        attempt: NativeAttemptObservation(attempt),
                        previous: revoked,
                        failure_reported: false,
                    }));
                    true
                }
                Err(error) => {
                    refused_revocation =
                        previous_document.filter(|prior| prior.revoked_by(&navigation));
                    refusal = error;
                    false
                }
            }
        };
        let admitted = role.try_admit_native(&mut enter);
        let Some(attempt) = accepted else {
            if let Some(prior) = refused_revocation {
                if let (Some(previous_context), Some(observer)) = (prior.context, &self.observer) {
                    observer(
                        Some(previous_context),
                        OriginalNavigationFact::Revoked {
                            previous: Some(prior.stamp),
                        },
                    );
                } else {
                    self.session.latch_failure(
                        "selected refusal revoked a document without its Host observer",
                        self.id,
                    );
                }
            }
            if admitted {
                self.session.latch_failure(
                    "selected load fence accepted without a native attempt",
                    self.id,
                );
            }
            return refuse(refusal);
        };
        let observed = NativeAttemptObservation(attempt);
        let dispatch = if let (true, Some(call)) = (admitted, self.call.as_ref()) {
            call.invoke()
        } else {
            self.session
                .latch_failure("selected load fence and retained call disagree", self.id);
            LoadDispatch::OutcomeUnknown
        };
        drop(previous_call);
        let failure_reported = self
            .inflight_load
            .take()
            .is_some_and(|call| call.failure_reported);
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
        (!failure_reported).then_some(OriginalNavigationFact::Failed {
            attempt: observed,
            revoked,
        })
    }

    pub(in crate::webview2) fn expire_original_navigation(
        &self,
        now: Instant,
    ) -> Result<(), WvError> {
        let expiration = {
            let mut nav = self
                .navigation
                .try_borrow_mut()
                .map_err(|_| failure("selected navigation timeout met a borrowed owner"))?;
            let observed = (
                nav.host_context(),
                nav.active_attempt(),
                nav.current_document().map(|lease| lease.stamp()),
            );
            nav.expire_unfinished(now).then_some(observed)
        };
        if let Some((Some(context), Some(attempt), revoked)) = expiration {
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use std::rc::Rc;
    use std::sync::{Arc, atomic::AtomicBool};

    use super::*;
    use crate::WebviewId;
    use crate::resource_source::windows::tests::Fixture;

    #[test]
    fn exhausted_admission_retains_the_real_prior_stamp_but_ordinary_refusal_does_not_revoke() {
        let fixture = Fixture::new();
        let view = WebviewId(71);
        let context = NativeNavigationContext::new(NonZeroU64::new(19).unwrap(), view);
        let source = fixture.source_arc();
        let mut nav = OriginalNavigation::new(view, Arc::clone(&source)).unwrap();
        let query = OriginalEntryQuery::from_optional_query(None).unwrap();
        let now = Instant::now();
        let prepared = nav.prepare(&query).unwrap();
        let holder = Rc::clone(&prepared);
        let role = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
        let attempt = nav
            .begin_modeled_host_load(prepared, context, role.clone(), now)
            .unwrap();
        assert!(holder.begin_native_call());
        assert!(holder.native_returned(true));
        let native = NonZeroU64::new(42).unwrap();
        assert!(nav.navigation_starting(
            native,
            holder.uri(),
            Some(holder.header_value()),
            false,
            now
        ));
        let stamp = nav
            .document_mut()
            .commit_admitted(
                attempt,
                source.source_id(),
                crate::native_document::NativeCommitIdentity(native),
            )
            .unwrap();
        let lease = nav.current_document().unwrap();
        let prior = PriorDocument::capture(&nav).unwrap();

        let stopped = RendererRoleWitness::new(Arc::new(AtomicBool::new(true)));
        let prepared = nav.prepare(&query).unwrap();
        assert_eq!(
            nav.begin_modeled_host_load(prepared, context, stopped, now),
            Err(NativeNavigationRefusal::Unavailable)
        );
        assert!(!prior.revoked_by(&nav));
        assert!(lease.is_current());

        let next = NativeNavigationContext::new(NonZeroU64::new(20).unwrap(), view);
        let prepared = nav.prepare(&query).unwrap();
        nav.document_mut().exhaust_next_attempt_for_test();
        assert_eq!(
            nav.begin_modeled_host_load(prepared, next, role, now),
            Err(NativeNavigationRefusal::Unavailable)
        );
        assert!(prior.revoked_by(&nav));
        assert_eq!(prior.context, Some(context));
        assert_eq!(prior.stamp, NativeDocumentStamp(stamp));
        assert!(!lease.is_current());
        assert!(nav.active_attempt().is_none());
    }

    #[test]
    fn inline_failure_keeps_the_exact_prior_revocation_and_only_matches_its_own_attempt() {
        let fixture = Fixture::new();
        let source = fixture.source_arc();
        let view = WebviewId(71);
        let mut owner = crate::native_document::NativeDocumentOwner::new(view);
        owner.bind_source(&source).unwrap();
        let first = owner.begin_host_load(source.source_id()).unwrap();
        let prior = owner
            .commit_admitted(
                first,
                source.source_id(),
                crate::native_document::NativeCommitIdentity(NonZeroU64::new(42).unwrap()),
            )
            .unwrap();
        let actual = NativeAttemptObservation(owner.begin_host_load(source.source_id()).unwrap());
        let current = NativeNavigationContext::new(NonZeroU64::new(19).unwrap(), view);
        let older = NativeNavigationContext::new(NonZeroU64::new(18).unwrap(), view);
        let prior = Some(NativeDocumentStamp(prior));
        let mut load = InFlightLoad {
            context: current,
            attempt: actual,
            previous: prior,
            failure_reported: false,
        };
        let empty = OriginalNavigationFact::Failed {
            attempt: actual,
            revoked: None,
        };
        let mut foreign = OriginalNavigationFact::Failed {
            attempt: NativeAttemptObservation(first),
            revoked: None,
        };
        load.observe_failure(Some(current), &mut foreign);
        assert!(!load.failure_reported);
        let mut wrong_context = OriginalNavigationFact::Failed {
            attempt: actual,
            revoked: None,
        };
        load.observe_failure(Some(older), &mut wrong_context);
        assert_eq!(wrong_context, empty);
        assert!(!load.failure_reported);

        let mut failure = empty;
        load.observe_failure(Some(current), &mut failure);
        assert_eq!(
            failure,
            OriginalNavigationFact::Failed {
                attempt: actual,
                revoked: prior
            }
        );
        assert!(load.failure_reported);
    }
}
