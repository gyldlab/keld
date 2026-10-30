---
name: "2026-08-19-ipc-macos-accepted-streams-inherit-nonblocking"
description: "macOS accepted streams may inherit nonblocking mode; restore blocking before applying the app-link deadline."
area: "ipc"
tags: ["ipc", "macos", "accepted", "streams", "inherit"]
platform: "macos"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L81"
related: ["docs/agents/learnings/ipc/2026-08-19-ipc-unlinking-bootstrap-socket-close-listener.md", "docs/agents/learnings/runtime/2026-08-19-runtime-macos-unix-socket-sun-len-private-tmp.md"]
---

## Historical source

> - 2026-08-19 [ipc] A non-blocking Unix listener can admit a stream that behaves non-blocking on macOS; normalize the accepted app-link stream back to blocking before applying `APP_LINK_IO_DEADLINE`, otherwise the session loop observes immediate `KELD-IPC-006` after a quiet-but-live reply boundary. (evidence: KEL-75, `crates/keld-ipc/src/bootstrap.rs`, `crates/keld-cli/src/echo_link.rs` `roundtrip_over_loopback_copies_fields`)
