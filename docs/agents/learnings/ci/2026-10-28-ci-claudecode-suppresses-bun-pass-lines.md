---
name: "2026-10-28-ci-claudecode-suppresses-bun-pass-lines"
description: "CLAUDECODE suppresses Bun pass lines; unset it for compatibility corpus admission."
area: "ci"
tags: ["ci", "claudecode", "suppresses", "bun", "pass"]
platform: "unspecified"
recorded: "2026-10-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L197"
related: []
---

## Historical source

> - 2026-10-28 [ci] `CLAUDECODE=1` hides Bun `(pass)` lines, so keld-compat corpus admission fails locally; run gates with `env -u CLAUDECODE`. (evidence: #1073)
