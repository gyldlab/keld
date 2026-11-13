//! Scenario and subprocess custody for an opt-in raw `WebView2` feasibility probe.
//! Exact fixture routing is not a substitute for the proposed shared guard/parser.

#[path = "windows_file_native_session.rs"]
mod session;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::WaitForSingleObject;

use super::observation;

#[path = "windows_file_native_parent_receipt.rs"]
mod parent_receipt;
use parent_receipt::{verify_parent_binding, verify_profile_absent};

const CHILD: &str = "windows_file_native_probe_child";
const CHILD_MARKER: &str = "w3-q22-native-v2";

fn create_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("sync {}: {error}", path.display()))
}

fn fixture_uri(path: &Path) -> Result<String, String> {
    // URI construction for one owned ASCII test filename, not parsing or authorizing input.
    let text = path
        .to_str()
        .ok_or("owned fixture path is not UTF-8")?
        .replace('\\', "/");
    if !path.is_absolute()
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/:._-".contains(&byte))
    {
        return Err("set KELD_W3_PROBE_ROOT to an existing absolute ASCII path without spaces for this fixture".into());
    }
    Ok(format!("file:///{text}"))
}

fn source_manifest() -> Value {
    let sources: [(&str, &[u8]); 10] = [
        (
            "tests/windows_file_native_probe.rs",
            include_bytes!("../windows_file_native_probe.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_probe.rs",
            include_bytes!("windows_file_native_probe.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_parent_receipt.rs",
            include_bytes!("windows_file_native_parent_receipt.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_session.rs",
            include_bytes!("windows_file_native_session.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_session_tests.rs",
            include_bytes!("windows_file_native_session_tests.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_callbacks.rs",
            include_bytes!("windows_file_native_callbacks.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_operations.rs",
            include_bytes!("windows_file_native_operations.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_observation.rs",
            include_bytes!("windows_file_native_observation.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_observation_tests.rs",
            include_bytes!("windows_file_native_observation_tests.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_document.html",
            include_bytes!("windows_file_native_document.html"),
        ),
    ];
    let hashes: Vec<_> = sources
        .iter()
        .map(|(path, bytes)| json!({"path":path,"sha256":format!("{:x}", Sha256::digest(bytes))}))
        .collect();
    json!({
        "compiled_source_head":option_env!("KELD_W3_SOURCE_HEAD"),
        "compiled_fixture_sources":hashes,"keld_wv_version":env!("CARGO_PKG_VERSION"),
        "os":std::env::consts::OS,"arch":std::env::consts::ARCH
    })
}

#[allow(unsafe_code)] // Test-only wait on the exact child process handle returned by Command::spawn.
pub(super) fn run_in_child() -> Result<(), String> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| format!("fixture nonce: {error}"))?;
    let nonce = format!("{:032x}", u128::from_le_bytes(random));
    let parent =
        std::env::var_os("KELD_W3_PROBE_ROOT").map_or_else(std::env::temp_dir, PathBuf::from);
    let root = parent.join(format!("keld-w3-q22-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&root)
        .map_err(|error| format!("create owned probe root {}: {error}", root.display()))?;
    let stdout =
        File::create_new(root.join("child.stdout.log")).map_err(|error| error.to_string())?;
    let stderr =
        File::create_new(root.join("child.stderr.log")).map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let executable_hash = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&executable).map_err(|error| error.to_string())?)
    );
    let mut child = Command::new(&executable)
        .args([
            "--exact",
            CHILD,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KELD_W3_NATIVE_CHILD", CHILD_MARKER)
        .env("KELD_W3_NATIVE_ROOT", &root)
        .env("KELD_W3_NATIVE_NONCE", &nonce)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("launch exact native helper: {error}"))?;
    // SAFETY: this borrowed HANDLE is the live child owned above; no PID lookup or other process is used.
    let wait = unsafe { WaitForSingleObject(HANDLE(child.as_raw_handle()), 110_000) };
    if wait != WAIT_OBJECT_0 {
        // This only terminates this test's child after its 90 s cleanup budget. Preserved
        // profile/receipt failures remain failures; there is no browser-PID kill or retry.
        let kill = child.kill();
        let reap = child.wait();
        return Err(format!(
            "native child {} (wait={wait:?}, timeout={}, kill={kill:?}, reap={reap:?}); preserve {} for cleanup",
            child.id(),
            wait == WAIT_TIMEOUT,
            root.display()
        ));
    }
    let status = child
        .wait()
        .map_err(|error| format!("reap native child: {error}"))?;
    let receipt_path = root.join("receipt.json");
    let receipt = std::fs::read(&receipt_path).map_err(|error| {
        format!(
            "native helper receipt absent (a zero-test run cannot pass): {error}; logs at {}",
            root.display()
        )
    })?;
    let receipt: Value = serde_json::from_slice(&receipt)
        .map_err(|error| format!("invalid native receipt: {error}"))?;
    // The parent owns the invocation identity and checks absence after reaping.
    // Neither the child-supplied flags nor its internal assessment prove either.
    let binding = verify_parent_binding(&receipt, &nonce, child.id(), &source_manifest());
    let profile = verify_profile_absent(&root.join("owned-webview2-profile"));
    let assessment = binding
        .and(profile)
        .and_then(|()| observation::assess(&receipt));
    let parent_receipt = json!({"child_pid":child.id(),"child_status":status.code(),"executable":executable,"executable_sha256":executable_hash,
        "receipt":receipt_path,"assessment":assessment.as_ref().ok(),"failure":assessment.as_ref().err()});
    create_file(
        &root.join("parent.json"),
        &serde_json::to_vec_pretty(&parent_receipt).map_err(|error| error.to_string())?,
    )?;
    println!(
        "W3_NATIVE_RECEIPT={}\n{}",
        receipt_path.display(),
        parent_receipt
    );
    if !status.success() {
        return Err(format!(
            "native helper failed: {status}; original logs and receipt at {}",
            root.display()
        ));
    }
    assessment.map(|_| ())
}

fn read_page(native: &session::Session) -> Result<Value, String> {
    let observation = native.evaluate("window.__w3Fixture", false)?;
    Ok(observation["body"].clone())
}

fn capture_native_same_uri_overlap(
    native: &session::Session,
    receipt: &mut Value,
    attempt: &str,
) -> Result<(), String> {
    native.phase("race-child");
    receipt["race_iframe_action"] = native.evaluate(
        "(() => { const frame=document.createElement('iframe'); frame.id='w3-ac06-held-iframe'; frame.src=location.href; document.body.appendChild(frame); return 'ac06-frame-dispatched'; })()", false)?;
    native.wait_event("race-child-held")?;
    native.phase("race-host");
    let race_request = native.host_request(attempt)?;
    if race_request["navigate_hresult"] != "S_OK" {
        return Err(format!(
            "AC06 native Host top-level load refused: {race_request}"
        ));
    }
    receipt["race_host_request"] = race_request;
    native.wait_event("navigation-starting")?;
    native.wait_event("resource-request")?;
    native.release_held_iframe()?;
    native.wait_event("navigation-completed")?;
    receipt["pages"]["race-host"] = read_page(native)?;
    let observed = native.events();
    verify_native_same_uri_overlap(receipt, &observed, attempt)
}

fn verify_native_same_uri_overlap(
    receipt: &mut Value,
    observed: &[Value],
    attempt: &str,
) -> Result<(), String> {
    let single = |phase: &str, kind: &str| -> Result<&Value, String> {
        let matches: Vec<_> = observed
            .iter()
            .filter(|event| event["phase"] == phase && event["kind"] == kind)
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "AC06 {phase}/{kind} observed {} events, expected 1",
                matches.len()
            ));
        }
        Ok(matches[0])
    };
    let held = single("race-child", "race-child-held")?;
    let child = single("race-child", "resource-request")?;
    let host_start = single("race-host", "navigation-starting")?;
    let host_resource = single("race-host", "resource-request")?;
    let release = single("race-host", "race-child-released")?;
    let top_content = single("race-host", "content-loading")?;
    let top_completed = single("race-host", "navigation-completed")?;
    let seq = |event: &Value| event["seq"].as_u64().unwrap_or(0);
    if !(seq(held) > 0
        && seq(held) <= seq(child)
        && seq(child) < seq(host_start)
        && seq(host_start) < seq(host_resource)
        && seq(host_resource) < seq(release)
        && seq(release) < seq(top_completed))
    {
        return Err(format!(
            "AC06 native race did not overlap in required order: held={}, child={}, start={}, top_resource={}, release={}, complete={}",
            seq(held),
            seq(child),
            seq(host_start),
            seq(host_resource),
            seq(release),
            seq(top_completed)
        ));
    }
    let correct = child["data"]["uri"] == receipt["uri"]
        && child["data"]["resource_context"] == 1
        && child["data"]["source_kind"] == host_resource["data"]["source_kind"]
        && child["data"]["header"]["state"] == "absent"
        && child["data"]["held_iframe"] == true
        && host_resource["data"]["uri"] == receipt["uri"]
        && host_resource["data"]["resource_context"] == 1
        && host_resource["data"]["header"]["state"] == "present"
        && host_resource["data"]["header"]["value"].as_str() == Some(attempt)
        && host_start["data"]["header"]["value"].as_str() == Some(attempt)
        && host_start["data"]["navigation_id"]
            .as_u64()
            .is_some_and(|id| {
                id != 0
                    && top_content["data"]["navigation_id"].as_u64() == Some(id)
                    && top_completed["data"]["navigation_id"].as_u64() == Some(id)
            })
        && top_content["data"]["is_error_page"] == false
        && top_completed["data"]["success"] == true
        && release["data"]["completed"] == true;
    if !correct {
        return Err(format!(
            "AC06 native race source/header/NavigationId mismatch: child={child}, host_start={host_start}, top_resource={host_resource}, content={top_content}, completed={top_completed}"
        ));
    }
    receipt["ac06_native_overlap"] = json!({
        "same_uri_iframe_no_host_nonce": true,
        "native_child_document_context": child["data"]["resource_context"],
        "native_child_source_kind": child["data"]["source_kind"],
        "child_deferral_held": true,
        "host_top_document_response_and_navigation_id_while_frame_held": true,
        "host_nonce_at_top_start_and_resource": true,
        "same_top_navigation_id_at_content_and_completion": true,
        "child_deferral_completed": true,
        "order": {
            "child_held": seq(held), "child_request": seq(child),
            "top_start": seq(host_start), "top_response": seq(host_resource),
            "child_release": seq(release), "top_completion": seq(top_completed)
        },
        "scope": "raw native SDK race only; not production Core admission or signed installed Host"
    });
    Ok(())
}

fn scenarios(native: &session::Session, receipt: &mut Value, nonce: &str) -> Result<(), String> {
    native.phase("host-header");
    let attempt = receipt["attempt_nonce"]
        .as_str()
        .ok_or("missing native attempt marker")?
        .to_owned();
    let request = native.host_request(&attempt)?;
    receipt["host_request"] = request.clone();
    if request["navigate_hresult"] == "S_OK" {
        native.wait_event("navigation-completed")?;
        if native.events().iter().any(|event| {
            event["phase"] == "host-header"
                && event["kind"] == "navigation-completed"
                && event["data"]["success"] == true
        }) {
            receipt["pages"]["host-header"] = read_page(native)?;
        }
    }
    native.phase("plain");
    native.navigate_plain()?;
    native.wait_event("navigation-completed")?;
    receipt["pages"]["plain"] = read_page(native)?;
    native.phase("iframe");
    receipt["iframe_action"] = native.evaluate("(() => { const frame = document.createElement('iframe'); frame.id = 'w3-owned-frame'; frame.src = location.href; document.body.appendChild(frame); return 'frame-created'; })()", false)?;
    native.wait_event("frame-navigation-completed")?;
    let page = native.evaluate("window.__w3FrameReady.then(() => window.__w3Fixture)", true)?;
    receipt["pages"]["iframe"] = page["body"]["result"]["value"].clone();
    capture_native_same_uri_overlap(native, receipt, &attempt)?;
    native.phase("page");
    native.navigate_from_page()?;
    native.wait_event("navigation-completed")?;
    receipt["pages"]["page"] = read_page(native)?;
    native.phase("evaluation");
    let expression = |prefix: &str| {
        serde_json::to_string(&format!("{prefix}-{nonce}")).map_err(|error| error.to_string())
    };
    let value = expression("execute-string")?;
    receipt["evaluations"]["execute_string"] = native.evaluate(&value, false)?;
    let value = expression("execute-promise")?;
    receipt["evaluations"]["execute_promise"] = native.evaluate(
        &format!("new Promise(resolve => queueMicrotask(() => resolve({value})))"),
        false,
    )?;
    let value = expression("cdp-string")?;
    receipt["evaluations"]["cdp_string"] = native.evaluate(&value, true)?;
    let value = expression("cdp-promise")?;
    receipt["evaluations"]["cdp_promise"] = native.evaluate(
        &format!("new Promise(resolve => queueMicrotask(() => resolve({value})))"),
        true,
    )?;
    let error = expression("throw")?;
    receipt["evaluations"]["cdp_throw"] = native.evaluate(
        &format!("(() => {{ throw new TypeError({error}); }})()"),
        true,
    )?;
    let error = expression("reject")?;
    receipt["evaluations"]["cdp_reject"] = native.evaluate(
        &format!(
            "new Promise((_, reject) => queueMicrotask(() => reject(new RangeError({error}))))"
        ),
        true,
    )?;
    let value = expression("cdp-recovery")?;
    receipt["evaluations"]["cdp_recovery"] = native.evaluate(
        &format!("new Promise(resolve => queueMicrotask(() => resolve({value})))"),
        true,
    )?;
    Ok(())
}

pub(super) fn child_entry() -> Result<(), String> {
    if std::env::var("KELD_W3_NATIVE_CHILD").as_deref() != Ok(CHILD_MARKER) {
        return Err(
            "this private native helper must be launched by its exact parent selector".into(),
        );
    }
    let root =
        PathBuf::from(std::env::var_os("KELD_W3_NATIVE_ROOT").ok_or("missing owned probe root")?);
    let nonce = std::env::var("KELD_W3_NATIVE_NONCE").map_err(|error| error.to_string())?;
    if nonce.len() != 32 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid fixture nonce".into());
    }
    let mut attempt_bytes = [0_u8; 16];
    getrandom::fill(&mut attempt_bytes).map_err(|error| format!("attempt marker: {error}"))?;
    let attempt_nonce = format!("{:032x}", u128::from_le_bytes(attempt_bytes));
    let document =
        include_str!("windows_file_native_document.html").replace("__FIXTURE_NONCE__", &nonce);
    let document_path = root.join("owned-document.html");
    create_file(&document_path, document.as_bytes())?;
    let uri = fixture_uri(&document_path)?;
    let profile = root.join("owned-webview2-profile");
    let mut native = session::Session::new(&profile, uri.clone(), document.into_bytes());
    let mut receipt = json!({"schema":observation::SCHEMA,"helper":CHILD,"source":source_manifest(),"helper_pid":std::process::id(),
        "nonce":nonce,"attempt_nonce":attempt_nonce,"uri":uri,"pages":{},"evaluations":{},"errors":[],
        "scope":"raw WebView2 feasibility; no Keld facade, file parser, document/role admission or transport delivery"});
    let result = (|| {
        let overrides: Vec<_> = std::env::vars_os()
            .filter(|(name, _)| {
                name.to_string_lossy()
                    .to_ascii_uppercase()
                    .starts_with("WEBVIEW2_")
            })
            .map(|(name, _)| name)
            .collect();
        if !overrides.is_empty() {
            return Err(format!(
                "browser override variable names present; no environment modified: {overrides:?}"
            ));
        }
        receipt["message_pump_calibration"] = session::Session::observe_message_pump()?;
        if receipt["message_pump_calibration"]["ready_before_queue_drained"] != true {
            return Err(
                "native message pump drained its queued messages before revisiting readiness"
                    .into(),
            );
        }
        native.start()?;
        scenarios(&native, &mut receipt, &nonce)
    })();
    if let Err(error) = result {
        receipt["errors"] = json!([error]);
    }
    receipt["runtime_version"] = json!(native.runtime_version);
    receipt["browser_pid"] = json!(native.browser_pid);
    receipt["devtools_enabled_readback"] = json!(native.devtools_enabled);
    receipt["cleanup"] = native.shutdown();
    receipt["events"] = json!(native.events());
    let assessment = observation::assess(&receipt);
    receipt["assessment"] = json!(assessment.as_ref().ok());
    receipt["assessment_error"] = json!(assessment.as_ref().err());
    let written = create_file(
        &root.join("receipt.json"),
        &serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
    );
    if let Err(error) = &written {
        eprintln!("native receipt persistence failed: {error}");
    }
    if native.creation_is_pending() {
        // Accepted creation has not quiesced. The receipt is durably failed; leave
        // apartment/window/interface state intact until this private process exits.
        // Parent/independent cleanup must reconcile any remaining owned profile/PIDs.
        std::process::exit(2);
    }
    written?;
    assessment.map(|_| ())
}
