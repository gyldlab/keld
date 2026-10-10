//! KEL-140 stock producer/public API proof. Native execution belongs to ROOT.
//! No authority boundary change; no pending native-effect or cancellation claim.
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::{
    NativeWindowObserver, await_same_native_windows, native_windows,
};
use crate::support::process::{await_process_gone, parent_process, process_group};
use serde_json::{Value, json};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Instant;

#[path = "public_fs/application_link.rs"]
mod application_link;
#[path = "public_fs/component.rs"]
mod component;
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
use application_link::assert_single_application_link;
use observer::Observation;
use outcomes::{
    assert_component_outcomes, assert_component_trace, assert_handler_attribution,
    assert_page_outcomes,
};
use process_image::process_image_path;
use project::{Exercise, Policy, PreparedLaunch, PublicFsProject, TITLE};

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

fn run(policy: Policy, launches: usize) {
    let fixture = PublicFsProject::new(policy);
    for launch in 0..launches {
        run_launch(&fixture, launch, Exercise::RendererPointer);
    }
}

fn run_launch(fixture: &PublicFsProject, launch: usize, exercise: Exercise) -> LaunchEvidence {
    let observer = Observation::bind();
    let prepared = match exercise {
        Exercise::RendererPointer => fixture.prepare(launch, observer.port),
        Exercise::PublicAppComponent => fixture.prepare_component(launch, observer.port),
    };
    let stage = &prepared.stage;
    if exercise == Exercise::PublicAppComponent {
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
    assert_product_quit(&output, &evidence, stage.root(), host_pid, exercise);
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
    await_ready(observer, cleanup, evidence, host_pid, prepared.exercise);
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
        Exercise::PublicAppComponent => {
            observe_component_actions(observer, evidence, fixture, prepared, host_pid);
        }
    }
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
    assert_component_outcomes(
        &evidence.reports,
        fixture.policy,
        &prepared.target,
        &fixture.outside,
        &fixture.scope,
        evidence.family.expect("observed public app family").bun,
    );
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
    exercise: Exercise,
) {
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
            cleanup.bun_group = Some(bun);
            evidence.family = Some(ProcessFamily {
                bun,
                descendant,
                guardian,
            });
            assert_app_identity(
                &report,
                evidence.family.expect("observed app family"),
                host_pid,
            );
        }
        assert!(
            report["phase"] == "app-ready"
                || report["phase"] == "page-ready"
                || (exercise == Exercise::PublicAppComponent && report["phase"] == "app-result"),
            "{report}"
        );
    }
}

fn assert_app_identity(report: &Value, family: ProcessFamily, host_pid: u32) {
    let ProcessFamily {
        bun,
        descendant,
        guardian,
    } = family;
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
    let expected_calls = if exercise == Exercise::RendererPointer {
        3
    } else {
        0
    };
    for marker in [
        "KELD_KEL142_RENDERER_ADMIT webview=",
        "KELD_KEL142_KIPC_CALL webview=",
        "KELD_KEL140_PUBLIC_HANDLER ",
    ] {
        assert_eq!(stderr.matches(marker).count(), expected_calls, "{stderr}");
    }
    assert!(stderr.contains("KELD_KEL142_BIND webview="), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "KELD_KEL140_PUBLIC_QUIT_REQUEST calls={expected_calls}"
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
        Exercise::PublicAppComponent => assert_component_trace(stderr, &evidence.reports),
    }
    for process in [host_pid, bun, descendant, guardian] {
        await_process_gone(process);
    }
    assert!(native_windows(host_pid, TITLE).is_empty());
    assert!(
        matches!(fs::symlink_metadata(stage_root), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "product Quit retained exact stage before fixture destruction: {}",
        stage_root.display()
    );
}

fn pid(report: &Value, key: &str) -> u32 {
    u32::try_from(report[key].as_u64().expect("reported PID")).expect("OS PID domain")
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
