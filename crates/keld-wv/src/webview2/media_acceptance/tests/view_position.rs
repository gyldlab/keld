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
type PositionReply = mpsc::Receiver<Result<[i32; 2], WvError>>;

fn request_page_position(webview: &ICoreWebView2) -> Result<PositionReply, WvError> {
    let (tx, rx) = mpsc::channel();
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |status, value| {
        let result = status
            .map_err(failure)
            .and_then(|()| serde_json::from_str(&value).map_err(failure));
        let _ = tx.send(result);
        Ok(())
    }));
    // SAFETY: the fixture owns this live UI-thread view. The completion owns its
    // sender, not a borrowed output slot. Return to the normal pump after dispatch.
    // https://learn.microsoft.com/microsoft-edge/webview2/concepts/threading-model
    unsafe { webview.ExecuteScript(&HSTRING::from("[window.screenX, window.screenY]"), &handler) }
        .map_err(failure)?;
    Ok(rx)
}

fn page_position(webview: &ICoreWebView2) -> Result<[i32; 2], WvError> {
    // Only the initial observation is taken before entering the actual Tao loop.
    crate::webview2::wait_for_launch_with_message_pump_until(
        &request_page_position(webview)?,
        Instant::now() + Duration::from_secs(3),
    )?
}

struct PositionReading {
    native: [i32; 4],
    page: [i32; 2],
}

fn observation_deadline(setup_limit: Instant, move_started: Instant) -> Instant {
    setup_limit.min(move_started + Duration::from_secs(5))
}

fn native_displacement_agrees(
    native_before: [i32; 4],
    native_after: [i32; 4],
    page_before: [i32; 2],
    page_after: [i32; 2],
    scale: f64,
) -> bool {
    if !scale.is_finite() || scale <= 0.0 || native_before[..2] == native_after[..2] {
        return false;
    }
    for axis in 0..2 {
        let size_before = i64::from(native_before[axis + 2]) - i64::from(native_before[axis]);
        let size_after = i64::from(native_after[axis + 2]) - i64::from(native_after[axis]);
        if size_before <= 0 || size_before != size_after {
            return false;
        }
        let expected = (f64::from(native_after[axis]) - f64::from(native_before[axis])) / scale;
        let actual = f64::from(page_after[axis]) - f64::from(page_before[axis]);
        if (actual - expected).abs() > 1.0 {
            return false;
        }
    }
    true
}

struct PositionSamples {
    before: [i32; 2],
    native_before: [i32; 4],
    scale: f64,
    pending: Option<PositionReply>,
    stale: u32,
    mismatched: u32,
    move_started: Option<Instant>,
}

impl PositionSamples {
    fn new(before: [i32; 2], native_before: [i32; 4], scale: f64) -> Self {
        // Replay the actual pre-move observation first. Success must still come
        // from a later real browser reply, not from an expected value we computed.
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(before)).expect("queue retained native baseline");
        Self {
            before,
            native_before,
            scale,
            pending: Some(rx),
            stale: 0,
            mismatched: 0,
            move_started: None,
        }
    }

    fn arm_move(&mut self, setup_limit: Instant) -> Instant {
        let start = Instant::now();
        self.move_started = Some(start);
        // AC26(a): charge the native call itself and never extend the old limit.
        observation_deadline(setup_limit, start)
    }

    fn elapsed(&self) -> Duration {
        self.move_started
            .map_or(Duration::ZERO, |start| start.elapsed())
    }

    fn report_match(&self, hello: bool, reading: &PositionReading) {
        println!(
            "KELD_HOST_POSITION hello={hello} native_before={:?} native_after={:?} page_before={:?} page_after={:?} scale={} move_elapsed_ms={} stale_samples={} mismatched_samples={}",
            self.native_before,
            reading.native,
            self.before,
            reading.page,
            self.scale,
            self.elapsed().as_millis(),
            self.stale,
            self.mismatched
        );
    }

    fn report_timeout(&self, stage: u8) {
        println!(
            "KELD_HOST_POSITION_TIMEOUT stage={stage} stale_samples={} mismatched_samples={} move_elapsed_ms={}",
            self.stale,
            self.mismatched,
            self.elapsed().as_millis()
        );
    }

    fn poll(
        &mut self,
        browser: &ICoreWebView2,
        hwnd: HWND,
    ) -> Result<Option<PositionReading>, WvError> {
        let Some(pending) = self.pending.as_ref() else {
            self.pending = Some(request_page_position(browser)?);
            return Ok(None);
        };
        match pending.try_recv() {
            Ok(result) => {
                self.pending = None;
                let position = result?;
                if position == self.before {
                    self.stale += 1;
                    return Ok(None);
                }
                let native = native_rect(hwnd)?;
                if !native_displacement_agrees(
                    self.native_before,
                    native,
                    self.before,
                    position,
                    self.scale,
                ) {
                    if self.mismatched == 0 {
                        println!(
                            "KELD_HOST_POSITION_INTERMEDIATE native={native:?} page={position:?} scale={}",
                            self.scale
                        );
                    }
                    self.mismatched += 1;
                    return Ok(None);
                }
                Ok(Some(PositionReading {
                    native,
                    page: position,
                }))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(failure("page position callback disconnected"))
            }
        }
    }
}

/// Select the existing loop's real termination input; never bypass its release.
fn request_fixture_quit(hello: bool, hwnd: HWND, commands: &mpsc::Sender<AppWindowCommand>) {
    if hello {
        // SAFETY: post a real close to the fixture-owned window from its UI
        // loop. Both message payloads are zero; no borrowed pointer escapes.
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    } else {
        let _ = commands.send(AppWindowCommand::Quit);
    }
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
    let mut samples = PositionSamples::new(before_page, before_native, view.window.scale_factor());
    let mut limit = Instant::now() + Duration::from_secs(10);
    let observer = host_window_probe::install(Box::new(move |point, views, flow| {
        if stage == 3 {
            return;
        }
        let observed = (|| -> Result<(), WvError> {
            if Instant::now() >= limit {
                samples.report_timeout(stage);
                return Err(failure(
                    "native parent move/page position observation deadline",
                ));
            }
            match (stage, point) {
                (0, host_window_probe::Point::Idle) => {
                    stage = 1;
                    limit = samples.arm_move(limit);
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
                    stage = 2;
                }
                (2, host_window_probe::Point::Idle) => {
                    let Some(reading) = samples.poll(&browser, hwnd)? else {
                        return Ok(());
                    };
                    samples.report_match(hello, &reading);
                    stage = 3;
                    *outcome.borrow_mut() = Some(Ok(()));
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = observed {
            stage = 3;
            *outcome.borrow_mut() = Some(Err(error));
        }
        if stage == 3 {
            request_fixture_quit(hello, hwnd, &commands);
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

#[test]
fn position_readback_requires_both_native_scaled_axes_and_unchanged_size() {
    let before = [0, 0, 800, 600];
    let after = [120, 80, 920, 680];
    assert!(native_displacement_agrees(
        before,
        after,
        [10, 20],
        [106, 84],
        1.25
    ));
    assert!(native_displacement_agrees(
        before,
        after,
        [10, 20],
        [130, 100],
        1.0
    ));
    assert!(native_displacement_agrees(
        before,
        after,
        [10, 20],
        [107, 83],
        1.25
    ));
    for wrong in [[74, 116], [108, 84], [106, 86], [-86, -44], [10, 20]] {
        assert!(
            !native_displacement_agrees(before, after, [10, 20], wrong, 1.25),
            "incorrect displacement admitted: {wrong:?}"
        );
    }
    assert!(!native_displacement_agrees(
        before,
        [120, 80, 921, 680],
        [10, 20],
        [106, 84],
        1.25
    ));
    assert!(!native_displacement_agrees(
        before,
        before,
        [10, 20],
        [11, 21],
        1.25
    ));
    assert!(native_displacement_agrees(
        [-600, -400, 200, 200],
        [-720, -480, 80, 120],
        [-500, -300],
        [-596, -364],
        1.25
    ));
}

#[test]
fn position_readback_rejects_invalid_scale_and_does_not_overflow_native_subtraction() {
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(!native_displacement_agrees(
            [0, 0, 800, 600],
            [120, 80, 920, 680],
            [10, 20],
            [106, 84],
            scale
        ));
    }
    assert!(!native_displacement_agrees(
        [i32::MIN, 0, i32::MAX, 600],
        [0, 80, 800, 680],
        [i32::MIN, 20],
        [i32::MAX, 100],
        1.0
    ));
}

#[test]
fn position_deadline_is_five_seconds_from_move_without_renewing_setup() {
    let now = Instant::now();
    assert_eq!(
        observation_deadline(now + Duration::from_secs(10), now),
        now + Duration::from_secs(5)
    );
    assert_eq!(
        observation_deadline(now + Duration::from_secs(2), now),
        now + Duration::from_secs(2)
    );
    assert_eq!(observation_deadline(now, now), now);
}
