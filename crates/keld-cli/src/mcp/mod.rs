//! `keld mcp serve` — stdio MCP server (KEL-42 / approved spec).

mod docs_search;
mod error;
mod permissions;
mod server;

pub use docs_search::{DocsSearchArgs, DocsSearchResult, search_docs};
pub use error::McpServeError;
pub use permissions::{PermissionsExplainArgs, PermissionsExplainResult, permissions_explain};
pub use server::{KeldMcpServer, tool_definitions};

use std::path::{Path, PathBuf};

/// Serves MCP over stdio until the client closes stdin.
///
/// Blocking; builds its own current-thread tokio runtime (cold tooling —
/// AGENTS.md async rule). `project_root` is forwarded to `keld_doctor`.
///
/// # Errors
///
/// Returns [`McpServeError`] when the runtime cannot start or the stdio
/// session fails.
pub fn run_mcp_serve(project_root: Option<&Path>) -> Result<(), McpServeError> {
    let root = project_root.map(Path::to_path_buf);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| McpServeError::Runtime(e.to_string()))?;
    runtime.block_on(serve_stdio(root))
}

async fn serve_stdio(project_root: Option<PathBuf>) -> Result<(), McpServeError> {
    use rmcp::{ServiceExt, transport::stdio};

    let server = KeldMcpServer::new(project_root);
    let running = server
        .serve(stdio())
        .await
        .map_err(|e| McpServeError::Serve(e.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|e| McpServeError::Serve(e.to_string()))?;
    Ok(())
}
