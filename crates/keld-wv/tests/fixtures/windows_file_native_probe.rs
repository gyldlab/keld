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
    let sources: [(&str, &[u8]); 9] = [
        (
            "tests/windows_file_native_probe.rs",
            include_bytes!("../windows_file_native_probe.rs"),
        ),
        (
            "tests/fixtures/windows_file_native_probe.rs",
            include_bytes!("windows_file_native_probe.rs"),
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
    let assessment = observation::assess(&receipt).and_then(|value| {
        assess_file_origin(&receipt)?;
        Ok(value)
    });
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
    native.wait_event("frame-navigation-completed")?;
    native.wait_event("navigation-completed")?;
    receipt["pages"]["race-host"] = read_page(native)?;
    let observed = native.events();
    verify_native_same_uri_overlap(receipt, &observed, attempt)
}

fn single_overlap_event<'a>(
    observed: &'a [Value],
    phase: &str,
    kind: &str,
) -> Result<&'a Value, String> {
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
}

fn reject_unexpected_top_level_child_callbacks(observed: &[Value]) -> Result<(), String> {
    // Equal URI and DOCUMENT source kind do not make an iframe a top-level load.
    if observed.iter().any(|event| {
        event["phase"] == "race-child"
            && matches!(
                event["kind"].as_str(),
                Some(
                    "navigation-starting"
                        | "content-loading"
                        | "dom-content-loaded"
                        | "navigation-completed"
                )
            )
    }) {
        return Err("AC06 iframe phase unexpectedly navigated the top-level document".into());
    }
    // A second iframe start in the Host phase could carry the Host nonce,
    // despite the expected child and top-level callbacks being correct.
    // Likewise, a child completing before release contradicts a held deferral.
    // This controlled fixture creates only one iframe and releases it once.
    if observed.iter().any(|event| {
        (event["phase"] == "race-host" && event["kind"] == "frame-navigation-starting")
            || (event["phase"] == "race-child" && event["kind"] == "frame-navigation-completed")
    }) {
        return Err(
            "AC06 unexpected iframe callback contradicts the held single-child race".into(),
        );
    }
    // This native event is emitted only by the failure-path Drop guard when
    // the acquired iframe deferral was completed instead of transferred to
    // Session. A successful held-then-released race cannot contain it.
    if observed.iter().any(|event| {
        (event["phase"] == "race-child" || event["phase"] == "race-host")
            && event["kind"] == "race-child-abort-complete"
    }) {
        return Err("AC06 native iframe deferral aborted during successful overlap".into());
    }
    Ok(())
}

fn verify_native_same_uri_overlap(
    receipt: &mut Value,
    observed: &[Value],
    attempt: &str,
) -> Result<(), String> {
    // Trace::record assigns one strictly increasing global sequence to each
    // native callback as it is appended. A reordered, lost or duplicated
    // receipt row must not be reconstructed into an apparently valid race.
    let mut previous_sequence: Option<u64> = None;
    for event in observed {
        let sequence = event["seq"]
            .as_u64()
            .filter(|seq| *seq > 0)
            .ok_or("AC06 native callback has no valid sequence")?;
        if previous_sequence.is_some_and(|previous| previous.checked_add(1) != Some(sequence)) {
            return Err("AC06 native callback log is reordered, incomplete or duplicated".into());
        }
        previous_sequence = Some(sequence);
    }
    let single = |phase: &str, kind: &str| single_overlap_event(observed, phase, kind);
    let frame_start = single("race-child", "frame-navigation-starting")?;
    let held = single("race-child", "race-child-held")?;
    let child = single("race-child", "resource-request")?;
    let host_start = single("race-host", "navigation-starting")?;
    let host_resource = single("race-host", "resource-request")?;
    let release = single("race-host", "race-child-released")?;
    let frame_completed = single("race-host", "frame-navigation-completed")?;
    let top_content = single("race-host", "content-loading")?;
    let top_dom = single("race-host", "dom-content-loaded")?;
    let top_completed = single("race-host", "navigation-completed")?;
    let seq = |event: &Value| event["seq"].as_u64().unwrap_or(0);
    if !(seq(frame_start) > 0
        && seq(frame_start) < seq(held)
        && seq(held) <= seq(child)
        && seq(child) < seq(host_start)
        && seq(host_start) < seq(host_resource)
        && seq(host_resource) < seq(release)
        && seq(release) < seq(frame_completed)
        && seq(frame_completed) < seq(top_completed)
        && seq(host_resource) < seq(top_content)
        && seq(top_content) < seq(top_dom)
        && seq(top_dom) < seq(top_completed))
    {
        return Err(format!(
            "AC06 native race callback ordering mismatch: {observed:?}"
        ));
    }
    reject_unexpected_top_level_child_callbacks(observed)?;
    let top_id = host_start["data"]["navigation_id"].as_u64();
    // A native successful navigation is not enough to prove the committed
    // page remained the claimed same-URI file document after redirects.
    let committed = &receipt["pages"]["race-host"];
    // A top-level callback alone cannot prove that the native Host actually
    // created and submitted the requested nonce-bearing request object.
    let native_host_request = &receipt["race_host_request"];
    let correct = native_host_request["uri"] == receipt["uri"]
        && native_host_request["header_name"] == "X-Keld-Host-Attempt"
        && native_host_request["sent_value"].as_str() == Some(attempt)
        && native_host_request["create_hresult"] == "S_OK"
        && native_host_request["input_header"] == json!({"state":"present","value":attempt})
        && native_host_request["navigate_hresult"] == "S_OK"
        && committed["href"] == receipt["uri"]
        && committed["origin"] == "file://"
        && committed["protocol"] == "file:"
        && committed["host"] == ""
        && committed["port"] == ""
        && frame_start["data"]["uri"] == receipt["uri"]
        && frame_start["data"]["header"] == json!({"state":"absent"})
        && frame_start["data"]["navigation_id"]
            .as_u64()
            .is_some_and(|id| {
                id != 0
                    && Some(id) != top_id
                    && frame_completed["data"]["navigation_id"].as_u64() == Some(id)
            })
        // A cancelled held iframe may legitimately report success=false, but
        // the native IsSuccess status must be present and boolean either way.
        && frame_completed["data"]["success"].as_bool().is_some()
        && held["data"]["real_deferral"] == true
        // The held native request must independently observe no Host nonce.
        && held["data"]["header"] == json!({"state":"absent"})
        && held["data"]["uri"] == receipt["uri"]
        && held["data"]["source_kind"] == 1
        && held["data"]["resource_context"] == 1
        && child["data"]["uri"] == receipt["uri"]
        && child["data"]["resource_context"] == 1
        && child["data"]["source_kind"] == 1
        && child["data"]["header"] == json!({"state":"absent"})
        && child["data"]["held_iframe"] == true
        && child["data"]["owned_fixture"] == true
        && host_start["data"]["uri"] == receipt["uri"]
        && host_start["data"]["header"]["state"] == "present"
        && host_resource["data"]["uri"] == receipt["uri"]
        && host_resource["data"]["resource_context"] == 1
        && host_resource["data"]["source_kind"] == 1
        && host_resource["data"]["owned_fixture"] == true
        && host_resource["data"]["header"]["state"] == "present"
        && host_resource["data"]["header"]["value"].as_str() == Some(attempt)
        && host_start["data"]["header"]["value"].as_str() == Some(attempt)
        && host_start["data"]["navigation_id"]
            .as_u64()
            .is_some_and(|id| {
                id != 0
                    && top_content["data"]["navigation_id"].as_u64() == Some(id)
                    && top_dom["data"]["navigation_id"].as_u64() == Some(id)
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
        "child_navigation_id_distinct_from_top": true,
        "native_child_document_context": child["data"]["resource_context"],
        "native_child_source_kind": child["data"]["source_kind"],
        "child_deferral_held": true,
        "host_top_document_response_and_navigation_id_while_frame_held": true,
        "host_nonce_at_top_start_and_resource": true,
        "same_top_navigation_id_at_content_and_completion": true,
        "same_top_navigation_id_at_dom_content_loaded": true,
        "child_deferral_completed": true,
        "order": {
            "frame_start": seq(frame_start), "child_held": seq(held),
            "child_request": seq(child), "top_start": seq(host_start),
            "top_response": seq(host_resource), "child_release": seq(release),
            "frame_completed": seq(frame_completed), "top_dom": seq(top_dom),
            "top_completion": seq(top_completed)
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
            native.wait_event("web-message")?;
        }
    }
    native.phase("plain");
    native.navigate_plain()?;
    native.wait_event("navigation-completed")?;
    receipt["pages"]["plain"] = read_page(native)?;
    native.wait_event("web-message")?;
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
    native.wait_event("web-message")?;
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
    let assessment = observation::assess(&receipt).and_then(|value| {
        assess_file_origin(&receipt)?;
        Ok(value)
    });
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

/// Actual native Source plus an immutable owned-document observation qualify
/// this runtime's serialization only. This does not implement production policy.
fn assess_file_origin(receipt: &Value) -> Result<(), String> {
    let uri = receipt["uri"]
        .as_str()
        .ok_or("missing native fixture URI")?;
    let nonce = receipt["nonce"].as_str().ok_or("missing owned nonce")?;
    let events = receipt["events"]
        .as_array()
        .ok_or("missing native events")?;
    for phase in ["host-header", "plain", "page"] {
        let page = &receipt["pages"][phase];
        if page["href"] != uri
            || page["origin"] != "file://"
            || page["protocol"] != "file:"
            || page["host"] != ""
            || page["port"] != ""
        {
            return Err(format!(
                "actual file origin observation disagrees in {phase}: {page}"
            ));
        }
        // Exactly one native-owned observation is expected per document phase.
        // `any` would let one valid callback conceal a conflicting sender,
        // foreign Source, or duplicate/replayed observation in the same phase.
        let messages: Vec<_> = events
            .iter()
            .filter(|event| event["phase"] == phase && event["kind"] == "web-message")
            .collect();
        if messages.len() != 1 {
            return Err(format!(
                "ambiguous native WebMessageReceived count for {phase}: {}",
                messages.len()
            ));
        }
        let matched = messages.into_iter().any(|event| {
            let data = &event["data"];
            let Some(raw) = data["raw_body"].as_str() else {
                return false;
            };
            let Ok(body) = serde_json::from_str::<Value>(raw) else {
                return false;
            };
            data["canonical_sender"] == true
                && data["native_source"] == uri
                && data["view_source"] == uri
                && body["kind"] == "owned-file-origin-observation"
                && body["nonce"] == nonce
                && body["href"] == uri
                && body["origin"] == "file://"
                && body["protocol"] == "file:"
                && body["host"] == ""
                && body["port"] == ""
                && body["is_main"] == true
        });
        if !matched {
            return Err(format!(
                "no actual native Source/origin observation for {phase}"
            ));
        }
    }
    Ok(())
}

// These are receipt-oracle mutations, not substituted native callbacks.
// They ensure a good raw SDK run cannot be reported when identity evidence is lost.
#[cfg(test)]
mod ac06_overlap_receipt_tests {
    use super::*;

    const URI: &str = "file:///D:/fixture/index.html";
    const NONCE: &str = "host-attempt";

    fn good_receipt_events() -> Vec<Value> {
        let uri = URI;
        let nonce = NONCE;
        let frame = json!({"phase":"race-child","kind":"frame-navigation-starting","seq":15,
            "data":{"uri":uri,"navigation_id":5,"header":{"state":"absent"}}});
        let held = json!({"phase":"race-child","kind":"race-child-held","seq":16,
            "data":{"uri":uri,"resource_context":1,"source_kind":1,"real_deferral":true,"header":{"state":"absent"}}});
        let child = json!({"phase":"race-child","kind":"resource-request","seq":17,
            "data":{"uri":uri,"resource_context":1,"source_kind":1,
                "header":{"state":"absent"},"held_iframe":true,"owned_fixture":true}});
        let top = json!({"phase":"race-host","kind":"navigation-starting","seq":18,
            "data":{"uri":uri,"navigation_id":6,
                "header":{"state":"present","value":nonce}}});
        let resource = json!({"phase":"race-host","kind":"resource-request","seq":19,
            "data":{"uri":uri,"resource_context":1,"source_kind":1,
                "header":{"state":"present","value":nonce},"owned_fixture":true}});
        let released = json!({"phase":"race-host","kind":"race-child-released","seq":20,
            "data":{"completed":true}});
        let frame_done = json!({"phase":"race-host","kind":"frame-navigation-completed","seq":21,
            "data":{"navigation_id":5,"success":false}});
        let content = json!({"phase":"race-host","kind":"content-loading","seq":22,
            "data":{"navigation_id":6,"is_error_page":false}});
        let dom_loaded = json!({"phase":"race-host","kind":"dom-content-loaded","seq":23,
            "data":{"navigation_id":6}});
        let top_done = json!({"phase":"race-host","kind":"navigation-completed","seq":24,
            "data":{"navigation_id":6,"success":true}});
        vec![
            frame, held, child, top, resource, released, frame_done, content, dom_loaded, top_done,
        ]
    }

    fn good_race_host_request() -> Value {
        json!({
            "uri": URI, "header_name": "X-Keld-Host-Attempt",
            "sent_value": NONCE, "create_hresult": "S_OK",
            "input_header": {"state": "present", "value": NONCE},
            "navigate_hresult": "S_OK"
        })
    }

    fn check(events: &[Value]) -> Result<(), String> {
        verify_native_same_uri_overlap(
            &mut json!({"uri":URI, "pages": {"race-host": {
                "href": URI, "origin":"file://", "protocol":"file:",
                "host":"", "port":""
            }}, "race_host_request": good_race_host_request()}),
            events,
            NONCE,
        )
    }

    #[test]
    fn reject_abort_completion_during_successful_native_overlap() {
        let mut observed = good_receipt_events();
        // Native Drop emits this only when an acquired iframe deferral is
        // completed on a failed response path, never on the successful hold.
        // Preserve a contiguous Trace::record sequence so this checks the
        // contradictory event, not the generic sequence validator.
        for event in observed.iter_mut().skip(3) {
            let seq = event["seq"].as_u64().expect("fixture callback sequence");
            event["seq"] = json!(seq + 1);
        }
        observed.insert(
            3,
            json!({
                "phase":"race-child", "kind":"race-child-abort-complete", "seq":18,
                "data":{"completed":true, "error":null}
            }),
        );
        assert!(
            check(&observed).is_err(),
            "native iframe deferral abort contradicted held success but was accepted"
        );
    }

    #[test]
    fn reject_reordered_or_reused_native_callback_sequence() {
        let good = good_receipt_events();
        assert!(check(&good).is_ok(), "valid native callback trace refused");

        // Trace::record writes a new seq at the point each callback is stored.
        // A sorted subset cannot stand in for the actual ordered native log.
        let mut reordered = good.clone();
        reordered.swap(1, 2);
        assert!(
            check(&reordered).is_err(),
            "native callback array order disagreed with seq but was accepted"
        );

        let mut reused = good.clone();
        reused[2]["seq"] = reused[1]["seq"].clone();
        assert!(
            check(&reused).is_err(),
            "two distinct native callbacks reused one sequence but were accepted"
        );
    }

    #[test]
    fn reject_missing_collapsed_or_swapped_frame_identity_and_forged_source() {
        let good = good_receipt_events();
        let nonce = NONCE;
        assert!(check(&good).is_ok(), "valid two-navigation receipt refused");

        let mut missing = good.clone();
        missing.remove(0);
        assert!(
            check(&missing).is_err(),
            "missing frame-start event accepted"
        );

        let mut collapsed = good.clone();
        collapsed[0]["data"]["navigation_id"] = json!(6);
        collapsed[6]["data"]["navigation_id"] = json!(6);
        assert!(
            check(&collapsed).is_err(),
            "shared iframe/top navigation ID accepted"
        );

        let mut wrong_completion = good.clone();
        wrong_completion[6]["data"]["navigation_id"] = json!(7);
        assert!(
            check(&wrong_completion).is_err(),
            "wrong iframe completion ID accepted"
        );

        let mut absent_completion = good.clone();
        absent_completion.remove(6);
        assert!(
            check(&absent_completion).is_err(),
            "absent iframe completion accepted"
        );

        // A terminal iframe event without a native IsSuccess result cannot
        // establish that the held frame was actually completed or cancelled.
        let mut missing_completion_status = good.clone();
        missing_completion_status[6]["data"]
            .as_object_mut()
            .expect("sample frame completion")
            .remove("success");
        assert!(
            check(&missing_completion_status).is_err(),
            "iframe completion without native success status accepted"
        );
        let mut forged_completion_status = good.clone();
        forged_completion_status[6]["data"]["success"] = json!("unknown");
        assert!(
            check(&forged_completion_status).is_err(),
            "nonboolean iframe completion status accepted"
        );

        // A held same-URI iframe resource must have an actual native absent
        // Host nonce; the subsequent child request cannot substitute for it.
        let mut held_header_with_nonce = good.clone();
        held_header_with_nonce[1]["data"]["header"] = json!({"state":"present","value":nonce});
        assert!(
            check(&held_header_with_nonce).is_err(),
            "held iframe with a native Host nonce accepted"
        );
        let mut contradictory_held_header = good.clone();
        contradictory_held_header[1]["data"]["header"]["value"] = json!(nonce);
        assert!(
            check(&contradictory_held_header).is_err(),
            "held iframe with absent state but a Host nonce value accepted"
        );
        let mut missing_held_header = good.clone();
        missing_held_header[1]["data"]
            .as_object_mut()
            .expect("sample held iframe")
            .remove("header");
        assert!(
            check(&missing_held_header).is_err(),
            "held iframe without a native header observation accepted"
        );
        let mut delayed_completion = good.clone();
        delayed_completion[6]["seq"] = json!(25);
        assert!(
            check(&delayed_completion).is_err(),
            "iframe completion after top completion accepted"
        );

        let mut no_deferral = good.clone();
        no_deferral[1]["data"]["real_deferral"] = json!(false);
        assert!(
            check(&no_deferral).is_err(),
            "forged held deferral accepted"
        );

        let mut late_start = good.clone();
        late_start[0]["seq"] = json!(19);
        assert!(
            check(&late_start).is_err(),
            "frame start after Host start accepted"
        );

        let mut wrong_top_uri = good.clone();
        wrong_top_uri[3]["data"]["uri"] = json!("file:///D:/fixture/other.html");
        assert!(
            check(&wrong_top_uri).is_err(),
            "foreign top-level start URI accepted"
        );

        let mut missing_top_uri = good.clone();
        missing_top_uri[3]["data"]
            .as_object_mut()
            .unwrap()
            .remove("uri");
        assert!(
            check(&missing_top_uri).is_err(),
            "missing top-level start URI accepted"
        );

        let mut missing_top_header_state = good.clone();
        missing_top_header_state[3]["data"]["header"]["state"] = json!("absent");
        assert!(
            check(&missing_top_header_state).is_err(),
            "Host start without present header state accepted"
        );
        let mut wrong_header = good.clone();
        wrong_header[0]["data"]["header"] = json!({"state":"present","value":nonce});
        assert!(
            check(&wrong_header).is_err(),
            "Host nonce on iframe navigation accepted"
        );

        // An absent state paired with a nonce value is contradictory native
        // evidence, even when other callbacks correctly report absence.
        for (label, index) in [("iframe start", 0), ("iframe resource", 2)] {
            let mut contradictory_absent = good.clone();
            contradictory_absent[index]["data"]["header"]["value"] = json!(nonce);
            assert!(
                check(&contradictory_absent).is_err(),
                "{label}: an absent header with Host nonce value was accepted"
            );
        }
    }

    #[test]
    fn reject_cross_phase_iframe_callbacks_that_contradict_held_deferral() {
        let good = good_receipt_events();
        assert!(check(&good).is_ok(), "valid native overlap was refused");

        // A second iframe start after the Host takeover can carry a Host nonce,
        // even when the expected old iframe start and Host resource remain valid.
        // The fixed document does not create any new iframe in the Host phase.
        for (label, header) in [
            (
                "host nonce inherited by unexpected iframe",
                json!({"state":"present","value":NONCE}),
            ),
            (
                "unexpected extra iframe navigation",
                json!({"state":"absent"}),
            ),
        ] {
            let mut extra_start = good.clone();
            extra_start.push(json!({
                "phase":"race-host","kind":"frame-navigation-starting","seq":25,
                "data":{"uri":URI,"navigation_id":7,"header":header}
            }));
            assert!(
                check(&extra_start).is_err(),
                "{label}: an extra Host-phase frame start was accepted"
            );
        }

        // A held COM deferral cannot have completed the same iframe before
        // the Host request and explicit release, even with a later valid finish.
        let mut premature_completion = good.clone();
        premature_completion.push(json!({
            "phase":"race-child","kind":"frame-navigation-completed","seq":17,
            "data":{"navigation_id":5,"success":true}
        }));
        assert!(
            check(&premature_completion).is_err(),
            "an iframe completion during its held deferral was accepted"
        );
    }

    #[test]
    fn reject_top_content_loading_out_of_order_or_child_top_level_callback() {
        let good = good_receipt_events();
        assert!(check(&good).is_ok(), "valid two-navigation receipt refused");

        // Each mutation changes one independently observed native callback fact.
        // An iframe response cannot stand in for a top-level document commit.
        let mut early_top_content = good.clone();
        early_top_content[7]["seq"] = json!(13);
        assert!(
            check(&early_top_content).is_err(),
            "top-level ContentLoading before Host navigation accepted"
        );
        let mut late_top_content = good.clone();
        late_top_content[7]["seq"] = json!(25);
        assert!(
            check(&late_top_content).is_err(),
            "top-level ContentLoading after completion accepted"
        );
        for kind in [
            "navigation-starting",
            "content-loading",
            "dom-content-loaded",
            "navigation-completed",
        ] {
            let mut unexpected_top_callback = good.clone();
            unexpected_top_callback.insert(
                0,
                json!({"phase":"race-child","kind":kind,"seq":14,
                    "data":{"uri":URI,"navigation_id":8,"header":{"state":"absent"}}}),
            );
            assert!(
                check(&unexpected_top_callback).is_err(),
                "top-level {kind} in iframe phase was accepted"
            );
        }
    }

    #[test]
    fn reject_missing_or_forged_native_top_dom_commit_witness() {
        let good = good_receipt_events();
        assert!(check(&good).is_ok(), "valid DOM-loaded witness refused");

        // Native ContentLoading alone cannot substitute for DOMContentLoaded
        // on the same top-level NavigationId, in the exact callback order.
        let mut missing = good.clone();
        missing.remove(8);
        assert!(
            check(&missing).is_err(),
            "missing native DOMContentLoaded accepted"
        );

        let mut foreign_id = good.clone();
        foreign_id[8]["data"]["navigation_id"] = json!(7);
        assert!(
            check(&foreign_id).is_err(),
            "foreign DOMContentLoaded navigation accepted"
        );

        let mut late = good.clone();
        late[8]["seq"] = json!(25);
        assert!(
            check(&late).is_err(),
            "DOMContentLoaded after completion accepted"
        );

        let mut duplicate = good.clone();
        duplicate.push(good[8].clone());
        assert!(
            check(&duplicate).is_err(),
            "duplicate native DOMContentLoaded accepted"
        );
    }

    #[test]
    fn reject_missing_or_forged_native_race_host_request_object() {
        let events = good_receipt_events();
        let valid = json!({
            "uri": URI,
            "pages": {"race-host": {
                "href": URI, "origin": "file://", "protocol": "file:",
                "host": "", "port": ""
            }},
            "race_host_request": good_race_host_request()
        });
        assert!(
            verify_native_same_uri_overlap(&mut valid.clone(), &events, NONCE).is_ok(),
            "valid native Host request-object proof refused"
        );

        let mut missing = valid.clone();
        missing
            .as_object_mut()
            .expect("sample receipt")
            .remove("race_host_request");
        assert!(
            verify_native_same_uri_overlap(&mut missing, &events, NONCE).is_err(),
            "native Host request-object witness missing but race accepted"
        );
        for (field, wrong) in [
            ("uri", json!("file:///D:/foreign.html")),
            ("header_name", json!("X-Foreign-Nonce")),
            ("sent_value", json!("stale-nonce")),
            ("create_hresult", json!("E_FAIL")),
            ("input_header", json!({"state":"absent"})),
            ("navigate_hresult", json!("E_FAIL")),
        ] {
            let mut forged = valid.clone();
            forged["race_host_request"][field] = wrong;
            assert!(
                verify_native_same_uri_overlap(&mut forged, &events, NONCE).is_err(),
                "forged native Host request {field} accepted"
            );
        }
    }

    #[test]
    fn reject_race_host_final_page_that_disagrees_with_native_same_uri() {
        let good = good_receipt_events();
        let good_page = json!({
            "href": URI, "origin": "file://", "protocol": "file:",
            "host": "", "port": ""
        });
        let good_receipt = json!({
            "uri": URI, "pages": {"race-host": good_page},
            "race_host_request": good_race_host_request()
        });
        assert!(
            verify_native_same_uri_overlap(&mut good_receipt.clone(), &good, NONCE).is_ok(),
            "valid native Host page refused"
        );
        for (name, field, forged) in [
            (
                "foreign final URL",
                "href",
                json!("file:///D:/foreign.html"),
            ),
            (
                "foreign final origin",
                "origin",
                json!("https://foreign.invalid"),
            ),
            ("wrong final protocol", "protocol", json!("https:")),
        ] {
            let mut receipt = good_receipt.clone();
            receipt["pages"]["race-host"][field] = forged;
            assert!(
                verify_native_same_uri_overlap(&mut receipt, &good, NONCE).is_err(),
                "{name}: a mismatched committed page was accepted as same-URI"
            );
        }
        let mut missing_final_page = json!({"uri": URI});
        assert!(
            verify_native_same_uri_overlap(&mut missing_final_page, &good, NONCE).is_err(),
            "missing committed Host document was accepted as same-URI"
        );
    }

    #[test]
    fn reject_worker_source_kind_non_document_context_and_stale_host_nonce() {
        let good = good_receipt_events();
        assert!(check(&good).is_ok(), "valid two-navigation receipt refused");
        // A simultaneous child/Host mutation could still fail if only one
        // source-kind check existed. Mutate the native hold and both requests
        // separately, using real WebView2 worker source-kind bit values.
        for (label, index) in [
            ("held iframe", 1),
            ("iframe request", 2),
            ("Host request", 4),
        ] {
            for worker_kind in [2, 4] {
                let mut worker = good.clone();
                worker[index]["data"]["source_kind"] = json!(worker_kind);
                assert!(
                    check(&worker).is_err(),
                    "{label}: worker source kind {worker_kind} accepted as DOCUMENT"
                );
            }
        }
        for (label, index) in [
            ("held iframe", 1),
            ("iframe request", 2),
            ("Host request", 4),
        ] {
            let mut non_document_context = good.clone();
            non_document_context[index]["data"]["resource_context"] = json!(0);
            assert!(
                check(&non_document_context).is_err(),
                "{label}: non-DOCUMENT resource context accepted"
            );
        }
        let mut foreign_host_nonce = good.clone();
        foreign_host_nonce[4]["data"]["header"]["value"] = json!("old-attempt");
        assert!(
            check(&foreign_host_nonce).is_err(),
            "stale Host resource nonce accepted"
        );
    }

    #[test]
    fn reject_native_response_not_routed_to_owned_fixture() {
        let mut good = good_receipt_events();
        for index in [2, 4] {
            good[index]["data"]["owned_fixture"] = json!(true);
        }
        assert!(
            check(&good).is_ok(),
            "owned native DOCUMENT response was refused"
        );

        // The callback records whether the actual native responder selected
        // the owned 200 response rather than the denied 403 response. A matching
        // URI/header alone must not substitute for this response-route witness.
        for (label, index) in [("iframe", 2), ("Host", 4)] {
            let mut refused = good.clone();
            refused[index]["data"]["owned_fixture"] = json!(false);
            assert!(
                check(&refused).is_err(),
                "{label} 403/foreign responder was accepted as an owned response"
            );

            let mut missing = good.clone();
            missing[index]["data"]
                .as_object_mut()
                .expect("sample request")
                .remove("owned_fixture");
            assert!(
                check(&missing).is_err(),
                "{label} response without a native owned-route witness was accepted"
            );
        }
    }

    #[test]
    fn file_origin_claim_requires_actual_native_web_message_source_and_sender() {
        let uri = URI;
        let nonce = "owned-page-nonce";
        let page = json!({
            "href":uri,"origin":"file://","protocol":"file:","host":"","port":""
        });
        let body = json!({
            "kind":"owned-file-origin-observation","nonce":nonce,
            "href":uri,"origin":"file://","protocol":"file:",
            "host":"","port":"","is_main":true
        })
        .to_string();
        let events: Vec<Value> = ["host-header", "plain", "page"]
            .into_iter()
            .map(|phase| {
                json!({
                    "phase":phase,"kind":"web-message",
                    "data":{"canonical_sender":true,"native_source":uri,
                        "view_source":uri,"raw_body":body}
                })
            })
            .collect();
        let good = json!({
            "uri":uri,"nonce":nonce,
            "pages":{"host-header":page,"plain":page,"page":page},
            "events":events
        });
        assert!(
            assess_file_origin(&good).is_ok(),
            "actual WebMessageReceived Source and owned page nonce should qualify"
        );
        for (field, wrong) in [
            ("canonical_sender", json!(false)),
            ("native_source", json!("file:///D:/foreign/index.html")),
            ("view_source", json!("file:///D:/foreign/index.html")),
            ("raw_body", json!("invalid native message JSON")),
        ] {
            let mut forged = good.clone();
            forged["events"][0]["data"][field] = wrong;
            assert!(
                assess_file_origin(&forged).is_err(),
                "Host web-message {field} was accepted without native Source"
            );
        }
        let mut forged_page = good.clone();
        forged_page["events"][1]["data"]["raw_body"] = json!(
            json!({
                "kind":"owned-file-origin-observation","nonce":"stale-page",
                "href":uri,"origin":"file://","protocol":"file:",
                "host":"","port":"","is_main":true
            })
            .to_string()
        );
        assert!(
            assess_file_origin(&forged_page).is_err(),
            "page-controlled message nonce impersonated native Source"
        );
        let mut missing = good.clone();
        missing["events"]
            .as_array_mut()
            .expect("sample events")
            .remove(2);
        assert!(
            assess_file_origin(&missing).is_err(),
            "missing native page WebMessageReceived was accepted"
        );
        // An accepted callback must not mask a second, contradictory callback
        // for the same phase. A positive native source is not exclusive proof.
        let mut mixed_sender = good.clone();
        mixed_sender["events"]
            .as_array_mut()
            .expect("sample events")
            .push(json!({
                "phase":"host-header","kind":"web-message",
                "data":{"canonical_sender":false,
                    "native_source":"file:///D:/foreign/index.html",
                    "view_source":uri,"raw_body":body}
            }));
        assert!(
            assess_file_origin(&mixed_sender).is_err(),
            "valid native Source masked a conflicting same-phase callback"
        );

        let mut replay = good.clone();
        let repeated = replay["events"][0].clone();
        replay["events"]
            .as_array_mut()
            .expect("sample events")
            .push(repeated);
        assert!(
            assess_file_origin(&replay).is_err(),
            "duplicate native-owned observation accepted as unique proof"
        );
    }
}
