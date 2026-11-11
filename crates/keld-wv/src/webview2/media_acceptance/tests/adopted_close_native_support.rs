//! Raw primary exit observation and independent cleanup for the Close-result fixture.

use super::super::failure;
use crate::WvError;
use crate::webview2 as backend;
use crate::webview2::media_acceptance::adopted_close_control::{
    BrowserExitFacts, NativeCapture, Scope,
};
use std::os::windows::io::AsRawHandle as _;
use std::sync::Arc;
use std::time::Instant;
use windows::Win32::Foundation::{HANDLE, HWND, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{GetCurrentThreadId, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, GetWindowThreadProcessId, IsWindow};

fn is_window(hwnd: HWND) -> bool {
    // SAFETY: read-only query of a captured fixture HWND; no pointer is dereferenced.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindow
    unsafe { IsWindow(Some(hwnd)) }.as_bool()
}

pub(super) struct ExitWitness {
    environment: backend::ICoreWebView2Environment5,
    primary_token: Option<i64>,
    binding: Arc<backend::BrowserBinding>,
    expected: Option<backend::BrowserInstance>,
    orphaned: bool,
    normal: u32,
}

impl ExitWitness {
    pub(super) fn new(engine: &backend::WebView2Engine) -> Result<Self, WvError> {
        // Keep the original registration alive, without adding another handler.
        // This clone contains no profile lease, View, receiver or release credit.
        let environment = engine
            .browser_exit_environment
            .as_ref()
            .ok_or_else(|| failure("managed fixture primary environment is missing"))?
            .clone();
        let primary_token = engine
            .browser_exit_token
            .ok_or_else(|| failure("managed fixture primary exit observer is missing"))?;
        Ok(Self {
            environment,
            primary_token: Some(primary_token),
            binding: Arc::clone(&engine.browser_binding),
            expected: None,
            orphaned: false,
            normal: 0,
        })
    }

    pub(super) fn prepare_after_app(&mut self, actual_close_fault: bool, witness_calls: u32) {
        assert!(self.expected.is_none(), "freeze the original binding once");
        self.expected = Some(self.binding.load());
        // The sole primary removal occurs after the synchronous Host witness.
        // Only this actual fault path transfers orphan cleanup to the fixture.
        self.orphaned = actual_close_fault && witness_calls == 0;
        if !self.orphaned {
            self.primary_token = None;
        }
    }

    fn drain(&mut self, pid: u32) -> Result<BrowserExitFacts, WvError> {
        let expected = self
            .expected
            .expect("binding frozen after actual app return");
        if !expected.is_bound() || expected.pid != pid {
            return Err(failure(
                "native capture disagrees with the frozen browser instance",
            ));
        }
        let facts = Scope::browser_exit_facts()
            .ok_or_else(|| failure("raw primary browser-exit observation is inactive"))?;
        match facts {
            BrowserExitFacts {
                calls: 0,
                receipt: None,
            } => self.normal = 0,
            BrowserExitFacts {
                calls: 1,
                receipt: Some((instance, actual, true)),
            } if instance == expected && actual == pid => self.normal = 1,
            _ => {
                return Err(failure(format!(
                    "unexpected raw primary browser exit {facts:?}; expected={expected:?} pid={pid}"
                )));
            }
        }
        Ok(facts)
    }

    fn remove_fault_orphan(&mut self) -> Result<(), WvError> {
        if self.orphaned
            && let Some(token) = self.primary_token.take()
        {
            eprintln!("KELD_ADOPTED_CLOSE_DIAG phase=primary-observer-remove-enter token={token}");
            let result = backend::remove_browser_exit_observer(&self.environment, token);
            eprintln!(
                "KELD_ADOPTED_CLOSE_DIAG phase=primary-observer-remove-return result={result:?}"
            );
            result?;
            println!(
                "KELD_ADOPTED_CLOSE_ORPHAN_OBSERVER_REMOVED cleanup_owner=fixture product_retirement_credit=false"
            );
        }
        Ok(())
    }

    pub(super) fn retire(mut self, pid: u32) -> Result<BrowserExitFacts, WvError> {
        // Successful cleanup proves native retirement; an error still requires
        // orphan cleanup. Sample the raw counter and never retry a taken token.
        let removed = self.remove_fault_orphan();
        let facts = self.drain(pid);
        removed?;
        let facts = facts?;
        if self.normal != 1 {
            return Err(failure(
                "exactly one raw primary normal event was not established",
            ));
        }
        Ok(facts)
    }
}

impl Drop for ExitWitness {
    fn drop(&mut self) {
        // Unwind cleanup only, after the actual fault proved primary custody.
        // Healthy/app-in-progress owners are never removed by this fixture.
        if self.orphaned
            && let Some(token) = self.primary_token.take()
        {
            let _ = backend::remove_browser_exit_observer(&self.environment, token);
        }
    }
}

/// Return whether production had already retired every captured native HWND.
/// A cleanup intervention is independently ineligible, never an intended RED.
pub(super) fn cleanup(
    capture: &NativeCapture,
    witness: &mut ExitWitness,
    deadline: Instant,
) -> Result<bool, WvError> {
    // SAFETY: read only the parent/children captured while this fixture was live.
    // No successor window is constructed before this case's cleanup finishes.
    let parent_live = is_window(capture.hwnd);
    let child_live = capture.children.iter().any(|hwnd| is_window(*hwnd));
    let already_retired = !parent_live && !child_live;
    if parent_live {
        // SAFETY: both queries only validate the retained fixture's original
        // owning UI thread before a possible cleanup intervention.
        let (owner, current) = unsafe {
            (
                GetWindowThreadProcessId(capture.hwnd, None),
                GetCurrentThreadId(),
            )
        };
        if owner != capture.owner_thread || owner != current {
            return Err(failure(
                "adopted-Close parent cleanup ownership is unproved",
            ));
        }
        // SAFETY: same-thread owned parent after its Rust owner was consumed.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-destroywindow
        unsafe { DestroyWindow(capture.hwnd) }.map_err(failure)?;
    }
    // Diagnostic state only; the native wait and eligibility decisions are unchanged.
    let mut reported = None;
    loop {
        witness.drain(capture.browser_pid)?;
        // SAFETY: this exact SYNCHRONIZE handle was captured while alive.
        // https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject
        let process = unsafe { WaitForSingleObject(HANDLE(capture.browser.as_raw_handle()), 0) };
        // SAFETY: read-only retirement check of the captured fixture tree.
        let native_dead =
            !is_window(capture.hwnd) && capture.children.iter().all(|hwnd| !is_window(*hwnd));
        let progress = (witness.normal, process, native_dead);
        if reported != Some(progress) {
            eprintln!(
                "KELD_ADOPTED_CLOSE_DIAG phase=cleanup-progress pid={} normal={} process={process:?} native_dead={native_dead}",
                capture.browser_pid, witness.normal
            );
            reported = Some(progress);
        }
        if process == WAIT_OBJECT_0 && witness.normal == 1 && native_dead {
            println!(
                "KELD_ADOPTED_CLOSE_CLEANED pid={} normal_receipt=1 process_signaled=true native_hwnds=0 already_retired={already_retired}",
                capture.browser_pid,
            );
            return Ok(already_retired);
        }
        if !matches!(process, WAIT_OBJECT_0 | WAIT_TIMEOUT) || witness.normal > 1 {
            return Err(failure("independent native cleanup observation is invalid"));
        }
        if Instant::now() >= deadline {
            return Err(failure(
                "adopted-Close native cleanup deadline; not a semantic result",
            ));
        }
        let handles =
            (process == WAIT_TIMEOUT).then_some([HANDLE(capture.browser.as_raw_handle())]);
        backend::pump_message_or_handle(
            handles.as_ref().map(<[HANDLE; 1]>::as_slice),
            backend::milliseconds_until(deadline)?,
        )?;
    }
}
