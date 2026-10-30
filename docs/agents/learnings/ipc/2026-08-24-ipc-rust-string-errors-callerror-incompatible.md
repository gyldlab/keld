---
name: "2026-08-24-ipc-rust-string-errors-callerror-incompatible"
description: "Rust String errors and CallError have incompatible wire shapes; test both runtime error mappings."
area: "ipc"
tags: ["ipc", "rust", "string", "errors", "callerror"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L122"
related: ["docs/agents/learnings/process/2026-08-30-process-sip-strips-inherited-dyld-settings.md"]
---

## Historical source

> - 2026-08-24 [ipc] A bare `String` `Err` payload and a `CallError` are mutually undecodable under `keld_ipc::codec::decode` (postcard `from_bytes` would silently read `code` as the whole string), so a payload-shape migration fails loudly: `KELD-IPC-003` in Rust, `KELD-IPC-005` in `@keld/electron`. (evidence: KEL-102, crates/keld-ipc/src/call_error.rs, crates/keld-ipc/tests/guard_dispatch.rs)
