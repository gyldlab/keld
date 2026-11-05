//! Three independent AC24 consumers with ordinary cleanup before disputed assertions.

use super::super::create;
use super::super::fixture::{
    BUILD_FAILURE, FacadeFixture, NATIVE_HOLD, capture, census, marker_number, ordered,
};
use super::failed_hwnd_live;
use crate::PRODUCT_DEADLINE;
use crate::support::window::post_close;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

struct HeldNativeRelease {
    path: PathBuf,
    pending: bool,
}

impl HeldNativeRelease {
    fn release(&mut self) {
        fs::write(&self.path, b"release\n").expect("release owned native hold");
        self.pending = false;
    }
}

impl Drop for HeldNativeRelease {
    fn drop(&mut self) {
        if self.pending {
            let _ = fs::write(&self.path, b"release\n");
        }
    }
}

#[allow(clippy::too_many_lines)] // Keep reply-time observations and post-cleanup assertions in one auditable sequence.
pub(super) fn case(release: &str) {
    let fixture = FacadeFixture::new();
    let hold = fixture.project.root.path().join("native-hold");
    fs::create_dir(&hold).expect("run-owned native hold directory");
    let mut hooks = vec![(NATIVE_HOLD, format!("destroyed:{}", hold.display()))];
    if release == "failed-build" {
        hooks.push((BUILD_FAILURE, "hwnd".to_owned()));
    }
    let (mut run, _) = fixture.spawn(&hooks);
    // Install cleanup before any command can enter the native hold.
    let mut held = HeldNativeRelease {
        path: hold.join("release"),
        pending: true,
    };
    let native = if release == "failed-build" {
        None
    } else {
        create(&mut run);
        Some(capture(census(run.pid(), 1)[0], run.pid()))
    };
    run.retain_descendants();
    let profile = fixture.profile_dir();
    match release {
        "close" => {
            run.send("ALLOW 1");
            run.read("ALLOW");
            post_close(native.as_ref().expect("created close HWND").hwnd as u64);
            assert_eq!(run.read("CLOSE_EVENT")["count"], 1);
        }
        "destroy" => {
            run.send("RAW_DESTROY 1");
            run.read("RAW_DESTROY_SENT");
        }
        "failed-build" => {
            run.send("RAW_CREATE failed");
            run.read("RAW_SENT");
        }
        _ => panic!("unknown AC24 consumer {release}"),
    }
    let (marker, _) = run.marker("DESTROY_HELD ", 0);
    let hwnd = marker_number(&marker);
    assert!(
        failed_hwnd_live(hwnd),
        "INVALID AC24 {release}: no TRUE IsWindow at DESTROY_HELD; zero at-risk runs"
    );
    if let Some(native) = &native {
        assert_eq!(hwnd, native.hwnd as u64);
    }
    println!("KELD_GH1188_AC24 case={release} at_risk=1 hwnd={hwnd}");
    let mut reply_in_hold = None;
    let held_observation = run.observe_release_during_hold(|| {
        let parent_live = failed_hwnd_live(hwnd);
        let retained_live = native
            .as_ref()
            .map(|native| native.live_handles())
            .unwrap_or_default();
        println!(
            "KELD_GH1188_AC24_REPLY case={release} hwnd={hwnd} parent_live={parent_live} retained_live={retained_live:?}"
        );
        reply_in_hold = Some((parent_live, retained_live));
    });
    held.release();
    // Drain the ordinary responses even when the held observation found the
    // semantic defect. A read error is not sensitivity credit for AC24.
    let records = match release {
        "close" => vec![("CLOSED".to_owned(), run.read("CLOSED"))],
        "destroy" => vec![run.next(), run.next()],
        _ => vec![("RAW_ERROR".to_owned(), run.read("RAW_ERROR"))],
    };
    let parent_at_reply = failed_hwnd_live(hwnd);
    let retained_at_reply = native
        .as_ref()
        .map(|native| native.live_handles())
        .unwrap_or_default();
    // Buffered early responses can precede DestroyWindow's eventual return.
    // Wait for actual retirement before Quit; this wait is cleanup, not proof.
    let retire_by = Instant::now() + PRODUCT_DEADLINE;
    while failed_hwnd_live(hwnd)
        || native
            .as_ref()
            .is_some_and(|native| !native.live_handles().is_empty())
    {
        assert!(
            Instant::now() < retire_by,
            "INVALID AC24 {release}: native cleanup did not retire the owned handles"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
    assert!(census(run.pid(), 0).is_empty());
    let trace = run.quit();
    let product_removed_profile = !profile.exists();
    fixture.close();

    let publication = held_observation
        .unwrap_or_else(|error| panic!("INVALID AC24 {release}: {error}; native trace:\n{trace}"));
    assert!(
        publication.is_none(),
        "AC24 {release}: publication preceded native retirement: {publication:?}; held_reply={reply_in_hold:?}\n{trace}"
    );
    assert!(
        reply_in_hold.is_none(),
        "reply arrived during the native hold"
    );
    assert!(
        !parent_at_reply && retained_at_reply.is_empty(),
        "reply published with native handles still live: parent={parent_at_reply} retained={retained_at_reply:?}"
    );
    match release {
        "close" => assert_eq!(records[0].1["count"], 1),
        "destroy" => {
            assert!(
                records
                    .iter()
                    .any(|(kind, value)| kind == "CLOSED" && value["count"] == 1)
            );
            assert!(
                records
                    .iter()
                    .any(|(kind, value)| kind == "DESTROY_REPLY" && value["reply"] == "Destroyed")
            );
        }
        _ => assert_eq!(
            records[0].1["code"], "KELD-CORE-041",
            "U7 fault must answer the actual build error, not a call timeout"
        ),
    }
    let before = if release == "failed-build" {
        format!("FAILED_BUILD_DESTROYED {hwnd}")
    } else {
        format!("DESTROYED {hwnd}")
    };
    let after = if release == "failed-build" {
        "FAILED_BUILD_ANSWERED ".to_owned()
    } else {
        "RELEASED ".to_owned()
    };
    ordered(
        &trace,
        &[
            before,
            format!("DESTROY_HELD {hwnd}"),
            format!("DESTROY_HOLD_END {hwnd}"),
            format!("TEARDOWN_CONFIRMED {hwnd}"),
            after,
        ],
    );
    assert!(
        product_removed_profile,
        "normal Quit left the product profile"
    );
}
