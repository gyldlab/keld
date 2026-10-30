---
name: "2026-08-14-ipc-client-hello-prevents-server-leaking"
description: "Client-first HELLO prevents a server from leaking its token to unauthenticated loopback peers."
area: "ipc"
tags: ["ipc", "client", "hello", "prevents", "server", "windows"]
platform: "windows"
recorded: "2026-08-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L54"
related: []
---

## Historical source

> - 2026-08-14 [ipc] Why `HELLO` is client-first (onboarding 03 `handshake_server`): a server that wrote the token before the peer proved possession would leak it to any Windows loopback TCP connector. (evidence: KEL-60, crates/keld-ipc/src/link.rs `handshake_server`)
