//! Native top-level movement and independent page screen-coordinate observations.

use super::super::{FIXTURE_DEADLINE, host_window_probe, run_with_watchdog};
use super::{
    blank_spec, failure, profile_fixture_engine, profile_fixture_root, run_renderer_until_quit,
};
use crate::profile::{EphemeralProfile, WebProfileSelection};
use crate::{AppWindowCommand, WebEngine, WvError};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tao::dpi::PhysicalPosition;
use tao::event_loop::ControlFlow;
use tao::platform::windows::WindowExtWindows as _;
use webview2_com::ExecuteScriptCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, PostMessageW, WM_CLOSE};
use windows::core::HSTRING;

fn native_rect(hwnd: HWND) -> Result<[i32; 4], WvError> {
    let mut r = RECT::default();
    // SAFETY: independently query the same live fixture's native HWND on its thread.
    unsafe { GetWindowRect(hwnd, &raw mut r) }.map_err(failure)?;
    Ok([r.left, r.top, r.right, r.bottom])
}
fn page_position(webview: &ICoreWebView2) -> Result<[i32; 2], WvError> {
    let (tx, rx) = mpsc::channel();
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |status, value| {
        let _ = tx.send(status.map(|()| value));
        Ok(())
    }));
    // SAFETY: the same-thread live test view owns this callback; the channel result
    // is independent browser JavaScript data, not the host's expected rectangle.
    unsafe { webview.ExecuteScript(&HSTRING::from("[window.screenX, window.screenY]"), &handler) }
        .map_err(failure)?;
    let text = crate::webview2::wait_for_launch_with_message_pump_until(
        &rx,
        Instant::now() + Duration::from_secs(3),
    )?
    .map_err(failure)?;
    serde_json::from_str(&text).map_err(failure)
}

fn position_case(hello: bool) -> Result<(), WvError> {
    let root = profile_fixture_root("host-position");
    std::fs::create_dir_all(&root).map_err(failure)?;
    println!("KELD_HOST_POSITION_ROOT {}", root.display());
    let mut engine = profile_fixture_engine(
        &root,
        WebProfileSelection::ephemeral_dev(EphemeralProfile::from_host_random([93; 32])?),
    )?;
    let (events, _event_rx) = mpsc::channel();
    let id = if hello {
        engine.create(&blank_spec())?
    } else {
        engine.create_app(&blank_spec(), events.clone())?
    };
    let view = &engine.views[&id.0];
    let hwnd = HWND(view.window.hwnd() as _);
    let window_id = view.window.id();
    let browser = view.webview.clone();
    let before_native = native_rect(hwnd)?;
    let before_page = page_position(&browser)?;
    let (commands, receiver) = mpsc::channel();
    let verdict = Rc::new(RefCell::new(None));
    let outcome = Rc::clone(&verdict);
    let mut stage = 0;
    let limit = Instant::now() + Duration::from_secs(10);
    let observer = host_window_probe::install(Box::new(move |point, views, flow| {
        if stage == 2 {
            return;
        }
        let observed = (|| -> Result<(), WvError> {
            if Instant::now() >= limit {
                return Err(failure("native parent position event deadline"));
            }
            match (stage, point) {
                (0, host_window_probe::Point::Idle) => {
                    stage = 1;
                    views[&id.0]
                        .window
                        .set_outer_position(PhysicalPosition::new(
                            before_native[0] + 120,
                            before_native[1] + 80,
                        ));
                }
                (1, host_window_probe::Point::Moved(event_id)) if event_id == window_id => {
                    let after_native = native_rect(hwnd)?;
                    if after_native[..2] == before_native[..2] {
                        return Ok(());
                    }
                    let after_page = page_position(&browser)?;
                    println!(
                        "KELD_HOST_POSITION hello={hello} native_before={before_native:?} native_after={after_native:?} page_before={before_page:?} page_after={after_page:?}"
                    );
                    if before_page == after_page {
                        return Err(failure(
                            "page screen position stayed stale after real native parent move",
                        ));
                    }
                    stage = 2;
                    *outcome.borrow_mut() = Some(Ok(()));
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = observed {
            stage = 2;
            *outcome.borrow_mut() = Some(Err(error));
        }
        if stage == 2 {
            if hello {
                // SAFETY: post a real close to the fixture-owned window from its UI
                // loop. Both message payloads are zero; no borrowed pointer escapes.
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            } else {
                let _ = commands.send(AppWindowCommand::Quit);
            }
        } else {
            *flow = ControlFlow::WaitUntil((Instant::now() + Duration::from_millis(10)).min(limit));
        }
    }));
    let release = if hello {
        engine.run_until_closed()
    } else {
        run_renderer_until_quit(engine, receiver, events)
    };
    drop(observer);
    let observation = verdict
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err(failure("no native position observation")));
    let cleanup = std::fs::remove_dir_all(&root).map_err(failure);
    if cleanup.is_ok() {
        println!("KELD_HOST_POSITION_CLEANED {}", root.display());
    }
    observation.and(release).and(cleanup)
}
#[test]
#[ignore = "real Windows app parent position and page coordinate observation"]
fn windows_app_view_native_parent_position() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || position_case(false))
}
#[test]
#[ignore = "real Windows hello parent position and page coordinate observation"]
fn windows_hello_view_native_parent_position() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || position_case(true))
}
