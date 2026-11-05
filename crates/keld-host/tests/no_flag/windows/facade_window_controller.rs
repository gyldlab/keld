//! AC26(a,b,c,f): public facade facts agree with independent Win32/page state.

use super::fixture::{FacadeFixture, FacadeRun, assert_retired, capture, census, number};
use super::host_observation::{self, OBSERVATION_BOUND, Pages, numeric, rect_size};
use super::{create, destroy};
use crate::support::window::native_control::{self, NativeState, PhysicalCoordinates};
use serde_json::Value;
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn state_until(
    run: &mut FacadeRun,
    id: u64,
    deadline: Instant,
    satisfies: impl Fn(&Value) -> bool,
) -> Value {
    loop {
        run.send(&format!("STATE {id}"));
        let (kind, value) = run
            .try_next_by(deadline)
            .unwrap_or_else(|error| panic!("bounded public state observation: {error}"));
        assert_eq!(kind, "STATE", "unexpected facade record: {kind} {value}");
        if satisfies(&value) {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "public state disagrees with the native witness: {value}"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn geometry_matches(state: &Value, native: &NativeState) -> bool {
    let rect = native.window.outer_rect;
    let size = rect_size(rect);
    (0..2).all(|axis| {
        let position = state["position"][axis].as_f64().expect("public position");
        let extent = state["size"][axis].as_f64().expect("public size");
        (position - f64::from(rect[axis]) / native.scale).abs() <= 1.0
            && (extent
                - f64::from(i32::try_from(size[axis]).expect("native Win32 extent")) / native.scale)
                .abs()
                <= 1.0
    })
}

#[test]
#[allow(clippy::too_many_lines)] // Keep ordered native stimuli, oracles and cleanup auditable together.
fn windows_host_window_obligations_reach_the_controller() {
    let _physical_coordinates = PhysicalCoordinates::enter();
    let fixture = FacadeFixture::new();
    let (mut run, ready) = fixture.spawn(&[("KELD_TEST_WINDOWS_PAGE_READBACK", "50".to_owned())]);
    assert_eq!(ready["windows"], serde_json::json!([]));
    let created = create(&mut run);
    let id = number(&created, "id");
    let native = capture(census(run.pid(), 1)[0], run.pid());
    let hwnd = native.hwnd;
    let pid = run.pid();
    let view = 1; // First native View, not a rule for the registry's public id.
    let mut pages = Pages::default();
    let initial = native_control::read(hwnd, pid);
    let size = rect_size(initial.window.outer_rect);
    for (physical, logical) in size.into_iter().zip([640.0, 480.0]) {
        assert!(
            (f64::from(i32::try_from(physical).expect("native Win32 size"))
                - logical * initial.scale)
                .abs()
                <= 1.0,
            "requested outer-frame size was treated as client size: {initial:?}"
        );
    }
    let initial_state = state_until(&mut run, id, Instant::now() + OBSERVATION_BOUND, |state| {
        state["createdState"].is_object()
    });
    let ui_focus = crate::support::window::initial_focus::read(hwnd, pid);
    if ui_focus.active == hwnd {
        println!("KELD_GH1188_AC13 initial_active_at_risk=1 initial_inactive_at_risk=0");
        assert_eq!(
            initial_state["createdState"]["focused"], true,
            "Created snapshot used top-level keyboard focus instead of activation"
        );
        assert!(
            ui_focus.focus_is_child,
            "initial native keyboard focus did not enter the WebView"
        );
        assert_eq!(
            initial_state["facts"]["focusFalse"], 0,
            "transient blur escaped the builder"
        );
        let focus_events = number(&initial_state["facts"], "focusTrue") + 1;
        assert_eq!(
            focus_events, 1,
            "Created(true) plus later focus facts duplicated initial focus"
        );
    } else {
        println!("KELD_GH1188_AC13 initial_active_at_risk=0 initial_inactive_at_risk=1");
        assert_eq!(
            initial_state["createdState"]["focused"], false,
            "inactive Created snapshot falsely reported focused"
        );
    }
    run.retain_descendants();
    let profile = fixture.profile_dir();

    // The move oracle comes before any activation, minimize or restore, so those
    // later stimuli cannot accidentally refresh a missing parent notification.
    let moved = host_observation::moved(run.stderr(), &mut pages, view, hwnd, pid);
    let before_minimize = state_until(&mut run, id, moved.deadline, |state| {
        geometry_matches(state, &moved.native)
    });
    let (_, deadline) = host_observation::minimized(run.stderr(), &mut pages, view, hwnd, pid);
    let minimized = state_until(&mut run, id, deadline, |state| {
        state["position"] == before_minimize["position"]
            && state["size"] == before_minimize["size"]
            && state["maximized"] == before_minimize["maximized"]
            && state["focusedId"].is_null()
    });
    assert_eq!(
        minimized["facts"]["bounds"], before_minimize["facts"]["bounds"],
        "minimized native placement leaked into canonical bounds"
    );
    let (_, deadline) = host_observation::restored(
        run.stderr(),
        &mut pages,
        view,
        hwnd,
        pid,
        moved.native.window.outer_rect,
    );
    state_until(&mut run, id, deadline, |state| {
        state["focusedId"] == id && geometry_matches(state, &moved.native)
    });

    run.send(&format!("MAXIMIZE {id}"));
    run.read("MAXIMIZE_SENT");
    let maximized = state_until(&mut run, id, Instant::now() + OBSERVATION_BOUND, |state| {
        state["maximized"] == true
    });
    let maximized_native = native_control::read(hwnd, pid);
    assert!(
        maximized_native.maximized,
        "public maximize did not affect the native window"
    );
    let (_, deadline) = host_observation::minimized(run.stderr(), &mut pages, view, hwnd, pid);
    let minimized_max = state_until(&mut run, id, deadline, |state| {
        state["maximized"] == true
            && state["size"] == maximized["size"]
            && state["position"] == maximized["position"]
            && state["focusedId"].is_null()
    });
    assert_eq!(
        minimized_max["facts"]["maximized"], maximized["facts"]["maximized"],
        "minimize emitted a transient maximized fact"
    );
    let (_, deadline) = host_observation::restored(
        run.stderr(),
        &mut pages,
        view,
        hwnd,
        pid,
        maximized_native.window.outer_rect,
    );
    let restored_max = state_until(&mut run, id, deadline, |state| {
        state["maximized"] == true && state["focusedId"] == id
    });
    assert_eq!(
        restored_max["facts"]["maximized"], maximized["facts"]["maximized"],
        "maximized restore sampled the Moved event before Tao restored its state"
    );
    println!("KELD_GH1188_AC26_MAXIMIZED_RESTORE at_risk=1");
    host_observation::focus_cycle(run.stderr(), &mut pages, view, hwnd, pid);
    state_until(&mut run, id, Instant::now() + OBSERVATION_BOUND, |state| {
        state["focusedId"] == id
    });

    // SW_RESTORE on the maximized, non-minimized owned window restores its normal
    // frame, then the independent no-move resize exercises the existing fitter.
    native_control::restore(hwnd, pid);
    state_until(&mut run, id, Instant::now() + OBSERVATION_BOUND, |state| {
        state["maximized"] == false
    });
    let resized = host_observation::resized(run.stderr(), &mut pages, view, hwnd, pid);
    state_until(&mut run, id, resized.deadline, |state| {
        geometry_matches(state, &resized.native)
    });

    run.send(&format!("FULLSCREEN {id} true"));
    run.read("FULLSCREEN_SENT");
    state_until(&mut run, id, Instant::now() + OBSERVATION_BOUND, |state| {
        state["fullScreen"] == true
    });
    run.send(&format!("FULLSCREEN {id} false"));
    run.read("FULLSCREEN_SENT");
    state_until(&mut run, id, Instant::now() + OBSERVATION_BOUND, |state| {
        state["fullScreen"] == false
    });
    let page = pages.wait(
        run.stderr(),
        view,
        Instant::now() + OBSERVATION_BOUND,
        |page| page["visibilityState"] == "visible",
    );
    assert!(numeric(&page, "innerWidth") > 0.0);
    run.retain_descendants();
    let final_native = capture(isize::try_from(hwnd).expect("HWND"), pid);
    destroy(&mut run, id);
    assert_retired(&native);
    assert_retired(&final_native);
    run.quit();
    assert!(!profile.exists());
    fixture.close();
}
