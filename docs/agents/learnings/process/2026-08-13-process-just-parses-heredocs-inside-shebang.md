---
name: "2026-08-13-process-just-parses-heredocs-inside-shebang"
description: "just parses heredocs inside shebang recipes as syntax; use arrays and printf fixtures."
area: "process"
tags: ["process", "just", "parses", "heredocs", "inside"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L50"
related: []
---

## Historical source

> - 2026-08-13 [process] just 1.58 still lexes `<<'EOF'` bodies inside shebang recipes as justfile syntax (`unknown start of token '-'`); use bash arrays/printf fixtures instead of heredocs in the justfile. (evidence: `just --list` on agents-md heredocs vs array rewrite)
