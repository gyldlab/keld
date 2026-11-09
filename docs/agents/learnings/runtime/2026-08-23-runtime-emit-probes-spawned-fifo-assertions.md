---
name: "2026-08-23-runtime-emit-probes-spawned-fifo-assertions"
description: "Emit probes after Spawned so FIFO assertions wait for the actual producer milestone."
area: "runtime"
tags: ["runtime", "emit", "probes", "spawned", "fifo"]
platform: "unspecified"
recorded: "2026-08-23"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L115"
related: ["docs/agents/learnings/runtime/2026-08-23-runtime-test-lifecycle-events-registry-feed.md", "docs/agents/learnings/runtime/2026-08-29-runtime-coordinator-generations-collide-across-declarations.md"]
---

## Historical source

> - 2026-08-23 [runtime] A role test probe emitted at `Provisioned` cannot prove `Spawned` is queued; emit it after `Spawned` so a registry FIFO assertion awaits the producer's real milestone instead of polling for timing. (evidence: KEL-75 PR #59 CodeRabbit follow-up)
