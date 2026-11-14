//! Real profile exit plus a labelled error after native wait cancellation.

use super::profile_release::raw_phase;
use super::{
    FIXTURE_DEADLINE, blank_spec, failure, profile_fixture_engine, profile_fixture_root,
    run_with_watchdog,
};
use crate::profile::{ProfileIdentity, WebProfileSelection};
use crate::webview2::media_acceptance::{exit_wake_control, release_receipt_control};
use crate::webview2::windows_profile_plan;
use crate::{AppWindowCommand, WebEngine, WvError};
use std::sync::mpsc;
use tao::platform::windows::WindowExtWindows as _;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, GetWindowThreadProcessId, IsWindow, PostMessageW, WM_CLOSE,
};

fn cleanup_live_parent(hwnd: HWND, owner_thread: u32) -> Result<(), WvError> {
    // Cleanup the retained native parent before asserting the frozen
    // result. Only the original UI thread can destroy this fixture.
    // SAFETY: both identity queries are read-only.
    let (current_owner, current_thread) =
        unsafe { (GetWindowThreadProcessId(hwnd, None), GetCurrentThreadId()) };
    if current_owner != owner_thread || current_owner != current_thread {
        return Err(failure("terminal HWND cleanup ownership is unproved"));
    }
    // SAFETY: original same-thread fixture parent, after its Rust owner
    // was consumed; no pointer payload or external window is touched.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-destroywindow
    unsafe { DestroyWindow(hwnd) }.map_err(failure)?;
    // SAFETY: independent post-cleanup query, not the semantic sample.
    if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err(failure("terminal HWND cleanup did not finish"));
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep the actual native wait, frozen product observations and same owned cleanup together in this bounded fixture case."
)]
fn case(hello: bool, inject_error: bool) -> Result<(), WvError> {
    let root = profile_fixture_root("exit-wake-retirement");
    std::fs::create_dir_all(&root).map_err(failure)?;
    println!("KELD_WAKE_RETIRE_ROOT {}", root.display());
    let result = (|| {
        let _scope = exit_wake_control::Scope::enter(inject_error);
        let _receipt =
            release_receipt_control::Scope::enter(release_receipt_control::Mode::Healthy);
        let identity =
            ProfileIdentity::from_host_verified_parts([92; 32], "dev.keld.exit-wake-retirement")?;
        let selection = WebProfileSelection::Persistent(identity);
        let plan = windows_profile_plan(&root, selection)?;
        let mut engine = profile_fixture_engine(&root, selection)?;
        let id = engine.create(&blank_spec())?;
        let initial = raw_phase(&plan)?;
        let hwnd = HWND(engine.views[&id.0].window.hwnd() as _);
        // SAFETY: read the exact live fixture parent on its creating thread.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindow
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid
        let (initially_live, owner_thread) = unsafe {
            (
                IsWindow(Some(hwnd)).as_bool(),
                GetWindowThreadProcessId(hwnd, None),
            )
        };
        if !initially_live || owner_thread == 0 {
            return Err(failure(
                "ineligible terminal HWND probe: parent was not live",
            ));
        }
        let (commands, receiver) = mpsc::channel();
        let (events, _events_alive) = mpsc::channel();
        let mut retained_engine = Some(engine);
        let result = if hello {
            // SAFETY: queue WM_CLOSE on our own live native window, on its owning
            // thread. No pointer payload; the real hello event loop performs Close.
            // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-postmessagew
            unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(failure)?;
            retained_engine
                .take()
                .ok_or_else(|| failure("missing fixture engine"))?
                .run_until_closed()
        } else {
            commands.send(AppWindowCommand::Quit).map_err(failure)?;
            retained_engine
                .as_mut()
                .ok_or_else(|| failure("missing fixture engine"))?
                .run_app_until_quit(
                    receiver,
                    events,
                    crate::webview2::AppReadyTrigger::NavigationReady,
                    || {},
                    |_| Ok::<(), WvError>(()),
                )
        };
        // First operation after the engine call returns: no pump or
        // profile read can turn an eventually-destroyed HWND into a pass.
        // SAFETY: read-only query of the captured fixture HWND.
        let hwnd_live_at_return = unsafe { IsWindow(Some(hwnd)) }.as_bool();
        println!(
            "KELD_TERMINAL_HWND hello={hello} inject={inject_error} hwnd={} live_at_return={hwnd_live_at_return}",
            hwnd.0 as usize
        );
        if hwnd_live_at_return {
            cleanup_live_parent(hwnd, owner_thread)?;
        }
        let phase = raw_phase(&plan)?;
        if !hello {
            let engine = retained_engine
                .as_ref()
                .ok_or_else(|| failure("missing actual engine"))?;
            if inject_error {
                let run = engine
                    .retained_run
                    .as_ref()
                    .ok_or_else(|| failure("failed retirement dropped the actual run owner"))?;
                let release = run.release.as_ref().ok_or_else(|| {
                    failure("failed retirement dropped the original release owner")
                })?;
                let original = std::rc::Rc::as_ptr(release);
                let weak = std::rc::Rc::downgrade(release);
                if !matches!(&*release.process.borrow(),
                    Some(crate::webview2::BrowserProcess::Watched(wake))
                    if matches!(wake.wait, crate::webview2::WaitRegistration::Unproved))
                    || release.retire_loop_wait().is_ok()
                    || weak
                        .upgrade()
                        .is_none_or(|same| std::rc::Rc::as_ptr(&same) != original)
                    || engine.event_loop.is_some()
                    || engine.profile.is_none()
                    || run.bridge.is_none()
                    || !run.managed_drained()
                {
                    return Err(failure(
                        "failed wait lost custody, retried or granted restart authority",
                    ));
                }
                println!(
                    "KELD_WAKE_CUSTODY original_run=true original_release=true phase=unproved second_retirement=refused"
                );
            } else if engine.retained_run.is_some() || engine.event_loop.is_some() {
                return Err(failure(
                    "healthy retirement retained a run or granted restart authority",
                ));
            }
        }
        let facts = exit_wake_control::Scope::observations()?;
        let (receipts, injected_receipts) = release_receipt_control::Scope::observations();
        println!(
            "KELD_WAKE_RETIRE_RESULT hello={hello} inject={inject_error} before={initial} after={phase} normal={receipts} facts={facts:?} result={result:?}"
        );
        if initial != "running"
            || receipts != 1
            || injected_receipts != 0
            || facts.attempts != 1
            || facts.native_successes != 1
        {
            return Err(failure(
                "fixture did not retire exactly one real browser wait after its actual normal exit",
            ));
        }
        if inject_error {
            let message = result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_default();
            if facts.injected_errors != 1
                || facts.retained_live_handles != 1
                || phase != "stopping"
                || !message.contains("KELD-WV-009")
                || !message.contains("browser-exit wait retirement")
            {
                return Err(failure(
                    "unconfirmed native wait retirement committed Idle or lost its typed failure",
                ));
            }
        } else if result.is_err()
            || phase != "idle"
            || facts.injected_errors != 0
            || facts.retained_live_handles != 0
        {
            return Err(failure(
                "healthy checked native wait retirement did not preserve normal Idle",
            ));
        }
        if hwnd_live_at_return {
            return Err(failure(
                "terminal returned while its captured native parent was still live",
            ));
        }
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root).map_err(failure);
    match &cleanup {
        Ok(()) => println!("KELD_WAKE_RETIRE_CLEANED {}", root.display()),
        Err(error) => eprintln!("KELD_WAKE_RETIRE_CLEANUP_ERROR {} {error}", root.display()),
    }
    result.and(cleanup)
}

#[test]
#[ignore = "real Windows app profile/browser exit and labelled native-retirement result fault"]
fn windows_app_retirement_is_checked_before_profile_idle() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        case(false, false)?;
        case(false, true)?;
        case(false, false)
    })
}

#[test]
#[ignore = "real Windows hello profile/browser exit and labelled native-retirement result fault"]
fn windows_hello_retirement_is_checked_before_profile_idle() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        case(true, false)?;
        case(true, true)?;
        case(true, false)
    })
}
