---
name: "2026-08-13-ci-rustdoc-rejects-public-links-private"
description: "rustdoc rejects public links to private items; render error codes as code text."
area: "ci"
tags: ["ci", "rustdoc", "rejects", "public", "links", "linux"]
platform: "linux"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L44"
related: ["docs/agents/learnings/ci/2026-08-13-ci-windows-clippy-unix-test-imports.md"]
---

## Historical source

> - 2026-08-13 [ci] rustdoc `-D warnings` fails on public docs linking private items (`rustdoc::private-intra-doc-links`); backtick error codes instead of `[`const`]` when the module is private. (evidence: crates/keld-cli/src/mcp/permissions.rs, PR #2 ubuntu rustdoc)
