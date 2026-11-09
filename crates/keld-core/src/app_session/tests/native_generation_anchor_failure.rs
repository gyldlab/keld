//! AC4 absent-anchor recovery: actual Native error, full guardian trace, and Runtime cleanup.
use super::{
    Fixture, OBSERVATION_BOUND, RecoveryArm, SENTINEL, apply_guardian_recovery_arm,
    assert_no_successor, roundtrip, sensor, support,
};
use nix::errno::Errno;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const DIRECT_CASE: &str = "guardian-native-anchor-absent";
const DIRECT_SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::anchor_failure::fresh_preflight_rejects_absent_anchor_before_successor_provision";

/// Holds namespace custody only: no broker, file descriptor, policy or recovery state.
pub(super) struct AnchorAbsence {
    original: PathBuf,
    renamed: PathBuf,
    target: PathBuf,
    restored: bool,
}

impl AnchorAbsence {
    pub(super) fn remove(root: &Path, target: &Path) -> Self {
        let renamed = root.with_file_name("kel143-ac4-retired-anchor");
        assert!(
            !renamed.exists(),
            "owned renamed-anchor destination already exists"
        );
        let relative = target
            .strip_prefix(root)
            .expect("receipt target beneath anchor");
        let moved_target = renamed.join(relative);
        let before = fs::metadata(root).expect("actual initial anchor identity");
        fs::rename(root, &renamed).expect("make unchanged literal anchor absent");
        let after = fs::metadata(&renamed).expect("retained renamed anchor identity");
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        assert!(matches!(fs::symlink_metadata(root), Err(error)
            if error.kind() == std::io::ErrorKind::NotFound));
        assert_eq!(
            fs::read(&moved_target).expect("renamed G1 sentinel"),
            SENTINEL
        );
        println!(
            "KELD_KEL143_AC4_ANCHOR_ABSENT dev={} ino={} original={} renamed={}",
            after.dev(),
            after.ino(),
            root.display(),
            renamed.display()
        );
        Self {
            original: root.to_owned(),
            renamed,
            target: moved_target,
            restored: false,
        }
    }

    pub(super) fn renamed_root(&self) -> &Path {
        &self.renamed
    }
    pub(super) fn renamed_target(&self) -> &Path {
        &self.target
    }

    pub(super) fn restore(mut self) {
        assert!(
            !self.original.exists(),
            "cleanup must not replace a new ambient object"
        );
        fs::rename(&self.renamed, &self.original)
            .expect("restore producer-owned anchor after cleanup");
        self.restored = true;
    }
}

impl Drop for AnchorAbsence {
    fn drop(&mut self) {
        if !self.restored
            && !self.original.exists()
            && let Err(error) = fs::rename(&self.renamed, &self.original)
        {
            eprintln!("KELD_KEL143_AC4_ANCHOR_RESTORE_FAILED {error}");
        }
    }
}

#[test]
#[ignore = "explicit physical macOS source-bound absent-anchor recovery/native lane"]
fn fresh_preflight_rejects_absent_anchor_before_successor_provision() {
    if !support::is_native_child(DIRECT_CASE) {
        support::isolated_guardian_case(DIRECT_CASE, DIRECT_SELECTOR);
        return;
    }
    let mut fixture = Fixture::start();
    fixture.ready();
    let guardian_group = fixture.guardian.guardian_pid();
    let _old = roundtrip(&fixture.handle(), "00000000000000000000000000004144");
    fixture
        .gate_taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual G1 work taken");
    let held = fixture
        .signals
        .held
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Native after-walk hold");
    assert!(!held.fixture_fault);
    assert_eq!((held.request_entries, held.request_returns), (1, 0));
    assert!(held.walk_entries > 0 && held.target_opens > 0);
    fixture.assert_native_held();
    let mut recovery = fixture.retire_while_held();
    fixture.return_and_observe_old_drop(&mut recovery);
    // The existing Retired receipt has queued Prepare but has not sent it yet.
    let absence = AnchorAbsence::remove(&fixture.inputs.allowed_root, &fixture.inputs.target);
    assert_eq!(sensor::directory_handle_count(absence.renamed_root()), 0);
    assert_eq!(sensor::object_handle_count(absence.renamed_target()), 0);
    let deadline = Instant::now() + OBSERVATION_BOUND;
    let error = loop {
        assert!(
            Instant::now() < deadline,
            "actual missing-anchor preparation error absent"
        );
        fixture.pump_failure_commands();
        while let Some(update) = fixture.guardian.recv_update(Duration::ZERO) {
            assert_no_successor(&update);
        }
        let handle = fixture.handle();
        if let Err(error) = recovery.poll(&handle) {
            break error;
        }
        apply_guardian_recovery_arm(
            RecoveryArm::Retired {
                retired: 1,
                prepared: 2,
            },
            &mut fixture.guardian,
            Some(&handle),
            &mut fixture.initial_arm,
            Some(&mut recovery),
        )
        .expect("existing owner arm remains closed before successful preflight");
        thread::yield_now();
    };
    assert_eq!(error.code(), "KELD-NATIVE-008");
    assert!(!recovery.ready && !recovery.released);
    let state = fixture
        .snapshot
        .fs
        .state
        .lock()
        .expect("actual failed-preflight metadata");
    assert_eq!(state.prepared_attempt, None);
    assert_eq!(state.in_flight, 0);
    drop(state);
    assert_eq!(sensor::directory_handle_count(absence.renamed_root()), 0);
    assert_eq!(sensor::object_handle_count(absence.renamed_target()), 0);
    fixture.guardian.deny_recovery();
    fixture
        .router
        .take()
        .expect("one router owner")
        .shutdown()
        .expect("failed-preflight worker/readers join");
    let fence = fixture
        .signals
        .final_snapshot
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Native final fence");
    assert!(!fence.fixture_fault);
    assert_eq!((fence.request_entries, fence.request_returns), (1, 1));
    let status = fixture
        .guardian
        .shutdown()
        .expect("existing Q/EOF/owned guardian reap");
    assert!(
        status.success(),
        "actual guardian cleanup status {status:?}"
    );
    // Shutdown joins the Runtime generation worker; drain every final queued event.
    while let Some(update) = fixture.guardian.recv_update(Duration::ZERO) {
        assert_no_successor(&update);
    }
    assert!(fixture.guardian.group_pid().is_none());
    for group in fixture.groups.iter().copied().chain([guardian_group]) {
        let group = Pid::from_raw(i32::try_from(group).expect("diagnostic group fits"));
        assert_eq!(
            killpg(group, None::<Signal>),
            Err(Errno::ESRCH),
            "failed-preflight cleanup retained an owned group"
        );
    }
    assert_eq!(sensor::directory_handle_count(absence.renamed_root()), 0);
    assert_eq!(sensor::object_handle_count(absence.renamed_target()), 0);
    assert!(
        !fixture.inputs.stage_root.exists(),
        "normal guardian cleanup retained exact stage"
    );
    println!(
        "KELD_KEL143_AC4_DIRECT_FAILURE code={} no_provisioned=true full_runtime_trace=true entries=1 returns=1 roots=0 target_handles=0 stage_absent=true",
        error.code()
    );
    absence.restore();
    println!(
        "KELD_AC7_CASE_DONE case={DIRECT_CASE} actual_guardian=true native_prepare_failure=true"
    );
}
