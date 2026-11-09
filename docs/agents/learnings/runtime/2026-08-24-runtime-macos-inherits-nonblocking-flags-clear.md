---
name: "2026-08-24-runtime-macos-inherits-nonblocking-flags-clear"
description: "macOS accept inherits nonblocking flags; clear them or receive deadlines become immediate EAGAIN."
area: "runtime"
tags: ["runtime", "macos", "inherits", "nonblocking", "flags", "multiple"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L125"
related: []
---

## Historical source

> - 2026-08-24 [runtime] BSD `accept()` gives the accepted socket the listener's file status flags, so on macOS a stream accepted from a `set_nonblocking(true)` listener arrives with `O_NONBLOCK` set and `SO_RCVTIMEO` is silently inert — reads return `EAGAIN`/`WouldBlock` immediately instead of waiting for the configured deadline. Linux's `accept4` does not inherit, so the same code is only flaky on macOS. Clear it explicitly on the accepted stream; a read deadline you never verified is not a deadline. (evidence: KEL-113, `crates/keld-runtime/src/unix_role_fixture.rs` `ControlPeer::new`, CI `clippy + test (macos-latest)` run 32765297839)
