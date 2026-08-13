//! MCP serve errors (`KELD-MCP*` codes).

use std::fmt;

/// Failure starting or running `keld mcp serve`.
#[derive(Debug)]
pub enum McpServeError {
    /// Tokio runtime could not be constructed.
    Runtime(String),
    /// rmcp stdio session failed.
    Serve(String),
}

impl fmt::Display for McpServeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(e) => write!(
                f,
                "KELD-MCP001: failed to start MCP runtime — {e}. \
                 Reinstall the `keld` binary or report a bug."
            ),
            Self::Serve(e) => write!(
                f,
                "KELD-MCP002: MCP stdio session ended with error — {e}. \
                 Ensure the client speaks MCP over stdio and keeps stdin open."
            ),
        }
    }
}

impl std::error::Error for McpServeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_error_names_code_and_fix() {
        let err = McpServeError::Runtime("boom".to_owned());
        let msg = err.to_string();
        assert!(msg.contains("KELD-MCP001"), "{msg}");
        assert!(msg.contains("Reinstall"), "{msg}");
    }

    #[test]
    fn serve_error_names_code_and_fix() {
        let err = McpServeError::Serve("eof".to_owned());
        let msg = err.to_string();
        assert!(msg.contains("KELD-MCP002"), "{msg}");
        assert!(msg.contains("stdio"), "{msg}");
    }
}
