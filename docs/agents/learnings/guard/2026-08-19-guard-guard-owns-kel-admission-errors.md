---
name: "2026-08-19-guard-guard-owns-kel-admission-errors"
description: "The guard owns KEL-78 admission errors; connection success does not prove OS denial."
area: "guard"
tags: ["guard", "owns", "kel", "admission"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L101"
related: ["docs/agents/learnings/guard/2026-08-19-guard-keld-guard015-diagnostics-identify-recorded.md", "docs/agents/learnings/runtime/2026-08-19-runtime-strict-mode-cannot-inherit-host.md"]
---

## Historical source

> - 2026-08-19 [guard] KEL-78 T1 `admit` is in `keld-guard` (`KELD-GUARD008`–`015`); the spec sketch's `KELD-RUNTIME-0xx` names the caller, not the emitting crate. Live `bind`/`connect` success or `ConnectionRefused` is not an OS-deny pass. (evidence: KEL-78, `crates/keld-guard/src/admit.rs`)
