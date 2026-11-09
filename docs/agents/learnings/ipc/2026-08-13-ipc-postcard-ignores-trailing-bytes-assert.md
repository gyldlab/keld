---
name: "2026-08-13-ipc-postcard-ignores-trailing-bytes-assert"
description: "postcard ignores trailing bytes; assert echo re-encoding differs instead of expecting a decode error."
area: "ipc"
tags: ["ipc", "postcard", "ignores", "trailing", "bytes"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L34"
related: []
---

## Historical source

> - 2026-08-13 [ipc] postcard `from_bytes` ignores trailing bytes (kipc `decode` rejects them, onboarding 04 §7), so a no-op echo test must assert re-encoded output ≠ input, not expect a codec error. (evidence: crates/keld-ipc/src/codec.rs, crates/keld-ipc/src/echo.rs `echo_roundtrip_copies_fields_not_input_bytes`)
