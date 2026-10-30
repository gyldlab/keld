---
name: "2026-08-31-ipc-normalize-win32-pipe-close-errors"
description: "Normalize Win32 pipe-close errors to EOF; graceful close must not discard unread bytes with DisconnectNamedPipe."
area: "ipc"
tags: ["ipc", "normalize", "win32", "pipe", "close", "windows"]
platform: "windows"
recorded: "2026-08-31"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L154"
related: ["docs/agents/learnings/runtime/2026-08-24-runtime-bun-version-report-canary-inspect.md", "docs/agents/learnings/ts/2026-10-08-ts-bun-cannot-half-close-socket.md"]
---

## Historical source

> - 2026-08-31 [ipc/windows] Win32 byte-mode named pipes report peer close as `ERROR_BROKEN_PIPE`/`ERROR_NO_DATA`/`ERROR_PIPE_NOT_CONNECTED`, not Rust socket EOF; normalize those read results to zero bytes so framing still owns `KELD-IPC-001` for premature EOF and completed session loops can terminate cleanly. A graceful close must not call `DisconnectNamedPipe` (it discards unread bytes): cancel own I/O, then close the HANDLE (#1073, gh527 §10). (evidence: KEL-101/T3 real Windows no-flag lifecycle and negative controls)
