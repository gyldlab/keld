//! MCP server handler — tools in fixed order for prompt-cache stability.

use std::borrow::Cow;
use std::path::PathBuf;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::{Json, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::doctor::{DoctorFinding, run_findings};
use crate::error_object::KeldErrorObject;
use crate::mcp::docs_search::{DocsSearchArgs, DocsSearchResult, search_docs};
use crate::mcp::permissions::{
    PermissionsExplainArgs, PermissionsExplainResult, permissions_explain,
};

/// Arguments for `keld_doctor`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DoctorToolArgs {
    /// Optional scaffolded project root (layout checks).
    #[serde(default)]
    pub project_root: Option<PathBuf>,
}

/// Stdio MCP server exposing the v1 toolset.
#[derive(Debug, Clone)]
pub struct KeldMcpServer {
    tool_router: ToolRouter<Self>,
    /// Default project root from `keld mcp serve` cwd discovery.
    default_project_root: Option<PathBuf>,
}

impl KeldMcpServer {
    /// Builds a server with the three v1 tools registered in fixed order.
    #[must_use]
    pub fn new(default_project_root: Option<PathBuf>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            default_project_root,
        }
    }
}

#[tool_router(router = tool_router)]
impl KeldMcpServer {
    /// Environment checks — same payload as `keld doctor --json`.
    #[tool(
        name = "keld_doctor",
        description = "Run Keld environment checks (Bun, project layout, renderer, webview). \
                       Same findings as `keld doctor --json`. Read-only."
    )]
    fn keld_doctor(
        &self,
        Parameters(args): Parameters<DoctorToolArgs>,
    ) -> Json<Vec<DoctorFinding>> {
        let root = args
            .project_root
            .as_deref()
            .or(self.default_project_root.as_deref());
        Json(run_findings(root))
    }

    /// Search the embedded architecture + error-code registry corpus.
    #[tool(
        name = "keld_docs_search",
        description = "Search Keld architecture docs and the KELD-* error registry \
                       (offline, embedded corpus). Returns ranked {title, source_path, \
                       snippet} chunks. Read-only."
    )]
    // `&self` required by rmcp `#[tool]` instance methods even when unused.
    #[allow(clippy::unused_self)]
    fn keld_docs_search(
        &self,
        Parameters(args): Parameters<DocsSearchArgs>,
    ) -> Json<DocsSearchResult> {
        Json(search_docs(&args))
    }

    /// Explain a permission allow/deny via keld-guard evaluate. Read-only.
    #[tool(
        name = "keld_permissions_explain",
        description = "Explain a capability allow/deny against keld.permissions.jsonc \
                       and return the exact manifest patch. Read-only — never writes \
                       the file. Missing manifest returns KELD-MCP010. A present \
                       `channel` returns KELD-MCP014 (v0 does not evaluate channel grants)."
    )]
    #[allow(clippy::unused_self)] // `&self` required by rmcp `#[tool]` instance methods even when unused.
    fn keld_permissions_explain(
        &self,
        Parameters(args): Parameters<PermissionsExplainArgs>,
    ) -> Result<Json<PermissionsExplainResult>, Json<KeldErrorObject>> {
        permissions_explain(&args).map(Json).map_err(Json)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KeldMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("keld", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Keld MCP server (stdio). Tools: keld_doctor, keld_docs_search, \
                 keld_permissions_explain. Offline, read-only.",
            )
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&[ProtocolVersion::V_2026_07_28, ProtocolVersion::V_2025_11_25])
    }

    /// Fixed order for prompt-cache stability (spec AC2) — router sorts alphabetically.
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        let supports_cache_hints = context
            .protocol_version()
            .is_some_and(|version| version >= ProtocolVersion::V_2026_07_28);
        let mut result = rmcp::model::ListToolsResult::with_all_items(tools_in_spec_order(
            &self.tool_router.list_all(),
        ));
        if supports_cache_hints {
            result = result
                .with_ttl_ms(0)
                .with_cache_scope(rmcp::model::CacheScope::Public);
        }
        Ok(result)
    }
}

/// Spec AC2 order — must not depend on `HashMap` / alphabetical router sort.
const TOOL_ORDER: [&str; 3] = [
    "keld_doctor",
    "keld_docs_search",
    "keld_permissions_explain",
];

fn tools_in_spec_order(tools: &[rmcp::model::Tool]) -> Vec<rmcp::model::Tool> {
    let mut ordered = Vec::with_capacity(TOOL_ORDER.len());
    for name in TOOL_ORDER {
        if let Some(tool) = tools.iter().find(|t| t.name.as_ref() == name) {
            ordered.push(tool.clone());
        }
    }
    ordered
}

/// Tool names + schemas in the fixed `tools/list` order (for snapshot tests).
#[must_use]
pub fn tool_definitions() -> Vec<rmcp::model::Tool> {
    tools_in_spec_order(&KeldMcpServer::new(None).tool_router.list_all())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_order_is_fixed() {
        let tools = tool_definitions();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(
            names,
            [
                "keld_doctor",
                "keld_docs_search",
                "keld_permissions_explain"
            ]
        );
        assert_eq!(
            tools.len(),
            3,
            "v1 must not grow extra tools without a spec"
        );
    }

    #[test]
    fn every_tool_declares_output_schema() {
        for tool in tool_definitions() {
            assert!(
                tool.output_schema.is_some(),
                "tool {} missing outputSchema",
                tool.name
            );
            assert!(
                !tool.input_schema.is_empty(),
                "tool {} missing inputSchema",
                tool.name
            );
        }
    }
}
