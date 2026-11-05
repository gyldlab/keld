//! GH589's private approved-graph launch seam, proved with first-party inert input.
//! These tests neither select the original app nor establish editor/product evidence.

#[path = "approved_graph_launch/d1.rs"]
mod d1;
#[path = "approved_graph_launch/user_agent.rs"]
mod user_agent;

use crate::graph_stage::prepare_with_role;
use crate::support::EVENT_DEADLINE;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::{
    NativeWindowObserver, compile_native_window_census, native_windows,
};
use crate::support::process::{
    ProcessExitObserver, await_process_gone_before, parent_process, process_group, process_ids,
    session_dirs_for,
};
use keld_cli::boot::{BootCompileError, DevBootStage};
use keld_core::app_session::graph_contract::ApprovedGraphCaptureInput;
use std::fs;
use std::os::unix::process::CommandExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Output, Stdio};
use std::time::{Duration, Instant};

// The manual original case includes mandatory cold Host/Guardian graph checks
// before physical family acquisition. Native/editor/exit clocks stay20s.
const ORIGINAL_INITIAL_FAMILY_BUDGET: Duration = Duration::from_secs(90);

fn original_initial_family_deadline(spawn_started: Instant) -> Instant {
    spawn_started + ORIGINAL_INITIAL_FAMILY_BUDGET
}

/// The integration owner selects this input; application files cannot opt in.
pub(super) fn stage_first_proof(
    project: &Path,
    host: &Path,
    approved: &ApprovedGraphCaptureInput,
) -> Result<DevBootStage, BootCompileError> {
    keld_cli::boot::stage_dev_boot_with_approved_graph(project, host, approved, None)
}

/// Owns exactly the existing staged Host and its stdin dev lease.
/// Caller-owned native/Host observers must establish each claimed app effect.
pub(super) struct FirstProofLaunch {
    cleanup: ShippingLaunchCleanup,
    lease: Option<ChildStdin>,
    stage: DevBootStage,
    host_pid: u32,
    spawn_started: Instant,
}

impl FirstProofLaunch {
    /// Admit through the sole graph producer before spawning the no-flag Host.
    pub(super) fn start(project: &Path, host: &Path, approved: &ApprovedGraphCaptureInput) -> Self {
        let stage = stage_first_proof(project, host, approved)
            .expect("root-selected original/fixture graph admission before Host spawn");
        Self::start_prepared(stage)
    }

    // Both inert and original callers use this exact existing spawn/lease owner.
    fn start_prepared(stage: DevBootStage) -> Self {
        Self::start_prepared_with_output(stage, None)
    }

    /// D1 physical failure receipts use the same factory and bounded live pipe
    /// owner. This does not establish native/app readiness or qualify policy.
    fn start_prepared_captured(stage: DevBootStage, deadline: Instant) -> Self {
        Self::start_prepared_with_output(stage, Some(deadline))
    }

    fn start_prepared_with_output(stage: DevBootStage, capture_deadline: Option<Instant>) -> Self {
        let mut command = Command::new(stage.host());
        command
            .current_dir(stage.root())
            .process_group(0)
            .env("KELD_DEV_LEASE", "stdin-v1")
            .stdin(Stdio::piped())
            .stdout(if capture_deadline.is_some() {
                Stdio::piped()
            } else {
                Stdio::inherit()
            })
            .stderr(if capture_deadline.is_some() {
                Stdio::piped()
            } else {
                Stdio::inherit()
            });
        // This owner captures the actual spawn-attempt epoch once. Cold graph
        // validation may run before Guardian/Bun exists; no poll may rearm it.
        let spawn_started = Instant::now();
        let child = command.spawn();
        let child = match child {
            Ok(child) => child,
            Err(error) => {
                // No child exists: this is only our newly allocated idle stage.
                let cleanup = fs::remove_dir_all(stage.root());
                panic!("approved-graph Host spawn failed: {error}; stage cleanup: {cleanup:?}");
            }
        };
        let host_pid = child.id();
        let mut cleanup = ShippingLaunchCleanup::new(child);
        cleanup.retain_owned_stage(stage.root());
        cleanup.host_group = Some(host_pid);
        // This is our actual child/process group, not an app-reported PID.
        let group = process_group(host_pid);
        assert_eq!(group, host_pid);
        cleanup.host_group = Some(group);
        if let Some(deadline) = capture_deadline {
            cleanup.capture_output_before(deadline);
        }
        let lease = cleanup
            .cli
            .as_mut()
            .expect("retained Host")
            .stdin
            .take()
            .expect("live stdin lease writer");
        Self {
            cleanup,
            lease: Some(lease),
            stage,
            host_pid,
            spawn_started,
        }
    }

    pub(super) const fn host_pid(&self) -> u32 {
        self.host_pid
    }

    pub(super) fn stage_root(&self) -> &Path {
        self.stage.root()
    }

    /// Test-only normal lease shutdown, not a claim of cancelable app Quit.
    pub(super) fn finish_by_lease(&mut self) -> Output {
        drop(self.lease.take());
        self.cleanup.wait(EVENT_DEADLINE)
    }

    /// Census actual parent/group relations after this fixture's native
    /// window is observed; the first-party role sends no readiness/PID token.
    fn observe_family(&mut self) -> ([u32; 4], Vec<ProcessExitObserver>) {
        let mut pids = [self.host_pid; 4];
        for index in 1..4 {
            let children = process_ids("ppid", pids[index - 1]);
            assert_eq!(
                children.len(),
                1,
                "exact Host -> Guardian -> Bun -> descendant topology"
            );
            pids[index] = children[0];
            assert_eq!(parent_process(pids[index]), pids[index - 1]);
        }
        let [host, guardian, bun, descendant] = pids;
        assert_eq!(process_group(host), host);
        assert_eq!(process_group(guardian), host);
        assert_eq!(process_group(bun), bun);
        assert_eq!(process_group(descendant), bun);
        self.cleanup.record_authenticated_groups(host, bun);
        self.cleanup.record_stage_processes(&pids);
        let exits = pids
            .into_iter()
            .map(|pid| {
                if pid == host {
                    ProcessExitObserver::arm_own_child(
                        self.cleanup.cli.as_ref().expect("retained Host"),
                    )
                } else {
                    ProcessExitObserver::arm(pid)
                }
            })
            .collect();
        (pids, exits)
    }

    /// Consume only after caller assertions prove native/family/stage cleanup.
    fn complete(mut self) {
        self.cleanup.disarm_after_verified_cleanup();
    }
}

fn inert_fixture(root: &Path) -> (PathBuf, ApprovedGraphCaptureInput) {
    let role =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gh589_inert_window_role.mjs");
    prepare_with_role(root, &role)
}

fn host() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_keld-host"))
}

#[test]
fn gh589_first_proof_caller_selects_schema_five_and_full_layout() {
    let root = tempfile::tempdir().expect("owned first-proof fixture");
    let (project, approved) = inert_fixture(root.path());
    let stage = stage_first_proof(&project, host(), &approved).expect("first-proof caller stage");
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(stage.root().join("keld.boot.json")).expect("actual producer descriptor"),
    )
    .expect("descriptor JSON for independent assertions");
    // No Host has been spawned. Save observations and remove only this idle stage
    // even on the tests-first ordinary-staging counterfactual.
    let graph_present = stage.root().join("keld.stage-graph.json").is_file();
    let asset = fs::read(stage.root().join("drawio/src/main/webapp/index.html"));
    fs::remove_dir_all(stage.root()).expect("remove this unlaunched test-owned stage");
    assert_eq!(
        descriptor["schema"], 5,
        "ordinary keld dev is not this caller"
    );
    assert_eq!(descriptor["graph"]["file"], "keld.stage-graph.json");
    assert!(graph_present);
    assert_eq!(
        asset.expect("full graph asset"),
        b"<html>inert graph-only asset</html>\n"
    );
}

#[test]
fn gh589_first_proof_caller_refuses_changed_root_before_host_spawn() {
    let root = tempfile::tempdir().expect("owned first-proof fixture");
    let (project, approved) = inert_fixture(root.path());
    let package = project.join("package.json");
    let mut changed = fs::read(&package).expect("fixture package");
    changed.push(b'\n');
    fs::write(&package, changed).expect("single root-byte mutation under fixed approval");
    let result = stage_first_proof(&project, host(), &approved);
    if let Ok(stage) = &result {
        fs::remove_dir_all(stage.root()).expect("remove only unlaunched counterfactual stage");
    }
    let error = result.expect_err("fixed independently selected root digest must refuse");
    assert!(error.to_string().contains("graph"), "{error}");
}

#[test]
fn gh589_first_proof_caller_refuses_wrong_provider_expectation_before_host_spawn() {
    let root = tempfile::tempdir().expect("owned first-proof fixture");
    let (project, mut approved) = inert_fixture(root.path());
    approved.expected_package_trees[0][0] ^= 1;
    let result = stage_first_proof(&project, host(), &approved);
    if let Ok(stage) = &result {
        fs::remove_dir_all(stage.root()).expect("remove only unlaunched counterfactual stage");
    }
    let error = result.expect_err("wrong independent provider fingerprint must refuse");
    assert!(
        error.to_string().contains("approved package artifact"),
        "{error}"
    );
}

#[test]
fn gh589_first_proof_caller_observes_native_host_and_lease_shutdown() {
    run_native_lease_shutdown(false);
}

#[test]
fn gh589_early_lease_eof_terminates_real_family_and_native_window() {
    run_native_lease_shutdown(true);
}

fn run_native_lease_shutdown(early_release: bool) {
    let root = tempfile::tempdir().expect("owned first-proof fixture");
    let (project, approved) = inert_fixture(root.path());
    let census = compile_native_window_census(root.path());
    let mut observer = NativeWindowObserver::arm_for_title(&census, "Keld");
    let mut launch = FirstProofLaunch::start(&project, host(), &approved);
    let pid = launch.host_pid();
    let stage_root = launch.stage_root().to_owned();
    let ids = observer.expect_initial(pid, "GH589 caller inert native regression");
    assert_eq!(ids.len(), 1);
    assert!(
        stage_root.is_dir(),
        "stage custody remains while native view is live"
    );
    let (pids, exits) = launch.observe_family();
    // Real native presentation is the observation; no role-reported READY,
    // control socket, synthetic page, editor or original source is involved.
    let output = if early_release {
        // This is the negative control for an accidentally dropped writer:
        // prove its actual lifecycle effects before normal finish is called.
        // No local Option/descriptor assertion can satisfy this branch.
        drop(launch.lease.take());
        launch.cleanup.wait(EVENT_DEADLINE)
    } else {
        launch.finish_by_lease()
    };
    assert!(output.status.success(), "lease shutdown failed: {output:?}");
    let deadline = Instant::now() + EVENT_DEADLINE;
    for (index, exit) in exits.into_iter().enumerate() {
        let status = exit.wait_before(deadline);
        if index < 2 {
            assert!(
                status.success(),
                "Host/Guardian orderly lease shutdown: {status:?}"
            );
        } else {
            assert!(
                status.success() || matches!(status.signal(), Some(15 | 9)),
                "Bun/descendant must reach the owned TERM/KILL shutdown tail: {status:?}"
            );
        }
    }
    for target in pids {
        await_process_gone_before(target, deadline);
        assert!(
            session_dirs_for(target).is_empty(),
            "lease shutdown left an owned app-link session"
        );
    }
    for group in [pids[0], pids[2]] {
        assert!(
            process_ids("pgid", group).is_empty(),
            "an owned group survived product cleanup"
        );
    }
    assert_eq!(native_windows(pid, "Keld"), [] as [u32; 0]);
    assert!(
        !stage_root.exists(),
        "actual Host cleanup must remove its owned stage"
    );
    launch.complete(); // groups/stage disarm only after every product oracle
}

fn selected_graph_test_inputs() -> (PathBuf, ApprovedGraphCaptureInput) {
    let project = PathBuf::from(
        std::env::var_os("KELD_TEST_GRAPH_PROJECT").expect("root-selected owned original copy"),
    );
    assert!(project.join(".keld/approved-artifacts").is_dir());
    let expectations = PathBuf::from(
        std::env::var_os("KELD_TEST_GRAPH_EXPECTATIONS")
            .expect("external archive-derived expectations"),
    );
    assert!(!expectations.starts_with(&project));
    let expected: serde_json::Value =
        serde_json::from_slice(&fs::read(expectations).expect("external selected source receipt"))
            .expect("source receipt JSON");
    let approved = ApprovedGraphCaptureInput {
        expected_package_sha256: serde_json::from_value(
            expected["expected_package_sha256"].clone(),
        )
        .expect("root digest"),
        expected_lock_sha256: serde_json::from_value(expected["expected_lock_sha256"].clone())
            .expect("solver lock digest"),
        expected_package_trees: serde_json::from_value(expected["expected_package_trees"].clone())
            .expect("three independent SDK provider trees"),
    };
    (project, approved)
}

#[test]
#[ignore = "root-selected current SDK + canonical updater frozen offline DATA install; no app launch"]
fn gh589_root_selected_frozen_install_local_updater() {
    let (project, approved) = selected_graph_test_inputs();
    let stage = stage_first_proof(&project, host(), &approved)
        .expect("actual frozen source must pass canonical graph capture and updater selector");
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(stage.root().join("keld.boot.json")).expect("actual producer descriptor"),
    )
    .expect("descriptor JSON");
    let graph_present = stage.root().join("keld.stage-graph.json").is_file();
    let updater = fs::read(
        stage
            .root()
            .join("node_modules/electron-updater/src/index.ts"),
    );
    // The stage is newly allocated and unlaunched; no process/native resource exists.
    fs::remove_dir_all(stage.root()).expect("remove only this idle data-test stage");
    assert_eq!(
        descriptor["schema"], 5,
        "ordinary staging cannot satisfy this test"
    );
    assert!(graph_present, "canonical producer graph is required");
    assert_eq!(
        updater.expect("captured actual adapter source"),
        fs::read(project.join("node_modules/electron-updater/src/index.ts"))
            .expect("frozen installed adapter source")
    );
}

/// An operator-controlled session, not an original editor acceptance verdict.
/// Root observes actual native/editor effects separately while this retains the
/// original existing graph stage, Host child and stdin dev lease.
#[test]
#[ignore = "root-selected original-copy session; requires physical native observers"]
fn gh589_manual_original_copy_session() {
    let (project, approved) = selected_graph_test_inputs();
    let host_sha = std::env::var("KELD_TEST_QUALIFIED_HOST_SHA256")
        .expect("independently selected qualified Host digest, not stage readback");
    assert_eq!(host_sha.len(), 64);
    let mut qualified_host = [0_u8; 32];
    for (index, value) in qualified_host.iter_mut().enumerate() {
        *value =
            u8::from_str_radix(&host_sha[index * 2..index * 2 + 2], 16).expect("Host SHA256 hex");
    }
    let stage = keld_cli::boot::stage_dev_boot_with_approved_graph(
        &project,
        host(),
        &approved,
        Some(qualified_host),
    )
    .expect("same canonical approved graph/policy/Host pair admission before spawn");
    let mut launch = FirstProofLaunch::start_prepared(stage);
    // Real OS relations and kernel exit handles establish failure cleanup
    // custody before any operator wait; this is no application readiness claim.
    let initial_family_deadline = original_initial_family_deadline(launch.spawn_started);
    let initial_family = launch
        .cleanup
        .observe_family_before(initial_family_deadline);
    println!(
        "ORIGINAL_PROCESS_SNAPSHOT observed={}",
        initial_family.len()
    );
    println!(
        "ORIGINAL_SESSION_CUSTODY host={} stage={}",
        launch.host_pid(),
        launch.stage_root().display()
    );
    println!(
        "No Ready/editor verdict: root must record independent native observations. Send 'finish' for lease shutdown, or 'app-quit' to observe actual app exit while retaining this lease."
    );
    let mut instruction = String::new();
    std::io::stdin()
        .read_line(&mut instruction)
        .expect("operator lifetime control");
    let app_quit = match instruction.trim() {
        "finish" => false,
        "app-quit" => true,
        _ => panic!("EOF/other input cannot claim observation or a successful end"),
    };
    let family = if app_quit {
        // The app may already have exited during operator input. Keep the
        // pre-held kernel/group witnesses; a new live census would be false.
        initial_family
    } else {
        // Input may span new forks or a supervised replacement. Refresh the
        // same owner before deliberately releasing the existing dev lease.
        launch
            .cleanup
            .observe_family_before(Instant::now() + EVENT_DEADLINE)
    };
    let groups = launch.cleanup.observed_group_ids();
    let stage_root = launch.stage_root().to_owned();
    let output = if app_quit {
        // The original real stdin lease remains held through actual Child
        // exit. This keyword never requests Quit or supplies a lifecycle cause.
        launch.cleanup.wait(EVENT_DEADLINE)
    } else {
        launch.finish_by_lease()
    };
    assert!(
        output.status.success(),
        "owned Host exit failed: {output:?}"
    );
    let deadline = Instant::now() + EVENT_DEADLINE;
    let exits = launch.cleanup.take_observed_exits();
    for (index, (_pid, exit)) in exits.into_iter().enumerate() {
        let status = exit.wait_before(deadline);
        if index < 2 {
            assert!(status.success(), "Host/Guardian observed exit: {status:?}");
        } else {
            assert!(
                status.success() || matches!(status.signal(), Some(15 | 9)),
                "owned Bun/descendant retirement: {status:?}"
            );
        }
    }
    for pid in family {
        await_process_gone_before(pid, deadline);
        assert!(
            session_dirs_for(pid).is_empty(),
            "owned app-link session remains"
        );
    }
    for group in groups {
        assert_eq!(process_ids("pgid", group), [] as [u32; 0]);
    }
    assert!(!stage_root.exists(), "actual Host must remove its stage");
    // Native/editor/callback/task/credit qualification remains root-observed;
    // this runner claims only the physical family and stage checks above.
    launch.complete();
}

#[test]
fn gh589_original_initial_family_clock_is_absolute_from_host_spawn() {
    let spawn_started = Instant::now();
    let deadline = original_initial_family_deadline(spawn_started);
    assert_eq!(
        deadline.duration_since(spawn_started),
        Duration::from_secs(90)
    );
    assert_eq!(
        original_initial_family_deadline(spawn_started),
        deadline,
        "repeated observation cannot rearm the spawn epoch"
    );
    let expired_observation = spawn_started + Duration::from_secs(91);
    assert!(
        expired_observation > deadline,
        "later polling must still see the original deadline expired"
    );
    assert_eq!(
        EVENT_DEADLINE,
        Duration::from_secs(20),
        "native/editor/exit and inert-case clocks remain independently bounded"
    );
}
