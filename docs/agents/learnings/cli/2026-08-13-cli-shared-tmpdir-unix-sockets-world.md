---
name: "2026-08-13-cli-shared-tmpdir-unix-sockets-world"
description: "Shared TMPDIR Unix sockets are world-connectable; use a private session directory and remove it on shutdown."
area: "cli"
tags: ["cli", "shared", "tmpdir", "unix", "sockets"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L41"
related: []
---

## Historical source

> - 2026-08-13 [cli] Unix echo sockets in shared `$TMPDIR` are world-connectable; bind inside a unique `0o700` session dir and remove the dir on Drop/shutdown. (evidence: crates/keld-core/src/echo_link.rs, formerly keld-cli)
