---
name: "2026-08-17-ci-nested-shell-expansion-freeze-stale"
description: "Nested shell expansion can freeze stale status; write scripts to files and preserve exit codes in traps."
area: "ci"
tags: ["ci", "nested", "shell", "expansion", "freeze", "linux"]
platform: "linux"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L69"
related: []
---

## Historical source

> - 2026-08-17 [ci] Nesting `\$?` inside layered `bash -c "..."`/`wsl ... -c "..."` strings lets an outer shell expand it first and bake in a stale status; write multi-layer harness scripts to files. A `trap '... || true' EXIT` clobbers `$?`; capture it first: `trap 'ec=$?; cleanup; exit "$ec"' EXIT`. (evidence: `.github/workflows/ci.yml` `linux-gui-smoke`, local WSL dry-run)
