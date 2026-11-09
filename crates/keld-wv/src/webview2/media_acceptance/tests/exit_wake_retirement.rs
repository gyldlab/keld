//! Real profile exit plus a labelled error after native wait cancellation.

use super::profile_release::raw_phase;
use super::{
    FIXTURE_DEADLINE, blank_spec, failure, profile_fixture_engine, profile_fixture_root,
    run_renderer_until_quit, run_with_watchdog,
};
use crate::profile::{ProfileIdentity, WebProfileSelection};
use crate::webview2::media_acceptance::{exit_wake_control, release_receipt_control};
use crate::webview2::windows_profile_plan;
use crate::{AppWindowCommand, WebEngine, WvError};
use std::sync::mpsc;
use tao::platform::windows::WindowExtWindows as _;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

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
        let (commands, receiver) = mpsc::channel();
        let (events, _events_alive) = mpsc::channel();
        let result = if hello {
            // SAFETY: queue WM_CLOSE on our own live native window, on its owning
            // thread. No pointer payload; the real hello event loop performs Close.
            // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-postmessagew
            unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(failure)?;
            engine.run_until_closed()
        } else {
            commands.send(AppWindowCommand::Quit).map_err(failure)?;
            run_renderer_until_quit(engine, receiver, events)
        };
        let phase = raw_phase(&plan)?;
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
