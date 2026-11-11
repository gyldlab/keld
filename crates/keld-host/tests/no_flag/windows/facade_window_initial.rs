//! AC6 initial facts: the immutable Created snapshot agrees with the real outer
//! HWND frame, including construction with only one supplied position axis.

use super::destroy;
use super::fixture::{FacadeFixture, assert_retired, capture, census, number};
use crate::support::window::native_control::{self, NativeState, PhysicalCoordinates};
use serde_json::Value;

pub(super) fn capture_native(hwnd: usize, pid: u32) -> NativeState {
    let _physical_coordinates = PhysicalCoordinates::enter();
    native_control::read(hwnd, pid)
}

pub(super) fn assert_created_geometry(
    created: &Value,
    native: &NativeState,
    requested_x: Option<f64>,
    requested_y: Option<f64>,
) {
    let state = &created["createdState"];
    assert!(
        state.is_object(),
        "the constructor must retain its original Created fact: {created}"
    );
    let bounds = &state["bounds"];
    let rect = native.window.outer_rect;
    let physical = [
        f64::from(rect[0]),
        f64::from(rect[1]),
        f64::from(rect[2]) - f64::from(rect[0]),
        f64::from(rect[3]) - f64::from(rect[1]),
    ];
    let fact = ["x", "y", "width", "height"].map(|field| {
        bounds[field]
            .as_f64()
            .expect("numeric initial Created bound")
    });
    let edges = [fact[0], fact[1], fact[0] + fact[2], fact[1] + fact[3]];
    for (edge, (fact, witness)) in edges.into_iter().zip(rect).enumerate() {
        assert!(
            (fact * native.scale - f64::from(witness)).abs() <= 1.0,
            "initial Created edge {edge} disagrees with the independent native outer frame: fact={state} native={native:?}",
        );
    }
    for (witness, requested) in physical[2..].iter().zip([640.0, 480.0]) {
        assert!(
            (*witness - requested * native.scale).abs() <= 1.0,
            "requested outer-frame dimensions became client dimensions: fact={state} native={native:?}",
        );
    }
    for (axis, requested) in [requested_x, requested_y].into_iter().enumerate() {
        if let Some(requested) = requested {
            assert!(
                (physical[axis] - requested * native.scale).abs() <= 1.0,
                "supplied position axis {axis} was not applied at the native scale: requested={requested} native={native:?}",
            );
        }
    }
    assert_eq!(
        created["position"],
        serde_json::json!([bounds["x"], bounds["y"]])
    );
    assert_eq!(
        created["size"],
        serde_json::json!([bounds["width"], bounds["height"]])
    );
    println!(
        "KELD_GH1188_AC6_INITIAL hwnd={} dpi={} scale={} requested_x={requested_x:?} requested_y={requested_y:?} created={state} outer={:?}",
        native.window.hwnd, native.dpi, native.scale, native.window.outer_rect,
    );
}

#[test]
fn windows_initial_created_bounds_match_native_outer_frame_for_each_supplied_axis() {
    let fixture = FacadeFixture::new();
    let (mut run, ready) = fixture.spawn(&[]);
    assert_eq!(ready["windows"], serde_json::json!([]));
    let profile = fixture.profile_dir();
    let mut observations = Vec::new();
    for (command, x, y) in [
        ("CREATE_X", Some(37.0), None),
        ("CREATE_Y", None, Some(91.0)),
    ] {
        run.send(command);
        let created = run.read("PUBLIC_CREATED");
        let native = capture(census(run.pid(), 1)[0], run.pid());
        let actual = capture_native(native.hwnd, run.pid());
        run.retain_descendants();
        destroy(&mut run, number(&created, "id"));
        assert_retired(&native);
        observations.push((created, actual, x, y));
    }
    run.quit();
    assert!(
        !profile.exists(),
        "initial-geometry fixture profile survived normal Quit"
    );
    fixture.close();
    // Check the disputed facts after normal retirement, so a geometry mutant
    // cannot turn emergency guard cleanup into its apparent failure evidence.
    for (created, native, x, y) in observations {
        assert_created_geometry(&created, &native, x, y);
    }
}
