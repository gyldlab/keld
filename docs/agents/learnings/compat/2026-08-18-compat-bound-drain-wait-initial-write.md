---
name: "2026-08-18-compat-bound-drain-wait-initial-write"
description: "Bound drain.wait by the initial write deadline so a slow peer cannot reset the timeout."
area: "compat"
tags: ["compat", "bound", "drain", "wait", "initial"]
platform: "unspecified"
recorded: "2026-08-18"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L78"
related: ["docs/agents/learnings/wv/2026-08-18-wv-media-authorization-belongs-minted-webviewid.md"]
---

## Historical source

> - 2026-08-18 [compat] writeOneFrame must bound drain.wait() by remaining wall-clock from the first write; restarting withIoDeadline per wait lets a draining peer never hit KELD-IPC-006. (evidence: PR #27 CodeRabbit, packages/@keld/electron/src/link.ts)
