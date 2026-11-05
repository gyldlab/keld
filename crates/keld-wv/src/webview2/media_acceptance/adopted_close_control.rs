//! One managed native Close, with an optional labelled failure of its result.
//! No environment hook, lifecycle policy, COM owner or process killer is added.
#![deny(unsafe_op_in_unsafe_fn)]

use crate::webview2 as backend;
use std::cell::RefCell;
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use windows::Win32::Foundation::{E_FAIL, HANDLE, HWND, LPARAM, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{GetCurrentThreadId, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetWindowThreadProcessId, IsWindow,
};
use windows::core::BOOL;

pub(in crate::webview2) struct NativeCapture {
    pub(in crate::webview2) hwnd: HWND,
    pub(in crate::webview2) children: Vec<HWND>,
    pub(in crate::webview2) owner_thread: u32,
    pub(in crate::webview2) browser_pid: u32,
    pub(in crate::webview2) browser: OwnedHandle,
}

#[derive(Default)]
pub(in crate::webview2) struct Facts {
    pub(in crate::webview2) calls: u32,
    pub(in crate::webview2) native_successes: u32,
    pub(in crate::webview2) injected_errors: u32,
    pub(in crate::webview2) results: Vec<i32>,
    pub(in crate::webview2) capture: Option<NativeCapture>,
    pub(in crate::webview2) capture_error: Option<String>,
}

#[derive(Default)]
struct State {
    active: bool,
    withhold: bool,
    facts: Facts,
}
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

#[must_use]
pub(in crate::webview2) struct Scope;
impl Scope {
    pub(in crate::webview2) fn enter(withhold: bool) -> Self {
        STATE.with_borrow_mut(|state| {
            assert!(!state.active, "one adopted-Close control per STA");
            *state = State {
                active: true,
                withhold,
                ..State::default()
            };
        });
        Self
    }

    pub(in crate::webview2) fn take_facts() -> Facts {
        STATE.with_borrow_mut(|state| std::mem::take(&mut state.facts))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|state| *state = State::default());
    }
}

unsafe extern "system" fn child(hwnd: HWND, context: LPARAM) -> BOOL {
    // SAFETY: EnumChildWindows synchronously receives a pointer to the stack
    // Vec below. No nested call aliases it; only this callback mutates it.
    let children = unsafe { &mut *(context.0 as *mut Vec<HWND>) };
    children.push(hwnd);
    BOOL(1)
}

fn capture(controller: &backend::ICoreWebView2Controller) -> Result<NativeCapture, String> {
    let mut hwnd = HWND::default();
    // SAFETY: the adopted controller is live on this same STA, before Close.
    // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller
    let webview = unsafe {
        controller
            .ParentWindow(&raw mut hwnd)
            .and_then(|()| controller.CoreWebView2())
    }
    .map_err(|error| error.to_string())?;
    let browser_pid = backend::webview_browser_process_id(&webview)
        .ok_or_else(|| String::from("live adopted controller supplied no browser PID"))?;
    let browser = backend::open_browser_process(browser_pid)
        .ok_or_else(|| String::from("cannot retain live browser SYNCHRONIZE handle"))?;
    let mut children = Vec::new();
    // SAFETY: independent read-only facts about this fixture's owned live parent
    // and a process handle opened while its actual controller is still live.
    // EnumChildWindows uses the synchronous stack callback above.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumchildwindows
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid
    // https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject
    let (live, owner_thread, current_thread, browser_alive) = unsafe {
        let _ = EnumChildWindows(
            Some(hwnd),
            Some(child),
            LPARAM(std::ptr::from_mut(&mut children) as isize),
        );
        (
            IsWindow(Some(hwnd)).as_bool(),
            GetWindowThreadProcessId(hwnd, None),
            GetCurrentThreadId(),
            WaitForSingleObject(HANDLE(browser.as_raw_handle()), 0) == WAIT_TIMEOUT,
        )
    };
    if !live || owner_thread == 0 || owner_thread != current_thread || !browser_alive {
        return Err(String::from("adopted-Close native eligibility is unproved"));
    }
    if children.is_empty() {
        return Err(String::from(
            "live controller has no independently observed native child",
        ));
    }
    Ok(NativeCapture {
        hwnd,
        children,
        owner_thread,
        browser_pid,
        browser,
    })
}

pub(in crate::webview2) fn before(
    controller: &backend::ICoreWebView2Controller,
    managed: bool,
) -> bool {
    let observe = STATE.with_borrow_mut(|state| {
        if !state.active || !managed {
            return false;
        }
        state.facts.calls += 1;
        true
    });
    if !observe {
        return false;
    }
    // Do not hold the RefCell borrow across any COM call or native observation.
    let captured = capture(controller);
    STATE.with_borrow_mut(|state| {
        if state.facts.calls != 1 {
            state.facts.capture_error = Some(String::from("more than one adopted Close"));
        } else {
            match captured {
                Ok(capture) => state.facts.capture = Some(capture),
                Err(error) => state.facts.capture_error = Some(error),
            }
        }
    });
    true
}

pub(in crate::webview2) fn after(
    result: windows::core::Result<()>,
    observed: bool,
) -> windows::core::Result<()> {
    if !observed {
        return result;
    }
    let withhold = STATE.with_borrow_mut(|state| {
        state
            .facts
            .results
            .push(result.as_ref().err().map_or(0, |error| error.code().0));
        if result.is_ok() {
            state.facts.native_successes += 1;
            if state.withhold {
                state.withhold = false;
                state.facts.injected_errors += 1;
                return true;
            }
        }
        false
    });
    if withhold {
        eprintln!("KELD_ADOPTED_CLOSE_RESULT_WITHHELD native_close_succeeded=true");
        Err(E_FAIL.into())
    } else {
        result
    }
}
