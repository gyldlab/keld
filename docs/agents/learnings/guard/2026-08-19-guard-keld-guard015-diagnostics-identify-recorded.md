---
name: "2026-08-19-guard-keld-guard015-diagnostics-identify-recorded"
description: "KELD-GUARD015 diagnostics must identify the recorded oracle layer, not just the expected layer."
area: "guard"
tags: ["guard", "keld", "guard015", "diagnostics", "identify"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L102"
related: ["docs/agents/learnings/guard/2026-08-19-guard-guard-owns-kel-admission-errors.md", "docs/agents/learnings/runtime/2026-08-28-runtime-proc-setpcontrol-parent-death-detection.md"]
---

## Historical source

> - 2026-08-19 [guard] `ProofLayerMismatch` (`KELD-GUARD015`) Display must include the recorded oracle; `recorded_layer == OsContainment` makes expected and recorded layer ids identical when a JS-shim or host-protocol oracle is written into an OS slot. (evidence: KEL-78 T1 PR #41, `crates/keld-guard/src/admit.rs`)
