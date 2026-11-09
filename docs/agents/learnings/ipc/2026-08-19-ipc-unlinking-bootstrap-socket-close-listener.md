---
name: "2026-08-19-ipc-unlinking-bootstrap-socket-close-listener"
description: "Unlinking a bootstrap socket does not close its listener; host-owned listener state coordinates shutdown."
area: "ipc"
tags: ["ipc", "unlinking", "bootstrap", "socket", "close"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L80"
related: ["docs/agents/learnings/ipc/2026-08-19-ipc-macos-accepted-streams-inherit-nonblocking.md"]
---

## Historical source

> - 2026-08-19 [ipc] Unlinking a Unix bootstrap socket path after authentication is not the same as closing the listener fd; one-shot admission needs host-owned listener state (`Option<UnixListener>`) so shutdown/deadline/bind close the fd and remove the locator together. (evidence: KEL-75, `crates/keld-ipc/src/bootstrap.rs`)
