---
name: "2026-08-29-runtime-macos-pipes-inherit-descriptors-unless"
description: "macOS pipes inherit descriptors unless both ends use FD_CLOEXEC; isolate liveness writers before spawn."
area: "runtime"
tags: ["runtime", "macos", "pipes", "inherit", "descriptors"]
platform: "macos"
recorded: "2026-08-29"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L143"
related: ["docs/agents/learnings/build/2026-08-28-build-place-integration-crate-documentation-cfg.md"]
---

## Historical source

> - 2026-08-29 [runtime] An anonymous pipe made with `pipe(2)` is inheritable on macOS unless both ends receive `FD_CLOEXEC`; a child-held copy of the host-liveness writer prevents guardian EOF indefinitely, so the production boundary and fixtures must validate/set descriptor isolation before spawn. (evidence: `crates/keld-runtime/src/macos_guardian.rs`; KEL-78/T2b hanging negative observation)
