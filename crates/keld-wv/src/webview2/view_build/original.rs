//! The original preload runs inside the existing retained NativeBuild candidate.
#![forbid(unsafe_code)]

use std::rc::Rc;
use std::time::Instant;
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};

use super::super::original_bridge::InstallStatus;
use super::super::original_callbacks::Observation;
use super::super::original_view::{OriginalView, ViewInputs};
use super::{
    BuildCompletion, BuildFailure, BuildNavigation, BuildProgress, BuildResources, CompletedBuild,
    ManagedBuild, NativeBuild, ProfileReleaseWait, WebviewId, WindowsLoopEvent, WvError, candidate,
    configure_application_name_for_user_agent,
};
use crate::ApplicationNameForUserAgent;
use crate::engine::WebviewSpec;
use crate::resource_intake::{MAX_PENDING_METADATA_BYTES, RetainedByteCredit};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::TestControl;

pub(in crate::webview2) struct OriginalBuild {
    pub(in crate::webview2) inputs: ViewInputs,
    pub(in crate::webview2) observer: Observation,
    pub(in crate::webview2) deadline: Instant,
    pub(in crate::webview2) application_name_for_user_agent: Option<ApplicationNameForUserAgent>,
}

impl NativeBuild {
    /// This entry is for the selected command owner after its real endpoint
    /// and renderer receiver attach. It never falls through to ordinary navigation.
    pub(in crate::webview2) fn begin_original(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        spec: &WebviewSpec,
        managed: &ManagedBuild<'_>,
        original: OriginalBuild,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] test_control: Option<
            &mut TestControl,
        >,
    ) -> Result<(), BuildFailure> {
        let progress = self.build_navigation(
            target,
            proxy,
            spec,
            Some(managed),
            BuildNavigation::Original(original),
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_control,
        )?;
        match progress {
            BuildProgress::OriginalPending => Ok(()),
            BuildProgress::Complete(_) => Err(BuildFailure::webview(candidate::candidate_error(
                "selected construction completed without its native preload receipt",
            ))),
        }
    }

    pub(super) fn attach_original(
        &mut self,
        id: WebviewId,
        deadline: Instant,
        build: u64,
        original: OriginalBuild,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
    ) -> Result<(), WvError> {
        let application_name = original
            .application_name_for_user_agent
            .map(|name| (name, original.inputs.source.retained_credit_account()));
        let prepared = OriginalView::prepare(original.inputs, id)?;
        self.view_mut()?.original = Some(prepared);
        self.completed = Some(BuildCompletion::OriginalPending {
            build,
            id,
            deadline,
        });
        let Some(BuildResources::View(view)) = self.candidate.as_mut() else {
            return Err(candidate::candidate_error(
                "selected build lost its retained View",
            ));
        };
        let guard = Self::install_guard(&view.webview, id)?;
        Self::configure_guarded_shell(view, id, Some(proxy))?;
        if let Some((name, account)) = application_name {
            // Bound the native strings and UTF-8 copies on this source's account.
            // Release this temporary reservation before preload installation.
            let _credit = RetainedByteCredit::reserve(&account, MAX_PENDING_METADATA_BYTES)
                .map_err(|_| {
                    candidate::candidate_error(
                        "applicationNameForUserAgent metadata budget exhausted",
                    )
                })?;
            configure_application_name_for_user_agent(&guard, &name)?;
        }
        let original_view = view.original.as_mut().ok_or_else(|| {
            candidate::candidate_error("selected build lost its attached original owner")
        })?;
        original_view.install_on_ui(
            &guard,
            &self.environment,
            Rc::clone(&view.close_custody),
            original.observer,
            proxy,
            deadline,
        )
    }

    /// The owning UI loop uses this original absolute deadline even if a
    /// callback wake never arrives. No event or readiness is published here.
    pub(in crate::webview2) fn pending_original(&self) -> Option<(u64, Instant)> {
        match self.completed {
            Some(BuildCompletion::OriginalPending {
                build, deadline, ..
            }) => Some((build, deadline)),
            _ => None,
        }
    }

    /// Complete native Create only after the retained View has real renderer
    /// subscriptions and the script completion. This conveys no navigation,
    /// document binding, renderer registration, or AppRegistered receipt.
    pub(in crate::webview2) fn resume_original_preload(
        &mut self,
        expected_build: u64,
        now: Instant,
        release: &ProfileReleaseWait,
    ) -> Result<Option<CompletedBuild>, BuildFailure> {
        super::resources::contain_build(|| {
            self.complete_original_preload(expected_build, now, release)
        })
    }

    fn complete_original_preload(
        &mut self,
        expected_build: u64,
        now: Instant,
        release: &ProfileReleaseWait,
    ) -> Result<Option<CompletedBuild>, BuildFailure> {
        let id =
            pending_original_id(self.completed, expected_build).map_err(BuildFailure::blocked)?;
        let owner = self
            .view()
            .map_err(BuildFailure::webview)?
            .original
            .as_ref()
            .ok_or_else(|| {
                BuildFailure::webview(candidate::candidate_error(
                    "selected preload owner is absent",
                ))
            })?;
        if owner.id() != id {
            return Err(BuildFailure::webview(candidate::candidate_error(
                "selected preload wake disagrees with retained View identity",
            )));
        }
        match owner.preload_status(now) {
            InstallStatus::Pending => Ok(None),
            InstallStatus::Ready => {
                let view = self.view().map_err(BuildFailure::webview)?;
                view.window.set_visible(true);
                let _ = view.return_focus_if_active();
                let snapshot = Some(super::snapshot(view).map_err(BuildFailure::webview)?);
                release.poll();
                if let Some(fault) = release.fault() {
                    return Err(BuildFailure::webview(super::super::profile_failure_with(
                        crate::profile::ProfileErrorKind::LifecycleUnproven,
                        fault.context(release.site),
                    )));
                }
                self.completed = Some(BuildCompletion::Ready { id, snapshot });
                Ok(Some(CompletedBuild { id, snapshot }))
            }
            InstallStatus::Refused => Err(BuildFailure::webview(candidate::candidate_error(
                "selected preload did not complete for its original creator and deadline",
            ))),
        }
    }
}

fn pending_original_id(
    completion: Option<BuildCompletion>,
    expected_build: u64,
) -> Result<WebviewId, WvError> {
    let Some(BuildCompletion::OriginalPending { build, id, .. }) = completion else {
        return Err(candidate::candidate_error(
            "selected preload wake has no pending candidate",
        ));
    };
    if build != expected_build {
        return Err(candidate::candidate_error(
            "selected preload wake does not identify this candidate",
        ));
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::{BuildCompletion, Instant, WebviewId, pending_original_id};

    #[test]
    fn selected_preload_resume_matches_only_its_pending_native_view() {
        let id = WebviewId(8);
        let pending = Some(BuildCompletion::OriginalPending {
            build: 17,
            id,
            deadline: Instant::now(),
        });
        assert_eq!(pending_original_id(pending, 17).unwrap(), id);
        assert!(pending_original_id(pending, 18).is_err());
        assert!(pending_original_id(None, 17).is_err());
        assert!(
            pending_original_id(Some(BuildCompletion::Ready { id, snapshot: None }), 17).is_err()
        );
    }
}
