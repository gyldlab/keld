---
name: "2026-08-29-core-blocking-stdin-lease-deadlock-quit"
description: "A blocking stdin lease can deadlock Quit; use bounded nonblocking drains and serialize teardown claims."
area: "core"
tags: ["core", "blocking", "stdin", "lease", "deadlock"]
platform: "unspecified"
recorded: "2026-08-29"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L145"
related: ["docs/agents/learnings/runtime/2026-08-29-runtime-guardian-poll-supervisor-events-beside.md", "docs/agents/learnings/runtime/2026-08-29-runtime-timed-fixed-size-reads-retain.md"]
---

## Historical source

> - 2026-08-29 [core] A joined blocking stdin lease reader deadlocks normal Quit because `keld dev` must retain its writer while waiting for the host; mark fd 0 nonblocking/CLOEXEC, drain only a fixed budget per guardian-owner poll, and serialize EOF/Call claims plus writes through one transition guard so data cannot starve liveness and exactly one teardown cause wins. (evidence: KEL-96/T2; `crates/keld-core/src/app_session.rs`)
