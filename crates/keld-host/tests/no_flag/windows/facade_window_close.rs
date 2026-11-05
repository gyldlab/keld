//! AC10: one pending close, and no release guard after a veto or held-last close.
//! File rendezvous is private to the existing staged role and `ProductFixture`.

use super::fixture::{
    FacadeFixture, FacadeRun, assert_retired, capture, census, number, pre_close_tree, same_tree,
};
use super::initial::capture_native;
use super::{create, destroy};
use crate::PRODUCT_DEADLINE;
use crate::support::window::native_control::{self, PhysicalCoordinates};
use crate::support::window::{app_top_level_windows, post_close};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

struct ReleaseCloseOnDrop(PathBuf);

impl Drop for ReleaseCloseOnDrop {
    fn drop(&mut self) {
        // Error-path fixture cleanup only; the normal path writes and checks it.
        let _ = fs::write(&self.0, b"release\n");
    }
}

fn close_record(run: &mut FacadeRun, root: &Path, name: &str, deadline: Instant) -> Value {
    let path = root.join(format!("{name}.json"));
    loop {
        assert!(
            !root.join("error.json").exists(),
            "fixture hold failed: {:?}",
            fs::read_to_string(root.join("error.json")),
        );
        if path.is_file() {
            return serde_json::from_slice(&fs::read(&path).expect("owned close receipt"))
                .expect("actual close receipt JSON");
        }
        assert!(
            run.host
                .child_mut()
                .try_wait()
                .expect("observe held-close Host")
                .is_none(),
            "Host ended before fixture {name} receipt:\n{}",
            run.trace(),
        );
        assert!(
            Instant::now() < deadline,
            "missing fixture {name} receipt at its existing bound"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn settled_close_state(run: &mut FacadeRun, id: u64) -> (Value, Vec<Value>) {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    let mut closes = Vec::new();
    loop {
        run.send(&format!("STATE {id}"));
        loop {
            let (kind, value) = run
                .try_next_by(deadline)
                .unwrap_or_else(|error| panic!("close state receipt: {error}"));
            match kind.as_str() {
                "CLOSE_EVENT" => closes.push(value),
                "STATE" => {
                    if value["pending"].is_null() {
                        return (value, closes);
                    }
                    break;
                }
                _ => panic!("unexpected close receipt: {kind} {value}"),
            }
        }
        assert!(
            Instant::now() < deadline,
            "the actual Cancel acknowledgment did not settle"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

#[test]
#[allow(clippy::too_many_lines)] // Keep the held callback, FIFO acknowledgment and cleanup in one ordered scenario.
fn windows_pending_duplicate_wm_close_emits_one_close_event() {
    let _coordinates = PhysicalCoordinates::enter();
    let fixture = FacadeFixture::new();
    let (mut run, _) = fixture.spawn(&[]);
    let created = create(&mut run);
    let id = number(&created, "id");
    let host_pid = run.pid();
    let native = capture(census(run.pid(), 1)[0], run.pid());
    let browser = fixture.browser(&run);
    run.retain_descendants();
    let profile = fixture.profile_dir();
    let root = fixture.project.project.join("close-hold");
    run.send(&format!("HOLD_CLOSE {id}"));
    assert_eq!(run.read("CLOSE_HOLD_ARMED")["id"], id);
    let release = ReleaseCloseOnDrop(root.join("release"));
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    let before_close = pre_close_tree(&native, run.pid());
    println!(
        "KELD_GH1188_DUPLICATE_CLOSE_PRE_STIMULUS hwnd={} native={before_close:?}",
        native.hwnd,
    );
    post_close(native.hwnd as u64);
    let pending = close_record(&mut run, &root, "pending", deadline);
    assert_eq!(pending["id"], id);
    assert_eq!(pending["generation"], created["generation"]);
    assert_ne!(
        pending["sequence"], "null",
        "actual Host close sequence must be pending"
    );
    let (_, cursor) = run
        .try_marker_by(&format!("CLOSE_DISPATCHED {}", native.hwnd), 0, deadline)
        .expect("first native close was dispatched");
    post_close(native.hwnd as u64);
    run.try_marker_by(
        &format!("CLOSE_DISPATCHED {}", native.hwnd),
        cursor,
        deadline,
    )
    .expect("second native close was dispatched while the first callback was held");

    let after_duplicate = capture(
        isize::try_from(native.hwnd).expect("original HWND"),
        run.pid(),
    );
    let live_after_duplicate = before_close.live_handles().len();
    println!(
        "KELD_GH1188_DUPLICATE_CLOSE_PRE_MOVE hwnd={} native={after_duplicate:?}",
        native.hwnd,
    );

    // This later Bounds event uses the same UI Sender and Core event FIFO as
    // the duplicate close. Its applied mirror state acknowledges Core consumption
    // of that close while the original callback cannot yet send any verdict.
    native_control::move_by(native.hwnd, run.pid(), 100, 50);
    let moved = capture_native(native.hwnd, run.pid());
    let target = serde_json::json!({
        "x": f64::from(moved.window.outer_rect[0]) / moved.scale,
        "y": f64::from(moved.window.outer_rect[1]) / moved.scale,
    });
    assert!(
        (target["x"].as_f64().expect("actual target X")
            - created["position"][0].as_f64().expect("initial public X"))
        .abs()
            > 1.0
            || (target["y"].as_f64().expect("actual target Y")
                - created["position"][1].as_f64().expect("initial public Y"))
            .abs()
                > 1.0,
        "the new native target must be outside the held mirror matcher tolerance: old={created} new={target}",
    );
    fs::write(
        root.join("drain.json.tmp"),
        serde_json::to_vec(&target).expect("move target"),
    )
    .expect("write owned native-move target");
    fs::rename(root.join("drain.json.tmp"), root.join("drain.json"))
        .expect("publish complete owned move target");
    let observed = close_record(&mut run, &root, "observed", deadline);
    fs::write(&release.0, b"release\n").expect("release the real facade close callback");
    drop(release);
    let (state, closes) = settled_close_state(&mut run, id);
    run.send(&format!("VERIFY_VETO {id}"));
    let veto = run.read("VETO_VERIFIED");
    let after = capture(census(run.pid(), 1)[0], run.pid());
    let live_after_veto = before_close.live_handles().len();
    let mut original_children = before_close.children.clone();
    let mut pending_children = after_duplicate.children.clone();
    let mut remaining_children = after.children.clone();
    original_children.sort_unstable();
    pending_children.sort_unstable();
    remaining_children.sort_unstable();
    let browser_after = fixture.browser(&run);
    fixture.assert_lease_held();
    destroy(&mut run, id);
    assert_retired(&native);
    assert_retired(&before_close);
    assert_retired(&after_duplicate);
    assert_retired(&after);
    let trace = run.quit();
    assert_eq!(app_top_level_windows(host_pid), Vec::new());
    assert!(!profile.exists());
    fixture.close();

    // First disputed assertion: a merge mutant must reach this after normal
    // cleanup and fail the actual public close-event count, not a timeout.
    assert_eq!(
        closes.len(),
        1,
        "two WM_CLOSE messages while one decision was pending emitted duplicate close events: {closes:?}"
    );
    assert_eq!(state["close"], 1);
    assert_eq!(observed["pending"], pending["sequence"]);
    assert_eq!(observed["count"], 1);
    assert_eq!(observed["generation"], created["generation"]);
    assert_eq!(veto["reply"], "ReplyStale");
    assert_eq!(veto["pending"], Value::Null);
    assert_eq!(veto["closed"], 0);
    assert_eq!(native.hwnd, before_close.hwnd);
    assert_eq!(before_close.hwnd, after_duplicate.hwnd);
    assert_eq!(native.hwnd, after.hwnd);
    assert_eq!(before_close.outer_rect, after_duplicate.outer_rect);
    assert_eq!(live_after_duplicate, before_close.children.len() + 1);
    assert_eq!(live_after_veto, before_close.children.len() + 1);
    assert_eq!(original_children, pending_children);
    assert_eq!(original_children, remaining_children);
    assert_eq!(browser["browser_pid"], browser_after["browser_pid"]);
    println!(
        "KELD_GH1188_AC10_PENDING at_risk=1 core_ack=applied-later-bounds pending={pending} observed={observed} public={state}\n{trace}"
    );
}

fn guard_survival(run: &mut FacadeRun, native: &crate::support::window::NativeWindowSnapshot) {
    // Duration oracle: strictly outlive the same three-second D16 guard supplied
    // below while the original native browser/controller remains alive.
    thread::sleep(Duration::from_secs(4));
    assert!(
        run.host
            .child_mut()
            .try_wait()
            .expect("observe unarmed live Host")
            .is_none(),
        "a managed close incorrectly armed the release guard:\n{}",
        run.trace(),
    );
    same_tree(native, run.pid());
    assert!(!run.trace().contains("KELD-WV-009"));
}

#[test]
fn windows_vetoed_and_followup_windows_outlive_the_release_guard() {
    let fixture = FacadeFixture::new();
    let (mut run, _) = fixture.spawn(&[("KELD_TEST_WINDOWS_HANG_GUARD_SECS", "3".to_owned())]);
    let created = create(&mut run);
    let id = number(&created, "id");
    let original = capture(census(run.pid(), 1)[0], run.pid());
    run.retain_descendants();
    let profile = fixture.profile_dir();
    let before_close = pre_close_tree(&original, run.pid());
    post_close(original.hwnd as u64);
    assert_eq!(run.read("CLOSE_EVENT")["count"], 1);
    run.send(&format!("VERIFY_VETO {id}"));
    assert_eq!(run.read("VETO_VERIFIED")["reply"], "ReplyStale");
    guard_survival(&mut run, &before_close);
    run.send(&format!("ALLOW {id}"));
    run.read("ALLOW");
    post_close(original.hwnd as u64);
    assert_eq!(run.read("CLOSE_EVENT")["count"], 2);
    assert_eq!(run.read("CLOSED")["count"], 1);
    assert_retired(&original);
    assert_retired(&before_close);
    assert_eq!(app_top_level_windows(run.pid()), []);

    let successor = create(&mut run);
    let next_id = number(&successor, "id");
    assert_ne!(next_id, id);
    let native = capture(census(run.pid(), 1)[0], run.pid());
    run.retain_descendants();
    guard_survival(&mut run, &native);
    destroy(&mut run, next_id);
    assert_retired(&native);
    let trace = run.quit();
    assert!(!profile.exists());
    fixture.close();
    println!(
        "KELD_GH1188_AC10_GUARD vetoed_at_risk=1 followup_at_risk=1 guard_secs=3 observed_secs_each=4\n{trace}"
    );
}
