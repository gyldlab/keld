//! D1 actual shipping Host + packed SDK fixture qualification. This authored
//! inert page is never original-editor acceptance or accepting-Session authority.

use super::{FirstProofLaunch, host};
use crate::graph_stage::prepare_with_role;
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::invalid_stage::{InvalidBoot, InvalidPolicy};
use crate::support::native_window::{
    NativeWindowObserver, compile_native_window_census, native_windows,
};
use crate::support::process::{await_process_gone_before, process_ids, session_dirs_for};
use keld_cli::boot::DevBootStage;
use sha2::{Digest as _, Sha256};
use std::fs;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

// Exact approved H2 bytes are a fixture oracle, not a second policy catalog.
const H2: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'";

fn fixture_stage(root: &Path, boot_error_control: bool) -> DevBootStage {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let (project, approved) = prepare_with_role(root, &fixtures.join("gh553_d1_window_role.mjs"));
    let main = project.join("drawio/src/main");
    let webapp = main.join("webapp");
    for name in ["index.html", "probe.js", "style.css"] {
        fs::copy(fixtures.join("gh553-d1").join(name), webapp.join(name))
            .expect("copy only authored D1 inert page assets");
    }
    if boot_error_control {
        let index = fs::read_to_string(webapp.join("index.html")).expect("authored D1 index");
        fs::write(
            webapp.join("index.html"),
            index.replace(
                "</head>",
                "<script defer src=\"boot-error-control.js\"></script></head>",
            ),
        )
        .expect("add only authored external error control");
        fs::write(
            webapp.join("boot-error-control.js"),
            "throw new TypeError('KELD_BOOT_DIAGNOSTIC_INERT_CONTROL');\n",
        )
        .expect("authored page exception, not an EditorUi replacement");
    }
    fs::write(webapp.join("samples.bin"), [19, 73, 109]).expect("three independent resource bytes");
    // Minimal wasm function () -> i32, exported as answer, returns literal 42.
    fs::write(
        webapp.join("answer.wasm"),
        [
            0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 7, 10, 1, 6, 97, 110,
            115, 119, 101, 114, 0, 0, 10, 6, 1, 4, 0, 65, 42, 11,
        ],
    )
    .expect("authored actual wasm module");
    fs::write(main.join("d1-outside.bin"), b"outside-source-root-secret")
        .expect("real sentinel outside the renderer source root");
    let original: String = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../keld-wv/tests/fixtures/drawio-original-preload.json"),
        )
        .expect("existing pinned original preload data"),
    )
    .expect("decode exact original bytes");
    assert_eq!(original.len(), 3_201);
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98",
        "fixture never rewrites the original preload"
    );
    let preload_main = project.join("src/main");
    fs::create_dir_all(&preload_main).expect("approved desktop preload layout");
    fs::write(
        preload_main.join("electron-preload.js"),
        original.as_bytes(),
    )
    .expect("unchanged pinned original preload");
    fs::write(project.join("keld.permissions.jsonc"),
        b"{\"profile\":\"legacy\",\"windows\":{\"d1-inert\":{\"web\":{\"csp\":\"original-editor-offline\"}}}}\n")
        .expect("exact approved typed selector, no extra grants");
    // Independently select the source Host bytes before the staging copy. This
    // binds the candidate under test; passing the digest is not qualification.
    let expected_host: [u8; 32] =
        Sha256::digest(fs::read(host()).expect("selected shipping Host source")).into();
    keld_cli::boot::stage_dev_boot_with_approved_graph(
        &project,
        host(),
        &approved,
        Some(expected_host),
    )
    .expect("same sole graph/pairing stage producer")
}

#[test]
fn gh553_d1_actual_host_original_preload_csp_wasm_and_outside_denial() {
    let _ = actual_host_case(false);
}

fn actual_host_case(boot_error_control: bool) -> std::process::Output {
    let root = tempfile::tempdir().expect("owned D1 Host fixture");
    let stage = fixture_stage(root.path(), boot_error_control);
    let stage_root = stage.root().to_owned();
    let outside = stage_root.join("drawio/src/main/d1-outside.bin");
    assert_eq!(
        fs::read(&outside).expect("real outside sentinel exists"),
        b"outside-source-root-secret"
    );
    let physical = fs::canonicalize(&outside).expect("exact outside file identity");
    let physical = physical.to_str().expect("fixture path UTF-8");
    assert!(
        !physical
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || matches!(byte, b'%' | b'?' | b'#')),
        "fixture path needs existing URL encoding rather than a guessed spelling"
    );
    let outside_title = format!("D1 outside denied requested=file://{physical}");
    let (ax, _compiler_output) = compile_d1_ax_observer(root.path());
    let census = compile_native_window_census(root.path());
    let mut native = NativeWindowObserver::arm_for_title(&census, "Keld");
    let deadline = Instant::now() + EVENT_DEADLINE;
    let mut launch = FirstProofLaunch::start_prepared_captured(stage, deadline);
    let pid = launch.host_pid();
    // Install kernel/group custody before native/AX assertions can fail. A
    // stable initial snapshot may precede the role's actual cat spawn.
    let _ = launch.cleanup.observe_family_before(deadline);
    assert_eq!(
        native
            .expect_initial(pid, "D1 inert actual Host resource view")
            .len(),
        1
    );
    // The first-party role creates its native window only after the real cat
    // spawn. Refresh the same owner now; retain the exact final family oracle.
    let pids = launch.cleanup.observe_family_before(deadline);
    assert_eq!(pids.len(), 4, "actual Host/Guardian/Bun/cat family");
    let guardians = process_ids("ppid", pid);
    assert_eq!(guardians.len(), 1, "actual direct Guardian child");
    let guardian = guardians[0];
    let groups = launch.cleanup.observed_group_ids();
    let preload = "D1 preload simple renderer bridge=true bun=true require=undefined";
    let header = format!("D1 header {H2}");
    observe_d1_titles(
        &ax,
        pid,
        deadline,
        &[
            "D1 resources 19,73,109 wasm=42",
            preload,
            "D1 inline blocked",
            outside_title.as_str(),
            header.as_str(),
        ],
    );
    assert!(
        stage_root.is_dir(),
        "retained stage while physical view is live"
    );
    drop(launch.lease.take());
    let output = launch
        .cleanup
        .wait(deadline.saturating_duration_since(Instant::now()));
    assert!(
        output.status.success(),
        "actual Host lease shutdown: {output:?}"
    );
    for (target, exit) in launch.cleanup.take_observed_exits() {
        let status = exit.wait_before(deadline);
        if target == pid || target == guardian {
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
            "owned app-link session survived"
        );
    }
    for group in groups {
        assert!(
            process_ids("pgid", group).is_empty(),
            "owned group survived actual product shutdown"
        );
    }
    assert_eq!(native_windows(pid, "Keld"), [] as [u32; 0]);
    assert!(
        !stage_root.exists(),
        "actual Host must remove its owned stage before disarm"
    );
    launch.complete();
    output
}

fn compile_d1_ax_observer(root: &Path) -> (std::path::PathBuf, std::process::Output) {
    let ax = root.join("d1-observe-ax");
    let compile = Command::new("/usr/bin/swiftc")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/macos_dialog_ax_press.swift"),
        )
        .arg("-o")
        .arg(&ax)
        .spawn()
        .expect("compile existing exact-PID AX observer");
    let output = wait_child_output(compile, EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "existing AX observer compiler failed: {output:?}"
    );
    (ax, output)
}

fn observe_d1_titles(ax: &Path, pid: u32, deadline: Instant, titles: &[&str]) {
    for &title in titles {
        let observer = Command::new(ax)
            .arg(pid.to_string())
            .arg(title)
            .arg("--observe")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("existing AX observation child");
        let output =
            wait_child_output(observer, deadline.saturating_duration_since(Instant::now()));
        assert!(
            output.status.success(),
            "actual native document missing {title:?}: {output:?}"
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("actual AX report"),
            format!("REAL_HOST_AX_BUTTON_VISIBLE pid={pid} title={title}\n")
        );
    }
}

#[test]
#[ignore = "Root-enabled real WK untrusted page-recorder error/CSP visibility control"]
fn gh553_d1_readonly_boot_diagnostic_observes_authored_page_error_and_csp() {
    assert_eq!(
        std::env::var("KELD_TEST_BOOT_DIAGNOSTIC").as_deref(),
        Ok("1"),
        "Root must explicitly enable the diagnostic on this test only"
    );
    let output = actual_host_case(true);
    let stderr = String::from_utf8(output.stderr).expect("actual captured Host diagnostics");
    let reports: Vec<serde_json::Value> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("KELD_BOOT_DIAGNOSTIC "))
        .map(|line| serde_json::from_str(line).expect("bounded actual native boot report"))
        .collect();
    assert!(
        !reports.is_empty(),
        "actual current-source UA report missing: {stderr}"
    );
    assert!(
        reports
            .iter()
            .all(|report| report["native_source_current"] == true
                && report["page_globals_observed"] == false)
    );
    assert!(reports.iter().any(|report| {
        report["user_agent"]
            .as_str()
            .is_some_and(|ua| !ua.is_empty())
            && report["ua_truncated"] == false
    }));
    assert!(reports.iter().all(
        |report| report["page_report"]["trust"] == "untrusted-page-dom"
            && report["page_report"]["native_authority"] == false
    ));
    let records: Vec<&serde_json::Value> = reports
        .iter()
        .filter(|report| report["page_report"]["status"] == "ok")
        .filter_map(|report| report["page_report"]["records"].as_array())
        .flatten()
        .collect();
    assert!(
        records
            .iter()
            .any(|row| row["kind"] == "js-error" && row["resource"] == "inert-control"),
        "fixed page-world recorder did not observe the actual authored page exception; visibility remains unproved: {stderr}"
    );
    assert!(
        records.iter().any(|row| row["kind"] == "csp"
            && row["directive"] == "script"
            && row["blocked"] == "inline"),
        "fixed page-world recorder did not observe the actual existing blocked inline CSP control: {stderr}"
    );
}

#[test]
fn gh553_d1_actual_host_missing_policy_refuses_before_resource_attempts() {
    refusal(true);
}

#[test]
fn gh553_d1_actual_host_malformed_policy_refuses_before_resource_attempts() {
    refusal(false);
}

fn refusal(missing: bool) {
    let root = tempfile::tempdir().expect("owned D1 refusal fixture");
    let stage = fixture_stage(root.path(), false);
    let stage_root = stage.root().to_owned();
    let expected = if missing {
        InvalidBoot::MissingPermissions.apply(stage.root(), root.path());
        InvalidBoot::MissingPermissions.expected_code()
    } else {
        // Existing exact malformed bytes + matching independent digest exercise
        // the actual Guard parser, rather than only a stale-digest refusal.
        InvalidPolicy::Malformed.apply(stage.root());
        InvalidPolicy::Malformed.expected_code()
    };
    let deadline = Instant::now() + EVENT_DEADLINE;
    let mut launch = FirstProofLaunch::start_prepared_captured(stage, deadline);
    let pid = launch.host_pid();
    // Keep the valid lease alive through the natural preflight refusal. Early
    // EOF must not supply the failure this policy test claims to observe.
    let output = launch
        .cleanup
        .wait(deadline.saturating_duration_since(Instant::now()));
    drop(launch.lease.take());
    assert!(!output.status.success(), "invalid policy became success");
    let stderr = String::from_utf8(output.stderr).expect("actual Host typed refusal stderr");
    assert!(
        stderr.contains(expected),
        "actual Host lost {expected}: {stderr}"
    );
    assert!(
        stderr.contains("[startup-resource-attempts listener=0 child=0 window=0]"),
        "Host entered resource construction before refusal: {stderr}"
    );
    await_process_gone_before(pid, deadline);
    assert!(
        process_ids("pgid", pid).is_empty(),
        "preflight refusal left an owned process"
    );
    assert_eq!(session_dirs_for(pid), [] as [std::path::PathBuf; 0]);
    assert_eq!(native_windows(pid, "Keld"), [] as [u32; 0]);
    // A preflight refusal never completed family admission. Explicitly remove
    // this now-idle fixture stage; this is test cleanup, not product success.
    if stage_root.exists() {
        fs::remove_dir_all(&stage_root).expect("remove only reaped preflight fixture stage");
    }
    launch.complete();
}
