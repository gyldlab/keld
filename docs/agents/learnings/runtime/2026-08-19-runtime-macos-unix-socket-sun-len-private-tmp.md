---
name: "2026-08-19-runtime-macos-unix-socket-sun-len-private-tmp"
description: "macOS temp paths can exceed SUN_LEN; reserve socket filename bytes with a short private /tmp directory."
area: "runtime"
tags: ["exceed", "macos", "paths", "runtime", "sun-len", "temp", "tmpdir", "unix-socket"]
platform: "macos"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L82"
related: ["docs/agents/learnings/ipc/2026-08-19-ipc-macos-accepted-streams-inherit-nonblocking.md", "docs/agents/learnings/shell/2026-08-19-shell-zsh-lowercase-path-aliases-different.md"]
---

## Historical source

> - 2026-08-19 [runtime] macOS Unix-domain socket fixture paths must reserve bytes for the socket file itself; `/var/folders/...` from `std::env::temp_dir()` can exceed `SUN_LEN`, so control sockets should use a short owner-only `/tmp/<short-id>` directory. (evidence: KEL-75, `crates/keld-runtime/src/primary.rs` `PrimaryFixture::new`)
