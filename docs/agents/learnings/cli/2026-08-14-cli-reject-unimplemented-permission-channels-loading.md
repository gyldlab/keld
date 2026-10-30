---
name: "2026-08-14-cli-reject-unimplemented-permission-channels-loading"
description: "Reject unimplemented permission channels before loading or evaluating the manifest."
area: "cli"
tags: ["cli", "reject", "unimplemented", "permission", "channels"]
platform: "unspecified"
recorded: "2026-08-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L51"
related: []
---

## Historical source

> - 2026-08-14 [cli] `keld_permissions_explain` must reject unimplemented `channel` before `load_manifest`/`evaluate`; otherwise an in-scope path returns `allow` and drops the channel. (evidence: KEL-58, `crates/keld-cli/src/mcp/permissions.rs`)
