//! GH-1188 T3: real public facade calls plus independent Win32/WebView2 witnesses.
//! Run only under the Ramani native lease, with --test-threads=1 and Host features
//! profile-test-hooks,kel135-profile-test-root. No environment admits managed windows.

#![cfg(all(feature = "profile-test-hooks", debug_assertions))]

#[path = "facade_window_baseline.rs"]
mod baseline;
#[path = "facade_window_close.rs"]
mod close;
#[path = "facade_window_controller.rs"]
mod controller;
#[path = "facade_window_faults.rs"]
mod faults;
#[path = "support/facade_window_fixture.rs"]
pub(crate) mod fixture;
#[path = "facade_window_focus.rs"]
mod focus;
#[path = "support/facade_window_observation.rs"]
mod host_observation;
#[path = "facade_window_initial.rs"]
mod initial;
#[path = "facade_window_legacy.rs"]
mod legacy;
#[path = "facade_window_lifecycle.rs"]
mod lifecycle;
#[path = "facade_window_minimized_focus.rs"]
mod minimized_focus;
#[path = "facade_window_pump.rs"]
mod pump;

use crate::support::process::{assert_process_signaled, open_process_for_wait};
use crate::support::window::{app_top_level_windows, post_close};
use fixture::{
    FacadeFixture, FacadeRun, assert_retired, capture, census, number, ordered, pre_close_tree,
    same_tree,
};
use serde_json::Value;
use std::thread;
use std::time::Duration;

fn create(run: &mut FacadeRun) -> Value {
    run.send("CREATE");
    let created = run.read("PUBLIC_CREATED");
    println!("KELD_GH1188_CREATE {created}");
    created
}

fn destroy(run: &mut FacadeRun, id: u64) -> (Value, Value, Value) {
    run.send(&format!("DESTROY {id}"));
    (
        run.read("CLOSED"),
        run.read("DESTROY_RETURN"),
        run.read("DESTROY_FINISHED"),
    )
}

#[test]
fn windows_ready_zero_then_first_browser_window_has_a_native_controller() {
    let fixture = FacadeFixture::new();
    let (run, ready) = fixture.spawn(&[]);
    assert_eq!(ready["readyEvents"], 1);
    assert_eq!(ready["windows"], serde_json::json!([]));
    assert_eq!(app_top_level_windows(run.pid()), []);

    let (mut run, created) = baseline::create_or_retire_unsupported(run, &fixture);
    assert_eq!(created["id"], 1);
    assert_eq!(created["size"], serde_json::json!([640, 480]));
    assert_eq!(
        created["creation"],
        serde_json::json!(["contents", "window"])
    );
    assert_eq!(created["liveCount"], 1);
    assert_eq!(created["hostDestroyed"], false);
    let generation = created["generation"]
        .as_str()
        .expect("Host generation")
        .parse::<u64>()
        .expect("u64 Host generation");
    assert!(generation > 0, "registry generation must be Host-minted");
    let native = capture(census(run.pid(), 1)[0], run.pid());
    let initial = initial::capture_native(native.hwnd, run.pid());
    assert!(
        !native.children.is_empty(),
        "a top-level HWND alone is not a WebView2 controller"
    );
    let browser = fixture.browser(&run);
    assert!(number(&browser, "browser_pid") > 0);
    fixture.assert_lease_held();
    run.retain_descendants();
    let profile = fixture.profile_dir();

    let (closed, returned, finished) = destroy(&mut run, 1);
    assert_eq!(closed["count"], 1);
    assert_eq!(returned["closed"], 1);
    assert_eq!(finished["contentsDestroyed"], 1);
    assert_retired(&native);
    assert_eq!(app_top_level_windows(run.pid()), []);
    let trace = run.quit();
    ordered(
        &trace,
        &[
            format!("DESTROYED {}", native.hwnd),
            format!("TEARDOWN_CONFIRMED {}", native.hwnd),
            "RELEASED ".to_owned(),
        ],
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("RELEASED "))
            .count(),
        1
    );
    assert!(
        !profile.exists(),
        "successful native retirement left the owned ephemeral profile"
    );
    fixture.close();
    initial::assert_created_geometry(&created, &initial, Some(37.0), Some(91.0));
}

#[test]
#[allow(clippy::too_many_lines)] // Keep ordered native stimuli, oracles and cleanup auditable together.
fn windows_electron_browser_window_uses_native_host_and_vetoes_wm_close() {
    let fixture = FacadeFixture::new();
    let (mut run, ready) = fixture.spawn(&[("KELD_TEST_WINDOWS_HANG_GUARD_SECS", "3".to_owned())]);
    assert_eq!(ready["windows"], serde_json::json!([]));
    let first = create(&mut run);
    let first_id = number(&first, "id");
    let first_native = capture(census(run.pid(), 1)[0], run.pid());
    let browser_before = fixture.browser(&run);
    let second = create(&mut run);
    let second_id = number(&second, "id");
    assert_ne!(
        first_id, second_id,
        "two facades must not share a canonical registry identity"
    );
    let generation = |value: &Value| {
        value["generation"]
            .as_str()
            .expect("generation")
            .parse::<u64>()
            .expect("generation u64")
    };
    assert!(
        generation(&second) > generation(&first),
        "the second identity is a fresh incarnation"
    );
    let second_hwnd = census(run.pid(), 2)
        .into_iter()
        .find(|hwnd| usize::try_from(*hwnd).expect("HWND") != first_native.hwnd)
        .expect("second independent native HWND");
    let second_native = capture(second_hwnd, run.pid());
    assert_ne!(second_native.children, Vec::<usize>::new());
    assert_eq!(
        fixture.browser(&run)["browser_pid"],
        browser_before["browser_pid"]
    );
    let webview_dirs = std::fs::read_dir(fixture.profile_dir())
        .expect("profile contents")
        .map(|entry| entry.expect("profile entry").file_name())
        .filter(|name| name.to_string_lossy().starts_with("webview2"))
        .count();
    assert_eq!(
        webview_dirs, 1,
        "a session must share one WebView2 user-data folder"
    );
    fixture.assert_lease_held();
    run.retain_descendants();
    let profile = fixture.profile_dir();

    // AC7 and AC10 NC2 duration oracles: neither the legacy navigation deadline nor
    // an incorrectly armed shortened release guard may retire either managed view.
    thread::sleep(keld_wv::INITIAL_MILESTONE_DEADLINE + Duration::from_secs(1));
    // WebView2 may add hosting children during startup. Preserve every original
    // HWND through the duration oracle, then freeze the complete current tree
    // immediately before WM_CLOSE. Close cancellation still requires exact sets.
    let [first_native, second_native] =
        [&first_native, &second_native].map(|before| pre_close_tree(before, run.pid()));
    assert!(
        run.host
            .child_mut()
            .try_wait()
            .expect("live managed Host")
            .is_none()
    );

    post_close(u64::try_from(second_hwnd).expect("second HWND"));
    let close = run.read("CLOSE_EVENT");
    assert_eq!(
        close["id"], second_id,
        "native close must route to its own view/generation"
    );
    assert_eq!(close["count"], 1);
    run.send(&format!("VERIFY_VETO {second_id}"));
    let veto = run.read("VETO_VERIFIED");
    assert_eq!(
        veto["reply"], "ReplyStale",
        "stale Allow cannot override Cancel"
    );
    assert_eq!(veto["live"], true);
    assert_eq!(veto["contentsLive"], true);
    assert_eq!(veto["closed"], 0);
    assert_eq!(veto["pending"], Value::Null);
    assert_eq!(veto["dispatched"], 0);
    same_tree(&second_native, run.pid());
    same_tree(&first_native, run.pid());
    assert_eq!(
        fixture.browser(&run)["browser_pid"],
        browser_before["browser_pid"]
    );
    fixture.assert_lease_held();
    run.send(&format!("STATE {first_id}"));
    assert_eq!(
        run.read("STATE")["close"],
        0,
        "close leaked into the other facade"
    );

    run.send(&format!("ALLOW {second_id}"));
    run.read("ALLOW");
    post_close(u64::try_from(second_hwnd).expect("second HWND"));
    assert_eq!(run.read("CLOSE_EVENT")["count"], 2);
    let closed = run.read("CLOSED");
    assert_eq!(closed["id"], second_id);
    assert_eq!(closed["count"], 1);
    assert_eq!(closed["destroyed"], true);
    assert_eq!(
        closed["contentsDestroyed"], true,
        "accepted-close contents retire before closed"
    );
    assert_retired(&second_native);
    same_tree(&first_native, run.pid());
    assert_eq!(
        census(run.pid(), 1),
        vec![isize::try_from(first_native.hwnd).expect("first HWND")]
    );
    assert_eq!(
        fixture.browser(&run)["browser_pid"],
        browser_before["browser_pid"]
    );
    run.send(&format!("REPEAT {second_id}"));
    let repeated = run.read("REPEATED");
    assert_eq!(repeated["closed"], 1);
    assert_eq!(repeated["contentsDestroyed"], 1);
    assert_eq!(repeated["retired"], "KELD-CORE-038");
    assert_eq!(repeated["sizeError"], "KELD-COMPAT-011");
    let (_, _, finished) = destroy(&mut run, first_id);
    assert_eq!(finished["liveCount"], 0);
    assert_retired(&first_native);
    let trace = run.quit();
    ordered(
        &trace,
        &[
            format!("DESTROYED {}", second_native.hwnd),
            format!("TEARDOWN_CONFIRMED {}", second_native.hwnd),
            "RELEASED ".to_owned(),
        ],
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("RELEASED "))
            .count(),
        2
    );
    assert!(
        trace.contains("environments=1"),
        "two windows constructed a second environment:\n{trace}"
    );
    assert!(
        !profile.exists(),
        "profile survived successful two-window shutdown"
    );
    fixture.close();
}

#[test]
fn windows_destroy_is_synchronous_once_and_stale_pairs_cannot_destroy_a_successor() {
    let fixture = FacadeFixture::new();
    let (mut run, _) = fixture.spawn(&[]);
    let first = create(&mut run);
    let first_id = number(&first, "id");
    let first_native = capture(census(run.pid(), 1)[0], run.pid());
    run.retain_descendants();
    let (closed, returned, finished) = destroy(&mut run, first_id);
    assert_retired(&first_native);
    assert_eq!(
        closed["contentsDestroyed"], false,
        "explicit destroy preserves contents during closed"
    );
    assert_eq!(
        returned["order"],
        serde_json::json!([
            format!("closed:{first_id}"),
            "all".to_owned(),
            format!("return:{first_id}"),
        ])
    );
    assert_eq!(
        returned["contentsDestroyed"], 0,
        "contents event must follow synchronous return"
    );
    assert_eq!(finished["contentsDestroyed"], 1);
    assert_eq!(finished["close"], 0, "destroy must bypass cancelable close");
    assert_eq!(finished["closed"], 1);

    let second = create(&mut run);
    let second_id = number(&second, "id");
    assert!(second_id > first_id, "Host logical IDs cannot be recycled");
    assert_ne!(second["generation"], first["generation"]);
    let second_native = capture(census(run.pid(), 1)[0], run.pid());
    run.retain_descendants();
    run.send(&format!("REPEAT {first_id}"));
    let repeated = run.read("REPEATED");
    assert_eq!(repeated["retired"], "KELD-CORE-038");
    assert_eq!(repeated["closed"], 1);
    assert_eq!(repeated["contentsDestroyed"], 1);
    run.send(&format!("STALE {first_id} {second_id}"));
    let stale = run.read("STALE");
    assert_eq!(stale["forged"], "KELD-CORE-038");
    assert_eq!(stale["future"], "KELD-CORE-038");
    assert_eq!(stale["live"], true);
    assert_eq!(stale["closed"], 0);
    same_tree(&second_native, run.pid());
    let profile = fixture.profile_dir();
    destroy(&mut run, second_id);
    assert_retired(&second_native);
    let trace = run.quit();
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("RELEASED "))
            .count(),
        2
    );
    assert!(!profile.exists());
    fixture.close();
}

#[test]
fn windows_facade_recovery_keeps_the_live_hwnd_and_does_not_replay_an_accepted_destroy() {
    let fixture = FacadeFixture::new();
    let (mut run, _) = fixture.spawn(&[]);
    let first = create(&mut run);
    let first_id = number(&first, "id");
    let first_native = capture(census(run.pid(), 1)[0], run.pid());
    let second = create(&mut run);
    let second_id = number(&second, "id");
    let second_hwnd = census(run.pid(), 2)
        .into_iter()
        .find(|hwnd| usize::try_from(*hwnd).expect("HWND") != first_native.hwnd)
        .expect("surviving second HWND");
    let second_native = capture(second_hwnd, run.pid());
    run.retain_descendants();
    destroy(&mut run, first_id);
    assert_retired(&first_native);
    // The surviving view was captured before its sibling was destroyed.
    // Retain every original HWND; log the first boundary before reading the browser.
    println!("KELD_GH1188_RECOVERY_CHILDREN phase=after-sibling-destroy");
    let after_sibling = pre_close_tree(&second_native, run.pid());
    let browser = fixture.browser(&run);
    let old_bun = run.bun_pid;
    let old_link = run.link.clone();
    let retired_bun = open_process_for_wait(old_bun, false);
    println!("KELD_GH1188_RECOVERY_CHILDREN phase=before-crash");
    let before_crash = pre_close_tree(&after_sibling, run.pid());
    run.send("CRASH");
    assert_eq!(run.read("CRASHING")["pid"], old_bun);
    let ready = fixture.reconnect(&mut run);
    println!("KELD_GH1188_RECOVERY_CHILDREN phase=after-reconnect");
    same_tree(&before_crash, run.pid());
    assert_ne!(run.bun_pid, old_bun);
    assert_ne!(
        run.link, old_link,
        "recovery must mint fresh process-generation authority"
    );
    assert_process_signaled(&retired_bun, "retired facade Bun generation");
    let replay = ready["windows"].as_array().expect("canonical Ready replay");
    assert_eq!(
        replay.len(),
        1,
        "accepted destroy resurrected or surviving window disappeared"
    );
    assert_eq!(replay[0]["id"], second_id);
    assert_eq!(replay[0]["generation"], second["generation"]);
    assert_eq!(replay[0]["closed"], 0);
    assert_eq!(census(run.pid(), 1), vec![second_hwnd]);
    assert_eq!(fixture.browser(&run)["browser_pid"], browser["browser_pid"]);
    fixture.assert_lease_held();
    run.retain_descendants();
    println!("KELD_GH1188_RECOVERY_CHILDREN phase=before-veto");
    let before_close = pre_close_tree(&before_crash, run.pid());
    post_close(u64::try_from(second_hwnd).expect("surviving HWND"));
    assert_eq!(run.read("CLOSE_EVENT")["id"], second_id);
    run.send(&format!("VERIFY_VETO {second_id}"));
    assert_eq!(run.read("VETO_VERIFIED")["reply"], "ReplyStale");
    println!("KELD_GH1188_RECOVERY_CHILDREN phase=after-veto");
    same_tree(&before_close, run.pid());
    let profile = fixture.profile_dir();
    destroy(&mut run, second_id);
    assert_retired(&second_native);
    assert_retired(&before_close);
    let trace = run.quit();
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("RELEASED "))
            .count(),
        2,
        "accepted destructive work was replayed across Bun recovery:\n{trace}"
    );
    assert!(
        !profile.exists(),
        "recovered generation left the profile leased"
    );
    fixture.close();
}
