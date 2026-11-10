//! Independent page/application outcome binding and native byte/no-effect oracles.
use super::private_diagnostics::{Phase, assert_private_eq, assert_report_phase};
use super::project::{
    COMPONENT_SENTINEL, CONTENT, INTERNAL_INITIAL, LINK_SENTINEL, NaturalCase, NaturalInputs,
    Policy, PreparedLaunch, READ_SENTINEL, SENTINEL, WRITE_SENTINEL, file_digest,
};
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
        assert_private_eq(
            handler,
            &json!({"call":index + 1,"pid":bun,
            "action":result["requests"][index]["action"],"nonce":result["requests"][index]["nonce"]}),
            "public FS handler trace record",
        );
    }
    for (index, outcome) in outcomes.iter().enumerate() {
        assert_private_eq(
            outcome,
            &json!({"call":index + 1,"nonce":result["requests"][index]["nonce"],
            "outcome":result["outcomes"][index]["outcome"]}),
            "public FS outcome trace record",
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
    assert_report_phase(result, "page-result", Phase::Observation);
    assert_eq!(
        result["trusted"], true,
        "synthetic DOM input is not acceptance"
    );
    for key in ["documentNonce", "javascriptToken", "initial"] {
        assert_private_eq(&before[key], &result[key], key);
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

pub(super) fn assert_natural_outcomes(
    reports: &[Value],
    policy: Policy,
    prepared: &PreparedLaunch,
    outside: &Path,
    bun: u32,
) {
    let completed: Vec<_> = reports
        .iter()
        .filter(|r| r["phase"] == "app-result")
        .collect();
    assert_eq!(
        completed.len(),
        1,
        "one natural public-main terminal result"
    );
    let result = completed[0];
    assert_eq!(result["pid"], bun);
    assert_eq!(result["componentOnly"], true);
    let ready = reports
        .iter()
        .find(|r| r["phase"] == "page-ready")
        .expect("natural document ready");
    assert_eq!(ready["requests"], json!([]));
    assert_eq!(ready["outcomes"], json!([]));
    assert!(
        matches!(fs::symlink_metadata(&prepared.target), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "natural cases must never reuse or create the original component target"
    );
    let input = prepared
        .natural
        .as_ref()
        .expect("owned natural fixture inputs");
    let outcomes = result["outcomes"]
        .as_array()
        .expect("closed natural outcome list");
    match input.case {
        NaturalCase::TraversalBounds => {
            assert!(matches!(policy, Policy::Narrow));
            assert_traversal_sentinels(input);
            assert_traversal_results(result, outcomes);
            assert_eq!(
                fs::read(&input.fresh).expect("healthy file after both traversal refusals"),
                CONTENT
            );
        }
        NaturalCase::LinksLimits => {
            assert!(matches!(policy, Policy::Narrow));
            assert_oversized_write_no_effect(input);
            assert_links_limits_results(result, outcomes, input);
            assert_links_limits_files(input);
        }
        NaturalCase::DenyRead => {
            assert!(!matches!(policy, Policy::Narrow));
            assert_denied_read_result(result, outcomes, input);
        }
    }
    assert_eq!(
        fs::read(&input.read_sentinel).expect("independent read sentinel"),
        READ_SENTINEL
    );
    assert_eq!(
        fs::read(outside).expect("independent outside alias no-effect sentinel"),
        SENTINEL
    );
}

fn assert_traversal_sentinels(input: &NaturalInputs) {
    // Direct OS effects are checked before either returned denial is trusted.
    // Each distinct seeded file falsifies only its own native-budget atom.
    for (path, original, boundary) in [
        (&input.component_sentinel, COMPONENT_SENTINEL, "component"),
        (&input.link_sentinel, LINK_SENTINEL, "link"),
    ] {
        assert_eq!(
            fs::metadata(path)
                .expect("traversal sentinel metadata")
                .len(),
            u64::try_from(original.len()).expect("small sentinel length fits u64"),
            "{boundary} boundary write changed independent sentinel length"
        );
        assert_eq!(
            fs::read(path).expect("independent traversal no-effect sentinel"),
            original,
            "{boundary} boundary write changed independent sentinel bytes"
        );
    }
}

fn assert_traversal_results(result: &Value, outcomes: &[Value]) {
    assert_eq!(result["naturalCase"], "traversal-bounds");
    let actions = [
        "components-pass-read",
        "components-deny-write",
        "links-pass-read",
        "links-deny-write",
        "healthy-write",
        "healthy-read",
    ];
    assert_eq!(outcomes.len(), actions.len());
    for (actual, expected) in outcomes.iter().zip(actions) {
        assert_eq!(actual["action"], expected);
    }
    assert_eq!(
        outcomes[0]["outcome"],
        json!({"ok":true,"bytes":COMPONENT_SENTINEL})
    );
    assert_eq!(
        outcomes[1]["outcome"],
        json!({"ok":false,"code":"KELD-NATIVE-004",
        "message":"KELD-NATIVE-004: filesystem processed-components limit exceeded (257 > 256). Shorten the request or use at most 8 MiB."})
    );
    assert_eq!(
        outcomes[2]["outcome"],
        json!({"ok":true,"bytes":LINK_SENTINEL})
    );
    assert_eq!(
        outcomes[3]["outcome"],
        json!({"ok":false,"code":"KELD-NATIVE-004",
        "message":"KELD-NATIVE-004: filesystem symlink-expansions limit exceeded (41 > 40). Shorten the request or use at most 8 MiB."})
    );
    assert_eq!(outcomes[4]["outcome"], json!({"ok":true,"bytes":[]}));
    assert_eq!(outcomes[5]["outcome"], json!({"ok":true,"bytes":CONTENT}));
}

fn assert_oversized_write_no_effect(input: &NaturalInputs) {
    // Check the OS effect before interpreting reported native outcomes. A
    // missing native limit fails on a short length predicate, never an 8 MiB
    // byte-array panic dump that obscures the independent sentinel finding.
    assert_eq!(
        fs::metadata(&input.oversized_write)
            .expect("oversized write target metadata")
            .len(),
        u64::try_from(WRITE_SENTINEL.len()).expect("sentinel length fits u64"),
        "oversized write changed the independent target length"
    );
    assert_eq!(
        fs::read(&input.oversized_write).expect("oversized write no-effect sentinel"),
        WRITE_SENTINEL
    );
}

fn assert_denied_read_result(result: &Value, outcomes: &[Value], input: &NaturalInputs) {
    assert_eq!(result["naturalCase"], "deny-read");
    assert_eq!(outcomes.len(), 1, "direct read has no preceding write");
    assert_eq!(outcomes[0]["action"], "denied-read");
    assert_eq!(
        outcomes[0]["outcome"],
        json!({"ok":false,"code":"KELD-GUARD001","message":format!(
            "KELD-GUARD001: capability `fs.read` is not granted. Append \"{}\" to `/app/fs/read` in keld.permissions.jsonc.",
            input.read_sentinel.display()
        )})
    );
}

fn assert_links_limits_results(result: &Value, outcomes: &[Value], input: &NaturalInputs) {
    assert_eq!(result["naturalCase"], "links-limits");
    let actions = [
        "internal-read",
        "internal-write",
        "internal-read-after-write",
        "external-read",
        "external-write",
        "oversize-write",
        "oversize-read",
        "healthy-write",
        "healthy-read",
    ];
    assert_eq!(outcomes.len(), actions.len());
    for (actual, expected) in outcomes.iter().zip(actions) {
        assert_eq!(actual["action"], expected);
    }
    assert_eq!(
        outcomes[0]["outcome"],
        json!({"ok":true,"bytes":INTERNAL_INITIAL})
    );
    assert_eq!(outcomes[1]["outcome"], json!({"ok":true,"bytes":[]}));
    assert_eq!(outcomes[2]["outcome"], json!({"ok":true,"bytes":CONTENT}));
    // Full literal oracle from FsError::ResolvedOutOfScope Display and the
    // retained walk's Parent-at-root failure, not the returned message.
    let escaped = json!({"ok":false,"code":"KELD-NATIVE-002","message":format!(
        "KELD-NATIVE-002: `{}` escaped or changed outside its retained scope — relative link escaped above the retained root. Move the target beneath the granted root or use a direct approved scope.",
        input.external_alias.display()
    )});
    assert_eq!(outcomes[3]["outcome"], escaped);
    assert_eq!(outcomes[4]["outcome"], escaped);
    let limited = json!({"ok":false,"code":"KELD-NATIVE-004","message":format!(
        "KELD-NATIVE-004: filesystem content-bytes limit exceeded ({} > {}). Shorten the request or use at most 8 MiB.",
        input.maximum_content + 1, input.maximum_content
    )});
    assert_eq!(outcomes[5]["outcome"], limited);
    assert_eq!(outcomes[6]["outcome"], limited);
    assert_eq!(outcomes[7]["outcome"], json!({"ok":true,"bytes":[]}));
    assert_eq!(outcomes[8]["outcome"], json!({"ok":true,"bytes":CONTENT}));
}

fn assert_links_limits_files(input: &NaturalInputs) {
    assert_eq!(
        fs::read(&input.internal_target).expect("independent internal write bytes"),
        CONTENT
    );
    assert_eq!(
        fs::read(&input.internal_alias).expect("independent internal alias bytes"),
        CONTENT
    );
    assert_eq!(
        fs::read_link(&input.internal_alias).expect("relative internal alias remains"),
        Path::new(
            input
                .internal_target
                .file_name()
                .expect("internal relative leaf")
        )
    );
    assert_eq!(
        fs::read_link(&input.external_alias).expect("escaping alias remains"),
        Path::new("../outside.txt")
    );
    assert_eq!(
        fs::metadata(&input.oversized_read)
            .expect("oversized read source metadata")
            .len(),
        u64::try_from(input.maximum_content + 1).expect("fixture length fits u64")
    );
    assert_eq!(
        file_digest(&input.oversized_read),
        input.oversized_read_digest
    );
    assert_eq!(
        fs::read(&input.fresh).expect("healthy operation after both limits"),
        CONTENT
    );
}
