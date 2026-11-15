//! Existing signed-host child and retained Bun HANDLE lifecycle owner.

use super::process::{assert_process_signaled, open_process_for_wait, wait_child};
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::process::{Child, ExitStatus};
use std::time::Instant;
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};

pub(crate) struct SignedStateProcessGuard {
    child: Option<Child>,
    bun: Option<OwnedHandle>,
}

impl SignedStateProcessGuard {
    pub(crate) fn new(child: Child) -> Self {
        Self {
            child: Some(child),
            bun: None,
        }
    }

    pub(crate) fn host_pid(&self) -> u32 {
        self.child.as_ref().expect("live signed host").id()
    }

    pub(crate) fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("live signed host")
    }

    pub(crate) fn observe_bun(&mut self, pid: u32) {
        self.bun = Some(open_process_for_wait(pid, true));
    }

    pub(crate) fn wait(mut self, deadline: Instant) -> ExitStatus {
        let status = wait_child(self.child_mut(), deadline);
        let _ = self.child.take();
        if let Some(bun) = self.bun.take() {
            assert_process_signaled(&bun, "signed-state Bun");
        }
        status
    }
}

impl Drop for SignedStateProcessGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(bun) = self.bun.take()
            // SAFETY: `bun` owns a live process handle opened for synchronize/terminate;
            // this zero-time wait neither closes nor transfers it.
            && unsafe { WaitForSingleObject(bun.as_raw_handle().cast(), 0) } != WAIT_OBJECT_0
        {
            // SAFETY: the guard owns the exact observed Bun process handle. Termination
            // is test-failure cleanup, followed by a bounded wait before handle drop.
            unsafe {
                let _ = TerminateProcess(bun.as_raw_handle().cast(), 1);
                let _ = WaitForSingleObject(bun.as_raw_handle().cast(), 5_000);
            }
        }
    }
}
