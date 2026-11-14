//! Original preload completion resumes the same retained `NativeBuild` candidate.
#![forbid(unsafe_code)]

use super::super::original_bridge::InstallStatus;
use super::super::original_view::OriginalView;
use super::{
    BuildCompletion, BuildFailure, BuildNavigation, BuildProgress, BuildResources, CompletedBuild,
    EventLoopProxy, InitialNavigation, Instant, NativeBuild, ProfileReleaseWait, Rc, WebviewId,
    WindowsLoopEvent, WvError, candidate, resources, snapshot,
};

impl NativeBuild {
    pub(super) fn attach_original(
        &mut self,
        navigation: BuildNavigation,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        id: WebviewId,
        deadline: Instant,
    ) -> Result<Option<InitialNavigation>, WvError> {
        let BuildNavigation::Original(original) = navigation else {
            let BuildNavigation::Renderer(renderer) = navigation else {
                return Err(WvError::EventLoop(String::from(
                    "invalid native build navigation",
                )));
            };
            return Ok(renderer);
        };
        let prepared = OriginalView::prepare(
            original.inputs,
            original.session,
            original.endpoint,
            proxy.clone(),
            original.events,
            id,
            deadline,
        )?;
        self.view_mut()?.original = Some(prepared);
        let Self {
            environment,
            candidate,
            ..
        } = self;
        let Some(BuildResources::View(view)) = candidate.as_mut() else {
            return Err(candidate::candidate_error(
                "original attachment lost its actual View",
            ));
        };
        view.original
            .as_mut()
            .ok_or_else(|| candidate::candidate_error("original attachment owner disappeared"))?
            .attach_native_owners(&view.webview, environment, Rc::clone(&view.close_custody))?;
        Ok(None)
    }

    pub(in crate::webview2) fn pending_original(&self) -> Option<(u64, Instant)> {
        match self.completed {
            Some(BuildCompletion::OriginalPending {
                build, deadline, ..
            }) => Some((build, deadline)),
            _ => None,
        }
    }

    pub(in crate::webview2) fn resume_original(
        &mut self,
        release: &ProfileReleaseWait,
    ) -> Result<BuildProgress, BuildFailure> {
        resources::contain_build(|| {
            let Some(BuildCompletion::OriginalPending { id, .. }) = self.completed else {
                return Err(BuildFailure::blocked(candidate::candidate_error(
                    "no original preload completion belongs to this builder",
                )));
            };
            let view = self.view().map_err(BuildFailure::webview)?;
            let original = view.original.as_ref().ok_or_else(|| {
                BuildFailure::webview(candidate::candidate_error(
                    "pending preload lost its retained original View",
                ))
            })?;
            match original.build_status(Instant::now()) {
                InstallStatus::Pending => return Ok(BuildProgress::PendingOriginal),
                InstallStatus::Refused => {
                    return Err(BuildFailure::webview(WvError::Webview(String::from(
                        "original preload did not complete for its live creator before the original deadline",
                    ))));
                }
                InstallStatus::Ready => {}
            }
            // A real successful AddScript return and its matching completion
            // preceded this first show. No nested pump or replacement owner.
            view.window.set_visible(true);
            let _ = view.return_focus_if_active();
            let snapshot = Some(snapshot(view).map_err(BuildFailure::webview)?);
            release.poll();
            if let Some(fault) = release.fault() {
                return Err(BuildFailure::webview(super::super::profile_failure_with(
                    crate::profile::ProfileErrorKind::LifecycleUnproven,
                    fault.context(release.site),
                )));
            }
            self.completed = Some(BuildCompletion::Ready { id, snapshot });
            Ok(BuildProgress::Complete(CompletedBuild { id, snapshot }))
        })
    }
}
