---
name: "2026-08-24-runtime-crash-counts-race-stdout-readiness"
description: "Crash counts race stdout readiness; compare the ready marker byte offset against the crash ledger."
area: "runtime"
tags: ["runtime", "crash", "counts", "race", "stdout"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L126"
related: ["docs/agents/learnings/runtime/2026-08-24-runtime-recovered-crashes-remain-successful-crash.md"]
---

## Historical source

> - 2026-08-24 [runtime] `keld-runtime` publishes stdout and `SupervisorEvent::Exited` *before* `record_crash`, so a host that samples the crash *count* when it first notices a ready marker can already see a post-ready death and fold it into its "recovered" baseline — reporting success over a dead app. Counting crashes cannot order them against app output; the ledger records stdout length at each crash and the host compares the marker's offset. (evidence: KEL-105 re-open, `crates/keld-runtime/src/lib.rs:725-756`, `crates/keld-core/src/hello_session.rs` `recovered_crash_baseline`)
