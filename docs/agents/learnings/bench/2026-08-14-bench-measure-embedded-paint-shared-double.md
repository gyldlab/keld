---
name: "2026-08-14-bench-measure-embedded-paint-shared-double"
description: "Measure embedded first paint with a shared double-rAF image beacon; title and fetch signals are invalid."
area: "bench"
tags: ["bench", "measure", "embedded", "paint", "shared"]
platform: "unspecified"
recorded: "2026-08-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L55"
related: []
---

## Historical source

> - 2026-08-14 [bench] Timing first paint in an embedded webview: `document.title` never reaches the native window caption (the framework owns it, not the document) and `fetch()` is CORS-blocked on the opaque `with_html`/`NavigateToString` origin — use an `<img>` beacon inside a double `requestAnimationFrame` against one local `HttpListener` so all arms share a clock; titled-`HWND` time is presentation policy, not paint (real Keld-vs-Tauri gap 1.8x, not the 6.6x that column implied). (evidence: docs/engineering/budget-scoreboard.md "Time to first paint (2026-08-14)", gyldlab/keld-benches MEASUREMENTS.md)
