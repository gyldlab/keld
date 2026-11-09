---
name: "2026-08-13-cli-mcp-server-discover-initialize-namespaced"
description: "MCP server/discover requires initialize and namespaced metadata keys."
area: "cli"
tags: ["cli", "mcp", "server", "discover", "initialize"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L36"
related: []
---

## Historical source

> - 2026-08-13 [cli] rmcp 3.1.2 accepts `server/discover` only after `initialize`, with namespaced `io.modelcontextprotocol/*` metadata keys; unnamespaced keys return JSON-RPC -32602. (evidence: crates/keld-cli/tests/doctor_mcp.rs `mcp_server_discover_advertises_versions_and_identity`)
