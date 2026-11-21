"""Independent fixture oracles. These do not decide production bridge admission."""
import json

from prepare_probe import PRELOAD_SHA256


def message_body(record):
    body = record.get("body")
    for _ in range(2):
        if isinstance(body, str):
            body = json.loads(body)
    return body if isinstance(body, dict) else {}


def matching(result, kind):
    return [(record, message_body(record)) for record in result["messages"]
            if message_body(record).get("kind") == kind]


def require_cleanup(result):
    assert result["nativeCreated"] is True, result
    assert result["status"] == "OBSERVED", result
    assert result["traceFailure"] is None, result
    assert result["browserExited"] is True, result
    cleanup = result["cleanup"]
    for name in ("controllerClosed", "browserExitEventObserved",
                 "browserHandleSignaled", "profileRemoved", "windowDestroyed"):
        assert cleanup[name] is True, (name, cleanup)
    assert cleanup["complete"] is True, cleanup


def assert_digest_rejection(result, expected, observed):
    assert result["status"] == "source-rejected", result
    assert result["nativeCreated"] is False, result
    assert result["cleanup"]["required"] is False, result
    assert result["cleanup"]["complete"] is True, result
    assert result["expectedBundleSha256"] == expected, result
    assert result["observedBundleSha256"] == observed, result
    assert expected != observed
    assert result["registrations"] == result["navigations"] == 0, result
    assert result["messages"] == [], result


def assert_order(result, config, prepared, early):
    require_cleanup(result)
    first = matching(result, "first-inline")
    completed = matching(result, "preload-complete")
    assert len(first) == len(completed) == 1, (first, completed)
    first_record, first_body = first[0]
    complete_record, complete_body = completed[0]
    assert first_record["source"] == config["url"], first_record
    assert complete_record["source"] == config["url"], complete_record
    observed = first_body["observed"]
    prefix = "W1_FIRST_INLINE:"
    assert result["finalTitle"].startswith(prefix), result["finalTitle"]
    # Engine-read title and WebMessage delivery independently retain the first snapshot.
    assert json.loads(result["finalTitle"][len(prefix):]) == observed
    assert complete_body["sourceDigest"] == PRELOAD_SHA256, complete_body
    assert complete_body["registeredChannels"] == ["mainResp", "fileChanged"]
    assert complete_body["contextIsolated"] is False
    assert observed["apiPresent"] is early, observed
    assert observed["requireExposed"] is False, observed
    sinks = matching(result, "preload-sink-observation")
    if early:
        assert complete_record["nativeSeq"] < first_record["nativeSeq"]
        assert observed["contextIsolated"] is False
        assert observed["keys"] == ["listenOnce", "registerMsgListener", "request", "sendMessage"]
        assert observed["types"] == ["function"] * 4
        assert observed["processType"] == "renderer"
        assert observed["frozen"] is True
        assert observed["configuration"] == {
            "defaultAdaptiveColors": prepared["expectedConfiguration"]
        }, observed
        assert len(sinks) == 2, sinks
        by_channel = {body["channel"]: body["args"] for _, body in sinks}
        assert by_channel == {
            "rendererReq": [{"action": "watchFile", "path": "fixture-only.txt", "reqId": 1}],
            "fixture-only": [{"value": "page"}],
        }, by_channel
        assert all(record["source"] == config["url"] for record, _ in sinks)
    else:
        assert first_record["nativeSeq"] < complete_record["nativeSeq"]
        assert observed["keys"] == []
        assert observed["types"] == ["undefined"] * 4
        assert observed["processType"] is None
        assert observed["configuration"] is None
        assert observed["contextIsolated"] is None
        assert observed["frozen"] is False
        assert sinks == [], sinks
    return {"firstInlineApiPresent": observed["apiPresent"],
            "preloadCompleted": True, "sinkMessages": len(sinks)}


def assert_stale_delivery(result, config, unfenced):
    require_cleanup(result)
    created = matching(result, "probe-created")
    assert any(body.get("initialHref") == config["successorUrl"]
               for _, body in created), created
    assert result["scenario"]["heldReplyAttempted"] is True, result
    raw = [(record, body) for record, body in matching(result, "probe-raw-delivery")
           if body.get("stale") is True]
    if unfenced:
        assert len(raw) >= 1, result
        assert all(body.get("initialHref") == config["successorUrl"]
                   for _, body in raw), raw
    else:
        assert len(raw) == 0, raw
    return {"stalePageDeliveries": len(raw), "unfenced": unfenced,
            "scope": "deliberately held synthetic reply; not a natural B7 race"}


def assert_natural_forcing(result, config):
    require_cleanup(result)
    scenario = result["scenario"]
    assert scenario["raceOrigin"] == "NATURAL", scenario
    loading = scenario["successorContentLoading"]
    assert loading["source"] == config["successorUrl"], loading
    tag = scenario["oldDocumentObservationTag"]
    old = [(record, body) for record, body in matching(result, "forcing-burst")
           if record["source"] == config["url"] and body.get("tag") == tag]
    at_risk = [record for record, _ in old
               if record["nativeSeq"] > loading["nativeSeq"]]
    assert len(old) == scenario["oldMessagesObserved"], scenario
    assert len(at_risk) == scenario["atRiskCount"], scenario
    if config["variant"] == "no-force":
        assert not at_risk, ("unforced control unexpectedly has late arrivals", at_risk)
    return {
        "oldMessagesObserved": len(old), "atRiskCount": len(at_risk),
        "ruleExercise": "ARRIVAL_COHORT_OBSERVED" if at_risk else "RULE_NOT_EXERCISED",
        "fullRuleDiscrimination": "NOT_RUN", "busyMs": config["busyMs"],
        "variant": config["variant"], "scope": "B7 Arm F subset; one trial per case",
    }
