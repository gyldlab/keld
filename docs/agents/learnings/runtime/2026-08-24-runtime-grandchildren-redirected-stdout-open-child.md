---
name: "2026-08-24-runtime-grandchildren-redirected-stdout-open-child"
description: "Grandchildren keep redirected stdout open after child termination; spawn test children directly."
area: "runtime"
tags: ["runtime", "grandchildren", "redirected", "stdout", "open"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L128"
related: []
---

## Historical source

> - 2026-08-24 [runtime] Killing a supervised child does not close its stdout pipe if a grandchild inherited it: the capture-thread join then blocks for the grandchild's whole lifetime (measured: 100us for `sh -c "sleep 5"` which execs, 4.75s for `sh -c "sleep 5 & wait"` which forks). A shell-wrapped test fixture is enough to hit it — `cmd /C "ping ... >NUL"` runs ping as a grandchild. Spawn test children directly unless the shell is the contract. (evidence: KEL-118, CI run 32780024481 failing at 30.007s = the child's lifetime)
