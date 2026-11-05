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
    loop {
        let state = native_control::read(hwnd, pid);
        if satisfies(&state) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
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

pub(super) fn minimized(
    pipe: &PipeText,
    pages: &mut Pages,
    view: u64,
    hwnd: usize,
    pid: u32,
) -> (NativeState, Instant) {
    let deadline = Instant::now() + OBSERVATION_BOUND;
    native_control::minimize(hwnd, pid);
    native_until(hwnd, pid, deadline, |state| state.minimized);
    println!("KELD_GH1188_AC26_MINIMIZE at_risk=1 view={view}");
    pages.wait(pipe, view, deadline, |page| {
        page["visibilityState"] == "hidden"
    });
    // The initial IsIconic transition can precede foreground settlement. Sample
    // the actual minimized/deactivated state within the same original bound.
    let native = native_until(hwnd, pid, deadline, |state| {
        state.minimized && state.foreground != hwnd
    });
    println!("KELD_GH1188_AC26_MINIMIZED_FOCUS at_risk=1 view={view}");
    (native, deadline)
}

pub(super) fn restored(
    pipe: &PipeText,
    pages: &mut Pages,
    view: u64,
    hwnd: usize,
    pid: u32,
    expected_rect: [i32; 4],
) -> (NativeState, Instant) {
    let deadline = Instant::now() + OBSERVATION_BOUND;
    native_control::restore(hwnd, pid);
    native_until(hwnd, pid, deadline, |state| !state.minimized);
    pages.wait(pipe, view, deadline, |page| {
        page["visibilityState"] == "visible"
    });
    let foreground = native_until(hwnd, pid, deadline, |state| state.foreground == hwnd);
    assert_eq!(
        foreground.window.outer_rect, expected_rect,
        "restore did not recover its prior native frame"
    );
    println!("KELD_GH1188_AC26_RESTORE_FOCUS at_risk=1 view={view}");
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
