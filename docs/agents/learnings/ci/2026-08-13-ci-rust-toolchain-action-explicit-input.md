---
name: "2026-08-13-ci-rust-toolchain-action-explicit-input"
description: "The Rust toolchain action requires an explicit toolchain input even with rust-toolchain.toml."
area: "ci"
tags: ["ci", "rust", "toolchain", "action", "explicit"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L42"
related: ["docs/agents/learnings/ci/2026-08-13-ci-windows-clippy-unix-test-imports.md"]
---

## Historical source

> - 2026-08-13 [ci] dtolnay/rust-toolchain@6c977a6 (2026-08-05) requires `with: toolchain:` even when rust-toolchain.toml exists; omitting it fails every job with 'toolchain' is a required input. (evidence: GitHub CI on PR #2 @ 6ed01b1)
