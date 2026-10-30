---
name: "2026-08-23-ipc-verify-named-pipe-readwrite-access"
description: "Verify named-pipe ReadWrite access with the exact mask and descriptor read-back."
area: "ipc"
tags: ["ipc", "verify", "named", "pipe", "readwrite"]
platform: "unspecified"
recorded: "2026-08-23"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L118"
related: ["docs/agents/learnings/ipc/2026-08-25-ipc-bun-unix-option-connects-windows.md"]
---

## Historical source

> - 2026-08-23 [ipc] `PipeAccessRights::ReadWrite` is `0x0002_019B`, not final pipe ACE `0x0012_019B`; an explicit `SYNCHRONIZE` OR and descriptor read-back keep the intended access mask testable. (evidence: KEL-101, CodeRabbit PR #69, local .NET `PipeAccessRights` reflection)
