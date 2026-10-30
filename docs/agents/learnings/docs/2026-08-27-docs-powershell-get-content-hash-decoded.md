---
name: "2026-08-27-docs-powershell-get-content-hash-decoded"
description: "PowerShell 5.1 Get-Content can hash decoded Markdown incorrectly; specify UTF-8 or hash raw bytes."
area: "docs"
tags: ["docs", "powershell", "get", "content", "hash", "windows"]
platform: "windows"
recorded: "2026-08-27"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L139"
related: ["docs/agents/learnings/host/2026-08-21-host-hostownedhellosession-core-owned-separate-keld.md", "docs/agents/learnings/runtime/2026-08-29-runtime-guardian-poll-supervisor-events-beside.md"]
---

## Historical source

> - 2026-08-27 [docs] Windows PowerShell 5.1 `Get-Content` does not default to UTF-8, so hashing decoded Markdown without `-Encoding utf8` produced a reproducible but wrong approval digest; byte-bound provenance checks must specify the decoder or hash raw bytes. (evidence: KEL-96 decision block; PowerShell and Bun SHA-256 cross-check)
