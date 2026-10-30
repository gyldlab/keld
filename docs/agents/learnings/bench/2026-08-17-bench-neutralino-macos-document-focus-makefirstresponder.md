---
name: "2026-08-17-bench-neutralino-macos-document-focus-makefirstresponder"
description: "Neutralino macOS document focus needs makeFirstResponder after the focus restore path."
area: "bench"
tags: ["bench", "neutralino", "macos", "document", "focus"]
platform: "macos"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L64"
related: ["docs/agents/learnings/bench/2026-08-17-bench-disable-opt-tracing-trace-path.md", "docs/agents/learnings/bench/2026-08-17-bench-key-neutralino-window-prove-webview.md"]
---

## Historical source

> - 2026-08-17 [bench] For Neutralino v6.9.0 on macOS, calling `NSWindow makeFirstResponder:` for the runtime `WKWebView` after the documented `window.focus()` path's bring-to-front/restore branch is the minimal focused-document fix: a clean-source build passed 11/11 strict KEL-64 beacons; startup/delegate reassertions and `setInitialFirstResponder:` were unnecessary. (evidence: gyldlab/keld-benches@4666d0a, `/tmp/keld-kel64-neutralino-focus-onecall-20260817.json`)
