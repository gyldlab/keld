//! Retirement of a prepared facade engine that never entered its UI loop.

use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use super::{
    PersistentStopTransition, ProfileErrorKind, ProfileReleaseWait, ReleaseSite, WebView2Engine,
    commit_profile_stop, ephemeral_scavenge_root, profile_failure_with, scavenge_after_release,
    synchronize_profile_stop,
};
use crate::WvError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeViewBuild {
    NotAttempted,
    Attempted,
}

impl WebView2Engine {
    /// Retires a prepared zero-controller application engine before loop entry.
    ///
    /// Core uses this when facade preparation succeeded but primary/router startup
    /// did not transfer the engine into the ordinary application loop. The borrowed
    /// Host witness remains mandatory. This method never publishes Host Ready or
    /// enters a private Tao loop, and uses the existing stop and release owners.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` if any native view build was attempted, the loop ran,
    /// a browser binding or native view exists, a queued release fault is present,
    /// or the existing stop, Host witness, or observer retirement cannot complete.
    /// Invalid preconditions leave this engine untouched for its existing native
    /// retirement path. Once valid retirement starts, the engine is sealed against
    /// later creation. A failed retirement proof does not grant Idle credit.
    pub fn retire_unstarted_app<E: fmt::Display>(
        &mut self,
        release_witness: impl FnOnce(Instant) -> Result<(), E>,
    ) -> Result<(), WvError> {
        if self.event_loop.is_none()
            || self.view_build_attempt != NativeViewBuild::NotAttempted
            || !self.views.is_empty()
            || self.app_window_created
            || self.pending_app_events.is_some()
            || self.browser_binding.load().is_bound()
        {
            return Err(profile_failure_with(
                ProfileErrorKind::LifecycleUnproven,
                "pre-run retirement requires an unstarted engine with no native view build attempt",
            ));
        }
        // Preconditions above are read-only. Keep the native loop alive during
        // checked callback retirement, but remove future construction authority.
        let _unrun_loop = self.event_loop.take();
        let scavenge_root = ephemeral_scavenge_root(self.profile.as_ref());
        let stop = PersistentStopTransition::from_profile(self.profile.as_ref());
        commit_profile_stop(stop.as_ref());
        let release = Rc::new(ProfileReleaseWait::new(
            self.browser_exit.take(),
            Arc::clone(&self.browser_binding),
            None,
            ReleaseSite::Host,
        ));
        // The existing no-controller exemption needs no invented native receipt.
        // Arming still drains the one receipt source and starts the witness budget.
        release.arm();
        if let Some(fault) = release.fault() {
            return Err(profile_failure_with(
                ProfileErrorKind::LifecycleUnproven,
                fault.context(release.site),
            ));
        }
        synchronize_profile_stop(stop.as_ref(), self.profile.as_mut())?;
        let release = release.retire_loop_wait()?;
        self.finish_app_profile_release(&release, release_witness)?;
        scavenge_after_release(scavenge_root)
    }

    /// Check before any creation-side lifecycle or observation mutation.
    pub(super) fn creation_event_loop(
        &self,
    ) -> Result<&tao::event_loop::EventLoop<super::WindowsLoopEvent>, WvError> {
        self.event_loop.as_ref().ok_or_else(|| {
            WvError::EventLoop(String::from(
                "run loop already started; create webviews before run_until_closed",
            ))
        })
    }

    pub(super) fn finish_app_profile_release<E: fmt::Display>(
        &mut self,
        release: &ProfileReleaseWait,
        release_witness: impl FnOnce(Instant) -> Result<(), E>,
    ) -> Result<(), WvError> {
        self.finish_profile_release_with(release.observed.get(), || {
            release_witness(release.release_witness_deadline()?).map_err(|error| {
                eprintln!("KELD_WV_RELEASE_WITNESS_FAILED {error}");
                profile_failure_with(
                    ProfileErrorKind::LifecycleUnproven,
                    "the host non-family process release witness failed; profile ownership is retained",
                )
            })
        })
    }
}
