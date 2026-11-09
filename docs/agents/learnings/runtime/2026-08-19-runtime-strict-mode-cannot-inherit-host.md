---
name: "2026-08-19-runtime-strict-mode-cannot-inherit-host"
description: "Strict mode cannot inherit host grants or child ambient sockets; leaving strict must fail admission."
area: "runtime"
tags: ["runtime", "strict", "mode", "cannot", "inherit"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L95"
related: ["docs/agents/learnings/guard/2026-08-19-guard-guard-owns-kel-admission-errors.md", "docs/agents/learnings/runtime/2026-08-19-runtime-strict-admission-complete-os-containment.md"]
---

## Historical source

> - 2026-08-19 [runtime] KEL-78 forbids inherit-from-host and child-ambient sockets from a guard grant; leaving Strict is admission failure, not an unsandboxed restart. (evidence: KEL-78, `docs/specs/kel78-strict-profile-sandbox.md`)
