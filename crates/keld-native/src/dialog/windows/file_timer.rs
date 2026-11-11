//! One callback timer dispatches cancellation inside `IFileDialog::Show`'s STA
//! message pump. It carries no raw Rust pointer and owns no custody policy.
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use keld_ipc::IpcError;
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Ole::IOleWindow;
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Shell::IFileDialog;
use windows::Win32::UI::WindowsAndMessaging::{
    GW_OWNER, GetWindow, GetWindowThreadProcessId, KillTimer, SendMessageW, SetTimer, WM_CLOSE,
};
use windows::core::{HRESULT, Interface as _};

use super::{Cancellation, invalid, native};

struct Registration {
    id: usize,
    dialog: IFileDialog,
    window: IOleWindow,
    cancel: Rc<Cancellation>,
    closing: Cell<bool>,
}

// A native SendMessage can reenter the timer. This is only an in-call guard;
// cancellation policy and its winning terminal remain in the original custody.
struct Closing<'a>(&'a Cell<bool>);
impl Drop for Closing<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl Registration {
    #[allow(unsafe_code)] // Exact current-dialog HWND cancellation in the nested allowance.
    fn request_close(&self) {
        if self.closing.replace(true) {
            return;
        }
        let _closing = Closing(&self.closing);
        // SAFETY: both typed interfaces are retained from this same current-STA
        // dialog. The original custody predicate is already latched as canceled.
        if unsafe { self.dialog.Close(HRESULT::from_win32(ERROR_CANCELLED.0)) }.is_err() {
            self.cancel.failed.set(true);
        }
        // Windows 11 build 26300 / shell32 10.0.26100.9549 returned S_OK from
        // Close without draining idle Show (real adapter regression). Query the
        // current object each time; never reuse a cached/queued HWND.
        // Microsoft documents this exact IFileDialog -> IOleWindow lookup:
        // https://learn.microsoft.com/windows/win32/shell/common-file-dialog
        // SAFETY: live same-STA IOleWindow belonging to this retained dialog.
        let Ok(window) = (unsafe { self.window.GetWindow() }) else {
            return; // Not created yet, or Close already retired its native window.
        };
        let mut process = 0;
        // SAFETY: opaque native identity queries only, with a live stack output.
        let thread = unsafe { GetWindowThreadProcessId(window, Some(&raw mut process)) };
        if thread == 0 {
            return; // The dialog already disappeared.
        }
        // SAFETY: current thread/process IDs and the freshly resolved HWND owner.
        let owned = unsafe {
            thread == GetCurrentThreadId()
                && process == GetCurrentProcessId()
                && GetWindow(window, GW_OWNER).ok() == Some(self.cancel.parent)
        };
        if !owned {
            self.cancel.failed.set(true);
            return;
        }
        // SAFETY: the exact current dialog is on this STA and owned by the
        // retained parent. No Rust borrow crosses dispatch; Closing blocks
        // nested dismissal. Synchronous WM_CLOSE uses normal native cancellation
        // without leaving a queued numeric HWND that can outlive Show/reuse.
        // https://learn.microsoft.com/windows/win32/winmsg/wm-close
        unsafe { SendMessageW(window, WM_CLOSE, None, None) };
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<Registration>>> = const { RefCell::new(None) };
}

pub(super) struct FileTimer(Rc<Registration>);

impl FileTimer {
    #[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
    pub(super) fn start(dialog: &IFileDialog, cancel: Rc<Cancellation>) -> Result<Self, IpcError> {
        if ACTIVE.with_borrow(Option::is_some) {
            return Err(invalid("Windows file dialog timer already retained"));
        }
        let window: IOleWindow = native(
            dialog.cast(),
            "Windows file dialog window interface unavailable",
        )?;
        // SAFETY: NULL owner/id allocates a thread timer. TIMERPROC contains no
        // foreign pointer and looks up retained state on this exact STA.
        let id = unsafe { SetTimer(None, 0, 100, Some(tick)) };
        if id == 0 {
            return Err(invalid("Windows file dialog cancellation timer failed"));
        }
        let retained = Rc::new(Registration {
            id,
            dialog: dialog.clone(),
            window,
            cancel,
            closing: Cell::new(false),
        });
        ACTIVE.set(Some(Rc::clone(&retained)));
        Ok(Self(retained))
    }

    pub(super) fn finish(self) -> Result<(), IpcError> {
        self.stop()
    }

    #[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
    fn stop(&self) -> Result<(), IpcError> {
        let active = ACTIVE.with_borrow(|slot| {
            slot.as_ref()
                .is_some_and(|value| Rc::ptr_eq(value, &self.0))
        });
        if !active {
            return Ok(());
        }
        // SAFETY: this id was returned by SetTimer on this thread and remains
        // registered until now. KillTimer does not erase queued WM_TIMER; removing
        // the lookup below makes such callbacks harmless without freed pointers.
        let stopped = unsafe { KillTimer(None, self.0.id) };
        ACTIVE.set(None);
        if stopped.is_err() {
            self.0.cancel.failed.set(true);
        }
        native(stopped, "Windows file dialog timer cleanup failed")
    }
}

impl Drop for FileTimer {
    fn drop(&mut self) {
        // Normal completion observes cleanup errors; unwinding still removes
        // the registration and retains no callback-state pointer.
        let _ = self.stop();
    }
}

#[allow(unsafe_code)] // Exact Windows dialog callback ABI in the nested allowance.
unsafe extern "system" fn tick(_: HWND, _: u32, id: usize, _: u32) {
    // Drop the RefCell borrow before native dismissal can reenter.
    let retained = ACTIVE
        .try_with(|slot| {
            slot.try_borrow()
                .ok()
                .and_then(|slot| slot.as_ref().filter(|value| value.id == id).cloned())
        })
        .ok()
        .flatten();
    let Some(retained) = retained else { return };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if retained.cancel.check() {
            retained.request_close();
        }
    }));
    if result.is_err() {
        retained.cancel.failed.set(true);
        retained.cancel.requested.set(true);
        // Closing's drop already released the in-call guard. Keep even this
        // failure-only native cleanup inside unwind containment.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            retained.request_close();
        }));
    }
}
