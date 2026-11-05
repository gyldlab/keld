//! Bounded native observations; no KELD admission rule or transport is copied here.
use crate::config::Config;
use crate::native::Session;
use crate::pump::{enqueue_execute, hr, pump_until, register_script, wait};
use crate::trace::{Shared, observed};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use windows::core::HSTRING;

fn navigate(
    wv: &ICoreWebView2,
    state: &Shared,
    config: &Config,
    url: &str,
) -> Result<Value, String> {
    let called = state
        .borrow_mut()
        .push("NavigateCalled", json!({"url": url}));
    // SAFETY: explicit fixture URL; navigation remains on the owning STA.
    unsafe { wv.Navigate(&HSTRING::from(url)) }.map_err(|e| format!("Navigate: {e}"))?;
    wait(
        state,
        config.timeout,
        "matching navigation completion",
        || {
            state.borrow().events.iter().any(|event| {
                event["event"] == "NavigationCompleted"
                    && event["source"].as_str() == Some(url)
                    && event["nativeSeq"].as_u64().is_some_and(|seq| seq > called)
            })
        },
    )?;
    let completed = state
        .borrow()
        .events
        .iter()
        .find(|event| {
            event["event"] == "NavigationCompleted"
                && event["source"].as_str() == Some(url)
                && event["nativeSeq"].as_u64().is_some_and(|seq| seq > called)
        })
        .cloned()
        .ok_or("matching completion lost")?;
    if completed["isSuccess"] != true {
        return Err(format!("fixture navigation failed: {completed}"));
    }
    wait(
        state,
        config.timeout,
        "document-created raw observer",
        || observed(state, "probe-created", url).is_some(),
    )?;
    wait(state, config.timeout, "page DOM-ready observation", || {
        observed(state, "probe-dom-ready", url).is_some()
    })?;
    let id = completed["navigationId"]
        .as_u64()
        .ok_or("navigation ID absent")?;
    if !state.borrow().events.iter().any(|event| {
        event["event"] == "ContentLoading"
            && event["navigationId"].as_u64() == Some(id)
            && event["source"].as_str() == Some(url)
    }) {
        return Err("matching ContentLoading not observed".to_owned());
    }
    Ok(completed)
}

fn observation_window(state: &Shared, millis: u64) -> Result<(), String> {
    state
        .borrow_mut()
        .push("ObservationWindowStarted", json!({"durationMs": millis}));
    // This fixed interval is the measured exposure window, not a synchronization sleep.
    pump_until(Instant::now() + Duration::from_millis(millis), || {
        state.borrow().failure.is_some()
    });
    if let Some(error) = state.borrow().failure.clone() {
        return Err(error);
    }
    state
        .borrow_mut()
        .push("ObservationWindowEnded", json!({"durationMs": millis}));
    Ok(())
}

fn post(wv: &ICoreWebView2, state: &Shared, value: &Value) -> Result<(), String> {
    // SAFETY: test-only payload sent through the native API on its owning STA.
    let result = unsafe { wv.PostWebMessageAsString(&HSTRING::from(value.to_string())) };
    state.borrow_mut().push(
        "PostWebMessageAsString",
        json!({"payload": value, "status": hr(&result)}),
    );
    result.map_err(|e| e.to_string())
}

fn raw_receipts(state: &Shared, source: &str, payload_kind: &str) -> Vec<Value> {
    state
        .borrow()
        .messages
        .iter()
        .filter(|message| {
            message["source"].as_str() == Some(source)
                && message["body"]["kind"] == "probe-raw-delivery"
                && message["body"]["value"]["kind"].as_str() == Some(payload_kind)
        })
        .cloned()
        .collect()
}

fn order(wv: &ICoreWebView2, state: &Shared, config: &Config) -> Result<Value, String> {
    navigate(wv, state, config, &config.url)?;
    wait(
        state,
        config.timeout,
        "page's first-inline observation",
        || observed(state, "first-inline", &config.url).is_some(),
    )?;
    if config.variant == "dom-content-loaded" {
        wait(state, config.timeout, "late bundle callback", || {
            state.borrow().late_injection_done
        })?;
    }
    wait(
        state,
        config.timeout,
        "eventual preload-complete observation",
        || observed(state, "preload-complete", &config.url).is_some(),
    )?;
    if config.variant == "document-created" {
        for channel in ["rendererReq", "fixture-only"] {
            wait(
                state,
                config.timeout,
                "early page's independent sink observation",
                || {
                    state.borrow().messages.iter().any(|m| {
                        m["source"].as_str() == Some(&config.url)
                            && m["body"]["kind"] == "preload-sink-observation"
                            && m["body"]["channel"].as_str() == Some(channel)
                    })
                },
            )?;
        }
    }
    let first =
        observed(state, "first-inline", &config.url).ok_or("first-inline observation lost")?;
    Ok(json!({
        "mechanics": "OBSERVED",
        "scope": "native document-start ordering with root-supplied fixture bundle",
        "injection": config.variant,
        "firstInline": first, "preloadComplete": observed(state, "preload-complete", &config.url),
        "pageDomReady": observed(state, "probe-dom-ready", &config.url),
        "productAdmission": "NOT_TESTED",
        "bunHandlerEntry": "NOT_TESTED"
    }))
}

fn stale(wv: &ICoreWebView2, state: &Shared, config: &Config) -> Result<Value, String> {
    navigate(wv, state, config, &config.url)?;
    let old = observed(state, "probe-created", &config.url).ok_or("old observer absent")?;
    let old_id = old["documentId"]
        .as_str()
        .ok_or("old document tag absent")?;
    let captured_serial = state.borrow().commit_serial;
    let payload = json!({"kind": "probe-held-reply", "fixtureEpoch": old_id,
        "marker": "old-document-held-native-reply"});
    state.borrow_mut().push(
        "FixtureHeldReplyCaptured",
        json!({
            "capturedCommitSerial": captured_serial, "oldDocumentTag": old_id,
            "scope": "synthetic fixture capture, no Bun operation or production grant"
        }),
    );
    let successor = config.successor.as_deref().ok_or("successor absent")?;
    let completion = navigate(wv, state, config, successor)?;
    let next = observed(state, "probe-created", successor).ok_or("successor observer absent")?;
    let next_id = next["documentId"]
        .as_str()
        .ok_or("successor document tag absent")?;
    let current_serial = state.borrow().commit_serial;
    if old_id == next_id || captured_serial == current_serial {
        return Err("replacement did not change observation tag and commit serial".to_owned());
    }
    let released = config.variant == "unfenced" || captured_serial == current_serial;
    state.borrow_mut().push(
        "FixtureHeldReplyDecision",
        json!({
            "variant": config.variant, "capturedCommitSerial": captured_serial,
            "currentCommitSerial": current_serial, "released": released,
            "decisionScope": "synthetic commit-serial fence, not KELD security policy",
            "releaseBoundary": "after successor ContentLoading and raw observer ready"
        }),
    );
    if released {
        post(wv, state, &payload)?;
    }
    // A current-document post proves the independently registered observer is live.
    post(
        wv,
        state,
        &json!({"kind": "probe-current-barrier", "fixtureEpoch": next_id,
        "marker": "healthy-successor-raw-observer"}),
    )?;
    wait(
        state,
        config.timeout,
        "current successor raw delivery",
        || !raw_receipts(state, successor, "probe-current-barrier").is_empty(),
    )?;
    if released {
        wait(
            state,
            config.timeout,
            "unfenced old payload raw delivery",
            || !raw_receipts(state, successor, "probe-held-reply").is_empty(),
        )?;
    }
    observation_window(state, config.observe_ms)?;
    let receipts = raw_receipts(state, successor, "probe-held-reply");
    let raw_stale = receipts
        .iter()
        .filter(|r| r["body"]["stale"] == true)
        .count();
    let expected = if released {
        raw_stale > 0
    } else {
        receipts.is_empty()
    };
    Ok(json!({
        "mechanics": if expected { "OBSERVED" } else { "UNEXPECTED" },
        "scope": "synthetic held-reply fence; native replacement and page-visible delivery",
        "raceOrigin": "SYNTHETIC",
        "b7NaturalArmH": "NOT_EXERCISED",
        "variant": config.variant, "oldDocumentTag": old_id, "newDocumentTag": next_id,
        "capturedCommitSerial": captured_serial, "currentCommitSerial": current_serial,
        "successorNavigationId": completion["navigationId"],
        "heldReplyAttempted": true, "oldHeldReplies": 1, "oldNativePosts": usize::from(released),
        "rawOldDeliveries": receipts.len(), "rawStaleDeliveries": raw_stale,
        "rawReceipts": receipts, "healthyCurrentDeliveries":
            raw_receipts(state, successor, "probe-current-barrier").len(),
        "observationMs": config.observe_ms, "expectedObservation": expected,
        "absenceScope": "only this bounded fixture interval",
        "productRevocation": "NOT_TESTED"
    }))
}

fn contentloading_race(
    wv: &ICoreWebView2,
    session: &mut Session,
    state: &Shared,
    config: &Config,
) -> Result<Value, String> {
    navigate(wv, state, config, &config.url)?;
    let before = session.census(state, "before-forcing")?;
    let source = include_str!("forcing.js")
        .replace(
            "__VARIANT__",
            &serde_json::to_string(&config.variant).map_err(|e| e.to_string())?,
        )
        .replace("__BURST__", &config.burst.to_string())
        .replace("__BUSY_MS__", &config.busy_ms.to_string());
    enqueue_execute(wv, state, &source)?;
    wait(state, config.timeout, "old-document forcing-start", || {
        observed(state, "forcing-start", &config.url).is_some()
    })?;
    let forcing = observed(state, "forcing-start", &config.url).ok_or("forcing marker lost")?;
    let tag = forcing["tag"]
        .as_str()
        .ok_or("forcing observation tag absent")?;
    let successor = config.successor.as_deref().ok_or("successor absent")?;
    let completion = navigate(wv, state, config, successor)?;
    observation_window(state, config.observe_ms)?;
    let after = session.census(state, "after-forcing")?;
    let loading = state
        .borrow()
        .events
        .iter()
        .find(|event| {
            event["event"] == "ContentLoading"
                && event["navigationId"] == completion["navigationId"]
        })
        .cloned()
        .ok_or("successor ContentLoading absent")?;
    let loading_seq = loading["nativeSeq"]
        .as_u64()
        .ok_or("ContentLoading sequence absent")?;
    let old_messages: Vec<Value> = state
        .borrow()
        .messages
        .iter()
        .filter(|message| {
            message["source"].as_str() == Some(&config.url)
                && message["body"]["kind"] == "forcing-burst"
                && message["body"]["tag"].as_str() == Some(tag)
        })
        .cloned()
        .collect();
    let at_risk: Vec<Value> = old_messages
        .iter()
        .filter(|message| {
            message["nativeSeq"]
                .as_u64()
                .is_some_and(|seq| seq > loading_seq)
        })
        .cloned()
        .collect();
    Ok(json!({
        "mechanics": "OBSERVED",
        "scope": "native B7 Arm F candidate-arrival forcing only",
        "raceOrigin": "NATURAL", "variant": config.variant,
        "busyMs": config.busy_ms, "requestedBurst": config.burst,
        "oldDocumentObservationTag": tag, "oldMessagesObserved": old_messages.len(),
        "successorContentLoading": loading, "atRiskCount": at_risk.len(),
        "atRiskMessages": at_risk,
        "ruleExercise": if at_risk.is_empty() { "RULE_NOT_EXERCISED" } else { "ARRIVAL_COHORT_OBSERVED" },
        "fullRuleDiscrimination": "NOT_RUN; no production admission policy copied",
        "observationMs": config.observe_ms, "processCensusBefore": before,
        "processCensusAfter": after, "rendererSwap": "inspect native census; not inferred from URL",
        "bfcache": "NOT_EXERCISED"
    }))
}

pub(crate) fn run(config: &Config, session: &mut Session, state: &Shared) -> Result<Value, String> {
    let wv = session.webview.as_ref().ok_or("WebView absent")?.clone();
    let observer = register_script(
        &wv,
        state,
        include_str!("observer.js"),
        "independent-raw-page-observer",
        config.timeout,
    )?;
    let bundle = if config.mode == "order" && config.variant == "document-created" {
        Some(register_script(
            &wv,
            state,
            config.bundle.as_deref().ok_or("bundle absent")?,
            "root-supplied-document-start-bundle",
            config.timeout,
        )?)
    } else {
        None
    };
    state.borrow_mut().push("AllDocumentStartRegistrationsCompleted",
        json!({"observerScriptId": observer, "bundleScriptId": bundle, "loadedBundleSha256": config.bundle_digest}));
    match config.mode.as_str() {
        "order" => order(&wv, state, config),
        "stale-delivery" => stale(&wv, state, config),
        "contentloading-race" => contentloading_race(&wv, session, state, config),
        _ => Err("unsupported mode".to_owned()),
    }
}
