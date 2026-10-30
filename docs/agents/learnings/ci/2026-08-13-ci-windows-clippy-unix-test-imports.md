---
name: "2026-08-13-ci-windows-clippy-unix-test-imports"
description: "Windows Clippy needs Unix-only test imports gated with cfg(unix)."
area: "ci"
tags: ["ci", "windows", "clippy", "unix", "test"]
platform: "windows"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L43"
related: ["docs/agents/learnings/ci/2026-08-13-ci-rust-toolchain-action-explicit-input.md", "docs/agents/learnings/ci/2026-08-13-ci-rustdoc-rejects-public-links-private.md"]
---

## Historical source

> - 2026-08-13 [ci] Windows clippy `-D warnings` fails on unused `std::sync::mpsc` in keld-ipc `echo_link` tests; gate Unix-only test imports with `cfg(unix)`. (evidence: crates/keld-ipc/tests/echo_link.rs, PR #2 windows-latest)
