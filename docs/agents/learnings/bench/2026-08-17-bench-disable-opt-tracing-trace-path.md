---
name: "2026-08-17-bench-disable-opt-tracing-trace-path"
description: "Disable opt-in tracing only when its trace path is absent, not when the benchmark token is absent."
area: "bench"
tags: ["bench", "disable", "opt", "tracing", "trace"]
platform: "unspecified"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L65"
related: ["docs/agents/learnings/bench/2026-08-17-bench-neutralino-macos-document-focus-makefirstresponder.md", "docs/agents/learnings/bench/2026-08-17-bench-pageloadevent-finished-navigation-completion-score.md"]
---

## Historical source

> - 2026-08-17 [bench] KEL-64 supplies `KELD_BENCH_TOKEN` to every arm; an opt-in fixture trace must disable itself when its trace-path variable is absent, not when the token is absent, or normal benchmark arms exit before launch ownership can be captured. (evidence: gyldlab/keld-benches@258756c, `/tmp/keld-startup-untraced-one-20260817.json`)
