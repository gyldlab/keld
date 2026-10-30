---
name: "2026-10-10-runtime-post-admission-crash-restart-onto"
description: "A post-admission crash can restart onto a consumed link; force race order instead of looping."
area: "runtime"
tags: ["runtime", "post", "admission", "crash", "restart", "macos"]
platform: "macos"
recorded: "2026-10-10"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L174"
related: []
---

## Historical source

> - 2026-10-10 [runtime] A post-admission crash in the legacy `HostOwnedHelloSession` used to restart into its consumed one-session link: successors died at `Bun.connect` ENOENT (`KELD-IPC-001`), and nested `KELD-RUNTIME-002` vs `-012` flipped at a ~55-90 ms caller-teardown delay (Apple M4) while 700 loop-under-load runs never failed. Reproduce such races by forcing the order (a diagnostic delay, or teardown after supervision ends itself), not by looping. (evidence: GH-674, `HostOwnedHelloSession` successor gate)
