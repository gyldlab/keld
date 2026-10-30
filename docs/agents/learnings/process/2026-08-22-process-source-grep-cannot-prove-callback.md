---
name: "2026-08-22-process-source-grep-cannot-prove-callback"
description: "Source grep cannot prove callback registration; invoke handlers and assert observable state."
area: "process"
tags: ["process", "source", "grep", "cannot", "prove"]
platform: "unspecified"
recorded: "2026-08-22"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L110"
related: ["docs/agents/learnings/wv/2026-08-22-wv-emit-nav-finished-never-navigation.md", "docs/agents/learnings/wv/2026-08-22-wv-wkwebview-startup-code-macos-gated.md"]
---

## Historical source

> - 2026-08-22 [process] `include_str!` / source-grep wiring tests pass when helper text exists but `create` never registers the callback; invoke the handler and assert observable state instead. (evidence: KEL-62, CodeRabbit #10, `crates/keld-wv/src/wkwebview/mod.rs` `startup_tests`)
