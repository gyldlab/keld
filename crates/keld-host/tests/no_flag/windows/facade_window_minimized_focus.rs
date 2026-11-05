//! Cleanup-first AC26 minimized-focus oracle. A completed in-bound public STATE
//! is the semantic evidence; a timeout, EOF or ineligible native state is not.

use super::controller::state_until;
use super::create;
use super::fixture::{FacadeFixture, FacadeRun, assert_retired, capture, census, number};
use super::host_observation::OBSERVATION_BOUND;
use crate::PRODUCT_DEADLINE;
use crate::support::process::pipe_line_after;
use crate::support::window::{
    initial_focus,
    native_control::{self, PhysicalCoordinates},
};
use serde_json::Value;
use std::thread;
use std::time::{Duration, Instant};

/// One cursor into `FacadeRun`'s existing reader. A partial line stays in that
/// owner across the observation bound and can finish during cleanup.
#[derive(Default)]
struct CapturedRecords {
    cursor: usize,
}

impl CapturedRecords {
    fn poll(&mut self, run: &FacadeRun) -> Result<Option<(String, Value)>, String> {
        let Some((line, cursor)) = pipe_line_after(run.captured_control(), "", self.cursor) else {
            return Ok(None);
        };
        self.cursor = cursor;
        let (kind, json) = line
            .split_once(' ')
            .ok_or_else(|| format!("INVALID captured facade record: {line}"))?;
        let value = serde_json::from_str(json)
            .map_err(|error| format!("INVALID captured facade JSON: {error}; {line}"))?;
        Ok(Some((kind.to_owned(), value)))
    }

    /// Cleanup only: this bound can drain a pending response, but cannot renew
    /// the native observation deadline or replace a frozen STATE sample.
    fn cleanup_record(&mut self, run: &mut FacadeRun, expected: &str) -> Value {
        let deadline = Instant::now() + PRODUCT_DEADLINE;
        loop {
            if let Some((kind, value)) = self.poll(run).unwrap_or_else(|error| panic!("{error}")) {
                assert_eq!(
                    kind, expected,
                    "INVALID cleanup response; no focus negative-control credit: {value}"
                );
                return value;
            }
            assert!(
                run.host
                    .child_mut()
                    .try_wait()
                    .expect("observe Host")
                    .is_none(),
                "INVALID Host exit before cleanup {expected}; no focus negative-control credit"
            );
            assert!(
                Instant::now() < deadline,
                "INVALID cleanup {expected} missing; no focus negative-control credit"
            );
            native_control::pump();
            thread::park_timeout(Duration::from_millis(10));
        }
    }
}

#[derive(Debug, Default)]
struct FocusObservation {
    last: Option<Value>,
    samples: usize,
    pending: bool,
    invalid: Option<String>,
}

fn valid_state(value: &Value, created: &Value) -> bool {
    value["id"] == created["id"]
        && value["generation"] == created["generation"]
        && value
            .get("focusedId")
            .is_some_and(|focus| focus.is_null() || focus.as_u64().is_some())
}

fn observe_focus(
    run: &mut FacadeRun,
    records: &mut CapturedRecords,
    created: &Value,
    hwnd: usize,
    deadline: Instant,
) -> FocusObservation {
    let id = number(created, "id");
    let started = deadline
        .checked_sub(OBSERVATION_BOUND)
        .expect("minimize bound has a start");
    let mut observed = FocusObservation::default();
    while Instant::now() < deadline {
        if !observed.pending {
            if let Err(error) = run.try_send(&format!("STATE {id}")) {
                observed.invalid = Some(format!("STATE send failed: {error}"));
                break;
            }
            observed.pending = true;
        }
        match records.poll(run) {
            Ok(Some((kind, value))) => {
                observed.pending = false;
                if kind != "STATE" || !valid_state(&value, created) {
                    observed.invalid = Some(format!("wrong STATE identity/shape: {kind} {value}"));
                    break;
                }
                let native = native_control::read(hwnd, run.pid());
                let focus = initial_focus::read(hwnd, run.pid());
                if !native.minimized || focus.active != 0 {
                    observed.invalid = Some(format!(
                        "native regime changed: minimized={} Host-thread={focus:?}",
                        native.minimized
                    ));
                    break;
                }
                // The response and its independent native eligibility must both
                // be observed inside the original five-second minimize bound.
                if Instant::now() >= deadline {
                    break;
                }
                let satisfied = value["focusedId"].is_null();
                observed.samples += 1;
                println!(
                    "KELD_GH1188_MINIMIZED_FOCUS_SAMPLE elapsed_ms={} host_thread={focus:?} state={value}",
                    started.elapsed().as_millis()
                );
                observed.last = Some(value);
                if satisfied {
                    break;
                }
            }
            Ok(None) => {}
            Err(error) => {
                observed.invalid = Some(error);
                break;
            }
        }
        native_control::pump();
        thread::park_timeout(Duration::from_millis(10));
    }
    observed
}

#[test]
#[allow(clippy::too_many_lines)] // Keep native stimulus, frozen oracle and normal cleanup in one cell.
fn windows_minimized_native_deactivation_reaches_public_focus_before_cleanup() {
    let _physical_coordinates = PhysicalCoordinates::enter();
    let fixture = FacadeFixture::new();
    let (mut run, _) = fixture.spawn(&[]);
    let created = create(&mut run);
    let id = number(&created, "id");
    let pid = run.pid();
    let native = capture(census(pid, 1)[0], pid);
    assert!(
        !native.children.is_empty(),
        "INVALID: no real controller children"
    );
    let hwnd = native.hwnd;
    run.retain_descendants();
    let profile = fixture.profile_dir();
    println!("KELD_GH1188_MINIMIZED_FOCUS_PROFILE {}", profile.display());

    // The target must first be genuinely active. Already-unfocused input cannot
    // prove that minimizing propagates a new deactivation through the facade.
    native_control::foreground(hwnd, pid);
    state_until(&mut run, id, Instant::now() + OBSERVATION_BOUND, |state| {
        state["focusedId"] == id
    });
    assert_eq!(
        initial_focus::read(hwnd, pid).active,
        hwnd,
        "INVALID: initial Host UI thread did not activate the target"
    );
    run.capture_control();
    let mut records = CapturedRecords::default();
    // No page-readback timer or other diagnostic Host wake runs in this cell.
    // The existing full controller row separately proves hidden-page visibility.
    // These read-only native queries do not post a product message.
    let deadline = Instant::now() + OBSERVATION_BOUND;
    native_control::minimize(hwnd, pid, deadline);
    let native_input = loop {
        let native = native_control::read(hwnd, pid);
        let actual = initial_focus::read(hwnd, pid);
        if Instant::now() >= deadline {
            break Err(format!(
                "native minimized/inactive input was not established within five seconds: minimized={} Host-thread={actual:?}",
                native.minimized
            ));
        }
        if native.minimized && actual.active == 0 {
            println!("KELD_GH1188_MINIMIZED_FOCUS_INPUT host_thread={actual:?}");
            break Ok(());
        }
        native_control::pump();
        thread::park_timeout(Duration::from_millis(10));
    };
    let mut observed = match native_input {
        Ok(()) => observe_focus(&mut run, &mut records, &created, hwnd, deadline),
        Err(error) => FocusObservation {
            invalid: Some(error),
            ..FocusObservation::default()
        },
    };
    println!("KELD_GH1188_MINIMIZED_FOCUS_FROZEN {observed:?}");

    // Never replace observed.last with a later response. At most one STATE was
    // issued, so exactly that outstanding record is drained before destruction.
    if observed.pending {
        let cleanup_only = records.cleanup_record(&mut run, "STATE");
        if !valid_state(&cleanup_only, &created) {
            observed.invalid = Some(format!("invalid pending STATE: {cleanup_only}"));
        }
        println!("KELD_GH1188_MINIMIZED_FOCUS_CLEANUP_ONLY {cleanup_only}");
    }
    let final_native = capture(isize::try_from(hwnd).expect("original HWND"), pid);
    run.send(&format!("DESTROY {id}"));
    let closed = records.cleanup_record(&mut run, "CLOSED");
    let returned = records.cleanup_record(&mut run, "DESTROY_RETURN");
    let finished = records.cleanup_record(&mut run, "DESTROY_FINISHED");
    assert_eq!(closed["id"], id);
    assert_eq!(closed["count"], 1);
    assert_eq!(returned["closed"], 1);
    assert_eq!(finished["contentsDestroyed"], 1);
    assert_retired(&native);
    assert_retired(&final_native);
    assert_eq!(census(pid, 0), Vec::<isize>::new());
    run.retain_descendants();
    run.send("QUIT");
    records.cleanup_record(&mut run, "QUIT_ACCEPTED");
    run.finish_quit();
    let profile_remained = profile.exists();
    fixture.close();

    assert!(!profile_remained, "normal Quit left the owned profile");
    println!("KELD_GH1188_MINIMIZED_FOCUS_CLEANED {}", profile.display());
    assert!(
        observed.invalid.is_none(),
        "INVALID minimized-focus observation; no semantic control credit: {observed:?}"
    );
    assert!(
        observed.samples > 0,
        "INVALID zero complete in-bound STATE records; a deadline alone is not the oracle"
    );
    let frozen = observed.last.expect("nonzero actual STATE samples");
    assert!(
        frozen["focusedId"].is_null(),
        "minimized native deactivation did not reach public focus within the original five-second bound; frozen={frozen}"
    );
}
