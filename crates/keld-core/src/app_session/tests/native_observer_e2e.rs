//! AC7 native/public-index mechanism scenarios; no no-flag app/ring claim.
use super::{
    assert_public_retirement_report, assert_public_success_report,
    fresh_public_fs_session_reads_its_write,
};
use std::fs;
use std::sync::{Arc, mpsc};
use std::time::Duration;

#[path = "native_create_new_race.rs"]
mod create_new_race;
#[path = "native_observer_fixture.rs"]
mod fixture;
#[path = "native_namespace_race.rs"]
mod namespace_race;
#[path = "native_observer_sensor.rs"]
pub(in crate::app_session) mod sensor;
#[path = "native_observer_support.rs"]
mod support;
use fixture::{DrainProbe, NativeFixture, held_snapshot};

#[path = "native_generation_recovery.rs"]
mod native_generation_recovery;

#[path = "native_deadline_e2e.rs"]
mod native_deadline_e2e;

const OBSERVATION_BOUND: Duration = Duration::from_secs(5);

#[test]
#[ignore = "explicit physical macOS native-observer qualified lane"]
fn public_fs_quiesced_preentry_never_calls_native() {
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::public_fs_quiesced_preentry_never_calls_native";
    if !support::is_native_child("entry") {
        support::isolated_case("entry", SELECTOR);
        return;
    }
    let mut fixture = NativeFixture::start("entry", "retirement", false, true);
    let target = fixture.router().allowed.join("native-entry.txt");
    let late = fixture.router().allowed.join("late-entry.txt");
    fs::write(&target, b"unchanged").expect("seed real entry sentinel");
    fixture.supply(&target, &late);
    support::calibrate_object_sensor(&target);
    fixture
        .router()
        .router
        .handle()
        .signal_ready()
        .expect("real public Ready");
    fixture
        .signals
        .pre_entry
        .as_ref()
        .expect("pre-entry receiver")
        .recv_timeout(OBSERVATION_BOUND)
        .expect("public work dequeued before accepting check");
    let pending = fixture
        .router()
        .snapshot
        .fs
        .pending_call_for(1)
        .expect("real pre-entry correlation");
    println!(
        "KELD_AC7_PREENTRY_HELD case=entry attempt={} corr={:?}",
        pending.attempt, pending.correlation
    );
    fixture
        .router()
        .snapshot
        .fs
        .quiesce()
        .expect("publish closed handler admission");
    fixture
        .router()
        .router
        .handle()
        .retire_generation(1)
        .expect("retire public generation");
    let report = fixture.finish_caller();
    assert_public_retirement_report(&report);
    // A mutant accepting the old item can still observe cancellation and no
    // effect, so only the actual native entry counter distinguishes that fault.
    fixture
        .router()
        .release
        .send(())
        .expect("prearm unchanged post-entry gate");
    fixture
        .signals
        .pre_release
        .take()
        .expect("pre-entry release")
        .send(())
        .expect("resume old item");
    let final_snapshot = fixture.finish_router(&target, &late, 0, b"unchanged");
    assert_eq!(final_snapshot.walk_entries + final_snapshot.target_opens, 0);
    // NativeFixture made the exact bytes/late-target checks before scope drop.
    fresh_public_fs_session_reads_its_write();
    println!("KELD_AC7_CASE_DONE case=entry entries=0 returns=0 settlements=1");
}

#[test]
#[ignore = "explicit physical macOS native-observer qualified lane"]
fn public_fs_same_bytes_has_one_native_invocation() {
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::public_fs_same_bytes_has_one_native_invocation";
    if !support::is_native_child("duplicate") {
        support::isolated_case("duplicate", SELECTOR);
        return;
    }
    let mut fixture = NativeFixture::start("duplicate", "reply-wins", true, false);
    let target = fixture.router().allowed.join("native-once.txt");
    let late = fixture.router().allowed.join("native-unused.txt");
    fs::write(&target, b"unchanged").expect("seed actual duplicate sentinel");
    fixture.supply(&target, &late);
    support::calibrate_object_sensor(&target);
    let (written_tx, written) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    fixture
        .router()
        .snapshot
        .fs
        .hold_next_terminal_write(written_tx, resume_rx);
    fixture.terminal_resume = Some(resume);
    fixture
        .router()
        .router
        .handle()
        .signal_ready()
        .expect("real public Ready");
    fixture
        .router()
        .taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("accepted public work");
    fixture
        .router()
        .release
        .send(())
        .expect("release old after-accept gate");
    held_snapshot(&fixture, "duplicate", &target);
    fixture.release_native();
    let returned = fixture
        .signals
        .returned
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual native return");
    assert!(!returned.fixture_fault);
    assert_eq!(
        returned.request_entries, 1,
        "duplicate native invocation before terminal reply"
    );
    assert_eq!(returned.request_returns, 1);
    written
        .recv_timeout(OBSERVATION_BOUND)
        .expect("real correlated native reply written");
    assert_eq!(fs::read(&target).expect("actual allowed write"), b"t2");
    fixture
        .router()
        .router
        .handle()
        .retire_generation(1)
        .expect("retire only after actual reply");
    fixture
        .terminal_resume
        .take()
        .expect("terminal release")
        .send(())
        .expect("resume replied worker");
    let report = fixture.finish_caller();
    assert_public_success_report(&report);
    let final_snapshot = fixture.finish_router(&target, &late, 1, b"t2");
    assert_eq!(
        final_snapshot.truncate_calls, 1,
        "same-byte duplicate truncate is observable"
    );
    assert!(
        final_snapshot.content_writes > 0,
        "real content I/O was not observed"
    );
    fresh_public_fs_session_reads_its_write();
    println!("KELD_AC7_CASE_DONE case=duplicate entries=1 returns=1 settlements=1");
}

#[test]
#[ignore = "explicit physical macOS native-observer qualified lane"]
fn public_fs_retirement_keeps_native_handle_until_owner_drain() {
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::public_fs_retirement_keeps_native_handle_until_owner_drain";
    if !support::is_native_child("drain") {
        support::isolated_case("drain", SELECTOR);
        return;
    }
    let mut fixture = NativeFixture::start("drain", "retirement", true, false);
    let target = fixture.router().allowed.join("native-held.txt");
    let late = fixture.router().allowed.join("native-late.txt");
    fs::write(&target, b"unchanged").expect("seed held native sentinel");
    fixture.supply(&target, &late);
    support::calibrate_object_sensor(&target);
    fixture
        .router()
        .router
        .handle()
        .signal_ready()
        .expect("real public Ready");
    fixture
        .router()
        .taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("accepted public work");
    fixture
        .router()
        .release
        .send(())
        .expect("release old post-entry gate");
    held_snapshot(&fixture, "drain", &target);
    fixture
        .router()
        .router
        .handle()
        .retire_generation(1)
        .expect("retire while actual native call remains held");
    let report = fixture.finish_caller();
    assert_public_retirement_report(&report);
    assert_eq!(
        support::object_handle_count(&target),
        1,
        "caller rejection dropped native handle"
    );
    assert_eq!(
        fs::read(&target).expect("held independent file"),
        b"unchanged"
    );
    let (wait_tx, waited) = mpsc::sync_channel(1);
    fixture
        .router()
        .snapshot
        .fs
        .observe_next_drain_wait(wait_tx);
    let probe = DrainProbe::start(
        Arc::clone(&fixture.router().snapshot.fs),
        fixture
            .signals
            .native_release
            .as_ref()
            .expect("owned release")
            .clone(),
    );
    waited
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual owner waits for native lease");
    fixture.assert_held_drain(&probe);
    fixture.release_native();
    let returned = fixture
        .signals
        .returned
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual native return after release");
    assert!(!returned.fixture_fault);
    assert_eq!((returned.request_entries, returned.request_returns), (1, 1));
    probe.finish();
    assert_eq!(
        support::object_handle_count(&target),
        0,
        "native call retained target after drain"
    );
    assert_eq!(
        fs::read(&target).expect("released independent sentinel"),
        b"unchanged"
    );
    assert!(!late.exists(), "post-retirement public call entered native");
    let final_snapshot = fixture.finish_router(&target, &late, 1, b"unchanged");
    assert_eq!(
        final_snapshot.truncate_calls + final_snapshot.content_writes,
        0,
        "cancelled pre-effect native call wrote bytes"
    );
    fresh_public_fs_session_reads_its_write();
    println!("KELD_AC7_CASE_DONE case=drain entries=1 returns=1 settlements=1");
}
