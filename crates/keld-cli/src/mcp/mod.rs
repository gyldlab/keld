//! `keld mcp serve` — stdio MCP server (KEL-42 / approved spec).

mod discovery_eof;
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
    use rmcp::transport::stdio;

    let (input, output) = stdio();
    serve_io(project_root, input, output).await
}

async fn serve_io<R, W>(
    project_root: Option<PathBuf>,
    input: R,
    output: W,
) -> Result<(), McpServeError>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    use rmcp::ServiceExt;
    use rmcp::service::ServerInitializeError;
    use rmcp::transport::async_rw::AsyncRwTransport;
    use std::sync::Arc;

    use discovery_eof::{DiscoveryEof, ObservedRead, ObservedTransport};

    let observation = Arc::new(DiscoveryEof::default());
    let input = ObservedRead::new(input, observation.clone());
    let transport =
        ObservedTransport::new(AsyncRwTransport::new(input, output), observation.clone());
    let server = KeldMcpServer::new(project_root);
    let running = match server.serve(transport).await {
        Ok(running) => running,
        Err(ServerInitializeError::ConnectionClosed(_)) if observation.is_clean() => return Ok(()),
        Err(error) => return Err(McpServeError::Serve(error.to_string())),
    };
    running
        .waiting()
        .await
        .map_err(|e| McpServeError::Serve(e.to_string()))?;
    Ok(())
}
