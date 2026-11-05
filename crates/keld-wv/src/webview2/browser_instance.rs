//! Browser-instance attribution inside the existing profile-release owner.
//!
//! The atomic value is the callback's one coherent identity snapshot. Only the
//! UI thread changes it; native callbacks never own lifecycle policy or handles.
#![deny(unsafe_op_in_unsafe_fn)]

use std::sync::Weak;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use tao::event_loop::EventLoopProxy;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    ICoreWebView2, ICoreWebView2ProcessFailedEventHandler,
};
use webview2_com::ProcessFailedEventHandler;
use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::WaitForSingleObject;

use super::{
    BrowserExitWake, BrowserProcess, OwnedHandle, ProfileErrorKind, ProfileReleaseFault,
    ProfileReleaseWait, ReleaseSite, WindowsLoopEvent, WvError, browser_exit_is_relevant,
    open_browser_process, profile_failure_with, webview_browser_process_id,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct BrowserInstance {
    pub(super) generation: u32,
    pub(super) pid: u32,
}

impl BrowserInstance {
    pub(super) const fn is_bound(self) -> bool {
        self.generation != 0 && self.pid != 0
    }
}

/// Generation zero means no identified browser has ever been bound. A callback
/// must never combine a generation from one publication with another one's pid.
pub(super) struct BrowserBinding(AtomicU64);

impl BrowserBinding {
    pub(super) const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub(super) fn load(&self) -> BrowserInstance {
        let packed = self.0.load(Ordering::Acquire);
        let [g0, g1, g2, g3, p0, p1, p2, p3] = packed.to_be_bytes();
        BrowserInstance {
            generation: u32::from_be_bytes([g0, g1, g2, g3]),
            pid: u32::from_be_bytes([p0, p1, p2, p3]),
        }
    }

    fn store(&self, instance: BrowserInstance) {
        self.0.store(
            (u64::from(instance.generation) << 32) | u64::from(instance.pid),
            Ordering::Release,
        );
    }

    /// Legacy hello binds at loop entry, and the recovery probe binds once.
    /// An idempotent second observation of that same legacy identity is allowed;
    /// managed rebinding belongs exclusively to `ProfileReleaseWait`.
    pub(super) fn bind_once(&self, pid: u32) -> Result<(), WvError> {
        let current = self.load();
        if pid == 0 || (current.is_bound() && current != (BrowserInstance { generation: 1, pid })) {
            return Err(profile_failure_with(
                ProfileErrorKind::LifecycleUnproven,
                "a one-time browser binding was missing or changed",
            ));
        }
        if !current.is_bound() {
            self.store(BrowserInstance { generation: 1, pid });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BrowserExitSource {
    Environment,
    ProcessFailed,
}

impl BrowserExitSource {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Environment => "browser-exited",
            Self::ProcessFailed => "process-failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BrowserExitReceipt {
    Normal(BrowserInstance),
    Abnormal {
        instance: BrowserInstance,
        source: BrowserExitSource,
    },
}

/// Normal credit needs the identity visible at callback delivery. Host abnormal
/// events are session faults even when their pid belongs to an older instance.
pub(super) fn browser_exit_receipt(
    current: BrowserInstance,
    pid: u32,
    normal: bool,
    site: ReleaseSite,
) -> Option<BrowserExitReceipt> {
    if !browser_exit_is_relevant(current.pid, pid, normal, site) {
        return None;
    }
    if normal {
        current
            .is_bound()
            .then_some(BrowserExitReceipt::Normal(current))
    } else {
        Some(BrowserExitReceipt::Abnormal {
            instance: BrowserInstance {
                generation: current.generation,
                pid,
            },
            source: BrowserExitSource::Environment,
        })
    }
}

/// The View callback holds a weak reference only. Keeping a controller or its
/// callback alive must not conceal disappearance of the environment observer.
fn queue_process_failed(
    sender: &Weak<Sender<BrowserExitReceipt>>,
    instance: BrowserInstance,
    kind: COREWEBVIEW2_PROCESS_FAILED_KIND,
) -> windows::core::Result<bool> {
    if kind != COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
        return Ok(false);
    }
    let sender = sender
        .upgrade()
        .ok_or_else(|| windows::core::Error::from(E_UNEXPECTED))?;
    sender
        .send(BrowserExitReceipt::Abnormal {
            instance,
            source: BrowserExitSource::ProcessFailed,
        })
        .map_err(|_| windows::core::Error::from(E_UNEXPECTED))?;
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    super::t2_test_observation::abnormal(instance, BrowserExitSource::ProcessFailed);
    Ok(true)
}

fn process_failed_handler(
    instance: BrowserInstance,
    sender: Weak<Sender<BrowserExitReceipt>>,
    wake: impl Fn() -> windows::core::Result<()> + 'static,
) -> ICoreWebView2ProcessFailedEventHandler {
    ProcessFailedEventHandler::create(Box::new(move |_, args| {
        let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
        let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
        // SAFETY: WebView2 invokes this handler on its owning STA with live
        // event args. The writable output lives through the synchronous call.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2processfailedeventargs#processfailedkind
        unsafe { args.ProcessFailedKind(&raw mut kind) }?;
        if queue_process_failed(&sender, instance, kind)? {
            wake()?;
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        super::t2_test_observation::native_release_step("process-failed.body-return", Some(0));
        Ok(())
    }))
}

impl ProfileReleaseWait {
    /// The existing fault cell is first-wins for every receipt/build/releasing
    /// failure. A later failure cannot erase the reason custody became unsafe.
    pub(super) fn latch_fault(&self, fault: ProfileReleaseFault) -> ProfileReleaseFault {
        if let Some(first) = self.fault.get() {
            return first;
        }
        self.fault.set(Some(fault));
        fault
    }

    fn fail(&self, fault: ProfileReleaseFault) -> WvError {
        let first = self.latch_fault(fault);
        profile_failure_with(
            ProfileErrorKind::LifecycleUnproven,
            first.context(self.site),
        )
    }

    fn check_fault(&self) -> Result<(), WvError> {
        if let Some(fault) = self.fault.get() {
            return Err(profile_failure_with(
                ProfileErrorKind::LifecycleUnproven,
                fault.context(self.site),
            ));
        }
        Ok(())
    }

    /// A controller already exists, so an unreadable identity cannot inherit
    /// the zero-controller exemption. D12 retains and retires that controller.
    pub(super) fn latch_unidentified_controller(&self) -> WvError {
        self.poll();
        self.fail(ProfileReleaseFault::UnidentifiedController)
    }

    /// Called immediately after controller/CoreWebView2 completion, before the
    /// View or any later fallible setup. It also runs for a subsequently failed
    /// build because that browser still owes a native retirement receipt.
    pub(super) fn bind_controller(
        &self,
        webview: &ICoreWebView2,
        proxy: EventLoopProxy<WindowsLoopEvent>,
    ) -> Result<(), WvError> {
        self.poll();
        self.check_fault()?;
        let pid = webview_browser_process_id(webview)
            .ok_or_else(|| self.latch_unidentified_controller())?;
        self.bind_browser_pid_with(
            pid,
            open_browser_process,
            |process| match process {
                BrowserProcess::Watched(wake) => wake.retire(),
                BrowserProcess::Polled(_) => Ok(()),
            },
            |process| BrowserExitWake::register(process, proxy).map(BrowserProcess::Watched),
        )
    }

    /// Operation seam for the binding table: the production path above supplies
    /// the existing open/checked-retire/register operations. No state is owned
    /// by the closures and no callback can run inside this nonpumping commit.
    fn bind_browser_pid_with(
        &self,
        pid: u32,
        open: impl FnOnce(u32) -> Option<OwnedHandle>,
        retire: impl FnOnce(BrowserProcess) -> Result<(), WvError>,
        register: impl FnOnce(OwnedHandle) -> Result<BrowserProcess, WvError>,
    ) -> Result<(), WvError> {
        self.poll();
        self.check_fault()?;
        if self.receiver.is_none() {
            return Err(self.fail(ProfileReleaseFault::ObserverDropped));
        }
        if self.armed_at.get().is_some() || pid == 0 {
            return Err(self.fail(ProfileReleaseFault::RebindUnproven));
        }
        let current = self.binding.load();
        if current.is_bound() && current.pid == pid {
            let process = self.process.borrow();
            let Some(process) = process.as_ref() else {
                return Err(self.fail(ProfileReleaseFault::RebindUnproven));
            };
            // SAFETY: this owner retains the SYNCHRONIZE handle for the full
            // zero-time query. It does not pump or transfer the native handle.
            // https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject
            let state = unsafe { WaitForSingleObject(process.raw(), 0) };
            if state == WAIT_TIMEOUT {
                return Ok(());
            }
            if state == WAIT_OBJECT_0 {
                // A retained process handle prevents pid recycling. A new
                // controller naming that exited process is not release proof.
                self.ambiguous.set(true);
                self.observed.set(false);
                self.normal_exit_observed_at.set(None);
                self.post_exit_deadline.set(None);
                return Ok(());
            }
            return Err(self.fail(ProfileReleaseFault::RebindUnproven));
        }
        let generation = current
            .generation
            .checked_add(1)
            .ok_or_else(|| self.fail(ProfileReleaseFault::RebindUnproven))?;
        // Open before retiring the old wake. An unsuccessful open must leave
        // the original kernel handle and registration intact.
        let next = open(pid).ok_or_else(|| self.fail(ProfileReleaseFault::RebindUnproven))?;
        // End the RefCell borrow before any native operation. Checked retirement
        // consumes the old registration even on failure; never retry by Drop.
        let old = self.process.borrow_mut().take();
        if let Some(old) = old {
            retire(old).map_err(|_| self.fail(ProfileReleaseFault::WakeRetireFailed))?;
        }
        let next = register(next).map_err(|_| self.fail(ProfileReleaseFault::RebindUnproven))?;
        *self.process.borrow_mut() = Some(next);
        let instance = BrowserInstance { generation, pid };
        self.binding.store(instance);
        self.ambiguous.set(false);
        self.observed.set(false);
        self.normal_exit_observed_at.set(None);
        self.post_exit_deadline.set(None);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        super::t2_test_observation::browser_rebound(instance);
        Ok(())
    }

    /// Install on the just-bound `CoreWebView2` before `View` construction. Closing
    /// its controller retires this callback; there is no second token owner.
    pub(super) fn install_process_failed(
        &self,
        webview: &ICoreWebView2,
        sender: Weak<Sender<BrowserExitReceipt>>,
        proxy: EventLoopProxy<WindowsLoopEvent>,
    ) -> Result<(), WvError> {
        self.install_process_failed_with(|instance| {
            let handler = process_failed_handler(instance, sender, move || {
                proxy
                    .send_event(WindowsLoopEvent::ProfileReleaseWake)
                    .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
            });
            let mut token = 0_i64;
            // SAFETY: the bound webview and callback belong to this UI STA.
            // WebView2 retains the callback until its controller is closed;
            // token is a live out-pointer and no thread receives COM state.
            // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2#add_processfailed
            unsafe { webview.add_ProcessFailed(&handler, &raw mut token) }
                .map_err(|error| WvError::Webview(format!("process-failed handler: {error}")))
        })
    }

    fn install_process_failed_with(
        &self,
        install: impl FnOnce(BrowserInstance) -> Result<(), WvError>,
    ) -> Result<(), WvError> {
        self.poll();
        self.check_fault()?;
        let instance = self.binding.load();
        if !instance.is_bound() {
            return Err(self.latch_unidentified_controller());
        }
        // Registration errors fail the build, but neither roll back the bind
        // nor forgive the browser's receipt. D12 retains failed-build custody.
        install(instance)
    }
}

#[cfg(test)]
#[path = "tests/browser_instance.rs"]
mod tests;
