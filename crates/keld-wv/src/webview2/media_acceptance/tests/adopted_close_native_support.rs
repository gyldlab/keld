//! Independent cleanup for the managed application Close-result fixture.

use super::super::failure;
use crate::WvError;
use crate::webview2 as backend;
use crate::webview2::media_acceptance::adopted_close_control::NativeCapture;
use std::os::windows::io::AsRawHandle as _;
use std::sync::{Arc, mpsc::TryRecvError};
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
    observation: Option<backend::BrowserExitObservation>,
    normal: u32,
}

impl ExitWitness {
    pub(super) fn new(engine: &backend::WebView2Engine) -> Result<Self, WvError> {
        Ok(Self {
            // Read-only native observation using the current engine binding.
            // This is not another release policy or profile lease owner.
            observation: Some(backend::observe_profile_browser_exit(
                &engine.environment,
                Arc::clone(&engine.browser_binding),
                None,
                backend::ReleaseSite::Host,
            )?),
            normal: 0,
        })
    }

    fn drain(&mut self, pid: u32) -> Result<(), WvError> {
        let observation = self.observation.as_ref().expect("live fixture observer");
        loop {
            match observation.receiver.try_recv() {
                Ok(backend::BrowserExitReceipt::Normal(instance)) if instance.pid == pid => {
                    self.normal += 1;
                }
                Ok(receipt) => {
                    return Err(failure(format!("unexpected cleanup receipt {receipt:?}")));
                }
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    return Err(failure("independent exit observer disconnected"));
                }
            }
        }
    }

    pub(super) fn retire(mut self) -> Result<(), WvError> {
        if let Some(observation) = self.observation.take() {
            backend::remove_browser_exit_observer(&observation.environment, observation.token)?;
        }
        Ok(())
    }
}

impl Drop for ExitWitness {
    fn drop(&mut self) {
        if let Some(observation) = self.observation.take() {
            let _ =
                backend::remove_browser_exit_observer(&observation.environment, observation.token);
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
    loop {
        witness.drain(capture.browser_pid)?;
        // SAFETY: this exact SYNCHRONIZE handle was captured while alive.
        // https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject
        let process = unsafe { WaitForSingleObject(HANDLE(capture.browser.as_raw_handle()), 0) };
        // SAFETY: read-only retirement check of the captured fixture tree.
        let native_dead =
            !is_window(capture.hwnd) && capture.children.iter().all(|hwnd| !is_window(*hwnd));
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
