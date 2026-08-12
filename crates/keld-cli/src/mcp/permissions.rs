//! `keld_permissions_explain` — stub until keld-guard evaluate API lands (T4).

use std::collections::HashMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error_object::KeldErrorObject;

/// Arguments for `keld_permissions_explain` (shape frozen for when T4 unblocks).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PermissionsExplainArgs {
    /// Path to `keld.permissions.jsonc`.
    pub manifest_path: PathBuf,
    /// The operation that was (or would be) denied.
    pub operation: DeniedOperation,
}

/// Operation under evaluation (mirrors future guard evaluate inputs).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DeniedOperation {
    /// Principal name, e.g. `app`.
    pub principal: String,
    /// Capability id, e.g. `fs.read`.
    pub capability: String,
    /// Capability arguments (path, host, etc.).
    #[serde(default)]
    pub args: HashMap<String, serde_json::Value>,
    /// Optional kipc channel name.
    #[serde(default)]
    pub channel: Option<String>,
}

/// Structured stub result — never invents allow/deny decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PermissionsExplainUnavailable {
    /// Always `"unavailable"` until guard evaluate ships.
    pub decision: String,
    /// §2 error naming the prerequisite.
    pub error: KeldErrorObject,
}

/// Code returned while `keld-guard` lacks manifest-parse/evaluate.
pub const PERMISSIONS_UNAVAILABLE_CODE: &str = "KELD-MCP030";

/// Returns a typed "not yet available" payload — no fake guard behavior.
#[must_use]
pub fn permissions_explain_unavailable(
    args: &PermissionsExplainArgs,
) -> PermissionsExplainUnavailable {
    let path = args.manifest_path.display().to_string();
    PermissionsExplainUnavailable {
        decision: "unavailable".to_owned(),
        error: KeldErrorObject::new(
            PERMISSIONS_UNAVAILABLE_CODE,
            "keld_permissions_explain is not available yet",
            "land the keld-guard public manifest-parse/evaluate API (tracked as T4 \
             prerequisite for KEL-42), then re-run this tool — do not invent grants",
        )
        .with_cause(format!(
            "requested principal={} capability={} manifest_path={path}",
            args.operation.principal, args.operation.capability
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_names_guard_prerequisite() {
        let result = permissions_explain_unavailable(&PermissionsExplainArgs {
            manifest_path: PathBuf::from("/tmp/keld.permissions.jsonc"),
            operation: DeniedOperation {
                principal: "app".to_owned(),
                capability: "fs.read".to_owned(),
                args: HashMap::new(),
                channel: None,
            },
        });
        assert_eq!(result.decision, "unavailable");
        assert_eq!(result.error.code, PERMISSIONS_UNAVAILABLE_CODE);
        assert!(result.error.fix.contains("keld-guard"));
        assert!(result.error.fix.contains("evaluate"));
        assert!(
            result
                .error
                .cause
                .as_ref()
                .is_some_and(|c| c.contains("fs.read"))
        );
        assert_ne!(
            result.decision, "deny",
            "stub must not invent a deny decision"
        );
        assert_ne!(
            result.decision, "allow",
            "stub must not invent an allow decision"
        );
    }
}
