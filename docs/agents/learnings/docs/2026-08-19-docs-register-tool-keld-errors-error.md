---
name: "2026-08-19-docs-register-tool-keld-errors-error"
description: "Register workspace-tool KELD errors in the error-code document; CLI tests scan tools directly."
area: "docs"
tags: ["docs", "register", "tool", "keld", "errors"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L83"
related: []
---

## Historical source

> - 2026-08-19 [docs] Any new `KELD-*` code emitted from a workspace tool must also be added to `docs/engineering/keld-error-codes.md`; the CLI registry test scans `tools/` and fails even when the tool itself compiles. (evidence: `tools/mermaid_docs.rs`, `tools/mermaid_render_check.sh`, `crates/keld-cli/tests/error_registry.rs`)
