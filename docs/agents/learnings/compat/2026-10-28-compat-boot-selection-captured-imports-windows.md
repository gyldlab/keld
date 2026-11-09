---
name: "2026-10-28-compat-boot-selection-captured-imports-windows"
description: "Boot selection is captured before imports; Windows inherits the hint, Linux clears it, and T1B omits API selection."
area: "compat"
tags: ["compat", "boot", "selection", "captured", "imports", "multiple"]
platform: "unspecified"
recorded: "2026-10-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L195"
related: ["docs/agents/learnings/compat/2026-10-28-compat-host-created-precedes-wrapper-ownership.md", "docs/agents/learnings/ipc/2026-10-28-ipc-captured-getownpropertydescriptor-triggers-proxy-traps.md"]
---

## Historical source

> - 2026-10-28 [compat] Mac preload captures one boot selection for Electron/API; parent fixture env precedes imports, so later edits cannot change that launch. Windows legacy inherited the hint; Linux clears ambient env. Raw T1B Ready omits API selection. (evidence: PR1181 r4396003281/r4396334969; browser-window-boot.test.ts; Windows lifecycle regression)
