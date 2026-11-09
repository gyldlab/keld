---
name: "2026-10-28-ipc-captured-getownpropertydescriptor-triggers-proxy-traps"
description: "Captured getOwnPropertyDescriptor still triggers Proxy traps; exclude proxies before native diagnostics."
area: "ipc"
tags: ["ipc", "captured", "getownpropertydescriptor", "triggers", "proxy"]
platform: "unspecified"
recorded: "2026-10-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L194"
related: ["docs/agents/learnings/compat/2026-10-28-compat-boot-selection-captured-imports-windows.md"]
---

## Historical source

> - 2026-10-28 [ipc] Captured getOwnPropertyDescriptor still enters Proxy traps. Bun1.4.4 native isProxy exclusion preserves ordinary own-data diagnostics with zero hooks and terminal/waiter settlement. (evidence: PR1181 r4396003293; transport-state-failure.test.ts)
