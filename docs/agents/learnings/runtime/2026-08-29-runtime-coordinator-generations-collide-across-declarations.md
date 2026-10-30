---
name: "2026-08-29-runtime-coordinator-generations-collide-across-declarations"
description: "Coordinator generations collide across declarations; use stable role IDs and monotonic owner state."
area: "runtime"
tags: ["runtime", "coordinator", "generations", "collide", "across"]
platform: "unspecified"
recorded: "2026-08-29"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L147"
related: ["docs/agents/learnings/ipc/2026-08-30-ipc-bootstrap-listener-consumption-skip-blocking.md", "docs/agents/learnings/runtime/2026-08-23-runtime-emit-probes-spawned-fifo-assertions.md"]
---

## Historical source

> - 2026-08-29 [runtime] A role principal keyed only by lifecycle category plus a per-coordinator generation collides when multiple `window-bound` declarations exist or a coordinator resets; the app-session owner must retain a stable role-declaration id and monotonic generation state. (evidence: KEL-75/T4a, `crates/keld-runtime/tests/window_bound_contract.rs` `coordinator_recreation_cannot_reset_role_generation`)
