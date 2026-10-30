---
name: "2026-08-29-runtime-timed-fixed-size-reads-retain"
description: "Timed fixed-size reads must retain partial bytes across WouldBlock instead of restarting the buffer."
area: "runtime"
tags: ["runtime", "timed", "fixed", "size", "reads"]
platform: "unspecified"
recorded: "2026-08-29"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L146"
related: ["docs/agents/learnings/cli/2026-08-30-cli-protected-child-path-swapped-file.md", "docs/agents/learnings/core/2026-08-29-core-blocking-stdin-lease-deadlock-quit.md"]
---

## Historical source

> - 2026-08-29 [runtime] A fixed-size record on a timed Unix stream still needs an incremental reader: `read_exact` can consume a prefix and then return `WouldBlock`, so retrying with a fresh buffer corrupts the next record. Retain the filled prefix across poll expiry and test split delivery. (evidence: KEL-96/T3, `macos_guardian::tests::generation_control_wire::generation_control_reader_retains_split_bytes_across_would_block`)
