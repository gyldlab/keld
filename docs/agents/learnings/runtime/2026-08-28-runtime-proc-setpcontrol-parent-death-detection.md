---
name: "2026-08-28-runtime-proc-setpcontrol-parent-death-detection"
description: "proc_setpcontrol is not parent-death detection; use a separate liveness guardian to signal the process group."
area: "runtime"
tags: ["runtime", "proc", "setpcontrol", "parent", "death", "macos"]
platform: "macos"
recorded: "2026-08-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L141"
related: ["docs/agents/learnings/build/2026-08-28-build-place-integration-crate-documentation-cfg.md", "docs/agents/learnings/guard/2026-08-19-guard-keld-guard015-diagnostics-identify-recorded.md"]
---

## Historical source

> - 2026-08-28 [runtime] macOS `proc_setpcontrol(PROC_SETPC_TERMINATE)` controls resource-starvation policy, not parent death; a raw staged host needs an independent guardian whose read end observes EOF from a host-owned non-inheritable liveness writer, after which the guardian signals the enrolled process group and waits the direct Bun child. (evidence: macOS 26.5 SDK `libproc.h`, `pipe(2)`, `killpg(2)`, KEL-78/T2a probe)
