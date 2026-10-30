---
name: "2026-08-19-runtime-strict-admission-complete-os-containment"
description: "Strict admission needs a complete OS-containment catalog and matching token/profile digest."
area: "runtime"
tags: ["runtime", "strict", "admission", "complete", "os"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L94"
related: ["docs/agents/learnings/runtime/2026-08-19-runtime-linux-mount-namespaces-inherit-parent.md", "docs/agents/learnings/runtime/2026-08-19-runtime-strict-mode-cannot-inherit-host.md"]
---

## Historical source

> - 2026-08-19 [runtime] KEL-78 `admit` cannot return Strict from protocol/cleanup/limit rows or a binary digest alone; the archive needs a complete OS-containment catalog and a matching token/profile digest. (evidence: KEL-78, `docs/specs/kel78-strict-profile-sandbox.md`)
