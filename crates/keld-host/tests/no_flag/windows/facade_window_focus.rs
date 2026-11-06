//! AC13: initial foreground-child and inactive snapshots, with independent system
//! foreground and Host-thread activation plus the approved controller-pump hold.

use super::controller::state_until;
use super::destroy;
use super::fixture::{
    FacadeFixture, FacadeRun, NATIVE_HOLD, assert_retired, capture, census, marker_number, number,
};
use super::host_observation::OBSERVATION_BOUND;
use super::initial::capture_native;
use crate::support::window::initial_focus::{self, ThreadFocus};
use crate::support::window::native_control::{self, PhysicalCoordinates};
use crate::support::window::{NativeWindowSnapshot, app_top_level_windows};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

fn held_build(
    run: &mut FacadeRun,
    existing: &[usize],
    cursor: usize,
) -> (u64, usize, NativeWindowSnapshot) {
    run.send("CREATE");
    let (marker, cursor) = run.marker("BUILD_PUMP_HELD ", cursor);
    let build = marker_number(&marker);
    let added: Vec<_> = census(run.pid(), existing.len() + 1)
        .into_iter()
        .filter(|hwnd| !existing.contains(&usize::try_from(*hwnd).expect("HWND")))
        .collect();
    assert_eq!(added.len(), 1, "exactly one new held native HWND");
    let native = capture(added[0], run.pid());
    assert!(
        app_top_level_windows(run.pid())
            .iter()
            .any(|window| window.hwnd == added[0] && !window.visible),
        "the approved hold must precede the first product show",
    );
    (build, cursor, native)
}

fn release_build(run: &mut FacadeRun, hold: &Path, build: u64, cursor: usize) -> (Value, usize) {
    fs::write(hold.join(format!("{build}.release")), b"release\n")
        .expect("release exact native build");
    let (_, cursor) = run.marker(&format!("BUILD_PUMP_EXIT {build}"), cursor);
    let created = run.read("PUBLIC_CREATED");
    println!("KELD_GH1188_CREATE {created}");
    (created, cursor)
}

fn activate_anchor(run: &mut FacadeRun, anchor: usize) -> bool {
    native_control::foreground(anchor, run.pid());
    let deadline = Instant::now() + OBSERVATION_BOUND;
    loop {
        let focus = initial_focus::read(anchor, run.pid());
        let foreground = native_control::read(anchor, run.pid()).foreground;
        if foreground == anchor && focus.active == anchor && focus.focus_is_child {
            return true;
        }
        if Instant::now() >= deadline {
            println!(
                "KELD_GH1188_AC13_VOID reason=anchor-activation-refused foreground={foreground} actual={focus:?}"
            );
            return false;
        }
        native_control::pump();
        thread::park_timeout(Duration::from_millis(10));
    }
}

struct InitialTrial {
    active: bool,
    inactive: bool,
    active_created: Value,
    active_settled: Value,
    active_native: ThreadFocus,
    active_foreground: usize,
    inactive_created: Value,
    inactive_before: ThreadFocus,
    inactive_foreground_before: usize,
    inactive_after: ThreadFocus,
    inactive_foreground_after: usize,
}

#[allow(clippy::too_many_lines)] // Keep the native preconditions, snapshot interval and normal cleanup ordered.
fn initial_trial() -> InitialTrial {
    let _coordinates = PhysicalCoordinates::enter();
    let fixture = FacadeFixture::new();
    let hold = fixture.project.root.path().join("initial-focus-build-hold");
    fs::create_dir(&hold).expect("owned initial-focus build hold");
    let (mut run, _) = fixture.spawn(&[(NATIVE_HOLD, format!("build-pump:{}", hold.display()))]);
    let (build, cursor, anchor) = held_build(&mut run, &[], 0);
    let (anchor_created, cursor) = release_build(&mut run, &hold, build, cursor);
    let anchor_id = number(&anchor_created, "id");
    run.retain_descendants();
    let profile = fixture.profile_dir();
    let anchor_was_active = activate_anchor(&mut run, anchor.hwnd);

    let (build, cursor, active) = held_build(&mut run, &[anchor.hwnd], cursor);
    let (active_created, cursor) = release_build(&mut run, &hold, build, cursor);
    // No test activation occurs between release and this native observation.
    let active_native = initial_focus::read(active.hwnd, run.pid());
    let active_foreground = native_control::read(active.hwnd, run.pid()).foreground;
    let active_at_risk = anchor_was_active
        && active_foreground == active.hwnd
        && active_native.active == active.hwnd
        && active_native.focus_is_child;
    let active_id = number(&active_created, "id");
    // A subsequent move is only an ordered drain for the initial Focus facts.
    // It cannot repair the immutable Created snapshot captured above.
    native_control::move_by(active.hwnd, run.pid(), 100, 50);
    let moved = capture_native(active.hwnd, run.pid());
    let x = f64::from(moved.window.outer_rect[0]) / moved.scale;
    let y = f64::from(moved.window.outer_rect[1]) / moved.scale;
    let active_settled = state_until(
        &mut run,
        active_id,
        Instant::now() + OBSERVATION_BOUND,
        |state| {
            (state["position"][0].as_f64().expect("public X") - x).abs() <= 1.0
                && (state["position"][1].as_f64().expect("public Y") - y).abs() <= 1.0
        },
    );
    destroy(&mut run, active_id);
    assert_retired(&active);
    let anchor_was_active = activate_anchor(&mut run, anchor.hwnd);

    let inactive_build = build.checked_add(1).expect("next exact build identity");
    fs::write(
        hold.join(format!("{inactive_build}.initially-inactive")),
        b"inactive\n",
    )
    .expect("request one native first-show stimulus");
    let (build, cursor, inactive) = held_build(&mut run, &[anchor.hwnd], cursor);
    assert_eq!(build, inactive_build, "stimulus belongs to this build only");
    let inactive_before = initial_focus::read(inactive.hwnd, run.pid());
    let inactive_foreground_before = native_control::read(inactive.hwnd, run.pid()).foreground;
    let (inactive_created, _) = release_build(&mut run, &hold, build, cursor);
    // The fixture has not activated any window since releasing this build.
    // Observe both the Host UI queue and independent system foreground ownership.
    let inactive_after = initial_focus::read(inactive.hwnd, run.pid());
    let inactive_foreground_after = native_control::read(inactive.hwnd, run.pid()).foreground;
    let shown = app_top_level_windows(run.pid()).iter().any(|window| {
        usize::try_from(window.hwnd).expect("HWND") == inactive.hwnd && window.visible
    });
    let inactive_at_risk = anchor_was_active
        && shown
        && inactive_foreground_before == anchor.hwnd
        && inactive_foreground_after == anchor.hwnd
        && inactive_before.active == anchor.hwnd
        && inactive_after.active == anchor.hwnd;
    run.retain_descendants();
    destroy(&mut run, number(&inactive_created, "id"));
    assert_retired(&inactive);
    destroy(&mut run, anchor_id);
    assert_retired(&anchor);
    run.quit();
    assert!(!profile.exists());
    fixture.close();
    InitialTrial {
        active: active_at_risk,
        inactive: inactive_at_risk,
        active_created,
        active_settled,
        active_native,
        active_foreground,
        inactive_created,
        inactive_before,
        inactive_foreground_before,
        inactive_after,
        inactive_foreground_after,
    }
}

#[test]
fn windows_initial_focus_snapshots_cover_active_child_and_inactive_windows() {
    let mut active = 0;
    let mut inactive = 0;
    for attempt in 1..=3 {
        let trial = initial_trial();
        println!(
            "KELD_GH1188_AC13_INITIAL attempt={attempt} active_at_risk={} inactive_at_risk={} active_thread={} active_focus={} active_foreground={} active_native={:?} inactive_foreground_before={} inactive_before={:?} inactive_foreground_after={} inactive_after={:?}",
            u8::from(trial.active),
            u8::from(trial.inactive),
            trial.active_native.thread,
            trial.active_native.focus,
            trial.active_foreground,
            trial.active_native,
            trial.inactive_foreground_before,
            trial.inactive_before,
            trial.inactive_foreground_after,
            trial.inactive_after,
        );
        // Every trial has completed native/process/profile cleanup first.
        if trial.active {
            active += 1;
            assert_eq!(
                trial.active_created["createdState"]["focused"], true,
                "initial Created disagreed with independent foreground and Host-thread child focus",
            );
            assert_eq!(
                trial.active_settled["facts"]["focusFalse"], 0,
                "a raw top-level Focused(false) leaked while its controller child was focused"
            );
            assert_eq!(
                trial.active_settled["facts"]["focusTrue"], 0,
                "Created(true) plus a later native focus fact duplicated initial focus"
            );
        } else {
            println!("KELD_GH1188_AC13_VOID reason=no-independent-active-child-initial-state");
        }
        if trial.inactive {
            inactive += 1;
            assert_eq!(
                trial.inactive_created["createdState"]["focused"], false,
                "an initially inactive owned HWND published a focused Created snapshot",
            );
        } else {
            println!(
                "KELD_GH1188_AC13_VOID reason=noactivate-or-anchor-precondition-did-not-hold-across-build"
            );
        }
        if active > 0 && inactive > 0 {
            break;
        }
    }
    println!("KELD_GH1188_AC13_INITIAL_TOTAL active_at_risk={active} inactive_at_risk={inactive}");
    assert!(
        active > 0 && inactive > 0,
        "both independent initial focus regimes require nonzero device counts; active={active} inactive={inactive}"
    );
}
