---
name: "2026-08-19-ipc-macos-wake-socket-notconnected-mean"
description: "macOS wake-socket NotConnected can mean cancellation succeeded; do not report listener failure."
area: "ipc"
tags: ["ipc", "macos", "wake", "socket", "notconnected"]
platform: "macos"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L97"
related: ["docs/agents/learnings/runtime/2026-08-19-runtime-latch-admission-completion-failure-deadline.md", "docs/agents/learnings/shell/2026-08-19-shell-zsh-lowercase-path-aliases-different.md"]
---

## Historical source

> - 2026-08-19 [ipc] A Unix bootstrap cancellation wake socket is only a nudge; on macOS `shutdown()` on the freshly connected wake stream can race the accepting side and return `NotConnected`. Treat that as successful cancellation, not a real listener failure. (evidence: KEL-75, `crates/keld-ipc/src/bootstrap.rs` `shutdown_unblocks_accept_and_removes_owner_only_directory`)
