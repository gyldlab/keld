---
name: "2026-08-18-compat-drain-queued-reader-output-asserting"
description: "Drain queued reader output before asserting WAITING without READY and signaling readiness."
area: "compat"
tags: ["compat", "drain", "queued", "reader", "output"]
platform: "unspecified"
recorded: "2026-08-18"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L74"
related: ["docs/agents/learnings/compat/2026-08-18-compat-bun-quit-fixture-close-kipc.md", "docs/agents/learnings/ts/2026-08-18-ts-bunfig-aliases-remap-runtime-electron.md"]
---

## Historical source

> - 2026-08-18 [compat] Integration stdout `has()` on an accumulator misses lines still queued on the reader `rx`; drain `try_recv` and assert WAITING-without-READY *before* `signal_ready` or a stub `whenReady` can pass. (evidence: KEL-72 finding 3, `crates/keld-compat/tests/electron_lifecycle.rs`)
