---
name: "2026-08-25-ipc-bun-unix-option-connects-windows"
description: "Bun's unix option connects to Windows named pipes; this does not prove the pipe DACL."
area: "ipc"
tags: ["ipc", "bun", "unix", "option", "connects", "windows"]
platform: "windows"
recorded: "2026-08-25"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L130"
related: ["docs/agents/learnings/ipc/2026-08-23-ipc-verify-named-pipe-readwrite-access.md", "docs/agents/learnings/runtime/2026-08-24-runtime-bun-version-report-canary-inspect.md"]
---

## Historical source

> - 2026-08-25 [ipc] On Windows, `Bun.connect({unix: "\\\\.\\pipe\\<name>"})` (a JS string literal; it evaluates to the path `\\.\pipe\<name>`) connects to a Win32 named pipe — the `unix:` option is the pipe path option too, so KEL-101 needs no second client API and the `process.platform === "win32"` TCP branch collapses into the existing call rather than gaining a third transport shape. `node:net` `createConnection({path})` works identically. Measured on Bun 1.4.0 stable / Windows 11 26200 against a .NET `NamedPipeServerStream` (`PipeOptions.Asynchronous`): full PING/PONG, 3/3 repeats, with a nonexistent-pipe negative control failing. Default pipe security only — this says nothing about the spec's `0x0012_019B` DACL, whose evidence is a separate probe. (evidence: KEL-101, `packages/@keld/electron/src/link.ts:503-513`, `crates/keld-cli/templates/hello/src/kipc.ts:417-427`)
