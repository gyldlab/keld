//! One fixture invocation and conservative result/cleanup accounting.
use crate::config::Config;
use crate::native::Session;
use crate::pump::wide;
use crate::scenarios;
use crate::trace::{Shared, Trace};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::io::Write;

fn scope() -> Value {
    json!({
        "kind": "standalone native feasibility fixture",
        "securityContractApproval": "NOT_ASSERTED",
        "productionBridgeAdmission": "NOT_IMPLEMENTED",
        "bunHandlerEntry": "NOT_OBSERVED",
        "world": "shared page world",
        "contextIsolated": false,
        "sourcePin": "caller-supplied fixture digest, not production trust root",
        "staleFence": "synthetic commit serial; raw page delivery is native",
        "b7NaturalClosure": "NOT_CLAIMED",
        "fullAppAcceptance": "NOT_RUN",
        "processCensusLimit": "WebView2 GetProcessInfos excludes crashpad"
    })
}

fn persist(mut file: File, state: &Shared, mut result: Value) -> Result<(), String> {
    state.borrow_mut().flush();
    let trace = state.borrow();
    result["messages"] = json!(trace.messages);
    result["traceFailure"] = json!(trace.failure);
    result["nativeEventCount"] = json!(trace.events.len());
    result["specScope"] = scope();
    serde_json::to_writer_pretty(&mut file, &result).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

pub(crate) fn run(config: Config) -> Result<u8, String> {
    std::fs::create_dir_all(&config.out_dir).map_err(|e| e.to_string())?;
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config.out_dir.join("result.json"))
        .map_err(|e| format!("reserve new result.json: {e}"))?;
    let state = Trace::open(&config.out_dir.join("trace.jsonl"))?;
    state
        .borrow_mut()
        .push("ProbeStarted", json!({"config": config.raw}));
    let source_pin = json!({
        "expectedSha256": config.expected_bundle_digest,
        "observedSha256": config.bundle_digest,
        "accepted": config.expected_bundle_digest == config.bundle_digest,
        "loadedUtf8Bytes": config.bundle.as_ref().map(String::len),
        "scope": "exact in-memory fixture bundle only"
    });
    state
        .borrow_mut()
        .push("FixtureSourcePin", source_pin.clone());
    if config.expected_bundle_digest != config.bundle_digest {
        state
            .borrow_mut()
            .push("FixtureSourcePinRefused", source_pin.clone());
        persist(
            output,
            &state,
            json!({
                "status": "source-rejected", "reason": "bundle-sha256-mismatch", "nativeCreated": false,
                "runtimeVersion": null, "finalTitle": "", "browserExited": false,
                "sourcePin": source_pin, "scenario": null, "registrations": 0, "navigations": 0,
                "expectedBundleSha256": config.expected_bundle_digest,
                "observedBundleSha256": config.bundle_digest,
                "cleanup": {"required": false, "complete": true,
                    "controllerClosed": false, "browserExitEventObserved": false,
                    "browserHandleSignaled": false, "profileRemoved": false,
                    "windowDestroyed": false,
                    "reason": "no native session, window or profile was allocated"}
            }),
        )?;
        return Ok(2);
    }
    state.borrow_mut().flush();
    let mut session = match Session::create(&config, &state) {
        Ok(session) => session,
        Err(error) => {
            state
                .borrow_mut()
                .push("NativeSetupFailed", json!({"error": error}));
            persist(
                output,
                &state,
                json!({
                    "status": "SETUP_FAILED", "nativeCreated": null,
                    "runtimeVersion": null, "finalTitle": "", "browserExited": false,
                    "sourcePin": source_pin, "error": error,
                    "cleanup": {"required": true, "complete": false,
                        "controllerClosed": false, "browserExitEventObserved": false,
                        "browserHandleSignaled": false, "profileRemoved": false,
                        "windowDestroyed": false,
                        "reason": "fallback resource release is not verified browser exit"}
                }),
            )?;
            return Ok(1);
        }
    };
    let scenario_result = scenarios::run(&config, &mut session, &state);
    let title_result = session
        .webview
        .as_ref()
        .ok_or("WebView absent".to_owned())
        .and_then(|wv| {
            // SAFETY: getter on this session's live WebView, still on its owning STA.
            unsafe { wide(|p| wv.DocumentTitle(p)) }.map_err(|e| e.to_string())
        });
    let runtime = session.runtime_version.clone();
    let cleanup = session.close(&state, &config);
    drop(session);
    let scenario = scenario_result.as_ref().ok().cloned();
    let error = scenario_result
        .err()
        .or_else(|| title_result.as_ref().err().cloned())
        .or_else(|| state.borrow().failure.clone());
    let mechanics_ok = scenario
        .as_ref()
        .is_some_and(|s| s["mechanics"] == "OBSERVED");
    let successful = error.is_none() && mechanics_ok && cleanup["complete"] == true;
    state
        .borrow_mut()
        .push("ProbeFinished", json!({"mechanicsSuccessful": successful}));
    persist(
        output,
        &state,
        json!({
            "status": if successful { "OBSERVED" } else { "FAILED" },
            "nativeCreated": true, "runtimeVersion": runtime,
            "finalTitle": title_result.unwrap_or_default(),
            "browserExited": cleanup["browserExitEventObserved"],
            "sourcePin": source_pin, "scenario": scenario, "error": error, "cleanup": cleanup
        }),
    )?;
    Ok(if successful { 0 } else { 1 })
}
