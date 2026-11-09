---
name: "2026-08-23-runtime-test-lifecycle-events-registry-feed"
description: "Test lifecycle events through the registry feed after supervisor completion, not its private queue."
area: "runtime"
tags: ["runtime", "test", "lifecycle", "events", "registry"]
platform: "unspecified"
recorded: "2026-08-23"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L114"
related: ["docs/agents/learnings/runtime/2026-08-23-runtime-emit-probes-spawned-fifo-assertions.md", "docs/agents/learnings/runtime/2026-08-23-runtime-lifecycle-drain-revoke-virtual-ports.md"]
---

## Historical source

> - 2026-08-23 [runtime] When `RoleRegistry` owns lifecycle synchronization, shutdown tests must await supervisor completion then assert through its `try_recv_*_event` feed; reading the supervisor queue directly cannot prove the public buffered contract. (evidence: KEL-75 PR #59 fresh CodeRabbit review)
