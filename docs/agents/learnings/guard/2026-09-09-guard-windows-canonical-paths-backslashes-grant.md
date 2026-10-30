---
name: "2026-09-09-guard-windows-canonical-paths-backslashes-grant"
description: "Windows canonical paths use backslashes; grant boundaries need forward slashes while backslash paths remain valid."
area: "guard"
tags: ["guard", "windows", "canonical", "paths", "backslashes"]
platform: "windows"
recorded: "2026-09-09"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L159"
related: ["docs/agents/learnings/test/2026-09-09-test-double-escaped-json-fixtures-grant.md"]
---

## Historical source

> - 2026-09-09 [guard/windows] `fs::canonicalize` gives all-backslash `\\?\C:\…`; `/**` matches root only there: resources need `/` at grant boundaries. Grant backslashes work (`C:\Users/**` strips); grant slash rewrites fail. (evidence: KEL-208 PR #203)
