---
name: "2026-10-24-ipc-inspector-omits-backing-edges-controlled"
description: "Inspector omits backing edges; controlled WeakRef probes show release, not physical-memory bounds."
area: "ipc"
tags: ["ipc", "inspector", "omits", "backing", "edges"]
platform: "unspecified"
recorded: "2026-10-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L185"
related: ["docs/agents/learnings/ipc/2026-10-24-ipc-mimalloc-rounds-huge-allocations-os.md"]
---

## Historical source

> - 2026-10-24 [ipc/test] On Bun 1.4.2/JSC `2e2aa229`, Inspector omits Map backing-to-value edges and Array indexed backing bytes: an Array reports 16 B even with 32,768 entries. Explicit `Bun.gc(true)` finalizes the kept-object epoch around full collection, including inside one callback. Separate-job WeakRef observations with live source/current/external controls detect replacement/end release; omitted Map.clear and omitted actual onEnd registration retain the four observed snapshot objects. These are release witnesses, not physical-byte bounds. (evidence: GH-1149, `packages/@keld/api/src/window-source-release.test-role.ts`, isolated close/Quit controls)
