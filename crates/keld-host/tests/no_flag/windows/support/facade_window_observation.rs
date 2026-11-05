//! AC26 independent native/page observations. The fixed page readback comes from
//! the approved U18 hook; this module computes no product state or authority.

use crate::PRODUCT_DEADLINE;
use crate::support::process::{PipeText, pipe_line_after, pipe_text_by};
use crate::support::window::native_control::{self, FocusComparator, NativeState};
use serde_json::Value;
use std::thread;
use std::time::{Duration, Instant};

pub(super) const OBSERVATION_BOUND: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(super) struct Pages {
    pub(super) cursor: usize,
}

impl Pages {
    pub(super) fn wait(
        &mut self,
        pipe: &PipeText,
        view: u64,
        deadline: Instant,
        satisfies: impl Fn(&Value) -> bool,
    ) -> Value {
        let mut last = None;
        loop {
            while let Some((line, cursor)) = pipe_line_after(pipe, "PAGE_READBACK ", self.cursor) {
                self.cursor = cursor;
                let rest = line
                    .strip_prefix("PAGE_READBACK ")
                    .expect("readback prefix");
                let (observed, json) = rest.split_once(' ').expect("view and actual JSON");
                let observed = observed.parse::<u64>().expect("native view number");
                if observed != view {
                    continue;
                }
                let value: Value =
                    serde_json::from_str(json).expect("actual ExecuteScript object JSON");
                assert!(
                    value["screenX"].is_number()
                        && value["screenY"].is_number()
                        && value["innerWidth"].is_number()
                        && value["visibilityState"].is_string()
                        && value["hasFocus"].is_boolean(),
                    "invalid fixed page readback: {line}"
                );
                if satisfies(&value) {
                    return value;
                }
                last = Some(value);
            }
            assert!(
                Instant::now() < deadline,
                "page did not satisfy the independently observed native condition within 5 s; view={view} actual={last:?}\n{}",
                pipe_text_by(pipe, Instant::now())
            );
            native_control::pump();
            thread::park_timeout(Duration::from_millis(10));
        }
    }
}

pub(super) fn native_until(
    hwnd: usize,
    pid: u32,
    deadline: Instant,
    satisfies: impl Fn(&NativeState) -> bool,
) -> NativeState {
    native_until_observed(hwnd, pid, deadline, satisfies, |_| {})
}

fn native_until_observed(
    hwnd: usize,
    pid: u32,
    deadline: Instant,
    satisfies: impl Fn(&NativeState) -> bool,
    on_timeout: impl Fn(&NativeState),
) -> NativeState {
    loop {
        let state = native_control::read(hwnd, pid);
        if satisfies(&state) {
            return state;
        }
        let within_deadline = Instant::now() < deadline;
        if !within_deadline {
            on_timeout(&state);
        }
        assert!(
            within_deadline,
            "INVALID native stimulus: required state was never established; zero at-risk runs: {state:?}"
        );
        native_control::pump();
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(super) fn numeric(value: &Value, name: &str) -> f64 {
    value[name]
        .as_f64()
        .unwrap_or_else(|| panic!("missing actual {name}: {value}"))
}

pub(super) fn rect_size(rect: [i32; 4]) -> [i64; 2] {
    [
        i64::from(rect[2]) - i64::from(rect[0]),
        i64::from(rect[3]) - i64::from(rect[1]),
    ]
}

pub(super) struct MoveObservation {
    pub(super) native: NativeState,
    pub(super) deadline: Instant,
}

pub(super) fn moved(
    pipe: &PipeText,
    pages: &mut Pages,
    view: u64,
    hwnd: usize,
    pid: u32,
) -> MoveObservation {
    let before = native_control::read(hwnd, pid);
    let page_before = pages.wait(pipe, view, Instant::now() + PRODUCT_DEADLINE, |page| {
        page["visibilityState"] == "visible"
            && (numeric(page, "innerWidth") - f64::from(before.client[0]) / before.scale).abs()
                <= 1.0
    });
    let cursor = pages.cursor;
    let started = Instant::now();
    let deadline = started + OBSERVATION_BOUND;
    native_control::move_by(hwnd, pid, 120, 80);
    let after = native_until(hwnd, pid, deadline, |state| {
        state.window.outer_rect[..2] != before.window.outer_rect[..2]
    });
    assert_eq!(
        rect_size(after.window.outer_rect),
        rect_size(before.window.outer_rect)
    );
    assert_eq!(
        after.dpi, before.dpi,
        "this move cell must stay on one DPI scale"
    );
    let dx = (f64::from(after.window.outer_rect[0]) - f64::from(before.window.outer_rect[0]))
        / after.scale;
    let dy = (f64::from(after.window.outer_rect[1]) - f64::from(before.window.outer_rect[1]))
        / after.scale;
    println!(
        "KELD_GH1188_AC26_MOVE at_risk=1 view={view} before={:?} after={:?} scale={}",
        before.window.outer_rect, after.window.outer_rect, after.scale
    );
    let page = pages.wait(pipe, view, deadline, |page| {
        (numeric(page, "screenX") - numeric(&page_before, "screenX") - dx).abs() <= 1.0
            && (numeric(page, "screenY") - numeric(&page_before, "screenY") - dy).abs() <= 1.0
    });
    assert!(
        pipe_line_after(pipe, &format!("PARENT_MOVED {view}"), cursor).is_some(),
        "native parent notification returned no D16 receipt"
    );
    println!(
        "KELD_GH1188_AC26_MOVE_PAGE view={view} elapsed_ms={} before={page_before} after={page}",
        started.elapsed().as_millis()
    );
    MoveObservation {
        native: after,
        deadline,
    }
}

/// Prepare ordinary restore input only after minimize's independent oracles.
/// The caller keeps this guard through restore, then drops it before another minimize.
pub(super) fn prepare_restore_input(hwnd: usize, pid: u32) -> FocusComparator {
    let before = native_control::read(hwnd, pid);
    let host = crate::support::window::initial_focus::read(hwnd, pid);
    assert!(
        before.minimized && before.foreground != hwnd && host.active != hwnd,
        "INVALID restore preparation: minimize must already be deactivated; native={before:?} host_thread={host:?}"
    );
    let deadline = Instant::now() + OBSERVATION_BOUND;
    let helper = FocusComparator::for_caption_input();
    helper.click_caption(deadline);
    let input = native_until(hwnd, pid, deadline, |state| {
        state.minimized && state.foreground == helper.hwnd()
    });
    let host = crate::support::window::initial_focus::read(hwnd, pid);
    assert!(
        host.active != hwnd,
        "INVALID restore preparation: minimized Host became active; host_thread={host:?}"
    );
    observe_activation_input("restore-input", &input, hwnd, pid, &helper);
    helper
}

/// Read-only input diagnostics; this performs no activation or message pumping.
fn observe_activation_input(
    phase: &str,
    native: &NativeState,
    hwnd: usize,
    pid: u32,
    helper: &FocusComparator,
) {
    let host = crate::support::window::initial_focus::read(hwnd, pid);
    let comparator = crate::support::window::initial_focus::read(helper.hwnd(), std::process::id());
    println!(
        "KELD_GH1188_AC26_ACTIVATION_INPUT phase={phase} target={hwnd} helper={} native={native:?} host_thread={host:?} helper_thread={comparator:?}",
        helper.hwnd()
    );
}

pub(super) fn minimized(
    pipe: &PipeText,
    pages: &mut Pages,
    view: u64,
    hwnd: usize,
    pid: u32,
) -> (NativeState, Instant) {
    let deadline = Instant::now() + OBSERVATION_BOUND;
    native_control::minimize(hwnd, pid, deadline);
    native_until(hwnd, pid, deadline, |state| state.minimized);
    println!("KELD_GH1188_AC26_MINIMIZE at_risk=1 view={view}");
    pages.wait(pipe, view, deadline, |page| {
        page["visibilityState"] == "hidden"
    });
    // The initial IsIconic transition can precede foreground settlement. Sample
    // the actual minimized/deactivated state within the same original bound.
    let native = native_until_observed(
        hwnd,
        pid,
        deadline,
        |state| state.minimized && state.foreground != hwnd,
        |state| {
            let focus = crate::support::window::initial_focus::read(hwnd, pid);
            println!("KELD_GH1188_MINIMIZE_TIMEOUT native={state:?} host_thread={focus:?}");
        },
    );
    // Foreground ownership can differ from the Host UI queue's activation.
    // D11 samples GetActiveWindow on that queue, so establish the matching
    // cross-thread observation before treating a missing public fact as RED.
    loop {
        // Pair the native and thread observations in this iteration. These
        // read-only calls are sequential, not an atomic Windows snapshot.
        let native_at_focus = native_control::read(hwnd, pid);
        let focus = crate::support::window::initial_focus::read(hwnd, pid);
        if focus.active != hwnd {
            println!(
                "KELD_GH1188_AC26_MINIMIZED_FOCUS at_risk=1 view={view} host_thread={focus:?}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "INVALID Host-thread deactivation: zero at-risk focus runs; actual={focus:?} native={native_at_focus:?}"
        );
        native_control::pump();
        thread::park_timeout(Duration::from_millis(10));
    }
    (native, deadline)
}

// Read existing complete pipe records only. These byte positions order receipt by
// the test reader; they do not claim that ExecuteScript started after Restore.
fn restore_pipe_cursor(pipe: &PipeText, mut cursor: usize) -> usize {
    while let Some((_, next)) = pipe_line_after(pipe, "", cursor) {
        cursor = next;
    }
    cursor
}

fn restore_page_trace(pipe: &PipeText, phase: &str, view: u64, after: usize) {
    let mut cursor = after;
    let mut records = 0;
    let mut readbacks = 0;
    let mut tail = std::collections::VecDeque::new();
    let prefix = format!("PAGE_READBACK {view} ");
    while let Some((line, next)) = pipe_line_after(pipe, "", cursor) {
        cursor = next;
        if line.starts_with(&prefix) || line.starts_with("KELD_D16_PAGE_") {
            records += 1;
            readbacks += usize::from(line.starts_with(&prefix));
            tail.push_back((cursor, line));
            if tail.len() > 12 {
                tail.pop_front();
            }
        }
    }
    println!(
        "KELD_GH1188_AC26_RESTORE_PIPE phase={phase} view={view} after={after} through={cursor} records={records} readbacks={readbacks} tail={tail:?}"
    );
}

pub(super) fn restored(
    pipe: &PipeText,
    pages: &mut Pages,
    view: u64,
    hwnd: usize,
    pid: u32,
    expected_rect: [i32; 4],
    helper: &FocusComparator,
) -> (NativeState, Instant) {
    let started = Instant::now();
    let deadline = started + OBSERVATION_BOUND;
    let before_restore_cursor = restore_pipe_cursor(pipe, pages.cursor);
    println!("KELD_GH1188_AC26_RESTORE_ENTER view={view}");
    native_control::restore(hwnd, pid, deadline);
    println!(
        "KELD_GH1188_AC26_RESTORE_RETURN view={view} elapsed_ms={}",
        started.elapsed().as_millis()
    );
    native_until(hwnd, pid, deadline, |state| !state.minimized);
    println!(
        "KELD_GH1188_AC26_RESTORE_NATIVE view={view} elapsed_ms={}",
        started.elapsed().as_millis()
    );
    let visible = pages.wait(pipe, view, deadline, |page| {
        page["visibilityState"] == "visible"
    });
    let visible_native = native_control::read(hwnd, pid);
    observe_activation_input("restore-visible", &visible_native, hwnd, pid, helper);
    println!(
        "KELD_GH1188_AC26_RESTORE_VISIBLE view={view} elapsed_ms={} page_cursor={} before_restore_cursor={before_restore_cursor} already_captured={} actual={visible}",
        started.elapsed().as_millis(),
        pages.cursor,
        pages.cursor <= before_restore_cursor
    );
    restore_page_trace(pipe, "visible", view, before_restore_cursor);
    // Keep Restore's frame/visibility oracles ahead of any new activation.
    // The same-thread comparator received the checked caption input; only that
    // retained owner may supply the existing ordinary foreground stimulus.
    let foreground_input = native_control::read(hwnd, pid);
    assert!(
        !foreground_input.minimized,
        "INVALID restore input: target minimized again before foreground transfer"
    );
    assert_eq!(
        foreground_input.window.outer_rect, expected_rect,
        "restore did not recover its prior native frame before foreground input"
    );
    if foreground_input.foreground != hwnd {
        assert_eq!(
            foreground_input.foreground,
            helper.hwnd(),
            "INVALID restore input: the owned caption comparator lost foreground"
        );
        assert!(
            Instant::now() < deadline,
            "INVALID restore input: original deadline expired before foreground transfer"
        );
        observe_activation_input(
            "restore-foreground-request",
            &foreground_input,
            hwnd,
            pid,
            helper,
        );
        println!(
            "KELD_GH1188_AC26_RESTORE_FOREGROUND_REQUEST view={view} restore_only_focus_at_risk=0"
        );
        native_control::foreground(hwnd, pid);
        assert!(
            Instant::now() < deadline,
            "INVALID restore input: foreground request exceeded the original deadline"
        );
    }
    let foreground = native_until_observed(
        hwnd,
        pid,
        deadline,
        |state| state.foreground == hwnd,
        |state| {
            observe_activation_input("restore-foreground-timeout", state, hwnd, pid, helper);
            println!(
                "KELD_GH1188_AC26_RESTORE_FOREGROUND_TIMEOUT view={view} elapsed_ms={} page_cursor={}",
                started.elapsed().as_millis(),
                pages.cursor
            );
            restore_page_trace(pipe, "foreground-timeout", view, before_restore_cursor);
        },
    );
    assert_eq!(
        foreground.window.outer_rect, expected_rect,
        "restore did not recover its prior native frame"
    );
    println!("KELD_GH1188_AC26_RESTORE_FOCUS at_risk=1 view={view} restore_only_focus_at_risk=0");
    pages.wait(pipe, view, deadline, |page| page["hasFocus"] == true);
    let native = native_control::read(hwnd, pid);
    assert!(
        native.focus_is_child,
        "native restored focus did not reach a WebView child: {native:?}"
    );
    println!(
        "KELD_GH1188_AC26_RESTORED_FOCUS view={view} child_hwnd={}",
        native.focus
    );
    (native, deadline)
}

pub(super) fn focus_cycle(pipe: &PipeText, pages: &mut Pages, view: u64, hwnd: usize, pid: u32) {
    let before = native_control::read(hwnd, pid);
    assert!(
        before.foreground == hwnd && before.focus_is_child,
        "INVALID focus-return input; target/page must initially own focus: {before:?}"
    );
    let helper = FocusComparator::new();
    helper.activate();
    let deadline = Instant::now() + OBSERVATION_BOUND;
    native_until(hwnd, pid, deadline, |state| {
        state.foreground == helper.hwnd()
    });
    pages.wait(pipe, view, deadline, |page| page["hasFocus"] == false);
    native_control::foreground(hwnd, pid);
    let deadline = Instant::now() + OBSERVATION_BOUND;
    native_until(hwnd, pid, deadline, |state| state.foreground == hwnd);
    println!(
        "KELD_GH1188_AC26_FOCUS_RETURN at_risk=1 view={view} comparator={}",
        helper.hwnd()
    );
    pages.wait(pipe, view, deadline, |page| page["hasFocus"] == true);
    let after = native_control::read(hwnd, pid);
    assert!(
        after.focus_is_child,
        "activation returned keyboard focus to the parent rather than WebView: {after:?}"
    );
    println!(
        "KELD_GH1188_AC26_RETURNED_FOCUS view={view} child_hwnd={}",
        after.focus
    );
    drop(helper);
}

pub(super) fn resized(
    pipe: &PipeText,
    pages: &mut Pages,
    view: u64,
    hwnd: usize,
    pid: u32,
) -> MoveObservation {
    let before = native_control::read(hwnd, pid);
    let page_before = pages.wait(pipe, view, Instant::now() + PRODUCT_DEADLINE, |page| {
        page["visibilityState"] == "visible"
            && (numeric(page, "innerWidth") - f64::from(before.client[0]) / before.scale).abs()
                <= 1.0
    });
    let deadline = Instant::now() + OBSERVATION_BOUND;
    native_control::resize_by(hwnd, pid, 160, 120);
    let after = native_until(hwnd, pid, deadline, |state| {
        rect_size(state.window.outer_rect) != rect_size(before.window.outer_rect)
    });
    assert_eq!(after.window.outer_rect[..2], before.window.outer_rect[..2]);
    assert_eq!(after.dpi, before.dpi);
    let old_width = rect_size(before.window.outer_rect)[0];
    let new_width = rect_size(after.window.outer_rect)[0];
    let expected =
        f64::from(i32::try_from(new_width - old_width).expect("Win32 resize delta")) / after.scale;
    println!(
        "KELD_GH1188_AC26_RESIZE at_risk=1 view={view} before={:?} after={:?}",
        before.window.outer_rect, after.window.outer_rect
    );
    pages.wait(pipe, view, deadline, |page| {
        (numeric(page, "innerWidth") - numeric(&page_before, "innerWidth") - expected).abs() <= 1.0
    });
    MoveObservation {
        native: after,
        deadline,
    }
}
