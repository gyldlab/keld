---
name: "2026-08-13-cli-reject-extra-keld-hello-arguments"
description: "Reject extra keld hello arguments before opening the window to avoid a hanging test."
area: "cli"
tags: ["cli", "reject", "extra", "keld", "hello"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L37"
related: []
---

## Historical source

> - 2026-08-13 [cli] `keld hello` extra args must be rejected before `run_hello_window` or the test hangs in tao `EventLoop::run`. (evidence: crates/keld-cli/tests/cli_kel29.rs `keld_hello_unknown_flag_exits_2_without_opening_window`)
