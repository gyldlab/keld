//! Independent page/application outcome binding and native byte/no-effect oracles.
use super::project::{CONTENT, Policy, SENTINEL};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

pub(super) fn assert_handler_attribution(stderr: &str, result: &Value, bun: u32) {
    let handlers: Vec<Value> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("KELD_KEL140_PUBLIC_HANDLER "))
        .map(|line| serde_json::from_str(line).expect("actual application handler trace"))
        .collect();
    let outcomes: Vec<Value> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("KELD_KEL140_PUBLIC_OUTCOME "))
        .map(|line| serde_json::from_str(line).expect("actual public FS outcome trace"))
        .collect();
    assert_eq!(handlers.len(), 3);
    assert_eq!(outcomes.len(), 2);
    for (index, handler) in handlers.iter().enumerate() {
        assert_eq!(
            handler,
            &json!({"call":index + 1,"pid":bun,
            "action":result["requests"][index]["action"],"nonce":result["requests"][index]["nonce"]})
        );
    }
    for (index, outcome) in outcomes.iter().enumerate() {
        assert_eq!(
            outcome,
            &json!({"call":index + 1,"nonce":result["requests"][index]["nonce"],
            "outcome":result["outcomes"][index]["outcome"]})
        );
    }
}

pub(super) fn assert_page_outcomes(
    reports: &[Value],
    policy: Policy,
    target: &Path,
    outside: &Path,
    scope: &str,
) {
    let before = reports
        .iter()
        .find(|report| report["phase"] == "page-ready")
        .expect("listener ready");
    let result = reports.last().expect("page result");
    assert_eq!(result["phase"], "page-result", "{result}");
    assert_eq!(
        result["trusted"], true,
        "synthetic DOM input is not acceptance"
    );
    for key in ["documentNonce", "javascriptToken", "initial"] {
        assert_eq!(before[key], result[key]);
    }
    assert_eq!(
        result["requests"].as_array().expect("request census").len(),
        3
    );
    assert_eq!(
        result["outcomes"].as_array().expect("outcome census").len(),
        3
    );
    for (index, action) in ["roundtrip", "deny", "unknown"].iter().enumerate() {
        assert_eq!(result["requests"][index]["action"], *action);
        assert_eq!(result["requests"][index]["count"], index + 1);
        let nonce = result["requests"][index]["nonce"]
            .as_str()
            .expect("page nonce");
        assert_eq!(nonce.len(), 32);
        assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(result["outcomes"][index]["action"], *action);
    }
    assert_fs_operation_outcomes(
        &result["outcomes"][0]["outcome"],
        &result["outcomes"][1]["outcome"],
        policy,
        target,
        outside,
        scope,
    );
    assert_eq!(
        result["outcomes"][2],
        json!({"action":"unknown","code":null,
        "error":"KELD-API-001: application channel call failed"})
    );
}

fn assert_fs_operation_outcomes(
    allowed_outcome: &Value,
    denied_outcome: &Value,
    policy: Policy,
    target: &Path,
    outside: &Path,
    scope: &str,
) {
    let denied = if matches!(policy, Policy::Narrow) {
        assert_eq!(*allowed_outcome, json!({"ok":true,"bytes":CONTENT}));
        assert_eq!(
            fs::read(target).expect("independent native allowed bytes"),
            CONTENT
        );
        format!(
            "KELD-GUARD002: capability `fs.write` denied by scope `{scope}`. Widen `/app/fs/write` in keld.permissions.jsonc so it includes `{}`.",
            outside.display()
        )
    } else {
        assert!(
            matches!(fs::symlink_metadata(target), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "all-denied public write had an OS effect"
        );
        assert_eq!(
            *allowed_outcome,
            json!({"ok":false,"code":"KELD-GUARD001",
            "message":format!("KELD-GUARD001: capability `fs.write` is not granted. Append \"{}\" to `/app/fs/write` in keld.permissions.jsonc.", target.display())})
        );
        format!(
            "KELD-GUARD001: capability `fs.write` is not granted. Append \"{}\" to `/app/fs/write` in keld.permissions.jsonc.",
            outside.display()
        )
    };
    let code = if matches!(policy, Policy::Narrow) {
        "KELD-GUARD002"
    } else {
        "KELD-GUARD001"
    };
    assert_eq!(
        *denied_outcome,
        json!({"ok":false,"code":code,"message":denied})
    );
    assert_eq!(
        fs::read(outside).expect("independent outside no-effect sentinel"),
        SENTINEL
    );
}

pub(super) fn assert_component_outcomes(
    reports: &[Value],
    policy: Policy,
    target: &Path,
    outside: &Path,
    scope: &str,
    bun: u32,
) {
    let completed: Vec<_> = reports
        .iter()
        .filter(|report| report["phase"] == "app-result")
        .collect();
    assert_eq!(completed.len(), 1, "one actual public-main result");
    let result = completed[0];
    assert_eq!(result["pid"], bun);
    assert_eq!(result["componentOnly"], true);
    assert_eq!(
        result["outcomes"]
            .as_array()
            .expect("component outcome census")
            .len(),
        2
    );
    assert_eq!(result["outcomes"][0]["action"], "roundtrip");
    assert_eq!(result["outcomes"][1]["action"], "deny");
    let ready = reports
        .iter()
        .find(|report| report["phase"] == "page-ready")
        .expect("actual component document");
    assert_eq!(ready["requests"], json!([]));
    assert_eq!(ready["outcomes"], json!([]));
    assert_fs_operation_outcomes(
        &result["outcomes"][0]["outcome"],
        &result["outcomes"][1]["outcome"],
        policy,
        target,
        outside,
        scope,
    );
}

pub(super) fn assert_component_trace(stderr: &str, reports: &[Value]) {
    let trace: Vec<Value> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("KELD_KEL140_PUBLIC_APP_COMPONENT "))
        .map(|line| serde_json::from_str(line).expect("actual public-main outcome trace"))
        .collect();
    let observed: Vec<_> = reports
        .iter()
        .filter(|report| report["phase"] == "app-result")
        .collect();
    assert_eq!(trace.len(), 1);
    assert_eq!(observed.len(), 1);
    assert_eq!(
        &trace[0], observed[0],
        "public-main terminal outcome bound to HTTP observation"
    );
}
