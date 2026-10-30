---
name: "2026-10-28-ipc-captured-native-messages-existing-private"
description: "Captured native messages need the existing private duplex port; synchronous liveness termination remains separate."
area: "ipc"
tags: ["ipc", "captured", "native", "messages", "existing"]
platform: "unspecified"
recorded: "2026-10-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L192"
related: ["docs/agents/learnings/ipc/2026-10-28-ipc-fixed-unsealed-slots-avoid-larger.md", "docs/agents/learnings/runtime/2026-10-28-runtime-windows-short-path-aliases-evaluate.md", "docs/agents/learnings/ts/2026-10-08-ts-bun-cannot-half-close-socket.md"]
---

## Historical source

> - 2026-10-28 [ipc] Bun 1.4.4 node Worker.postMessage forwards through a mutable MessagePort prototype lookup even when the outer Worker method is captured. The existing private duplex port supports direct captured native control messages; the separate synchronous liveness-termination obligation remains unchanged. (evidence: GH-1149, PR #1181; docs/specs/gh527-worker-owned-blocking-call-transport.md §§4.2,4.5)
