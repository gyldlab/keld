---
name: "2026-08-19-runtime-latch-admission-completion-failure-deadline"
description: "Latch admission completion on failure or deadline so later revoke cannot replace the original error."
area: "runtime"
tags: ["runtime", "latch", "admission", "completion", "failure"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L98"
related: ["docs/agents/learnings/ipc/2026-08-19-ipc-macos-wake-socket-notconnected-mean.md", "docs/agents/learnings/runtime/2026-08-23-runtime-lifecycle-drain-revoke-virtual-ports.md"]
---

## Historical source

> - 2026-08-19 [runtime] Latch bootstrap `admission_done` on deadline/failure/disconnect; otherwise a later `revoke` can overwrite the real admission error with `Disconnected`. (evidence: KEL-75 T1b review, `crates/keld-runtime/src/unix_role.rs`)
