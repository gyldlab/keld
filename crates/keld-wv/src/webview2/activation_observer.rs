//! UI-thread activation observation at Tao's outer message boundary.
//!
//! Core retains public focus state and filtering. This observer remembers only
//! the last native HWND sampled, then asks the current live resource owner for facts.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};

use tao::event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::platform::windows::EventLoopBuilderExtWindows as _;
use windows::Win32::Foundation::HWND;

use super::managed_app::ManagedWindows;
use super::{
    ProfileErrorKind, ProfileReleaseFault, ProfileReleaseWait, WindowsLoopEvent,
    profile_failure_with, view_host,
};
use crate::WvError;

/// The engine constructors require this pair, including native fixture engines.
pub(super) struct EngineEventLoop {
    pub(super) native: EventLoop<WindowsLoopEvent>,
    pub(super) observer: Rc<ActivationObserver>,
}

impl EngineEventLoop {
    pub(super) fn build(builder: &mut EventLoopBuilder<WindowsLoopEvent>) -> Self {
        let observer = Rc::new(ActivationObserver::default());
        let hook = Rc::clone(&observer);
        // Tao 0.35.3 calls this after outer GetMessageW and before dispatch.
        // Its window callback requests an internal paint even when it suppresses
        // Focused after focus moved to a WebView child. No MSG is dereferenced.
        // https://docs.rs/tao/0.35.3/tao/platform/windows/trait.EventLoopBuilderExtWindows.html#tymethod.with_msg_hook
        builder.with_msg_hook(move |_| {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            super::t2_test_observation::outer_dispatch_boundary();
            hook.observe();
            false
        });
        Self {
            native: builder.build(),
            observer,
        }
    }
}

struct ActivationBinding {
    windows: Weak<RefCell<ManagedWindows>>,
    release: Weak<ProfileReleaseWait>,
    // Tao's proxy carries a sender/target HWND, not ownership of the event loop.
    // It also remains usable when the resource owner is already borrowed.
    wake: EventLoopProxy<WindowsLoopEvent>,
}

#[derive(Default)]
pub(super) struct ActivationObserver {
    binding: OnceCell<ActivationBinding>,
    // Observation-work gate only: no view ID or last-sent public focus state.
    last_active: Cell<Option<HWND>>,
}

impl ActivationObserver {
    pub(super) fn attach(
        &self,
        windows: &Rc<RefCell<ManagedWindows>>,
        release: &Rc<ProfileReleaseWait>,
        wake: EventLoopProxy<WindowsLoopEvent>,
    ) -> Result<(), WvError> {
        let binding = ActivationBinding {
            windows: Rc::downgrade(windows),
            release: Rc::downgrade(release),
            wake,
        };
        if let Err(binding) = self.binding.set(binding) {
            Self::fail(release, &binding.wake);
            return Err(profile_failure_with(
                ProfileErrorKind::LifecycleUnproven,
                ProfileReleaseFault::ActivationObservationFailed.context(release.site),
            ));
        }
        Ok(())
    }

    fn observe(&self) {
        let Some(binding) = self.binding.get() else {
            return;
        };
        let Some(release) = binding.release.upgrade() else {
            return;
        };
        // Never turn a failed delivery into a self-sustaining wake loop.
        if release.fault().is_some() {
            return;
        }
        let Some(windows) = binding.windows.upgrade() else {
            return;
        };
        let active = view_host::active_window();
        // No COM calls, pumping, focus mutation or retained per-window identity.
        // Use the same native sample for every member of the current live map.
        let failed = self
            .observe_sample(active, |active| {
                let windows = windows.try_borrow().map_err(|_| ())?;
                windows.observe_activation(active).map_err(|_| ())
            })
            .is_err();
        if failed {
            Self::fail(&release, &binding.wake);
        }
    }

    /// Direct focus facts and new membership can supersede the last hook sample.
    /// Keep no public focus bits: the next outer message must read the live map.
    pub(super) fn invalidate(&self) {
        self.last_active.set(None);
    }

    fn observe_sample<E>(
        &self,
        active: HWND,
        observe_current: impl FnOnce(HWND) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.last_active.replace(Some(active)) == Some(active) {
            return Ok(());
        }
        observe_current(active)
    }

    fn fail(release: &ProfileReleaseWait, wake: &EventLoopProxy<WindowsLoopEvent>) {
        // The existing first-fault owner decides shutdown. Wake delivery is only
        // a prompt; the latch remains authoritative if the loop is unavailable.
        release.latch_fault(ProfileReleaseFault::ActivationObservationFailed);
        let _ = wake.send_event(WindowsLoopEvent::ProfileReleaseWake);
    }
}

#[cfg(test)]
#[path = "tests/activation_observer.rs"]
mod tests;
