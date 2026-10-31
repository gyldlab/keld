//! GH1037 verifies current limits without changing production staging.
//! Sibling-loader and renderer-asset observations use independent native cases.
//! Neither case needs focus, clicks, accessibility input or Screen Recording.

#[path = "multifile_staging/fixture.rs"]
mod fixture;
#[path = "multifile_staging/lifecycle.rs"]
mod lifecycle;

use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::process::session_dirs_for;
use crate::support::renderer::Beacon;
use lifecycle::BoundRole;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};

fn record_stage(label: &str, stage: &keld_cli::boot::DevBootStage) {
    let descriptor =
        fs::read_to_string(stage.root().join("keld.boot.json")).expect("actual descriptor");
    let html = fs::read_to_string(stage.root().join("index.html")).expect("actual staged HTML");
    eprintln!(
        "GH1037_STAGE {label} root={} descriptor={descriptor} html={html:?} sibling={} css={} image={}",
        stage.root().display(),
        stage.root().join("src/sibling.ts").exists(),
        stage.root().join("relative.css").exists(),
        stage.root().join("relative.svg").exists(),
    );
}

#[test]
fn shipping_stage_missing_sibling_has_an_explicit_injected_byte_counterweight() {
    let mut missing = fixture::project("gh1037-sibling-missing");
    fixture::assert_original_inputs_exist(&missing);
    missing.entry_source = format!(
        "import {{ siblingMarker }} from \"./sibling.ts\";\nconsole.error(siblingMarker);\n{}",
        missing.entry_source
    );
    let stage = missing.stage();
    record_stage("missing-sibling-normal-producer", &stage);
    assert_eq!(
        fs::read(stage.root().join("src/main.ts")).expect("normal stage entry"),
        missing.entry_source.as_bytes(),
        "normal producer must execute the declared entry bytes"
    );
    assert!(!stage.root().join("src/sibling.ts").exists());
    let mut host = Command::new(stage.host())
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping no-flag Host with normal missing-sibling stage");
    let lease = host.stdin.take().expect("retain dev lease through exit");
    let host_pid = host.id();
    let output = wait_child_output(host, EVENT_DEADLINE);
    drop(lease);
    let stderr = String::from_utf8(output.stderr).expect("Host stderr");
    eprintln!(
        "GH1037_SIBLING_MISSING host={host_pid} status={}\n{stderr}",
        output.status
    );
    assert!(
        !output.status.success(),
        "missing sibling became Host success"
    );
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(
        stderr.contains("before its initial authenticated generation bound"),
        "{stderr}"
    );
    // The macOS Guardian's error path returns before forwarding captured
    // child stderr. Do not infer that the entry never evaluated from silence.
    assert!(
        session_dirs_for(host_pid).is_empty(),
        "Host left app-link locator"
    );
    assert!(!stage.root().exists(), "failed Host left private stage");

    let mut present = fixture::project("gh1037-sibling-counterweight");
    present.entry_source.clone_from(&missing.entry_source);
    let control = present.stage();
    assert_eq!(
        fs::read(control.root().join("src/main.ts")).expect("counterweight entry"),
        missing.entry_source.as_bytes(),
        "both arms must execute identical main bytes"
    );
    assert!(!control.root().join("src/sibling.ts").exists());
    // TEST-ONLY counterweight: add exactly one known sibling after the normal
    // producer returns, without editing the producer. This unsupported mutated
    // stage is causal diagnostic data, never a packaging/product pass.
    let destination = control.root().join("src/sibling.ts");
    fs::copy(present.project.join("src/sibling.ts"), &destination)
        .expect("inject only test sibling");
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o400))
        .expect("readonly test byte");
    assert_eq!(
        fs::read(&destination).expect("injected bytes"),
        fs::read(present.project.join("src/sibling.ts")).expect("original sibling bytes")
    );
    record_stage("TEST-ONLY-injected-sibling-unsupported", &control);
    let role = BoundRole::launch(&present, &control);
    let complete_capture = role.quit(&control);
    assert!(
        complete_capture.contains("GH1037_SIBLING_EVALUATED"),
        "successful Guardian path must expose the loaded sibling effect: {complete_capture}"
    );
    eprintln!(
        "GH1037_SIBLING_COUNTERWEIGHT injected=src/sibling.ts supported_product_stage=false HELLO READY ECHO QUIT CLEANUP"
    );
}

fn request(beacon: &mut Beacon, label: &str) -> String {
    let bytes = beacon
        .request
        .recv_timeout(EVENT_DEADLINE)
        .unwrap_or_else(|error| panic!("{label} native renderer observation missing: {error}"));
    let request = String::from_utf8(bytes).expect("renderer request UTF-8");
    beacon
        .handle
        .take()
        .expect("observer owner")
        .join()
        .expect("observer joins");
    eprintln!(
        "GH1037_RENDERER_{label} {}",
        request.lines().next().expect("request line")
    );
    request
}

#[test]
fn shipping_stage_calibrates_native_html_script_load_and_relative_asset_states() {
    let fixture = fixture::project("gh1037-renderer-assets");
    fixture::assert_original_inputs_exist(&fixture);
    let mut parsed = Beacon::bind("GH1037_PARSE");
    let mut scripted = Beacon::bind("GH1037_SCRIPT");
    let mut loaded = Beacon::bind("GH1037_ASSETS");
    let errors = Beacon::bind("GH1037_SCRIPT_ERROR");
    fixture::renderer(
        &fixture,
        [parsed.port(), scripted.port(), loaded.port(), errors.port()],
    );
    let stage = fixture.stage();
    record_stage("independent-renderer-arm", &stage);
    let role = BoundRole::launch(&fixture, &stage);
    assert!(request(&mut parsed, "PARSE").starts_with("GET /GH1037_PARSE "));
    assert!(request(&mut scripted, "SCRIPT").starts_with("GET /GH1037_SCRIPT "));
    let observation = loaded.request.recv_timeout(EVENT_DEADLINE);
    if observation.is_err() {
        eprintln!(
            "GH1037_SCRIPT_ERROR_DIAGNOSTIC {:?}",
            errors.request.try_recv()
        );
    }
    let bytes =
        observation.expect("native window-load snapshot, independent of asset terminal callbacks");
    let request_line = String::from_utf8(bytes).expect("snapshot UTF-8");
    let request_line = request_line.lines().next().expect("snapshot request");
    eprintln!(
        "GH1037_ASSET_SNAPSHOT host={} guardian={} bun={} {request_line}",
        role.host_pid, role.guardian_pid, role.bun_pid
    );
    loaded
        .handle
        .take()
        .expect("load observer")
        .join()
        .expect("load observer joins");
    let fields: Vec<_> = request_line
        .split_whitespace()
        .nth(1)
        .expect("snapshot URI")
        .split('/')
        .collect();
    assert_eq!(
        fields.len(),
        10,
        "one explicit state per measured field: {request_line}"
    );
    assert_eq!(fields[1], "GH1037_ASSETS");
    assert!(["load", "error", "pending"].contains(&fields[2]));
    assert!(["load", "error", "pending"].contains(&fields[3]));
    assert_eq!(
        fields[6], "load",
        "inline image positive control: {request_line}"
    );
    assert_eq!(fields[7], "3", "inline image natural width: {request_line}");
    assert_eq!(
        fields[8], "19-73-109",
        "inline style positive control: {request_line}"
    );
    // Record pending as pending. No event/status timeout or data control can
    // manufacture a relative-resource failure or a supported-origin claim.
    let _capture = role.quit(&stage);
}
