---
name: "2026-10-08-ci-ubuntu-apt-stalls-persisted-contention"
description: "Ubuntu apt stalls persisted without contention; follow the per-leg install rule."
area: "ci"
tags: ["ci", "ubuntu", "apt", "stalls", "persisted", "linux"]
platform: "linux"
recorded: "2026-10-08"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L172"
related: []
---

## Historical source

> - 2026-10-08 [ci] Supersedes the 2026-08-19 apt-contention entry: slow and hung Ubuntu WebKitGTK apt steps (642 s download; ~44 min `apt-get update` hang) ran with no concurrent apt, so contention was not the cause; the per-leg install rule is in `.agents/ci.md`. (evidence: #624, #630, PR #642)
