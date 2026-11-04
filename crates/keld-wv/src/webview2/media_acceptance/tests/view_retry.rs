//! Inject one failed native step on a real view, then prove retry by native reads.
//! This tests failure handling, not a claim that Windows spontaneously failed.

use super::super::{FIXTURE_DEADLINE, run_with_watchdog};
use super::{
    blank_spec, failure, profile_fixture_engine, profile_fixture_root, run_renderer_until_quit,
};
use crate::profile::{EphemeralProfile, WebProfileSelection};
use crate::webview2::View;
use crate::webview2::view_host::test_control::{Failure, Step};
use crate::{AppWindowCommand, WvError};
use std::sync::mpsc;
use tao::dpi::PhysicalSize;
use tao::event::WindowEvent;
use tao::platform::windows::WindowExtWindows as _;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetActiveWindow, GetFocus, SetActiveWindow, SetFocus,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsChild, IsIconic, SC_MINIMIZE, SC_RESTORE, SendMessageW, WM_SYSCOMMAND,
};
use windows::core::BOOL;

fn visible(view: &View) -> Result<bool, WvError> {
    let mut value = BOOL(0);
    // SAFETY: same-thread live controller and a local initialized output.
    unsafe { view.controller.IsVisible(&raw mut value) }.map_err(failure)?;
    Ok(value.as_bool())
}
fn resize(view: &View) -> Result<(), WvError> {
    let mut rect = RECT::default();
    let hwnd = HWND(view.window.hwnd() as _);
    // SAFETY: native size is independently queried, not derived from backend state.
    unsafe { GetClientRect(hwnd, &raw mut rect) }.map_err(failure)?;
    let size = PhysicalSize::new(
        u32::try_from(rect.right - rect.left).map_err(failure)?,
        u32::try_from(rect.bottom - rect.top).map_err(failure)?,
    );
    view.handle_host_window_event(&WindowEvent::Resized(size));
    Ok(())
}
fn native_minimize(view: &View, yes: bool) -> Result<(), WvError> {
    let hwnd = HWND(view.window.hwnd() as _);
    // SAFETY: the caller retains this same-thread fixture through both transitions.
    unsafe {
        let _ = SendMessageW(
            hwnd,
            WM_SYSCOMMAND,
            Some(WPARAM(if yes { SC_MINIMIZE } else { SC_RESTORE } as usize)),
            Some(LPARAM(0)),
        );
    }
    if unsafe { IsIconic(hwnd) }.as_bool() != yes {
        return Err(failure("native minimize stimulus disagrees"));
    }
    Ok(())
}
fn focus_parent(view: &View) -> Result<(), WvError> {
    let hwnd = HWND(view.window.hwnd() as _);
    // SAFETY: native activation and focus on this owned fixture's thread. Previous
    // HWND return can be null; independently read current activation/focus below.
    unsafe {
        let _ = SetActiveWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
    }
    if unsafe { GetActiveWindow() } != hwnd || unsafe { GetFocus() } != hwnd {
        return Err(failure("native retry focus stimulus disagrees"));
    }
    Ok(())
}
fn exercise(view: &View, step: Step) -> Result<(), WvError> {
    native_minimize(view, true)?;
    if step == Step::Hide {
        let fault = Failure::once(step);
        resize(view)?;
        if !fault.fired() || !visible(view)? {
            return Err(failure("hide failure was not independently observed"));
        }
        drop(fault);
        resize(view)?;
        if visible(view)? {
            return Err(failure("failed hide was not retried"));
        }
        native_minimize(view, false)?;
        resize(view)?;
    } else {
        resize(view)?;
        if visible(view)? {
            return Err(failure("retry prerequisite did not hide actual controller"));
        }
        native_minimize(view, false)?;
        if step == Step::Focus {
            focus_parent(view)?;
        }
        let fault = Failure::once(step);
        resize(view)?;
        if !fault.fired() {
            return Err(failure("native step fault was never consumed"));
        }
        if step != Step::Focus && visible(view)? {
            return Err(failure("failed restore exposed a hidden controller"));
        }
        drop(fault);
        resize(view)?;
        if !visible(view)? {
            return Err(failure(
                "restore retry lost the hidden controller obligation",
            ));
        }
        if step == Step::Focus {
            let hwnd = HWND(view.window.hwnd() as _);
            let focus = unsafe { GetFocus() };
            if unsafe { GetActiveWindow() } != hwnd || !unsafe { IsChild(hwnd, focus) }.as_bool() {
                return Err(failure(
                    "failed focus was not retried into actual WebView child",
                ));
            }
        }
    }
    if !visible(view)? {
        return Err(failure("healthy retry did not leave controller visible"));
    }
    println!("KELD_HOST_VIEW_RETRY step={step:?} native=true recovered=true");
    Ok(())
}

#[test]
#[ignore = "real WebView2 minimize/restore with one-shot native failure and healthy retry"]
fn windows_view_host_failed_native_steps_remain_retryable() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let root = profile_fixture_root("host-retry");
        std::fs::create_dir_all(&root).map_err(failure)?;
        println!("KELD_HOST_RETRY_ROOT {}", root.display());
        let mut engine = profile_fixture_engine(
            &root,
            WebProfileSelection::ephemeral_dev(EphemeralProfile::from_host_random([94; 32])?),
        )?;
        let (events, _event_rx) = mpsc::channel();
        let id = engine.create_app(&blank_spec(), events.clone())?;
        let observation = [Step::Hide, Step::Bounds, Step::Show, Step::Focus]
            .into_iter()
            .try_for_each(|step| exercise(&engine.views[&id.0], step));
        // A failed assertion never skips the original release path. Late native
        // events still use the healthy production owner with no pending test fault.
        let (tx, rx) = mpsc::channel();
        tx.send(AppWindowCommand::Quit).map_err(failure)?;
        let release = run_renderer_until_quit(engine, rx, events);
        let cleanup = std::fs::remove_dir_all(&root).map_err(failure);
        if cleanup.is_ok() {
            println!("KELD_HOST_RETRY_CLEANED {}", root.display());
        }
        observation.and(release).and(cleanup)
    })
}
