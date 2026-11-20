//! KEL-140 stock producer/public API proof. Native execution belongs to ROOT.
//! No authority boundary change; no pending native-effect or cancellation claim.
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::{
    NativeWindowObserver, await_same_native_windows, native_windows,
};
use crate::support::process::{
    await_process_gone, kill_pid, parent_process, process_exists, process_group,
};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::net::UnixStream;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Instant;

#[path = "public_fs/actual_auth_app.rs"]
mod actual_auth_app;
#[path = "public_fs/application_link.rs"]
mod application_link;
#[path = "public_fs/component.rs"]
mod component;
// GH-1081 round-1 research experiments only (never merged).
#[path = "public_fs/crash_recovery_r1.rs"]
mod crash_recovery_r1;
#[path = "public_fs/descendant_quit.rs"]
mod descendant_quit;
#[path = "public_fs/observer.rs"]
mod observer;
#[path = "public_fs/outcomes.rs"]
mod outcomes;
#[path = "public_fs/policy_startup.rs"]
mod policy_startup;
#[path = "public_fs/preclick.rs"]
mod preclick;
#[path = "public_fs/process_image.rs"]
mod process_image;
#[path = "public_fs/project.rs"]
mod project;
#[path = "public_fs/retirement.rs"]
mod retirement;
use application_link::assert_single_application_link;
use observer::{Observation, RecoveryCommand, RecoveryPhase};
use outcomes::{
    assert_component_outcomes, assert_component_trace, assert_handler_attribution,
    assert_natural_outcomes, assert_page_outcomes,
};
use process_image::process_image_path;
use project::{
    CONTENT, Exercise, Policy, PreparedLaunch, PublicFsProject, RECOVERY_AFTER_CONTENT, SENTINEL,
    TITLE,
};

#[derive(Clone, Copy, Debug)]
struct ProcessFamily {
    bun: u32,
    descendant: u32,
    guardian: u32,
}

#[derive(Default)]
struct LaunchEvidence {
    reports: Vec<Value>,
    click: Option<Output>,
    family: Option<ProcessFamily>,
    window: Vec<u32>,
    retired_family: Option<ProcessFamily>,
    recovery_boot: Option<RecoveryBoot>,
}

struct RecoveryBoot {
    pid: u32,
    app_link: String,
}

#[test]
fn real_pointer_public_fs_uses_stock_project_policy() {
    run(Policy::Narrow, 2);
}

#[test]
fn explicit_empty_policy_denies_public_fs_without_effect() {
    run(Policy::Empty, 1);
}

#[test]
fn absent_project_policy_denies_public_fs_without_effect() {
    run(Policy::Absent, 1);
}

/// Completed-call consumer recovery, not interrupted-effect or full KEL-143 proof.
#[test]
fn renderer_public_fs_recovers_same_document_after_real_bun_loss() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let recovered = run_recovery_launch(&fixture, 0);
    // A separate ordinary public-main launch independently checks healthy reuse.
    let relaunched = run_launch(&fixture, 1, Exercise::PublicAppComponent);
    let document = |evidence: &LaunchEvidence, key: &str| {
        evidence
            .reports
            .iter()
            .find(|report| report["phase"] == "page-ready")
            .expect("actual initial document")[key]
            .as_str()
            .expect("renderer-created identity")
            .to_owned()
    };
    for key in ["documentNonce", "javascriptToken"] {
        assert_ne!(
            document(&recovered, key),
            document(&relaunched, key),
            "healthy new launch must create a new volatile {key}"
        );
    }
}

/// KEL143 AC4/continuity: the HTTP barrier holds only an application handler.
/// Native-held/no-early-provision and old broker handle census are separate rows.
#[test]
fn renderer_pending_public_fs_call_rejects_once_and_rebinds_replaced_root() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let recovered = run_launch(&fixture, 0, Exercise::RendererPendingRecovery);
    let relaunched = run_launch(&fixture, 1, Exercise::PublicAppComponent);
    let document = |evidence: &LaunchEvidence| {
        evidence
            .reports
            .iter()
            .find(|report| report["phase"] == "page-ready")
            .expect("actual document")["documentNonce"]
            .clone()
    };
    assert_ne!(
        document(&recovered),
        document(&relaunched),
        "healthy relaunch creates a new document"
    );
}

fn run_recovery_launch(fixture: &PublicFsProject, launch: usize) -> LaunchEvidence {
    run_launch(fixture, launch, Exercise::RendererRecovery)
}

fn run(policy: Policy, launches: usize) {
    let fixture = PublicFsProject::new(policy);
    for launch in 0..launches {
        run_launch(&fixture, launch, Exercise::RendererPointer);
    }
}

fn run_launch(fixture: &PublicFsProject, launch: usize, exercise: Exercise) -> LaunchEvidence {
    run_launch_with_descendant_quit(fixture, launch, exercise, None)
}

fn run_launch_with_descendant_quit(
    fixture: &PublicFsProject,
    launch: usize,
    exercise: Exercise,
    mut descendant: Option<&mut descendant_quit::DescendantQuit>,
) -> LaunchEvidence {
    let observer = if matches!(
        exercise,
        Exercise::RendererRecovery | Exercise::RendererPendingRecovery
    ) {
        Observation::bind_recovery()
    } else {
        Observation::bind()
    };
    let prepared = match exercise {
        Exercise::RendererPointer => fixture.prepare(launch, observer.port),
        Exercise::RendererRecovery => fixture.prepare_recovery(launch, observer.port),
        Exercise::RendererPendingRecovery => {
            fixture.prepare_pending_recovery(launch, observer.port)
        }
        Exercise::PublicAppComponent => fixture.prepare_component(launch, observer.port),
        Exercise::PublicAppNaturalCases => fixture.prepare_natural(launch, observer.port),
        Exercise::PublicAppRetirement => fixture.prepare_retirement(launch, observer.port),
        Exercise::RendererNativeJoint => fixture.prepare_native_joint(launch, observer.port),
    };
    let stage = &prepared.stage;
    if exercise != Exercise::RendererPointer {
        assert!(
            matches!(fs::symlink_metadata(&prepared.target), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "component target must be absent before host spawn"
        );
    }
    let mut presentation = NativeWindowObserver::arm_for_title(&fixture.census, TITLE);
    let mut cleanup = spawn_shipping_host(stage);
    let host = cleanup.cli.as_mut().expect("host custody");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain dev lease until host exit");
    let mut evidence = LaunchEvidence::default();
    let observation_result = catch_unwind(AssertUnwindSafe(|| {
        observe_launch(
            &observer,
            &mut cleanup,
            &mut presentation,
            &mut evidence,
            fixture,
            &prepared,
            host_pid,
        );
        if let Some(descendant) = descendant.as_deref_mut() {
            assert_eq!(exercise, Exercise::PublicAppComponent);
            descendant.arm(
                evidence
                    .family
                    .expect("independently attributed product family"),
            );
        }
        eprintln!(
            "KELD_KEL140_PUBLIC_WITNESS policy={:?} launch={launch} host={host_pid} window={:?} identities={:?} policy_sha256={} host_sha256={} reports={:?} exercise={exercise:?}",
            fixture.policy,
            evidence.window,
            evidence.family,
            prepared.digest,
            prepared.host_digest,
            evidence.reports,
        );
    }));
    if observation_result.is_err() {
        // Capture pipes even if the host exited first. Fallback is never success.
        let _ = cleanup.cli.as_mut().expect("retained failed host").kill();
    } else {
        observer
            .quit
            .send(())
            .expect("release public app Quit after observations");
    }
    // Keep the existing launch/group custody and collect host output even when
    // a post-Quit kernel/descendant assertion fails under the production mutation.
    let descendant_result = catch_unwind(AssertUnwindSafe(|| {
        if observation_result.is_ok()
            && let Some(descendant) = descendant.as_deref_mut()
        {
            descendant.observe_cooperative_exit_and_guardian_reap();
        }
    }));
    let output = wait_child_output(
        cleanup.cli.take().expect("capture host before unwind"),
        EVENT_DEADLINE,
    );
    eprintln!(
        "KELD_KEL140_PUBLIC_DIAGNOSTICS policy={:?} launch={launch} click={:?} reports={:?} host_output={output:?}",
        fixture.policy, evidence.click, evidence.reports,
    );
    if let Err(failure) = observation_result {
        resume_unwind(failure);
    }
    if let Err(failure) = descendant_result {
        resume_unwind(failure);
    }
    assert_product_quit(&output, &evidence, stage.root(), host_pid, exercise);
    if exercise == Exercise::PublicAppRetirement {
        retirement::assert_after_quit(&output, &evidence, &prepared.target);
    }
    cleanup.bun_group = None;
    eprintln!(
        "KELD_KEL140_PUBLIC_CLEANUP launch={launch} exact_stage={} family_gone=true window_gone=true fixture_drop_pending=true",
        stage.root().display(),
    );
    evidence
}

fn observe_launch(
    observer: &Observation,
    cleanup: &mut ShippingLaunchCleanup,
    presentation: &mut NativeWindowObserver,
    evidence: &mut LaunchEvidence,
    fixture: &PublicFsProject,
    prepared: &PreparedLaunch,
    host_pid: u32,
) {
    if matches!(
        prepared.exercise,
        Exercise::RendererRecovery | Exercise::RendererPendingRecovery
    ) {
        let boot = await_recovery_boot(observer, &mut evidence.reports);
        observer.send_recovery_command(RecoveryCommand::Admit(boot.pid));
        evidence.recovery_boot = Some(boot);
    }
    await_ready(observer, cleanup, evidence, host_pid, prepared);
    let ready = evidence
        .reports
        .iter()
        .find(|report| report["phase"] == "page-ready")
        .expect("page ready");
    assert_eq!(
        ready["initial"],
        json!({"bridge":true,"frozen":true,"nativeHidden":true})
    );
    evidence.window = presentation.expect_initial(host_pid, "kel140-public-fs");
    assert_eq!(evidence.window.len(), 1);
    match prepared.exercise {
        Exercise::RendererPointer => {
            observe_pointer_actions(observer, evidence, fixture, prepared, host_pid);
        }
        Exercise::RendererRecovery | Exercise::RendererPendingRecovery => {
            observe_recovery_actions(observer, cleanup, evidence, fixture, prepared, host_pid);
        }
        Exercise::RendererNativeJoint => panic!("joint fixture uses its root-controlled producer"),
        Exercise::PublicAppComponent
        | Exercise::PublicAppNaturalCases
        | Exercise::PublicAppRetirement => {
            observe_component_actions(observer, evidence, fixture, prepared, host_pid);
        }
    }
}

fn await_recovery_boot(observer: &Observation, journal: &mut Vec<Value>) -> RecoveryBoot {
    let report = observer
        .reports
        .recv_timeout(EVENT_DEADLINE)
        .expect("real recovery app boot before its public whenReady");
    record_recovery_boot(report, journal)
}

fn record_recovery_boot(mut report: Value, journal: &mut Vec<Value>) -> RecoveryBoot {
    let app_link = report["appLink"]
        .as_str()
        .expect("fixture-owned bootstrap data")
        .to_owned();
    let endpoint = app_link
        .rsplit_once('#')
        .expect("host-minted app bootstrap")
        .0
        .to_owned();
    let boot = RecoveryBoot {
        pid: pid(&report, "pid"),
        app_link,
    };
    // The possession token is only used in the local hostile connection. Do
    // not copy it into diagnostic reports or retained acceptance artifacts.
    report
        .as_object_mut()
        .expect("boot report object")
        .remove("appLink");
    report["endpoint"] = json!(endpoint);
    assert_eq!(report["phase"], "recovery-app-boot", "{report:?}");
    eprintln!("KELD_KEL140_RECOVERY_BOOT {report}");
    journal.push(report);
    boot
}

/// Observe completed G1 work and optional handler hold before the visible crash.
fn observe_recovery_before_loss(
    observer: &Observation,
    evidence: &mut LaunchEvidence,
    fixture: &PublicFsProject,
    prepared: &PreparedLaunch,
    first_bun: u32,
    initial_document: &Value,
) -> Option<std::path::PathBuf> {
    let recovery = prepared.recovery.as_ref().expect("owned recovery targets");
    // Consume the existing completed G1 bridge/public-FS proof before any
    // pending call or crash; Ready publication alone is not the arm receipt.
    observer.send_recovery_command(RecoveryCommand::Invoke(RecoveryPhase::Before));
    let before = await_recovery_page(observer, evidence, RecoveryPhase::Before);
    assert_recovery_phase(
        &before,
        initial_document,
        RecoveryPhase::Before,
        fixture,
        prepared,
    );
    assert_completed_recovery_files(prepared, RecoveryPhase::Before);
    if recovery.pending_handler {
        observer.send_recovery_command(RecoveryCommand::Invoke(RecoveryPhase::Pending));
        let started = observer
            .reports
            .recv_timeout(EVENT_DEADLINE)
            .expect("actual pending renderer request");
        assert_eq!(started["phase"], "recovery-pending-start", "{started}");
        assert_eq!(
            started["requests"]
                .as_array()
                .expect("completed G1 plus pending request")
                .len(),
            3
        );
        assert_eq!(
            started["outcomes"], before["outcomes"],
            "no pending settlement before loss"
        );
        let admitted = observer
            .reports
            .recv_timeout(EVENT_DEADLINE)
            .expect("actual application handler HTTP barrier");
        assert_eq!(admitted["phase"], "recovery-handler-pending", "{admitted}");
        assert_eq!(admitted["barrier"], "application-handler-only");
        assert_eq!(pid(&admitted, "pid"), first_bun);
        assert_eq!(admitted["nonce"], started["requests"][2]["nonce"]);
        assert_completed_recovery_files(prepared, RecoveryPhase::Pending);
        evidence.reports.extend([started, admitted]);
        Some(fixture.replace_recovery_anchor(prepared))
    } else {
        None
    }
}

/// Collect the independent old settlement and successor boot in either order.
fn await_recovery_after_loss(
    observer: &Observation,
    evidence: &mut LaunchEvidence,
    pending_handler: bool,
    first_bun: u32,
) -> (RecoveryBoot, Vec<usize>, usize) {
    if pending_handler {
        // Page settlement and G2 startup are independent observations; either
        // may arrive first. Neither ordinary HTTP record grants process custody.
        let deadline = Instant::now() + EVENT_DEADLINE;
        let mut settled_reports = Vec::new();
        let mut boot = None;
        let mut boot_count = 0;
        while settled_reports.is_empty() || boot.is_none() {
            let report = observer
                .reports
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("old rejection and actual successor boot");
            match report["phase"].as_str() {
                Some("recovery-pending-settled") => {
                    // Preserve the actual report before any terminal synthesis.
                    let index = evidence.reports.len();
                    evidence.reports.push(report);
                    if settled_reports.is_empty() {
                        observer.send_recovery_command(RecoveryCommand::RetirePending(first_bun));
                    }
                    settled_reports.push(index);
                }
                Some("recovery-app-boot") => {
                    let generation_boot = record_recovery_boot(report, &mut evidence.reports);
                    boot_count += 1;
                    if boot.is_none() {
                        boot = Some(generation_boot);
                    }
                }
                _ => panic!("unexpected pending recovery observation: {report}"),
            }
        }
        (
            boot.expect("observed actual successor"),
            settled_reports,
            boot_count,
        )
    } else {
        (
            await_recovery_boot(observer, &mut evidence.reports),
            Vec::new(),
            1,
        )
    }
}

fn observe_recovery_actions(
    observer: &Observation,
    cleanup: &mut ShippingLaunchCleanup,
    evidence: &mut LaunchEvidence,
    fixture: &PublicFsProject,
    prepared: &PreparedLaunch,
    host_pid: u32,
) {
    let first_boot = evidence.recovery_boot.take().expect("G1 fixture boot");
    let first = evidence.family.expect("OS-attributed G1 family");
    assert_eq!(first_boot.pid, first.bun);
    let initial_document = evidence
        .reports
        .iter()
        .find(|report| report["phase"] == "page-ready")
        .expect("actual original document")
        .clone();
    let recovery = prepared.recovery.as_ref().expect("owned recovery targets");
    assert!(matches!(fs::symlink_metadata(&recovery.after), Err(error)
        if error.kind() == std::io::ErrorKind::NotFound));
    assert_eq!(fs::read(&recovery.witness).expect("empty witness"), b"");

    let retired_root = observe_recovery_before_loss(
        observer,
        evidence,
        fixture,
        prepared,
        first.bun,
        &initial_document,
    );
    assert!(evidence.click.is_none(), "recovery has no input action");
    let expected_bun_image = fs::canonicalize(process_image_path(first.bun))
        .expect("independently attributed G1 Bun image");

    // Only this independently attributed admitted Bun PID is killed. The
    // persistent guardian remains the process-family and restart owner.
    if recovery.pending_handler {
        assert!(
            matches!(
                observer.reports.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ),
            "pending renderer settled before owned G1 loss"
        );
    }
    kill_pid(first.bun);
    let (successor_boot, settled_reports, boot_count) =
        await_recovery_after_loss(observer, evidence, recovery.pending_handler, first.bun);
    assert_ne!(first.bun, successor_boot.pid);
    assert_bun_cleanup_custody(
        successor_boot.pid,
        first.guardian,
        host_pid,
        &expected_bun_image,
        prepared.stage.host(),
    );
    // Until every independent census above passes, fallback cleanup retains
    // only the already-attributed G1 group. Ordinary HTTP data grants no custody.
    cleanup.bun_group = Some(successor_boot.pid);
    assert_retired_credentials(&first_boot.app_link, &successor_boot.app_link, prepared);
    observer.send_recovery_command(RecoveryCommand::Admit(successor_boot.pid));
    let ready = observer
        .reports
        .recv_timeout(EVENT_DEADLINE)
        .expect("G2 public whenReady returned after fresh host admission");
    assert_eq!(ready["phase"], "app-ready", "{ready}");
    assert_eq!(pid(&ready, "pid"), successor_boot.pid);
    let successor = ProcessFamily {
        bun: successor_boot.pid,
        descendant: pid(&ready, "descendantPid"),
        guardian: parent_process(successor_boot.pid),
    };
    assert_eq!(first.guardian, successor.guardian, "persistent guardian");
    assert_app_identity(&ready, successor, host_pid, prepared);
    evidence.reports.push(ready);
    evidence.retired_family = Some(first);
    evidence.family = Some(successor);
    await_process_gone(first.bun);
    await_process_gone(first.descendant);
    assert!(process_exists(host_pid) && process_exists(first.guardian));
    assert_eq!(
        await_same_native_windows(host_pid, TITLE, &evidence.window),
        evidence.window,
        "actual Bun loss replaced the original native window"
    );

    assert_collected_recovery_terminals(evidence, &initial_document, &settled_reports, boot_count);

    observer.send_recovery_command(RecoveryCommand::Invoke(RecoveryPhase::After));
    let after = await_recovery_page(observer, evidence, RecoveryPhase::After);
    assert_recovery_phase(
        &after,
        &initial_document,
        RecoveryPhase::After,
        fixture,
        prepared,
    );
    assert_recovery_replacement_files(prepared, retired_root.as_deref());
    assert_eq!(
        await_same_native_windows(host_pid, TITLE, &evidence.window),
        evidence.window
    );
    assert_eq!(
        evidence
            .reports
            .iter()
            .filter(|report| report["phase"] == "page-ready")
            .count(),
        1,
        "same document; no second initial-page observation"
    );
    observer.send_recovery_command(RecoveryCommand::Quit(successor.bun));
    eprintln!(
        "KELD_KEL140_RECOVERY_CONSUMER host={host_pid} window={:?} first_bun={} second_bun={} input_actions=0 completed_witness=[1,2] interrupted_effect_acceptance=false blocked_syscall_acceptance=false full_kel143_acceptance=false",
        evidence.window, first.bun, successor.bun,
    );
}

/// Independent old/new-root effects remain separate from reported G2 success.
fn assert_recovery_replacement_files(prepared: &PreparedLaunch, retired_root: Option<&Path>) {
    let recovery = prepared.recovery.as_ref().expect("owned recovery targets");
    if let Some(retired) = retired_root {
        assert_eq!(
            fs::read(retired.join(recovery.witness.file_name().expect("old witness leaf")))
                .expect("old-root independent replay witness"),
            [1],
            "G2 reused or replayed the old root"
        );
        assert_eq!(
            fs::read(retired.join(prepared.target.file_name().expect("old target leaf")))
                .expect("old-root target after G2"),
            CONTENT
        );
        assert!(
            matches!(fs::symlink_metadata(&prepared.target), Err(error)
            if error.kind() == std::io::ErrorKind::NotFound),
            "G2 replayed the original target"
        );
    }
    assert_completed_recovery_files(prepared, RecoveryPhase::After);
}

fn assert_completed_recovery_files(prepared: &PreparedLaunch, phase: RecoveryPhase) {
    let recovery = prepared.recovery.as_ref().expect("owned recovery targets");
    let target_message = match phase {
        RecoveryPhase::Before | RecoveryPhase::Pending => "completed G1 bytes",
        RecoveryPhase::After => "retained G1 bytes",
    };
    if !recovery.pending_handler || !matches!(phase, RecoveryPhase::After) {
        assert_eq!(fs::read(&prepared.target).expect(target_message), CONTENT);
    }
    match phase {
        RecoveryPhase::Before | RecoveryPhase::Pending => {
            assert_eq!(fs::read(&recovery.witness).expect("G1 witness"), [1]);
            assert!(matches!(fs::symlink_metadata(&recovery.after), Err(error)
                if error.kind() == std::io::ErrorKind::NotFound));
        }
        RecoveryPhase::After => {
            assert_eq!(
                fs::read(&recovery.after).expect("independent G2 bytes"),
                RECOVERY_AFTER_CONTENT
            );
            assert_eq!(fs::read(&recovery.witness).expect("G1/G2 witness"), [1, 2]);
        }
    }
}

fn assert_bun_cleanup_custody(
    candidate: u32,
    guardian: u32,
    host: u32,
    expected_bun_image: &Path,
    expected_host_image: &Path,
) {
    let expected_host_image =
        fs::canonicalize(expected_host_image).expect("owned staged host image");
    for owned in [host, guardian] {
        assert!(
            process_exists(owned),
            "owned host/guardian must remain live"
        );
        assert_eq!(
            fs::canonicalize(process_image_path(owned)).expect("kernel host/guardian image"),
            expected_host_image,
            "owned host/guardian image must remain unchanged"
        );
    }
    assert_eq!(parent_process(guardian), host, "owned guardian parent");
    assert_eq!(
        parent_process(candidate),
        guardian,
        "candidate must be the owned guardian's direct Bun child"
    );
    assert_eq!(process_group(candidate), candidate, "candidate Bun group");
    assert_eq!(
        fs::canonicalize(process_image_path(candidate)).expect("kernel candidate Bun image"),
        expected_bun_image,
        "candidate must run the independently observed Bun image"
    );
}

fn assert_retired_credentials(old_link: &str, successor_link: &str, prepared: &PreparedLaunch) {
    // One canonical hostile probe, shared with the Linux GH-1081 experiment.
    const PROBE: &str = include_str!("../../fixtures/retired_token_probe.mjs");
    let (old_endpoint, old_token) = old_link.rsplit_once('#').expect("G1 bootstrap");
    let (endpoint, token) = successor_link.rsplit_once('#').expect("G2 bootstrap");
    assert_ne!(old_endpoint, endpoint, "fresh endpoint");
    assert!(old_token != token, "fresh possession token");
    assert!(
        UnixStream::connect(old_endpoint).is_err(),
        "retired endpoint accepts a reconnect"
    );
    // A separate hostile fixture consumes the canonical staged wire helpers.
    // It is not an application adapter or a second link in the public app.
    let probe = Command::new("bun")
        .args(["-e", PROBE])
        .env(
            "KELD_RECOVERY_PROBE_TRANSPORT",
            prepared.stage.root().join("src/kipc-transport.ts"),
        )
        .env("KELD_RECOVERY_PROBE_OLD", old_link)
        .env("KELD_RECOVERY_PROBE_FRESH", successor_link)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("isolated canonical stale-token probe");
    let output = wait_child_output(probe, EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "server-side stale-token refusal failed: {output:?}"
    );
    assert_eq!(output.stdout, b"RETIRED_TOKEN_SERVER_EOF_NO_REPLY\n");
}

fn await_recovery_page(
    observer: &Observation,
    evidence: &mut LaunchEvidence,
    phase: RecoveryPhase,
) -> Value {
    let report = observer
        .reports
        .recv_timeout(EVENT_DEADLINE)
        .expect("actual recovery renderer outcome");
    assert_eq!(
        report["phase"],
        format!("recovery-{}", phase.as_str()),
        "{report}"
    );
    eprintln!("KELD_KEL140_RECOVERY_PAGE {report}");
    evidence.reports.push(report.clone());
    report
}

/// Synthesize only after actual reports and fresh live-owner observations exist.
fn assert_collected_recovery_terminals(
    evidence: &LaunchEvidence,
    initial_document: &Value,
    settled_reports: &[usize],
    boot_count: usize,
) {
    assert_eq!(boot_count, 1, "one actual successor boot");
    if !settled_reports.is_empty() {
        assert_eq!(settled_reports.len(), 1, "one pending settlement report");
        assert_pending_recovery_rejection(&evidence.reports[settled_reports[0]], initial_document);
    }
}

fn assert_pending_recovery_rejection(report: &Value, initial_document: &Value) {
    for key in ["documentNonce", "javascriptToken", "initial"] {
        assert_eq!(
            report[key], initial_document[key],
            "same pending renderer {key}"
        );
    }
    let request = &report["requests"][2];
    assert_eq!(request["action"], "recovery-pending");
    assert_eq!(
        report["outcomes"][2],
        json!({"action":"recovery-pending","name":"Error",
        "error":"KELD-WV-011: application link ended before the renderer reply","code":null})
    );
    let terminals = report["terminals"]
        .as_array()
        .expect("passive real bridge terminals");
    let old: Vec<_> = terminals
        .iter()
        .filter(|terminal| terminal["nonce"] == request["nonce"])
        .collect();
    assert_eq!(
        old,
        [
            &json!({"nonce":request["nonce"],"ok":false,"code":"KELD-WV-011",
        "detail":"application link ended before the renderer reply"})
        ]
    );
    assert_eq!(
        terminals.len(),
        report["outcomes"]
            .as_array()
            .expect("actual settlements")
            .len(),
        "one bridge terminal for each actual Promise settlement"
    );
}

fn assert_recovery_phase(
    report: &Value,
    initial_document: &Value,
    phase: RecoveryPhase,
    fixture: &PublicFsProject,
    prepared: &PreparedLaunch,
) {
    for key in ["documentNonce", "javascriptToken", "initial"] {
        assert_eq!(initial_document[key], report[key], "same renderer {key}");
    }
    for key in ["documentNonce", "javascriptToken"] {
        let value = report[key].as_str().expect("renderer-created identity");
        assert_eq!(value.len(), 32);
        assert!(value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    assert_eq!(report["automaticRecovery"], true);
    let pending = prepared
        .recovery
        .as_ref()
        .expect("recovery inputs")
        .pending_handler;
    let count = match phase {
        RecoveryPhase::Before => 2,
        RecoveryPhase::After if pending => 5,
        RecoveryPhase::After => 4,
        RecoveryPhase::Pending => panic!("pending admission uses its separate observation"),
    };
    let requests = report["requests"]
        .as_array()
        .expect("actual renderer requests");
    let outcomes = report["outcomes"]
        .as_array()
        .expect("actual renderer outcomes");
    assert_eq!(requests.len(), count);
    assert_eq!(outcomes.len(), count);
    let mut nonces = std::collections::BTreeSet::new();
    if pending && matches!(phase, RecoveryPhase::After) {
        assert_pending_recovery_rejection(report, initial_document);
    }
    let actions: &[&str] = if pending {
        &[
            "recovery-before",
            "deny",
            "recovery-pending",
            "recovery-after",
            "deny",
        ]
    } else {
        &["recovery-before", "deny", "recovery-after", "deny"]
    };
    for (index, action) in actions.iter().take(count).enumerate() {
        assert_eq!(requests[index]["action"], *action);
        assert_eq!(
            requests[index]["count"],
            if pending {
                if index < 3 { index + 1 } else { index - 2 }
            } else {
                index % 2 + 1
            }
        );
        let nonce = requests[index]["nonce"]
            .as_str()
            .expect("renderer-created request nonce");
        assert_eq!(nonce.len(), 32);
        assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(nonces.insert(nonce), "each completed call is distinct");
        assert_eq!(outcomes[index]["action"], *action);
        if pending && index == 2 {
            continue;
        }
        let allowed = if pending {
            index == 0 || index == 3
        } else {
            index % 2 == 0
        };
        let expected = if allowed {
            json!({"ok":true,"bytes":if index == 0 { CONTENT } else { RECOVERY_AFTER_CONTENT }})
        } else {
            json!({"ok":false,"code":"KELD-GUARD002","message":format!(
                "KELD-GUARD002: capability `fs.write` denied by scope `{}`. Widen `/app/fs/write` in keld.permissions.jsonc so it includes `{}`.",
                fixture.scope, fixture.outside.display())})
        };
        assert_eq!(outcomes[index]["outcome"], expected);
    }
    assert_eq!(
        fs::read(&fixture.outside).expect("independent outside sentinel"),
        SENTINEL
    );
    assert!(
        prepared.recovery.is_some(),
        "ordinary staged recovery fixture"
    );
}

fn observe_component_actions(
    observer: &Observation,
    evidence: &mut LaunchEvidence,
    fixture: &PublicFsProject,
    prepared: &PreparedLaunch,
    host_pid: u32,
) {
    if !evidence
        .reports
        .iter()
        .any(|report| report["phase"] == "app-result")
    {
        let result = observer
            .reports
            .recv_timeout(EVENT_DEADLINE)
            .expect("actual public-main FS result");
        eprintln!("KELD_KEL140_OBSERVATION {result}");
        evidence.reports.push(result);
    }
    assert!(
        evidence.click.is_none(),
        "component must issue no synthetic input"
    );
    let bun = evidence.family.expect("observed public app family").bun;
    if prepared.exercise == Exercise::PublicAppNaturalCases {
        assert_natural_outcomes(
            &evidence.reports,
            fixture.policy,
            prepared,
            &fixture.outside,
            bun,
        );
    } else {
        assert_component_outcomes(
            &evidence.reports,
            fixture.policy,
            &prepared.target,
            &fixture.outside,
            &fixture.scope,
            bun,
        );
    }
    assert_eq!(
        await_same_native_windows(host_pid, TITLE, &evidence.window),
        evidence.window,
        "public-main filesystem work replaced the actual native window"
    );
}

fn observe_pointer_actions(
    observer: &Observation,
    evidence: &mut LaunchEvidence,
    fixture: &PublicFsProject,
    prepared: &PreparedLaunch,
    host_pid: u32,
) {
    assert!(
        matches!(fs::symlink_metadata(&prepared.target), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "target exists before the real pointer action"
    );
    let input_script = preclick::composed_click_script();
    evidence.click = Some(
        Command::new("/usr/bin/xcrun")
            .args(["swift", "-e", &input_script, &host_pid.to_string(), TITLE])
            .output()
            .expect("real CoreGraphics pointer action"),
    );
    assert!(
        evidence
            .click
            .as_ref()
            .expect("retained click result")
            .status
            .success(),
        "{:?}",
        evidence.click
    );
    observer.await_page_result(&mut evidence.reports);
    assert_page_outcomes(
        &evidence.reports,
        fixture.policy,
        &prepared.target,
        &fixture.outside,
        &fixture.scope,
    );
}

fn await_ready(
    observer: &Observation,
    cleanup: &mut ShippingLaunchCleanup,
    evidence: &mut LaunchEvidence,
    host_pid: u32,
    prepared: &PreparedLaunch,
) {
    let exercise = prepared.exercise;
    let deadline = Instant::now() + EVENT_DEADLINE;
    while !evidence
        .reports
        .iter()
        .any(|report| report["phase"] == "app-ready")
        || !evidence
            .reports
            .iter()
            .any(|report| report["phase"] == "page-ready")
    {
        let report = observer
            .reports
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("app Ready and installed page listener observations");
        eprintln!("KELD_KEL140_OBSERVATION {report}");
        // Retain the observation before any identity assertion can unwind.
        evidence.reports.push(report.clone());
        if report["phase"] == "app-ready" {
            let bun = pid(&report, "pid");
            let descendant = pid(&report, "descendantPid");
            let guardian = parent_process(bun);
            let family = ProcessFamily {
                bun,
                descendant,
                guardian,
            };
            assert_app_identity(&report, family, host_pid, prepared);
            // Ordinary app data gains fallback custody only after the
            // independent live host/guardian/group/image/link census passes.
            cleanup.bun_group = Some(bun);
            evidence.family = Some(family);
        }
        assert!(
            report["phase"] == "app-ready"
                || report["phase"] == "page-ready"
                || (exercise != Exercise::RendererPointer && report["phase"] == "app-result"),
            "{report}"
        );
    }
}

fn assert_app_identity(
    report: &Value,
    family: ProcessFamily,
    host_pid: u32,
    prepared: &PreparedLaunch,
) {
    let ProcessFamily {
        bun,
        descendant,
        guardian,
    } = family;
    assert_bun_cleanup_custody(
        bun,
        guardian,
        host_pid,
        &prepared.bun_image,
        prepared.stage.host(),
    );
    assert_eq!(parent_process(guardian), host_pid);
    assert_eq!(parent_process(descendant), bun);
    assert_eq!(process_group(bun), bun);
    assert_eq!(process_group(descendant), bun);
    let executable = Command::new("/bin/ps")
        .args(["-p", &bun.to_string(), "-o", "comm="])
        .output()
        .expect("process display diagnostic");
    eprintln!("KELD_KEL140_PROCESS_DISPLAY pid={bun} ps_output={executable:?}");
    let image = process_image_path(bun);
    eprintln!(
        "KELD_KEL140_PROCESS_IMAGE_MATCH pid={bun} kernel_image={} reported_image={}",
        image.display(),
        report["executable"]
    );
    assert_eq!(
        fs::canonicalize(&image).expect("kernel Bun image"),
        fs::canonicalize(report["executable"].as_str().expect("app executable"))
            .expect("reported Bun image")
    );
    assert_single_application_link(
        host_pid,
        bun,
        report["endpoint"].as_str().expect("endpoint"),
    );
}

fn assert_product_quit(
    output: &Output,
    evidence: &LaunchEvidence,
    stage_root: &Path,
    host_pid: u32,
    exercise: Exercise,
) {
    assert!(output.status.success(), "{output:?}");
    let stderr = std::str::from_utf8(&output.stderr).expect("host diagnostic UTF-8");
    let expected_calls = match exercise {
        Exercise::RendererPointer | Exercise::RendererNativeJoint => 3,
        Exercise::RendererRecovery => 4,
        Exercise::RendererPendingRecovery => 5,
        Exercise::PublicAppComponent
        | Exercise::PublicAppNaturalCases
        | Exercise::PublicAppRetirement => 0,
    };
    for marker in [
        "KELD_KEL142_RENDERER_ADMIT webview=",
        "KELD_KEL142_KIPC_CALL webview=",
        "KELD_KEL140_PUBLIC_HANDLER ",
    ] {
        assert_eq!(stderr.matches(marker).count(), expected_calls, "{stderr}");
    }
    assert!(stderr.contains("KELD_KEL142_BIND webview="), "{stderr}");
    let quit_calls = if matches!(
        exercise,
        Exercise::RendererRecovery
            | Exercise::RendererPendingRecovery
            | Exercise::RendererNativeJoint
    ) {
        2
    } else {
        expected_calls
    };
    assert!(
        stderr.contains(&format!(
            "KELD_KEL140_PUBLIC_QUIT_REQUEST calls={quit_calls}"
        )),
        "{stderr}"
    );
    let ProcessFamily {
        bun,
        descendant,
        guardian,
    } = evidence.family.expect("OS-attributed application family");
    match exercise {
        Exercise::RendererPointer => assert_handler_attribution(
            stderr,
            evidence.reports.last().expect("final page report"),
            bun,
        ),
        Exercise::RendererRecovery | Exercise::RendererPendingRecovery => {
            assert_recovery_trace(stderr, evidence);
        }
        Exercise::PublicAppComponent
        | Exercise::PublicAppNaturalCases
        | Exercise::PublicAppRetirement => {
            assert_component_trace(stderr, &evidence.reports);
        }
        Exercise::RendererNativeJoint => panic!("joint fixture uses its root-controlled producer"),
    }
    if let Some(retired) = evidence.retired_family {
        await_process_gone(retired.bun);
        await_process_gone(retired.descendant);
    }
    for process in [host_pid, bun, descendant, guardian] {
        await_process_gone(process);
    }
    assert_eq!(native_windows(host_pid, TITLE).len(), 0);
    assert!(
        matches!(fs::symlink_metadata(stage_root), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "product Quit retained exact stage before fixture destruction: {}",
        stage_root.display()
    );
}

fn assert_recovery_trace(stderr: &str, evidence: &LaunchEvidence) {
    let first = evidence.retired_family.expect("retired G1 identity");
    let successor = evidence.family.expect("fresh G2 identity");
    let result = evidence
        .reports
        .iter()
        .find(|report| report["phase"] == "recovery-after")
        .expect("same document's completed G2 calls");
    let handlers: Vec<Value> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("KELD_KEL140_PUBLIC_HANDLER "))
        .map(|line| serde_json::from_str(line).expect("actual recovery handler trace"))
        .collect();
    let outcomes: Vec<Value> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("KELD_KEL140_PUBLIC_OUTCOME "))
        .map(|line| serde_json::from_str(line).expect("actual recovery public FS trace"))
        .collect();
    let pending = result["requests"][2]["action"] == "recovery-pending";
    assert_eq!(
        handlers.len(),
        if pending { 5 } else { 4 },
        "one invocation per request"
    );
    assert_eq!(
        outcomes.len(),
        4,
        "no returned old pending handler or replay"
    );
    for (index, handler) in handlers.iter().enumerate() {
        let first_generation = if pending { index < 3 } else { index < 2 };
        assert_eq!(
            handler,
            &json!({"call":result["requests"][index]["count"],
            "pid":if first_generation { first.bun } else { successor.bun },
            "action":result["requests"][index]["action"],
            "nonce":result["requests"][index]["nonce"]})
        );
    }
    for (index, outcome) in outcomes.iter().enumerate() {
        let request = if pending && index >= 2 {
            index + 1
        } else {
            index
        };
        assert_eq!(
            outcome,
            &json!({"call":result["requests"][request]["count"],
            "nonce":result["requests"][request]["nonce"],
            "outcome":result["outcomes"][request]["outcome"]})
        );
    }
}

fn pid(report: &Value, key: &str) -> u32 {
    u32::try_from(report[key].as_u64().expect("reported PID")).expect("OS PID domain")
}

const JOINT_REPORT_PHASES: [&str; 6] = [
    "joint-page-ready",
    "joint-pending-start",
    "joint-pending-settled",
    "joint-final",
    "joint-native-expiry",
    "joint-g2-released",
];

struct JointReportJournal {
    path: std::path::PathBuf,
    file: fs::File,
    app_ready: Vec<u32>,
    phases: std::collections::BTreeSet<String>,
    records: usize,
    bytes: usize,
}

impl JointReportJournal {
    fn create(path: std::path::PathBuf) -> Self {
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("fresh empty producer-owned joint report");
        file.flush().expect("publish empty report object");
        Self {
            path,
            file,
            app_ready: Vec::new(),
            phases: std::collections::BTreeSet::new(),
            records: 0,
            bytes: 0,
        }
    }

    fn append(&mut self, report: &Value) -> String {
        use std::io::Write as _;
        let phase = report["phase"].as_str().expect("named joint report phase");
        if phase == "app-ready" {
            let app_pid = pid(report, "pid");
            assert!(
                !self.app_ready.contains(&app_pid),
                "one Ready per distinct Bun generation"
            );
            self.app_ready.push(app_pid);
            assert!(self.app_ready.len() <= 2, "exactly G1/G2 app Ready rows");
        } else {
            assert!(
                JOINT_REPORT_PHASES.contains(&phase),
                "unexpected joint report: {report}"
            );
            assert!(
                self.phases.insert(phase.to_owned()),
                "duplicate joint report phase {phase}"
            );
        }
        let mut line = serde_json::to_vec(report).expect("actual JSON report bytes");
        line.push(b'\n');
        assert!(
            line.len() <= 4096,
            "joint record exceeds 4096 bytes: {phase}"
        );
        self.bytes = self
            .bytes
            .checked_add(line.len())
            .expect("bounded report bytes");
        self.records += 1;
        assert!(
            self.records <= 8 && self.bytes <= 32768,
            "joint report budget exceeded"
        );
        self.file
            .write_all(&line)
            .expect("append actual joint observation");
        self.file.flush().expect("publish each actual observation");
        phase.to_owned()
    }

    fn assert_complete(&self) {
        assert_eq!(self.records, 8, "exact eight-row joint journal");
        assert_eq!(
            self.app_ready.len(),
            2,
            "actual G1 and G2 whenReady observations"
        );
        assert_ne!(
            self.app_ready[0], self.app_ready[1],
            "G2 uses a fresh Bun PID"
        );
        assert_eq!(
            self.phases.len(),
            6,
            "all page/native/Core facts exactly once"
        );
        for phase in JOINT_REPORT_PHASES {
            assert!(
                self.phases.contains(phase),
                "missing required joint observation {phase}"
            );
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn records(&self) -> usize {
        self.records
    }

    fn bytes(&self) -> usize {
        self.bytes
    }

    fn g2_pid(&self) -> u32 {
        self.app_ready[1]
    }

    fn assert_no_late_report(&mut self, observation: &mut Observation) {
        use std::io::Write as _;
        self.file.flush().expect("final joint report flush");
        observation.stop_and_join();
        let late_report = observation.reports.try_recv();
        assert!(
            matches!(
                late_report,
                Err(std::sync::mpsc::TryRecvError::Empty
                    | std::sync::mpsc::TryRecvError::Disconnected)
            ),
            "HTTP observer queued a report beyond the eight-row journal: {late_report:?}"
        );
    }
}

struct JointFixtureOwner {
    _fixture: PublicFsProject,
    observation: Observation,
    prepared: PreparedLaunch,
    journal: JointReportJournal,
}

fn prepare_joint_fixture(receipt_path: &Path) -> JointFixtureOwner {
    use std::io::Write as _;
    let fixture = PublicFsProject::new(Policy::Narrow);
    let observation = Observation::bind_recovery();
    let prepared = fixture.prepare_native_joint(0, observation.port);
    assert_eq!(prepared.exercise, Exercise::RendererNativeJoint);
    assert!(prepared.recovery.is_none() && prepared.natural.is_none());
    fs::write(&prepared.target, b"unchanged").expect("seed retained Native target");
    let journal = JointReportJournal::create(fixture.owned_root().join("joint-observations.jsonl"));
    let mut receipt = staged_fixture_receipt(&prepared, observation.port, &fixture);
    receipt["jointRenderer"] = json!({
        "schema":"keld.kel143.joint-native-renderer/v1",
        "pageIdentity":staged_fixture_identity(&prepared.stage.root().join("index.html")),
        "reportIdentity":staged_fixture_identity(journal.path()),
        "censusIdentity":staged_fixture_identity(&fixture.census),
        "observerPort":observation.port,
        "title":TITLE,
    });
    let mut receipt_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(receipt_path)
        .expect("fresh immutable fixture receipt");
    receipt_file
        .write_all(&serde_json::to_vec_pretty(&receipt).expect("fixture receipt JSON"))
        .expect("write source-bound fixture receipt");
    receipt_file
        .flush()
        .expect("publish completed fixture receipt");
    drop(receipt_file);
    println!(
        "KELD_KEL143_JOINT_FIXTURE_READY receipt={}",
        receipt_path.display()
    );
    JointFixtureOwner {
        _fixture: fixture,
        observation,
        prepared,
        journal,
    }
}

/// Root runs this owner alongside the real Core/Runtime and renderer consumer.
/// It stages inputs and observes reports only; it starts no application process.
#[test]
#[ignore = "Root-controlled joint Native/renderer producer; stdin finish after consumer DONE"]
fn kel143_joint_native_renderer_fixture_producer() {
    use std::io::BufRead as _;
    let receipt_path = std::path::PathBuf::from(
        std::env::var("KELD_KEL143_FIXTURE_RECEIPT").expect("absolute Root-owned receipt path"),
    );
    assert!(
        receipt_path.is_absolute(),
        "fixture receipt must be absolute"
    );
    let JointFixtureOwner {
        _fixture,
        mut observation,
        prepared,
        mut journal,
    } = prepare_joint_fixture(&receipt_path);
    let mut quit_pending = false;
    let mut quit_sent = false;
    while journal.records() < 8 {
        let report = observation
            .reports
            .recv_timeout(EVENT_DEADLINE)
            .expect("bounded joint Host/Core/page observation");
        let phase = journal.append(&report);
        if phase == "joint-g2-released" {
            observation.send_recovery_command(RecoveryCommand::Invoke(RecoveryPhase::After));
        }
        if phase == "joint-final" {
            quit_pending = true;
        }
        if quit_pending && !quit_sent && journal.app_ready.len() == 2 {
            observation.send_recovery_command(RecoveryCommand::Quit(journal.g2_pid()));
            quit_sent = true;
        }
    }
    assert!(
        quit_sent,
        "joint-final released only the actual G2 app Quit"
    );
    journal.assert_complete();
    let mut command = String::new();
    assert_ne!(
        std::io::stdin()
            .lock()
            .read_line(&mut command)
            .expect("Root finish control"),
        0,
        "producer stdin closed before actual consumer completion"
    );
    assert_eq!(
        command.trim(),
        "finish",
        "only Root consumer-finished control"
    );
    assert!(
        matches!(fs::symlink_metadata(prepared.stage.root()), Err(error)
            if error.kind() == std::io::ErrorKind::NotFound),
        "Core consumer must remove the exact stage before producer finish"
    );
    journal.assert_no_late_report(&mut observation);
    println!(
        "KELD_KEL143_JOINT_FIXTURE_DONE records={} bytes={}",
        journal.records(),
        journal.bytes()
    );
}

/// Root runs this owner alongside a Core/Runtime consumer; it starts no app.
#[test]
#[ignore = "Root-controlled staged public-FS fixture service; stdin finish after consumer DONE"]
fn kel143_staged_public_fs_fixture_producer() {
    use std::io::{BufRead as _, Write as _};
    let receipt_path = std::path::PathBuf::from(
        std::env::var("KELD_KEL143_FIXTURE_RECEIPT").expect("absolute Root-owned receipt path"),
    );
    assert!(
        receipt_path.is_absolute(),
        "fixture receipt must be absolute"
    );
    let fixture = PublicFsProject::new(Policy::Narrow);
    let observation = Observation::bind_staged_fixture();
    let prepared = fixture.prepare(0, observation.port);
    assert_eq!(prepared.exercise, Exercise::RendererPointer);
    assert!(prepared.recovery.is_none() && prepared.natural.is_none());
    // Existing regular object gives Core's Native Hold an actual target handle.
    // Distinct bytes make a no-op write differ from expectedContent.
    fs::write(&prepared.target, b"unchanged").expect("seed owned retained-file target");
    let receipt = staged_fixture_receipt(&prepared, observation.port, &fixture);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&receipt_path)
        .expect("fresh immutable fixture receipt");
    file.write_all(&serde_json::to_vec_pretty(&receipt).expect("fixture receipt JSON"))
        .expect("write source-bound fixture receipt");
    file.flush().expect("publish completed fixture receipt");
    drop(file);
    println!(
        "KELD_KEL143_STAGED_FIXTURE_READY receipt={}",
        receipt_path.display()
    );
    let mut command = String::new();
    assert_ne!(
        std::io::stdin()
            .lock()
            .read_line(&mut command)
            .expect("Root finish control"),
        0,
        "producer stdin closed before actual consumer completion"
    );
    assert_eq!(
        command.trim(),
        "finish",
        "only Root consumer-finished control"
    );
    // Consumer Q/EOF owns normal removal; producer Drop cannot supply proof.
    assert!(
        matches!(fs::symlink_metadata(prepared.stage.root()), Err(error)
        if error.kind() == std::io::ErrorKind::NotFound),
        "consumer retained stage before producer fallback"
    );
    while let Ok(report) = observation.reports.try_recv() {
        eprintln!("KELD_KEL143_STAGED_FIXTURE_OBSERVATION {report}");
    }
    drop(observation);
    println!(
        "KELD_KEL143_STAGED_FIXTURE_DONE receipt={}",
        receipt_path.display()
    );
}

fn staged_fixture_receipt(
    prepared: &PreparedLaunch,
    observer_port: u16,
    fixture: &PublicFsProject,
) -> Value {
    use std::io::Write as _;
    let captured_sources: [(&str, &[u8]); 6] = [
        ("hostFixture", include_bytes!("public_fs.rs")),
        ("hostObserver", include_bytes!("public_fs/observer.rs")),
        (
            "core",
            include_bytes!("../../../../keld-core/src/app_session.rs"),
        ),
        (
            "runtimeGuardian",
            include_bytes!("../../../../keld-runtime/src/macos_guardian.rs"),
        ),
        (
            "runtimeLibrary",
            include_bytes!("../../../../keld-runtime/src/lib.rs"),
        ),
        (
            "nativeFs",
            include_bytes!("../../../../keld-native/src/fs.rs"),
        ),
    ];
    let mut compiled_source_hashes = serde_json::Map::new();
    for (key, bytes) in captured_sources {
        let mut captured_file = tempfile::NamedTempFile::new_in(fixture.owned_root())
            .expect("owned compile-time source bytes");
        captured_file
            .write_all(bytes)
            .expect("materialize captured source bytes");
        captured_file
            .flush()
            .expect("publish captured source bytes");
        compiled_source_hashes.insert(
            key.to_owned(),
            json!(project::file_digest(captured_file.path())),
        );
    }
    let stage = &prepared.stage;
    let allowed = prepared.target.parent().expect("owned allowed root");
    json!({
        "schema":"keld.kel143.staged-public-fs-fixture/v1",
        "producerPid":std::process::id(),
        "stagedHost":stage.host(), "stageRoot":stage.root(),
        "entryRelative":"src/main.ts", "allowedRoot":allowed, "target":prepared.target,
        "expectedContent":CONTENT, "initialContent":b"unchanged", "observerPort":observer_port,
        "outside":fixture.outside,
        "outsideContent":fs::read(&fixture.outside).expect("captured existing outside sentinel bytes"),
        "outsideIdentity":staged_fixture_identity(&fixture.outside),
        "application":{"automaticComponent":false,"recoveryInputs":null,"handlerOnlyHold":false},
        "stageIdentity":staged_fixture_identity(stage.root()),
        "allowedRootIdentity":staged_fixture_identity(allowed),
        "targetIdentity":staged_fixture_identity(&prepared.target),
        "permissionsIdentity":staged_fixture_identity(&stage.root().join("keld.permissions.jsonc")),
        "permissionsSha256":prepared.digest,
        "entryIdentity":staged_fixture_identity(&stage.root().join("src/main.ts")),
        "entrySha256":project::file_digest(&stage.root().join("src/main.ts")),
        "transportIdentity":staged_fixture_identity(&stage.root().join("src/kipc-transport.ts")),
        "stagedHostIdentity":staged_fixture_identity(stage.host()), "stagedHostSha256":prepared.host_digest,
        "compiledSourceHashes":compiled_source_hashes,
        "lifetime":"Root keeps producer alive; consumer performs Q then EOF/shutdown/DONE; stdin finish last",
        "claims":"ordinary staged data only; no app spawn/Ready/arm/Q/native action by producer"
    })
}

fn staged_fixture_identity(path: &Path) -> Value {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = fs::symlink_metadata(path).expect("owned fixture object identity");
    assert!(
        metadata.is_file() || metadata.is_dir(),
        "fixture object must be regular/directory"
    );
    json!({"path":path,"dev":metadata.dev(),"ino":metadata.ino(),
        "kind":if metadata.is_file() { "file" } else { "directory" },
        "sha256":metadata.is_file().then(|| project::file_digest(path))})
}

fn spawn_shipping_host(stage: &keld_cli::boot::DevBootStage) -> ShippingLaunchCleanup {
    let child = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_KEL142_ACCEPTANCE_REPORT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch staged shipping no-flag host");
    ShippingLaunchCleanup::new(child)
}
