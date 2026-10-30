---
name: "2026-10-28-compat-host-created-precedes-wrapper-ownership"
description: "Host Created precedes wrapper ownership; clean only unpublished Creates and inject after actual correlation."
area: "compat"
tags: ["compat", "host", "created", "precedes", "wrapper"]
platform: "unspecified"
recorded: "2026-10-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L196"
related: ["docs/agents/learnings/compat/2026-10-28-compat-boot-selection-captured-imports-windows.md"]
---

## Historical source

> - 2026-10-28 [compat] Host Created precedes wrapper ownership. Captured registries and confirmed cleanup protect unpublished new Creates; adopted/published owners stay. Native post-effect injection starts after actual Created, not before correlation allocation. (evidence: PR1181 r4396003287; browser-window-ownership.test.ts; facade_window native test)
