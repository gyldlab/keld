//! Genuine hello/app native minimize/restore state, not managed-window admission.

use super::super::{FIXTURE_DEADLINE, host_window_probe, run_with_watchdog};
use super::{
    blank_spec, failure, profile_fixture_engine, profile_fixture_root, run_renderer_until_quit,
};
use crate::profile::{EphemeralProfile, WebProfileSelection};
use crate::{AppWindowCommand, WebEngine, WvError};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tao::event_loop::ControlFlow;
use tao::platform::windows::WindowExtWindows as _;
use tao::window::WindowId;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsIconic, PostMessageW, SC_MINIMIZE, SC_RESTORE, SendMessageW, WM_CLOSE,
    WM_SYSCOMMAND,
};
use windows::core::BOOL;

fn controller_state(controller: &ICoreWebView2Controller) -> Result<([i32; 4], bool), WvError> {
    let mut rect = RECT::default();
    let mut visible = BOOL(0);
    // SAFETY: live controller and local out-parameters on its owning UI thread.
    // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller
    unsafe {
        controller
            .Bounds(&raw mut rect)
            .and_then(|()| controller.IsVisible(&raw mut visible))
    }
    .map_err(failure)?;
    Ok((
        [rect.left, rect.top, rect.right, rect.bottom],
        visible.as_bool(),
    ))
}
fn client_bounds(hwnd: HWND) -> Result<[i32; 4], WvError> {
    let mut rect = RECT::default();
    // SAFETY: independent read of this fixture's live native parent.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getclientrect
    unsafe { GetClientRect(hwnd, &raw mut rect) }.map_err(failure)?;
    Ok([rect.left, rect.top, rect.right, rect.bottom])
}

#[derive(Clone, Copy)]
enum Phase {
    Start,
    Minimized,
    Restored,
}

/// Owns only the test's action/observation sequence. It never supplies a result to
/// the production operation or reads the backend's hidden-for-minimize flag.
struct MinimizeProbe {
    hello: bool,
    hwnd: HWND,
    window_id: WindowId,
    controller: ICoreWebView2Controller,
    initial: ([i32; 4], bool),
    phase: Phase,
    count: Rc<Cell<u32>>,
    failure: Option<String>,
    deadline: Instant,
}
impl MinimizeProbe {
    fn advance(&mut self, point: host_window_probe::Point) -> Result<bool, WvError> {
        if Instant::now() >= self.deadline {
            return Err(failure("native minimize/restore event deadline"));
        }
        match (self.phase, point) {
            (Phase::Start, host_window_probe::Point::Idle) => {
                self.phase = Phase::Minimized;
                // SAFETY: same-thread fixture window. Use the genuine system
                // command: Tao0.35.3 updates its minimize flags on WM_SYSCOMMAND.
                // https://learn.microsoft.com/windows/win32/menurc/wm-syscommand
                unsafe {
                    let _ = SendMessageW(
                        self.hwnd,
                        WM_SYSCOMMAND,
                        Some(WPARAM(SC_MINIMIZE as usize)),
                        Some(LPARAM(0)),
                    );
                }
            }
            (Phase::Minimized, host_window_probe::Point::Resized(id)) if id == self.window_id => {
                // SAFETY: actual native minimize state, not a 0x0 heuristic.
                if !unsafe { IsIconic(self.hwnd) }.as_bool() {
                    return Ok(false);
                }
                self.count.set(self.count.get() + 1);
                let current = controller_state(&self.controller)?;
                println!(
                    "KELD_HOST_VIEW_MIN hello={} initial={:?} current={current:?}",
                    self.hello, self.initial
                );
                if current.1 || current.0 != self.initial.0 {
                    self.failure = Some(format!(
                        "minimized controller must be hidden without changing bounds: {current:?}"
                    ));
                }
                self.phase = Phase::Restored;
                // SAFETY: restore the same retained fixture, including after a
                // negative verdict; normal backend release still precedes cleanup.
                unsafe {
                    PostMessageW(
                        Some(self.hwnd),
                        WM_SYSCOMMAND,
                        WPARAM(SC_RESTORE as usize),
                        LPARAM(0),
                    )
                }
                .map_err(failure)?;
            }
            (Phase::Restored, host_window_probe::Point::Resized(id)) if id == self.window_id => {
                // SAFETY: query the fixture-owned native parent on its UI thread.
                if unsafe { IsIconic(self.hwnd) }.as_bool() {
                    return Ok(false);
                }
                self.count.set(self.count.get() + 1);
                let current = controller_state(&self.controller)?;
                let client = client_bounds(self.hwnd)?;
                println!(
                    "KELD_HOST_VIEW_RESTORE hello={} current={current:?} native_client={client:?}",
                    self.hello
                );
                if !current.1 || current.0 != client {
                    return Err(failure("restored native bounds and visibility disagree"));
                }
                if let Some(message) = self.failure.take() {
                    return Err(failure(message));
                }
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
}

fn minimize_restore(hello: bool) -> Result<(), WvError> {
    let root = profile_fixture_root("host-view");
    std::fs::create_dir_all(&root).map_err(failure)?;
    println!("KELD_HOST_VIEW_ROOT {}", root.display());
    let mut engine = profile_fixture_engine(
        &root,
        WebProfileSelection::ephemeral_dev(EphemeralProfile::from_host_random([91; 32])?),
    )?;
    let (events, _event_rx) = mpsc::channel();
    let id = if hello {
        engine.create(&blank_spec())?
    } else {
        engine.create_app(&blank_spec(), events.clone())?
    };
    let view = &engine.views[&id.0];
    let hwnd = HWND(view.window.hwnd() as _);
    let initial = controller_state(&view.controller)?;
    if !initial.1 || initial.0[2] <= 0 || initial.0[3] <= 0 {
        return Err(failure(
            "initial native controller must be visible and nonempty",
        ));
    }
    let (commands, receiver) = mpsc::channel();
    let verdict = Rc::new(RefCell::new(None));
    let outcome = Rc::clone(&verdict);
    let native_events = Rc::new(Cell::new(0_u32));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut probe = MinimizeProbe {
        hello,
        hwnd,
        window_id: view.window.id(),
        controller: view.controller.clone(),
        initial,
        phase: Phase::Start,
        count: Rc::clone(&native_events),
        failure: None,
        deadline,
    };
    let mut done = false;
    let hook = host_window_probe::install(Box::new(move |point, _, flow| {
        if done {
            return;
        }
        match probe.advance(point) {
            Ok(false) => {
                // Test-only bounded wake to drain Tao's buffered native events.
                // The deadline and native state, never elapsed sleeps, decide success.
                *flow = ControlFlow::WaitUntil(
                    (Instant::now() + Duration::from_millis(10)).min(deadline),
                );
                return;
            }
            Ok(true) => *outcome.borrow_mut() = Some(Ok(())),
            Err(error) => *outcome.borrow_mut() = Some(Err(error)),
        }
        done = true;
        if hello {
            // SAFETY: native close of our live fixture, handled by the real hello loop.
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        } else {
            let _ = commands.send(AppWindowCommand::Quit);
        }
    }));
    let release = if hello {
        engine.run_until_closed()
    } else {
        run_renderer_until_quit(engine, receiver, events)
    };
    drop(hook);
    let observation = verdict
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err(failure("no complete native host observation")));
    let cleanup = std::fs::remove_dir_all(&root).map_err(failure);
    if cleanup.is_ok() {
        println!("KELD_HOST_VIEW_CLEANED {}", root.display());
    }
    if let Err(error) = &release {
        eprintln!("KELD_HOST_VIEW_RELEASE_ERROR {error}");
    }
    println!(
        "KELD_HOST_VIEW_EVENT_COUNT hello={hello} count={}",
        native_events.get()
    );
    observation.and(release).and(cleanup)
}
#[test]
#[ignore = "real Windows app-loop minimize/restore native controller state"]
fn windows_app_view_minimize_restore_obligations() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || minimize_restore(false))
}
#[test]
#[ignore = "real Windows hello-loop minimize/restore native controller state"]
fn windows_hello_view_minimize_restore_obligations() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || minimize_restore(true))
}
