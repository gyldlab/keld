//! KEL-140: public outer expiry does not drop held native work or its target.
//! No boundary change. The native Hold is a pre-effect fixture wait; this case
//! makes no native-budget, kernel-syscall, root-census or no-flag recovery claim.

use super::super::super::{
    GuardedFsTestSetup, GuardedTestRouter, PrimaryEchoCall, PrimaryEchoReply,
};
use super::super::{WorkerLinkRole, report_value};
use super::fixture::{DrainProbe, NativeFixture, held_snapshot};
use super::support;
use super::{OBSERVATION_BOUND, fresh_public_fs_session_reads_its_write};
use keld_ipc::codec::{decode, encode};
use keld_ipc::echo::{EchoRequest, EchoResponse};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, mpsc};

const CASE: &str = "outer-deadline";
const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_deadline_e2e::public_fs_outer_expiry_keeps_native_work_until_explicit_retirement_and_drain";
const SENTINEL: &[u8] = b"unchanged";

fn start_deadline_caller(setup: GuardedFsTestSetup) -> (WorkerLinkRole, GuardedTestRouter) {
    let listener = keld_ipc::BootstrapListener::bind().expect("public deadline listener");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/app_session/tests/fixtures/public_fs_deadline_role.ts");
    let mut command = Command::new("bun");
    command
        .arg(script)
        .env("KELD_APP_LINK", listener.app_link())
        .env_remove("KELD_KIPC_TEST_HOOKS")
        .stdin(Stdio::piped());
    WorkerLinkRole::spawn_authenticated_with_test_setup(&listener, command, Some(setup))
}

fn assert_public_deadline_reply(call: &PrimaryEchoCall) {
    let reply = call
        .reply
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual public deadline outcome over Echo")
        .expect("host Echo response remains live during native hold");
    let PrimaryEchoReply::Reply(bytes) = reply else {
        panic!("public handler failed instead of returning its FS outcome: {reply:?}");
    };
    let response: EchoResponse = decode(&bytes).expect("owning Echo response codec");
    assert_eq!(response.count, 1, "independent request correlation witness");
    let outcome: serde_json::Value =
        serde_json::from_str(&response.message).expect("bounded fixture outcome");
    assert_eq!(
        outcome,
        json!({
            "outcome": {
                "status": "rejected",
                "code": "KELD-IPC-006",
                "message": "KELD-IPC-006: call deadline of 5000 ms expired with no host reply"
            },
            "settlements": 1
        }),
        "public caller expiry must remain the actual outer IPC006"
    );
}

fn assert_expired_call_still_held(fixture: &NativeFixture, target: &Path, late: &Path) {
    assert_eq!(
        fixture
            .router()
            .snapshot
            .fs
            .state
            .lock()
            .expect("outer-expired native state")
            .in_flight,
        1,
        "public expiry retired the actual native lease"
    );
    assert!(matches!(
        fixture.signals.returned.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(
        support::object_handle_count(target),
        1,
        "public expiry dropped the independently observed target handle"
    );
    assert_eq!(
        fs::read(target).expect("held independent sentinel"),
        SENTINEL
    );
    assert!(!late.exists(), "outer expiry acquired an unrelated effect");
}

fn retire_and_drain(fixture: &mut NativeFixture, target: &Path, late: &Path) {
    fixture
        .router()
        .router
        .handle()
        .retire_generation(1)
        .expect("explicit retirement follows the public outcome");
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
            .expect("owned native release")
            .clone(),
    );
    waited
        .recv_timeout(OBSERVATION_BOUND)
        .expect("owner drain really waits for the retained native lease");
    fixture.assert_held_drain(&probe);
    assert_expired_call_still_held(fixture, target, late);
    fixture.release_native();
    let returned = fixture
        .signals
        .returned
        .recv_timeout(OBSERVATION_BOUND)
        .expect("native actually returned after explicit release");
    assert!(
        !returned.fixture_fault,
        "invalid native return: {returned:?}"
    );
    assert_eq!((returned.request_entries, returned.request_returns), (1, 1));
    probe.finish();
    assert_eq!(
        support::object_handle_count(target),
        0,
        "target survives drain"
    );
    assert_eq!(fs::read(target).expect("post-drain sentinel"), SENTINEL);
}

fn finish_owned_scope(mut fixture: NativeFixture, target: &Path, late: &Path) {
    let report = fixture.finish_caller();
    assert_eq!(
        report_value(&report, "handler-calls"),
        Some("1"),
        "{report:?}"
    );
    assert_eq!(
        report_value(&report, "settlements"),
        Some("1"),
        "{report:?}"
    );
    let final_snapshot = fixture.finish_router(target, late, 1, SENTINEL);
    assert_eq!(
        final_snapshot.truncate_calls + final_snapshot.create_calls + final_snapshot.content_writes,
        0,
        "explicitly retired pre-effect work changed content"
    );
    assert!(matches!(
        fixture.signals.returned.try_recv(),
        Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
    ));
    // All old native/router/caller owners and its temporary scope precede the
    // existing independent fresh public write/read helper.
    drop(fixture);
}

fn held_public_call(fixture: &mut NativeFixture, target: &Path, late: &Path) -> PrimaryEchoCall {
    fs::write(target, SENTINEL).expect("seed independent native target");
    fixture.supply(target, late);
    support::calibrate_object_sensor(target);
    fixture
        .router()
        .router
        .handle()
        .signal_ready()
        .expect("real public Ready");
    let payload = encode(&EchoRequest {
        message: CASE.to_owned(),
        count: 1,
    })
    .expect("owning Echo request codec");
    let call = fixture
        .router()
        .router
        .handle()
        .begin_echo_call(&payload)
        .expect("one same-link host Echo CALL");
    fixture
        .router()
        .taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("public write reached the existing native owner");
    fixture
        .router()
        .release
        .send(())
        .expect("release the existing short post-entry gate");
    held_snapshot(fixture, CASE, target);
    call
}

#[test]
#[ignore = "explicit physical macOS native-observer qualified lane"]
fn public_fs_outer_expiry_keeps_native_work_until_explicit_retirement_and_drain() {
    if !support::is_native_child(CASE) {
        support::isolated_case(CASE, SELECTOR);
        return;
    }
    let mut fixture = NativeFixture::start_with_caller(CASE, true, false, start_deadline_caller);
    let target: PathBuf = fixture.router().allowed.join("native-outer-deadline.txt");
    let late = fixture
        .router()
        .allowed
        .join("native-outer-deadline-unused.txt");
    let call = held_public_call(&mut fixture, &target, &late);
    // This wait observes the actual canonical caller outcome; elapsed time is
    // not native-budget evidence and the existing bound is only a kill switch.
    assert_public_deadline_reply(&call);
    drop(call);
    assert_expired_call_still_held(&fixture, &target, &late);
    retire_and_drain(&mut fixture, &target, &late);
    finish_owned_scope(fixture, &target, &late);
    fresh_public_fs_session_reads_its_write();
    println!(
        "KELD_AC7_CASE_DONE case={CASE} entries=1 returns=1 settlements=1 public_code=KELD-IPC-006 native_budget_acceptance=false syscall_acceptance=false root_census=false recovery_acceptance=false"
    );
}
