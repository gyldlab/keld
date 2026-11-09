---
name: "2026-08-30-process-sip-strips-inherited-dyld-settings"
description: "SIP strips inherited DYLD settings; set them inside the suspended shell before executing the staged host."
area: "process"
tags: ["process", "sip", "strips", "inherited", "dyld", "macos"]
platform: "macos"
recorded: "2026-08-30"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L148"
related: ["docs/agents/learnings/ipc/2026-08-24-ipc-rust-string-errors-callerror-incompatible.md"]
---

## Historical source

> - 2026-08-30 [process/macos] macOS SIP strips inherited `DYLD_INSERT_LIBRARIES` when a test launches protected `/bin/sh`; set the variable inside the suspended shell immediately before `exec` of the unsigned staged host, and use a `__DATA,__interpose` entry rather than flat-namespace symbol replacement. (evidence: KEL-102/T2 `PolicyReadFault`, `crates/keld-host/tests/no_flag_macos.rs`)
