//! Integration tests for `keld doctor --json` and `keld mcp` (KEL-42).

#![allow(clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use keld_cli::create::create_project;
use keld_cli::doctor::{BUN_MISSING_CODE, BUN_MISSING_FIX, PROJECT_LAYOUT_CODE, run_findings};
use serde_json::Value;

fn keld_bin() -> &'static str {
    env!("CARGO_BIN_EXE_keld")
}

fn empty_path_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("empty PATH dir")
}

#[test]
fn doctor_json_emits_findings_array_matching_library() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");

    let output = Command::new(keld_bin())
        .args(["doctor", "--json"])
        .current_dir(&project)
        .output()
        .expect("spawn doctor --json");

    assert!(
        output.status.success(),
        "doctor --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(stdout.trim()).expect("parse doctor json");
    let arr = value.as_array().expect("top-level array");
    assert!(!arr.is_empty());
    for item in arr {
        assert!(item.get("label").and_then(Value::as_str).is_some());
        assert!(item.get("ok").and_then(Value::as_bool).is_some());
        assert!(item.get("detail").and_then(Value::as_str).is_some());
        if item["ok"] == Value::Bool(false) {
            let code = item
                .pointer("/error/code")
                .and_then(Value::as_str)
                .expect("failed finding needs error.code");
            assert!(code.starts_with("KELD-"), "{code}");
            assert!(
                item.pointer("/error/fix")
                    .and_then(Value::as_str)
                    .is_some_and(|f| !f.is_empty()),
                "failed finding needs error.fix: {item}"
            );
        }
    }

    let lib = run_findings(Some(project.as_path()));
    assert_eq!(arr.len(), lib.len());
    for (cli, lib_f) in arr.iter().zip(lib.iter()) {
        assert_eq!(cli["label"].as_str(), Some(lib_f.label.as_str()));
        assert_eq!(cli["ok"].as_bool(), Some(lib_f.ok));
        if lib_f.label != "project" {
            assert_eq!(cli["detail"].as_str(), Some(lib_f.detail.as_str()));
        }
        assert_eq!(
            cli.get("error").is_some(),
            lib_f.error.is_some(),
            "error presence mismatch for {}",
            lib_f.label
        );
    }
}

#[test]
fn doctor_json_bun_missing_fix_exact_with_empty_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let empty = empty_path_dir();
    let output = Command::new(keld_bin())
        .args(["doctor", "--json"])
        .current_dir(dir.path())
        .env("PATH", empty.path())
        .env("Path", empty.path())
        .output()
        .expect("spawn doctor");

    assert_eq!(output.status.code(), Some(1));
    let value: Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("json");
    let bun = value
        .as_array()
        .expect("array")
        .iter()
        .find(|f| f.get("label") == Some(&Value::String("bun".into())))
        .expect("bun finding");
    assert_eq!(bun.get("ok"), Some(&Value::Bool(false)));
    assert_eq!(
        bun.pointer("/error/code").and_then(Value::as_str),
        Some(BUN_MISSING_CODE)
    );
    let fix = bun
        .pointer("/error/fix")
        .and_then(Value::as_str)
        .expect("fix");
    assert_eq!(fix, BUN_MISSING_FIX);
}

#[test]
fn doctor_json_project_layout_code_when_files_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    // `keld.config.ts` makes this a project root; missing `src/main.ts` must fail.
    // An empty cwd is *not* a failure (`project` is ok with "no project directory").
    std::fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
    let output = Command::new(keld_bin())
        .args(["doctor", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("spawn doctor");
    assert_eq!(output.status.code(), Some(1));
    let value: Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("json");
    let project = value
        .as_array()
        .expect("array")
        .iter()
        .find(|f| f.get("label") == Some(&Value::String("project".into())))
        .expect("project finding");
    assert_eq!(project.get("ok"), Some(&Value::Bool(false)));
    assert_eq!(
        project.pointer("/error/code").and_then(Value::as_str),
        Some(PROJECT_LAYOUT_CODE)
    );
    let fix = project
        .pointer("/error/fix")
        .and_then(Value::as_str)
        .expect("fix");
    assert!(fix.contains("keld create"), "{fix}");
}

#[test]
fn mcp_unknown_subcommand_exits_2() {
    let output = Command::new(keld_bin())
        .args(["mcp", "bogus"])
        .output()
        .expect("spawn mcp bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage: keld mcp serve"),
        "misuse must print usage, got {stderr}"
    );
    assert!(
        stderr.contains("bogus"),
        "stderr must name the bad subcommand: {stderr}"
    );
}

#[test]
fn mcp_missing_subcommand_exits_2() {
    let output = Command::new(keld_bin())
        .arg("mcp")
        .output()
        .expect("spawn mcp");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage: keld mcp serve"), "{stderr}");
}

#[test]
fn mcp_server_discover_advertises_versions_and_identity() {
    let response = mcp_request(
        "server/discover",
        &serde_json::json!({
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }),
    );

    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(
        response["result"]["supportedVersions"],
        serde_json::json!(["2026-07-28", "2025-11-25"])
    );
    assert_eq!(
        response["result"]["_meta"]["io.modelcontextprotocol/serverInfo"],
        serde_json::json!({
            "name": "keld",
            "version": env!("CARGO_PKG_VERSION")
        })
    );
}

#[test]
fn mcp_tools_list_matches_reviewed_snapshot() {
    let response = mcp_request("tools/list", &serde_json::json!({}));
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(response["result"]["ttlMs"], 0);
    assert_eq!(response["result"]["cacheScope"], "public");

    let tools = response["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        [
            "keld_doctor",
            "keld_docs_search",
            "keld_permissions_explain"
        ]
    );

    // Canonicalize object-key order through serde_json before the byte comparison;
    // the wire and checked-in JSON may preserve different insertion orders.
    let snapshot: Value =
        serde_json::from_str(include_str!("snapshots/tools_list.json")).expect("parse snapshot");
    let actual = serde_json::to_string_pretty(&response["result"]).expect("serialize response");
    let expected = serde_json::to_string_pretty(&snapshot).expect("serialize snapshot");
    assert_eq!(
        actual, expected,
        "tools/list changed; review the public tool schemas and update the snapshot intentionally"
    );
}

#[test]
fn mcp_doctor_matches_cli_json_over_stdio() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    // macOS aliases `/var` to `/private/var`; use one canonical spelling so the
    // subprocess cwd and explicit MCP argument produce byte-identical findings.
    let project = std::fs::canonicalize(dir.path().join("app")).expect("canonical project");

    let cli = Command::new(keld_bin())
        .args(["doctor", "--json"])
        .current_dir(&project)
        .output()
        .expect("spawn doctor --json");
    assert!(
        matches!(cli.status.code(), Some(0 | 1)),
        "doctor must return findings, stderr={}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let cli_findings: Value =
        serde_json::from_slice(&cli.stdout).expect("parse doctor --json output");

    let response = mcp_tools_call(&serde_json::json!({
        "name": "keld_doctor",
        "arguments": { "project_root": project }
    }));
    let result = &response["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["structuredContent"], cli_findings);

    let findings = result["structuredContent"]
        .as_array()
        .expect("findings array");
    assert!(!findings.is_empty());
    for finding in findings {
        assert!(finding["label"].is_string(), "{finding}");
        assert!(finding["ok"].is_boolean(), "{finding}");
        assert!(finding["detail"].is_string(), "{finding}");
    }
}

#[test]
fn mcp_docs_search_returns_real_security_chunk_over_stdio() {
    let response = mcp_tools_call(&serde_json::json!({
        "name": "keld_docs_search",
        "arguments": {
            "query": "capability manifest",
            "max_results": 5
        }
    }));

    let result = &response["result"];
    assert_eq!(result["resultType"], "complete");
    let structured = &result["structuredContent"];
    let results = structured["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "{structured}");
    assert!(results.len() <= 5, "{structured}");
    assert!(
        results
            .iter()
            .any(|chunk| chunk["source_path"] == "docs/architecture/03-security.md"),
        "wire response must contain a real security-doc chunk: {structured}"
    );
}

#[test]
fn mcp_permissions_explain_deny_patch_over_stdio() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = dir.path().join("keld.permissions.jsonc");
    std::fs::write(&manifest, r#"{"app":{"fs":{"write":["$APPDATA/**"]}}}"#).expect("write");
    let before = std::fs::read(&manifest).expect("hash before");

    let resp = mcp_tools_call(&serde_json::json!({
        "name": "keld_permissions_explain",
        "arguments": {
            "manifest_path": manifest.to_string_lossy(),
            "operation": {
                "principal": "app",
                "capability": "fs.read",
                "args": { "path": "$DOCUMENTS/notes.txt" }
            }
        }
    }));

    assert_eq!(resp["id"], 2);
    let result = &resp["result"];
    assert_ne!(result.get("isError"), Some(&Value::Bool(true)), "{result}");
    let structured = &result["structuredContent"];
    assert_eq!(structured["decision"], "deny");
    assert_eq!(structured["deny_reason"]["kind"], "not_granted");
    assert_eq!(
        structured["error"]["fix"],
        "Append \"$DOCUMENTS/notes.txt\" to `/app/fs/read` in keld.permissions.jsonc."
    );
    assert_eq!(structured["patch"]["json_pointer"], "/app/fs/read");
    let after = std::fs::read(&manifest).expect("hash after");
    assert_eq!(before, after, "tool must not write the manifest");
}

#[test]
fn mcp_permissions_explain_missing_manifest_is_error_mcp010() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("absent").join("keld.permissions.jsonc");

    let resp = mcp_tools_call(&serde_json::json!({
        "name": "keld_permissions_explain",
        "arguments": {
            "manifest_path": missing.to_string_lossy(),
            "operation": {
                "principal": "app",
                "capability": "fs.read",
                "args": { "path": "$DOCUMENTS/notes.txt" }
            }
        }
    }));

    assert_eq!(resp["id"], 2);
    let result = &resp["result"];
    assert_eq!(result.get("isError"), Some(&Value::Bool(true)), "{result}");
    let structured = &result["structuredContent"];
    assert_eq!(structured["code"], "KELD-MCP010");
    let fix = structured["fix"].as_str().expect("fix");
    assert!(
        fix.contains("keld.permissions.jsonc"),
        "fix must name expected file: {fix}"
    );
    assert!(
        fix.contains(&missing.display().to_string()) || fix.contains("absent"),
        "fix must name the tried path: {fix}"
    );
}

#[test]
fn keld_cli_dependency_tree_has_no_http_transport_crates() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "keld-cli",
            "--prefix",
            "none",
            "--format",
            "{p}",
            "--edges",
            "normal,build",
        ])
        .output()
        .expect("cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8_lossy(&output.stdout);
    for banned in ["hyper", "axum", "reqwest"] {
        let present = tree.lines().any(|line| {
            line.split_whitespace()
                .next()
                .is_some_and(|name| name == banned)
        });
        assert!(
            !present,
            "keld-cli must not depend on HTTP crate `{banned}`:\n{tree}"
        );
    }
}

/// Reads the next non-empty stdout line as JSON.
///
/// Blocks on a complete newline-delimited frame from the MCP server (no sleep).
fn read_json_line(reader: &mut BufReader<impl std::io::Read>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read line");
    assert!(
        !line.trim().is_empty(),
        "expected JSON-RPC response line from mcp serve"
    );
    serde_json::from_str(line.trim()).unwrap_or_else(|e| {
        panic!("invalid JSON from mcp serve: {e}; line={line:?}");
    })
}

/// Initialize a stdio MCP session and `tools/call` once.
fn mcp_tools_call(params: &Value) -> Value {
    mcp_request("tools/call", params)
}

/// Initialize a 2026-07-28 stdio MCP session and send one request.
///
/// Awaits complete newline-delimited JSON-RPC frames (no sleep).
fn mcp_request(method: &str, params: &Value) -> Value {
    // Drive newline-delimited JSON-RPC over stdio — await complete response lines,
    // never sleep (AGENTS.md anti-flake).
    let mut child = Command::new(keld_bin())
        .args(["mcp", "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp serve");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2026-07-28",
            "capabilities": {},
            "clientInfo": { "name": "keld-test", "version": "0.0.1" }
        }
    });
    writeln!(stdin, "{init}").expect("write init");
    stdin.flush().expect("flush");
    let init_resp = read_json_line(&mut reader);
    assert_eq!(init_resp["id"], 1, "{init_resp}");
    assert_eq!(
        init_resp["result"]["protocolVersion"], "2026-07-28",
        "{init_resp}"
    );
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "keld");
    assert_eq!(
        init_resp["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{initialized}").expect("write initialized");

    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": method,
        "params": params
    });
    writeln!(stdin, "{call}").expect("write call");
    stdin.flush().expect("flush");
    let resp = read_json_line(&mut reader);

    drop(stdin);
    let status = child.wait().expect("wait for mcp serve");
    assert!(status.success(), "mcp serve exited {status}");
    resp
}
