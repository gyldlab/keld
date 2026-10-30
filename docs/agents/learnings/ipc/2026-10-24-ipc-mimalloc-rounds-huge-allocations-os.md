---
name: "2026-10-24-ipc-mimalloc-rounds-huge-allocations-os"
description: "mimalloc rounds huge allocations through the OS and shrinking can retain larger bins."
area: "ipc"
tags: ["ipc", "mimalloc", "rounds", "huge", "allocations", "linux"]
platform: "linux"
recorded: "2026-10-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L186"
related: ["docs/agents/learnings/ipc/2026-10-24-ipc-inspector-omits-backing-edges-controlled.md", "docs/agents/learnings/runtime/2026-10-28-runtime-windows-short-path-aliases-evaluate.md"]
---

## Historical source

> - 2026-10-24 [ipc/perf] The pinned Linux x64 Bun 1.4.2 release uses external mimalloc 3.5.0 (`6a64e1ba`). Its `mi_good_size(1572880)` reports 1576960, while actual usable allocation is 1638400: huge requests use the separate OS-rounding path. Shrinking realloc can retain an older larger bin, so current request size alone does not price retained backing. This allocation term excludes global allocator pages/RSS. (evidence: GH-1149 allocator follow-up; 524,289 size cases and 49 native allocation samples)
