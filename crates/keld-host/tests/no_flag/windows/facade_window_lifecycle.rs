//! AC14: default and draw.io-style last-window Quit, release proof and relaunch.
//! Both variants reuse the staged facade role and its original process owner.

use super::create;
use super::fixture::{
    FacadeFixture, LastWindowAction, assert_retired, capture, census, number, ordered,
};
use crate::facade_ready::stage_boot;
use crate::support::product_cycle::ProductEvidence;
use crate::support::window::{app_top_level_windows, post_close};
use crate::ui_end_kill_switch;
use serde_json::Value;
use std::time::Instant;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Receipt {
    Observed,
    Withheld,
}

fn records(control: &str, kind: &str) -> Vec<Value> {
    let prefix = format!("{kind} ");
    control
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix))
        .map(|json| serde_json::from_str(json).expect("actual lifecycle receipt JSON"))
        .collect()
}

fn assert_fresh(first: &ProductEvidence, second: &ProductEvidence) {
    assert_ne!(
        first.host_pid, second.host_pid,
        "relaunch reused the Host process"
    );
    assert_ne!(
        first.bun_pid, second.bun_pid,
        "relaunch reused the Bun process"
    );
    assert_ne!(
        first.app_link, second.app_link,
        "relaunch reused process-generation authority"
    );
}

fn quit_and_relaunch(action: LastWindowAction) {
    let mut fixture = FacadeFixture::with_last_window_action(action);
    let first = last_window_cycle(&fixture, action, Receipt::Observed);
    // As run_product_cycle does, launch a fresh canonical stage of the same project.
    // The first run has already proved native retirement and profile deletion.
    fixture.stage = stage_boot(&fixture.project.project);
    let second = last_window_cycle(&fixture, action, Receipt::Observed);
    assert_fresh(&first, &second);
    fixture.close();
}

#[test]
fn windows_default_last_window_quit_releases_every_resource_and_relaunches() {
    quit_and_relaunch(LastWindowAction::DefaultQuit);
}

#[test]
fn windows_drawio_last_window_quit_releases_every_resource_and_relaunches() {
    quit_and_relaunch(LastWindowAction::DrawIoQuit);
}

#[test]
fn windows_last_window_quit_with_a_missing_receipt_fails_without_profile_credit() {
    for action in [LastWindowAction::DefaultQuit, LastWindowAction::DrawIoQuit] {
        let fault_fixture = FacadeFixture::with_last_window_action(action);
        let failed = last_window_cycle(&fault_fixture, action, Receipt::Withheld);
        // The fault profile was observed retained. This is explicit harness cleanup,
        // not a product release or a claim of same-profile persistent recovery.
        fault_fixture.close();
        let healthy_fixture = FacadeFixture::with_last_window_action(action);
        let recovered = last_window_cycle(&healthy_fixture, action, Receipt::Observed);
        assert_fresh(&failed, &recovered);
        healthy_fixture.close();
    }
}

#[allow(clippy::too_many_lines)] // Keep the last-window stimulus and native/profile oracles together.
fn last_window_cycle(
    fixture: &FacadeFixture,
    action: LastWindowAction,
    receipt: Receipt,
) -> ProductEvidence {
    let hooks = if receipt == Receipt::Withheld {
        vec![("KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY", "1".to_owned())]
    } else {
        Vec::new()
    };
    let (mut run, ready) = fixture.spawn(&hooks);
    assert_eq!(ready["windows"], serde_json::json!([]));
    assert_eq!(ready["lastWindowAction"], action.as_str());
    assert_eq!(app_top_level_windows(run.pid()), []);
    let created = create(&mut run);
    let id = number(&created, "id");
    let native = capture(census(run.pid(), 1)[0], run.pid());
    assert!(
        !native.children.is_empty(),
        "last-window cell has no controller at risk"
    );
    let browser = fixture.browser(&run);
    let browser_pid = number(&browser, "browser_pid");
    fixture.assert_lease_held();
    let profile = fixture.profile_dir();
    let evidence = ProductEvidence {
        host_pid: run.pid(),
        bun_pid: run.bun_pid,
        app_link: run.link.clone(),
    };
    run.retain_descendants();
    run.send(&format!("ALLOW {id}"));
    run.read("ALLOW");
    // Read continuously before WM_CLOSE: the normal Quit tail can retire Bun
    // before a delayed reader would drain Winsock's buffered callback records.
    run.capture_control();
    let started = Instant::now();
    post_close(u64::try_from(native.hwnd).expect("last HWND"));
    let (status, trace, control) = run.finish_observed(started + ui_end_kill_switch());
    assert_retired(&native);
    assert_eq!(app_top_level_windows(evidence.host_pid), []);
    let closes = records(&control, "CLOSE_EVENT");
    assert_eq!(
        closes.len(),
        1,
        "one real WM_CLOSE must reach its facade:\n{control}"
    );
    assert_eq!(closes[0]["id"], id);
    assert_eq!(closes[0]["count"], 1);
    let closed = records(&control, "CLOSED");
    assert_eq!(
        closed.len(),
        1,
        "the last window must retire exactly once:\n{control}"
    );
    assert_eq!(closed[0]["id"], id);
    assert_eq!(closed[0]["count"], 1);
    assert_eq!(closed[0]["destroyed"], true);
    assert_eq!(closed[0]["contentsDestroyed"], true);
    let listener = records(&control, "LAST_WINDOW_QUIT");
    match action {
        LastWindowAction::DefaultQuit => assert!(
            listener.is_empty(),
            "the default branch installed a user last-window listener"
        ),
        LastWindowAction::DrawIoQuit => {
            assert_eq!(
                listener.len(),
                1,
                "draw.io-style listener did not run once:\n{control}"
            );
            assert_eq!(listener[0]["platform"], "win32");
            assert_eq!(listener[0]["liveCount"], 0);
            assert_eq!(listener[0]["count"], 1);
        }
        LastWindowAction::Hold => panic!("AC14 requires a last-window Quit branch"),
    }
    assert!(
        records(&control, "AUTO_QUIT_ERROR").is_empty(),
        "public last-window Quit was rejected:\n{control}"
    );
    if receipt == Receipt::Observed {
        assert!(
            status.success(),
            "last-window Quit failed: {status}\n{trace}"
        );
        assert!(
            !trace.contains("KELD-WV-009"),
            "native fault was swallowed:\n{trace}"
        );
        assert!(
            trace.contains("KELD_T2_RELEASE_WITNESS completed=true"),
            "{trace}"
        );
        assert!(
            !profile.exists(),
            "successful last-window Quit retained its ephemeral profile"
        );
    } else {
        assert!(
            !status.success(),
            "withheld receipt credited successful Quit:\n{trace}"
        );
        assert!(trace.contains("KELD-WV-009")
            && trace.contains("host browser process exited without raising BrowserProcessExited within the post-exit grace"),
            "the existing h9 control did not fail at the host post-exit grace:\n{trace}");
        assert!(
            !trace.contains("KELD_T2_RELEASE_WITNESS completed=true"),
            "missing browser receipt earned normal profile credit:\n{trace}"
        );
        assert!(
            profile.exists(),
            "failed release deleted its unproven profile"
        );
    }
    let exits = trace
        .lines()
        .filter(|line| line.starts_with("KELD_T2_LOOP_EXIT "))
        .collect::<Vec<_>>();
    assert_eq!(
        exits.len(),
        1,
        "one completed native loop required:\n{trace}"
    );
    assert!(exits[0].contains("environments=1 controllers=1"), "{trace}");
    assert!(
        exits[0].contains("armed_by=Quit"),
        "AC14 NC1: the actual release barrier was not armed by the terminal Quit:\n{trace}"
    );
    assert!(
        exits[0].contains("releasing=0"),
        "an owed native release remained:\n{trace}"
    );
    ordered(
        &trace,
        &[
            format!("DESTROYED {}", native.hwnd),
            format!("TEARDOWN_CONFIRMED {}", native.hwnd),
            "RELEASED ".to_owned(),
            "TERMINAL Quit releasing=0".to_owned(),
            "KELD_T2_LOOP_EXIT ".to_owned(),
        ],
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("RELEASED "))
            .count(),
        1
    );
    println!(
        "KELD_GH1188_AC14 action={} receipt_withheld={} at_risk=1 host_pid={} bun_pid={} browser_pid={browser_pid} elapsed_ms={} profile_retained={}",
        action.as_str(),
        receipt == Receipt::Withheld,
        evidence.host_pid,
        evidence.bun_pid,
        started.elapsed().as_millis(),
        profile.exists(),
    );
    evidence
}
