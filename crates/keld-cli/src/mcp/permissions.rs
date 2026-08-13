//! `keld_permissions_explain` — wraps `keld-guard` evaluate (KEL-42 T4).

use std::collections::HashMap;
use std::path::PathBuf;

use keld_guard::{Decision, DenyReason, ManifestError, evaluate, load_manifest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error_object::KeldErrorObject;

/// Arguments for `keld_permissions_explain`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PermissionsExplainArgs {
    /// Path to `keld.permissions.jsonc`.
    pub manifest_path: PathBuf,
    /// The operation that was (or would be) denied.
    pub operation: DeniedOperation,
}

/// Operation under evaluation (mirrors guard `evaluate` inputs).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DeniedOperation {
    /// Principal name, e.g. `app`.
    pub principal: String,
    /// Capability id, e.g. `fs.read`.
    pub capability: String,
    /// Capability arguments (path, host, etc.).
    #[serde(default)]
    pub args: HashMap<String, serde_json::Value>,
    /// Optional kipc channel name. Omit in v0 — a present value returns `KELD-MCP014`.
    #[serde(default)]
    pub channel: Option<String>,
}

/// Structured allow/deny result. Never writes the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PermissionsExplainResult {
    /// `"allow"` or `"deny"`.
    pub decision: String,
    /// Present iff `decision` is `"deny"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny_reason: Option<DenyReasonView>,
    /// §2 error whose `fix` is the exact manifest patch (deny only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<KeldErrorObject>,
    /// JSON-pointer patch that would grant the operation (deny only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<ManifestPatch>,
}

/// MCP view of [`keld_guard::DenyReason`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DenyReasonView {
    /// `not_granted` | `out_of_scope` (v0). `channel_forbidden` is not produced.
    pub kind: String,
    /// Capability id when the deny is grant/scope related.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// Failing scope text when `kind` is `out_of_scope`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Unused in v0 (`evaluate` does not return `ChannelForbidden`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Manifest JSON pointer named in the fix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_pointer: Option<String>,
}

/// Suggested append at a JSON pointer. The tool never applies this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ManifestPatch {
    /// JSON pointer, e.g. `/app/fs/read`.
    pub json_pointer: String,
    /// Value to append (the requested path/host).
    pub value: serde_json::Value,
    /// One-line description of the edit.
    pub snippet: String,
}

/// Missing `keld.permissions.jsonc` (spec AC8).
pub const MANIFEST_MISSING_CODE: &str = "KELD-MCP010";
/// Manifest exists but cannot be read.
pub const MANIFEST_UNREADABLE_CODE: &str = "KELD-MCP013";
/// Manifest is not valid JSONC.
pub const MANIFEST_PARSE_CODE: &str = "KELD-MCP011";
/// Principal is not `app` (v0 evaluate is app-process grants only).
pub const UNKNOWN_PRINCIPAL_CODE: &str = "KELD-MCP012";
/// `channel` was set; v0 evaluate does not cover channel grants.
pub const CHANNEL_UNSUPPORTED_CODE: &str = "KELD-MCP014";

const APP_PRINCIPAL: &str = "app";

/// Explains allow/deny for `args` by calling `keld-guard` evaluate.
///
/// Missing/unreadable/invalid manifests, unknown principals, and a present
/// `channel` are `Err` (MCP `isError: true`). Allow and deny are `Ok` — deny
/// is a normal result.
///
/// # Errors
///
/// Returns a §2 object: `KELD-MCP010`, `KELD-MCP013`, `KELD-MCP011`,
/// `KELD-MCP012`, or `KELD-MCP014`.
pub fn permissions_explain(
    args: &PermissionsExplainArgs,
) -> Result<PermissionsExplainResult, KeldErrorObject> {
    if args.operation.principal != APP_PRINCIPAL {
        return Err(KeldErrorObject::new(
            UNKNOWN_PRINCIPAL_CODE,
            format!(
                "unknown principal `{}`",
                args.operation.principal
            ),
            format!(
                "v0 evaluate only supports principal \"{APP_PRINCIPAL}\" — pass \"{APP_PRINCIPAL}\" \
                 (window/plugin principals are not in this slice)"
            ),
        )
        .with_cause(format!(
            "requested principal={} capability={} manifest_path={}",
            args.operation.principal,
            args.operation.capability,
            args.manifest_path.display()
        )));
    }

    if let Some(channel) = args.operation.channel.as_deref() {
        return Err(KeldErrorObject::new(
            CHANNEL_UNSUPPORTED_CODE,
            format!("channel grants are not evaluated in v0 (`{channel}`)"),
            "omit `channel` — v0 evaluate covers app path/host scopes only",
        )
        .with_cause(format!(
            "requested principal={} capability={} channel={channel} manifest_path={}",
            args.operation.principal,
            args.operation.capability,
            args.manifest_path.display()
        )));
    }

    let manifest = load_manifest(&args.manifest_path).map_err(|e| manifest_error(&e))?;
    let path = operation_path(&args.operation.args);
    match evaluate(&manifest, &args.operation.capability, path) {
        Decision::Allow => Ok(PermissionsExplainResult {
            decision: "allow".to_owned(),
            deny_reason: None,
            error: None,
            patch: None,
        }),
        Decision::Deny(reason) => Ok(deny_result(&reason)),
    }
}

fn operation_path(args: &HashMap<String, serde_json::Value>) -> &str {
    args.get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn manifest_error(err: &ManifestError) -> KeldErrorObject {
    match err {
        ManifestError::NotFound { path } => KeldErrorObject::new(
            MANIFEST_MISSING_CODE,
            format!("permissions manifest not found at `{}`", path.display()),
            format!(
                "create keld.permissions.jsonc at {} (expected file name: keld.permissions.jsonc)",
                path.display()
            ),
        )
        .with_cause(err.to_string()),
        ManifestError::Read { path, detail } => KeldErrorObject::new(
            MANIFEST_UNREADABLE_CODE,
            format!("cannot read permissions manifest at `{}`", path.display()),
            format!(
                "check that `{}` is a readable file (not a directory) and retry — {detail}",
                path.display()
            ),
        )
        .with_cause(err.to_string()),
        ManifestError::Parse { path, detail } => {
            let tried = path
                .as_ref()
                .map_or_else(|| "<memory>".to_owned(), |p| p.display().to_string());
            KeldErrorObject::new(
                MANIFEST_PARSE_CODE,
                format!("permissions manifest at `{tried}` is not valid JSONC"),
                format!(
                    "fix JSON at `{tried}` (comments are allowed; trailing commas are not) — {detail}"
                ),
            )
            .with_cause(err.to_string())
        }
    }
}

fn deny_result(reason: &DenyReason) -> PermissionsExplainResult {
    let patch = match reason {
        DenyReason::NotGranted {
            json_pointer,
            requested,
            ..
        }
        | DenyReason::OutOfScope {
            json_pointer,
            requested,
            ..
        } => Some(ManifestPatch {
            json_pointer: json_pointer.clone(),
            value: serde_json::Value::String(requested.clone()),
            snippet: format!("append {requested:?} to `{json_pointer}`"),
        }),
        DenyReason::ChannelForbidden { .. } => None,
    };
    PermissionsExplainResult {
        decision: "deny".to_owned(),
        deny_reason: Some(DenyReasonView::from(reason)),
        error: Some(
            KeldErrorObject::new(reason.code(), reason.to_string(), reason.fix())
                .with_cause(reason.to_string()),
        ),
        patch,
    }
}

impl From<&DenyReason> for DenyReasonView {
    fn from(reason: &DenyReason) -> Self {
        match reason {
            DenyReason::NotGranted {
                capability,
                json_pointer,
                ..
            } => Self {
                kind: reason.kind().to_owned(),
                capability: Some(capability.clone()),
                scope: None,
                channel: None,
                json_pointer: Some(json_pointer.clone()),
            },
            DenyReason::OutOfScope {
                capability,
                scope,
                json_pointer,
                ..
            } => Self {
                kind: reason.kind().to_owned(),
                capability: Some(capability.clone()),
                scope: Some(scope.clone()),
                channel: None,
                json_pointer: Some(json_pointer.clone()),
            },
            DenyReason::ChannelForbidden { channel } => Self {
                kind: reason.kind().to_owned(),
                capability: None,
                scope: None,
                channel: Some(channel.clone()),
                json_pointer: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write as _;

    use super::*;

    fn args(dir: &std::path::Path, capability: &str, path: &str) -> PermissionsExplainArgs {
        let mut map = HashMap::new();
        map.insert(
            "path".to_owned(),
            serde_json::Value::String(path.to_owned()),
        );
        PermissionsExplainArgs {
            manifest_path: dir.join("keld.permissions.jsonc"),
            operation: DeniedOperation {
                principal: APP_PRINCIPAL.to_owned(),
                capability: capability.to_owned(),
                args: map,
                channel: None,
            },
        }
    }

    #[test]
    fn missing_manifest_is_mcp010() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nope").join("keld.permissions.jsonc");
        let result = permissions_explain(&PermissionsExplainArgs {
            manifest_path: path.clone(),
            operation: DeniedOperation {
                principal: APP_PRINCIPAL.to_owned(),
                capability: "fs.read".to_owned(),
                args: HashMap::new(),
                channel: None,
            },
        });
        let err = result.expect_err("missing file");
        assert_eq!(err.code, MANIFEST_MISSING_CODE);
        assert!(
            err.fix.contains(&path.display().to_string()),
            "fix must name the tried path: {}",
            err.fix
        );
        assert!(err.fix.contains("keld.permissions.jsonc"), "{}", err.fix);
    }

    #[test]
    fn unreadable_manifest_is_mcp013_not_mcp010() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("keld.permissions.jsonc");
        fs::create_dir(&path).expect("directory where a file is required");
        let err = permissions_explain(&args(dir.path(), "fs.read", "$APPDATA/x"))
            .expect_err("directory manifest must fail as unreadable, not missing");
        assert_eq!(err.code, MANIFEST_UNREADABLE_CODE);
        assert_ne!(err.code, MANIFEST_MISSING_CODE);
        assert!(
            err.fix.contains(&path.display().to_string()),
            "fix must name the tried path: {}",
            err.fix
        );
        assert!(
            !err.fix.contains("create keld.permissions.jsonc"),
            "must not tell the caller to create a file that already exists: {}",
            err.fix
        );
    }

    #[test]
    fn ac7_not_granted_patch_and_bytes_unchanged() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("keld.permissions.jsonc");
        let body = r#"{"app":{"fs":{"write":["$APPDATA/**"]}}}"#;
        fs::write(&path, body).expect("write fixture");
        let before = fs::read(&path).expect("read before");

        let result = permissions_explain(&args(dir.path(), "fs.read", "$DOCUMENTS/notes.txt"))
            .expect("evaluate");

        assert_eq!(result.decision, "deny");
        let kind = result
            .deny_reason
            .as_ref()
            .map(|r| r.kind.as_str())
            .expect("deny_reason");
        assert_eq!(kind, "not_granted");
        let error = result.error.as_ref().expect("error");
        assert_eq!(error.code, "KELD-GUARD001");
        assert_eq!(
            error.fix,
            "Append \"$DOCUMENTS/notes.txt\" to `/app/fs/read` in keld.permissions.jsonc."
        );
        let suggested = result.patch.as_ref().expect("patch");
        assert_eq!(suggested.json_pointer, "/app/fs/read");
        assert_eq!(
            suggested.value,
            serde_json::Value::String("$DOCUMENTS/notes.txt".to_owned())
        );

        let after = fs::read(&path).expect("read after");
        assert_eq!(before, after, "explain must not write the manifest");
    }

    #[test]
    fn allow_when_path_in_scope() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("keld.permissions.jsonc");
        fs::write(&path, r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("write");
        let result = permissions_explain(&args(dir.path(), "fs.read", "$APPDATA/notes.txt"))
            .expect("evaluate");
        assert_eq!(result.decision, "allow");
        assert!(result.deny_reason.is_none());
        assert!(result.error.is_none());
        assert!(result.patch.is_none());
    }

    #[test]
    fn unknown_principal_is_mcp012() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = permissions_explain(&PermissionsExplainArgs {
            manifest_path: dir.path().join("keld.permissions.jsonc"),
            operation: DeniedOperation {
                principal: "webview".to_owned(),
                capability: "fs.read".to_owned(),
                args: HashMap::new(),
                channel: None,
            },
        })
        .expect_err("unknown principal");
        assert_eq!(err.code, UNKNOWN_PRINCIPAL_CODE);
        assert!(err.fix.contains("app"), "{}", err.fix);
    }

    #[test]
    fn channel_on_in_scope_path_is_mcp014_not_allow() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("keld.permissions.jsonc");
        fs::write(&path, r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("write");
        let mut explain = args(dir.path(), "fs.read", "$APPDATA/notes.txt");
        explain.operation.channel = Some("echo".to_owned());
        let err = permissions_explain(&explain)
            .expect_err("channel must fail closed instead of evaluating the path");
        assert_eq!(err.code, CHANNEL_UNSUPPORTED_CODE);
        assert_ne!(err.code, UNKNOWN_PRINCIPAL_CODE);
        assert!(err.message.contains("echo"), "{}", err.message);
        assert!(
            err.fix.contains("omit `channel`"),
            "fix must tell the caller to omit channel: {}",
            err.fix
        );
    }

    #[test]
    fn empty_channel_string_is_mcp014() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut explain = args(dir.path(), "fs.read", "$APPDATA/x");
        explain.operation.channel = Some(String::new());
        let err = permissions_explain(&explain).expect_err("empty channel is still present");
        assert_eq!(err.code, CHANNEL_UNSUPPORTED_CODE);
    }

    #[test]
    fn channel_present_is_mcp014_before_missing_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = permissions_explain(&PermissionsExplainArgs {
            manifest_path: dir.path().join("absent").join("keld.permissions.jsonc"),
            operation: DeniedOperation {
                principal: APP_PRINCIPAL.to_owned(),
                capability: "fs.read".to_owned(),
                args: HashMap::new(),
                channel: Some("notes.save".to_owned()),
            },
        })
        .expect_err("unimplemented channel must not fall through to manifest I/O");
        assert_eq!(err.code, CHANNEL_UNSUPPORTED_CODE);
        assert_ne!(err.code, MANIFEST_MISSING_CODE);
        assert!(err.message.contains("notes.save"), "{}", err.message);
    }

    #[test]
    fn invalid_jsonc_is_mcp011() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("keld.permissions.jsonc");
        let mut file = fs::File::create(&path).expect("create");
        file.write_all(b"{").expect("write");
        drop(file);
        let err =
            permissions_explain(&args(dir.path(), "fs.read", "$APPDATA/x")).expect_err("parse");
        assert_eq!(err.code, MANIFEST_PARSE_CODE);
        assert!(err.fix.contains("keld.permissions.jsonc"), "{}", err.fix);
    }
}
