---
name: "2026-08-30-ipc-bootstrap-listener-consumption-skip-blocking"
description: "After bootstrap listener consumption, skip the blocking cancellation waker and let bounded session polling stop."
area: "ipc"
tags: ["ipc", "bootstrap", "listener", "consumption", "skip", "windows"]
platform: "windows"
recorded: "2026-08-30"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L149"
related: ["docs/agents/learnings/runtime/2026-08-29-runtime-coordinator-generations-collide-across-declarations.md"]
---

## Historical source

> - 2026-08-30 [ipc/windows] After an authenticated loopback bootstrap consumes its listener, a connect-to-wake cancellation can block while the accepted socket still retains the local port; track listener liveness and skip the accept waker after consumption, leaving authenticated-session shutdown to the bounded reader poll. (evidence: KEL-75/T8, `echo_link::tests::shutdown_interrupts_a_quiet_authenticated_client`)
