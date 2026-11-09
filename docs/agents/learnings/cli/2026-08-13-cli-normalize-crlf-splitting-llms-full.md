---
name: "2026-08-13-cli-normalize-crlf-splitting-llms-full"
description: "Normalize CRLF before splitting the llms-full corpus on marker lines."
area: "cli"
tags: ["cli", "normalize", "crlf", "splitting", "llms", "windows"]
platform: "windows"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L46"
related: []
---

## Historical source

> - 2026-08-13 [cli] llms-full.txt corpus split on LF-only markers yields empty chunks on Windows CRLF checkout; normalize \r\n in docs_search::corpus(). (evidence: GHA windows-latest docs_search tests on 6cc9749)
