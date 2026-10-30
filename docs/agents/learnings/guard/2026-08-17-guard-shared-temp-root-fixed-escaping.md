---
name: "2026-08-17-guard-shared-temp-root-fixed-escaping"
description: "A shared temp root makes fixed escaping filenames collide; give each denial fixture a unique suffix."
area: "guard"
tags: ["guard", "shared", "temp", "root", "fixed"]
platform: "unspecified"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L70"
related: []
---

## Historical source

> - 2026-08-17 [guard] Test trap for `..` denial (owned by keld-guard): `<granted-dir>/../<name>` resolves to the shared temp root whatever the granted dir's suffix, so a fixed escaping name collides with a crashed or parallel run; give the escaping filename its own unique suffix. (evidence: KEL-71; that test left with b4634076, whose retained broker rejects `..` as `KELD-GUARD002` before resolution)
