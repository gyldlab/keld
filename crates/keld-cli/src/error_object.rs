//! Arch/07 §2 error object — shared by `keld doctor --json` and MCP tools.

use schemars::JsonSchema;
use serde::Serialize;

/// Structured error payload agents can act on without reading prose.
///
/// Shape matches `docs/architecture/07-agent-experience.md` §2. Serialized into
/// doctor findings and MCP `structuredContent` (never into JSON-RPC error codes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct KeldErrorObject {
    /// Stable greppable code, e.g. `KELD-MCP010`.
    pub code: String,
    /// What failed, with the failing value/field named.
    pub message: String,
    /// Specific input/state that triggered the failure, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    /// Imperative next step: exact command, patch, or API to use.
    pub fix: String,
    /// Task-oriented docs URL (`https://keld.dev/e/<code>`).
    pub docs: String,
}

impl KeldErrorObject {
    /// Builds a §2 object with the conventional docs URL for `code`.
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        let code = code.into();
        let docs = format!("https://keld.dev/e/{code}");
        Self {
            code,
            message: message.into(),
            cause: None,
            fix: fix.into(),
            docs,
        }
    }

    /// Attaches an optional cause string.
    #[must_use]
    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_url_uses_code() {
        let err = KeldErrorObject::new(
            "KELD-MCP010",
            "missing manifest",
            "create keld.permissions.jsonc",
        );
        assert_eq!(err.docs, "https://keld.dev/e/KELD-MCP010");
        assert_eq!(err.fix, "create keld.permissions.jsonc");
        assert!(err.cause.is_none());
    }
}
