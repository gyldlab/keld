---
name: "2026-08-21-process-competitor-clones-visible-agents-listed"
description: "Competitor clones are visible to agents only when listed in the lock file."
area: "process"
tags: ["process", "competitor", "clones", "visible", "agents"]
platform: "unspecified"
recorded: "2026-08-21"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L105"
related: []
---

## Historical source

> - 2026-08-21 [process] Local analysis clones under gitignored `competitors/` (bun, wry, tao) are invisible to agents unless listed in `competitors.lock.toml`; `just competitors-sync` only fetches lock entries. (evidence: competitors.lock.toml, tools/competitors_sync.rs, KEL-68)
