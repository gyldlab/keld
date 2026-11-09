---
name: "2026-08-19-runtime-bun-kill-exit-behavior-differs"
description: "Bun kill-after-exit behavior differs from Node; probe parent and child runtime combinations."
area: "runtime"
tags: ["runtime", "bun", "kill", "exit", "behavior"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L99"
related: ["docs/agents/learnings/runtime/2026-08-19-runtime-corpus-digest-length-framing-concatenated.md"]
---

## Historical source

> - 2026-08-19 [runtime] Child-process divergences are often parent-attributed: Bun 1.3.14 `subprocess.kill()` after `'exit'` returned `true` without delivering a signal (fixed in Bun 1.4; Node returns `false`). Probe the parent x child runtime matrix before blaming the child. (evidence: KEL-77, crates/keld-runtime/fixtures/child-process/)
