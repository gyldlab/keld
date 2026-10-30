---
name: "2026-08-17-ipc-second-echo-call-repeats-hello"
description: "A second echo_call repeats HELLO; use echo_invoke for further calls on the live app link."
area: "ipc"
tags: ["ipc", "second", "echo", "call", "repeats"]
platform: "unspecified"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L71"
related: ["docs/agents/learnings/bridge/2026-08-15-bridge-bun-unix-socket-writes-partial.md", "docs/agents/learnings/ci/2026-08-21-ci-gui-dependency-discovery-follow-normal.md"]
---

## Historical source

> - 2026-08-17 [ipc] A second `echo_call` on a live app-link sends HELLO again: the server fails with `KELD-IPC-005`, the client observes `KELD-IPC-001` because the peer closed — not 005 on both sides. Use `echo_invoke` for further CALLs. (evidence: KEL-30, `crates/keld-ipc/tests/echo_link.rs` `second_echo_call_on_same_stream_client_001_server_005`)
