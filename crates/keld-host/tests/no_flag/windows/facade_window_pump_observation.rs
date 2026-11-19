//! Native pump progress observations. The parent scenario owns every resource;
//! these helpers only read its control stream and drive a checked owned move.

use super::super::fixture::FacadeRun;
use super::super::initial::capture_native;
use crate::support::window::native_control;
use crate::{PRODUCT_DEADLINE, ui_end_kill_switch};
use serde_json::Value;
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn pump_record(
    run: &mut FacadeRun,
    expected: &str,
    closes: &mut Vec<Value>,
    deadline: Instant,
) -> Value {
    loop {
        let (kind, value) = run
            .try_next_by(deadline)
            .unwrap_or_else(|error| panic!("native pump {expected} receipt: {error}"));
        if kind == "CLOSE_EVENT" {
            closes.push(value);
        } else {
            assert_eq!(kind, expected, "unexpected actual pump response: {value}");
            return value;
        }
    }
}

pub(super) fn later_bounds_after_pump(
    run: &mut FacadeRun,
    id: u64,
    hwnd: usize,
    closes: &mut Vec<Value>,
) -> Value {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    run.send(&format!("STATE {id}"));
    let before = pump_record(run, "STATE", closes, deadline);
    let prior_bounds = before["facts"]["bounds"]
        .as_u64()
        .expect("actual Bounds dispatch count");
    native_control::move_by(hwnd, run.pid(), 100, 50);
    let moved = capture_native(hwnd, run.pid());
    let x = f64::from(moved.window.outer_rect[0]) / moved.scale;
    let y = f64::from(moved.window.outer_rect[1]) / moved.scale;
    assert!(
        (x - before["position"][0].as_f64().expect("pre-move X")).abs() > 1.0
            || (y - before["position"][1].as_f64().expect("pre-move Y")).abs() > 1.0,
        "a new native Bounds fact must differ from the old mirror beyond its tolerance"
    );
    loop {
        run.send(&format!("STATE {id}"));
        let state = pump_record(run, "STATE", closes, deadline);
        let at_target = state["position"][0]
            .as_f64()
            .is_some_and(|old| (old - x).abs() <= 1.0)
            && state["position"][1]
                .as_f64()
                .is_some_and(|old| (old - y).abs() <= 1.0);
        let dispatched = state["facts"]["bounds"]
            .as_u64()
            .expect("actual Bounds count")
            > prior_bounds;
        if at_target && dispatched && state["pending"].is_null() {
            // Same UI Sender/Core FIFO, and the later Bounds user callback has
            // run. Any preceding close listener and its Cancel have settled.
            // A missing close can now be asserted after cleanup without timeout.
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "later native Bounds/Cancel did not settle: {state}"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(super) fn held_build_or_terminal(run: &mut FacadeRun, after: usize) -> Option<(String, usize)> {
    // A lost Destroyed faults at the unchanged product releasing bound. Use the
    // existing native UI-end kill switch to observe that actual terminal; a
    // missing progress line by itself is never the mutation's proof.
    match run.try_marker_by(
        "BUILD_PUMP_HELD ",
        after,
        Instant::now() + ui_end_kill_switch(),
    ) {
        Ok(line) => Some(line),
        Err(error) => {
            if run
                .host
                .child_mut()
                .try_wait()
                .expect("native terminal state")
                .is_some()
            {
                println!("KELD_GH1188_AC25_TERMINAL_BEFORE_HELD {error}");
                None
            } else {
                panic!("INVALID: no held build and no actual native terminal: {error}");
            }
        }
    }
}
