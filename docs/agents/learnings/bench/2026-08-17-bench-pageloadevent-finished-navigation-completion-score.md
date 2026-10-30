---
name: "2026-08-17-bench-pageloadevent-finished-navigation-completion-score"
description: "PageLoadEvent::Finished is navigation completion; score untraced first paint with the external double-rAF beacon."
area: "bench"
tags: ["bench", "pageloadevent", "finished", "navigation", "completion", "macos"]
platform: "macos"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L66"
related: ["docs/agents/learnings/bench/2026-08-17-bench-disable-opt-tracing-trace-path.md"]
---

## Historical source

> - 2026-08-17 [bench] wry `PageLoadEvent::Finished` is navigation completion, not composited first paint, and a product clock started after spawn cannot support architecture 01 §5; KEL-64 scores are the external double-rAF beacon on untraced arms only. (evidence: gyldlab/keld#10, keld-benches@aae2e12 `macos/keld/hello/kel64-startup-attribution.json`)
