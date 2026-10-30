---
name: "2026-08-31-test-socket-flush-proves-local-buffering"
description: "Socket flush proves only local buffering; require peer acknowledgment before an intentional crash exit."
area: "test"
tags: ["test", "socket", "flush", "proves", "local", "windows"]
platform: "windows"
recorded: "2026-08-31"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L152"
related: []
---

## Historical source

> - 2026-08-31 [test/windows] Bun `Socket.flush()` proves only a local flush attempt, not that a Rust peer observed queued control records before `process.exit`; an abrupt exit can yield `WSAECONNRESET` with an empty read. Crash-order fixtures need a peer acknowledgment after the required records, then the intentional exit, so the witness is causal without sleep synchronization. (evidence: KEL-151, GitHub Windows run `33355298740`)
