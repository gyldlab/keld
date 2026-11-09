---
name: "2026-09-09-test-double-escaped-json-fixtures-grant"
description: "Double-escaped JSON fixtures can grant different paths; parse with the real parser and prove rows by mutation."
area: "test"
tags: ["test", "double", "escaped", "json", "fixtures"]
platform: "unspecified"
recorded: "2026-09-09"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L158"
related: ["docs/agents/learnings/guard/2026-09-09-guard-wildcard-url-grants-match-scheme.md", "docs/agents/learnings/guard/2026-09-09-guard-windows-canonical-paths-backslashes-grant.md"]
---

## Historical source

> - 2026-09-09 [test] A fixture string is escaped twice — once for JSON, once by whatever wrote it — so `"https:\\t//**"` in JSONC decodes to backslash+`t`, not a tab, and the row pins a different grant than its comment claims. A row sharing a grant list with a broader entry also records that entry's deny. Decode with the crate's own stripper plus a real JSON parser, and prove each row load-bearing by mutation. (evidence: KEL-208 PR #203, `scopes.jsonc`)
