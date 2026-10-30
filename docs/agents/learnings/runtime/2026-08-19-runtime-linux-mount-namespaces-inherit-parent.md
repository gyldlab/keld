---
name: "2026-08-19-runtime-linux-mount-namespaces-inherit-parent"
description: "Linux mount namespaces inherit parent mounts; strict isolation needs explicit host-path denial tests."
area: "runtime"
tags: ["runtime", "linux", "mount", "namespaces", "inherit"]
platform: "linux"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L93"
related: ["docs/agents/learnings/runtime/2026-08-19-runtime-strict-admission-complete-os-containment.md"]
---

## Historical source

> - 2026-08-19 [runtime] `CLONE_NEWNS` initializes a copy of the parent's mount list; Linux strict still needs an explicit host-path deny plus role-private allow tests, and Landlock is not that deny. (evidence: KEL-78, `mount_namespaces(7)` man-pages 6.18, `docs/specs/kel78-strict-profile-sandbox.md`)
