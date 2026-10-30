---
name: "2026-08-14-wv-webview2-bounds-attach-measure-child"
description: "Set WebView2 bounds after attach; measure child startup at entry instead of PowerShell launch delay."
area: "wv"
tags: ["wv", "webview2", "bounds", "attach", "measure", "windows"]
platform: "windows"
recorded: "2026-08-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L56"
related: ["docs/agents/learnings/wv/2026-08-22-wv-wkwebview-startup-code-macos-gated.md"]
---

## Historical source

> - 2026-08-14 [wv/windows] wry sets WebView2 controller bounds only in its WM_SIZE subclass hook; unresized after attach: 0x0, no Chromium composition (~640 ms hello first-paint stall). Call `webview.set_bounds(window.inner_size())` once after build. PowerShell `Start-Process`: 450-700 ms before child main(); measure wall-clock spawn offset at child entry, or use .NET `Process.Start` (~11 ms). (evidence: KEL-62, crates/keld-wv/src/webview2/mod.rs, keld-benches windows/bench)
