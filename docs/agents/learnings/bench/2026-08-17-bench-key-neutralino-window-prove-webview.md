---
name: "2026-08-17-bench-key-neutralino-window-prove-webview"
description: "A key Neutralino window does not prove WebView focus; fail closed without focused-document evidence."
area: "bench"
tags: ["bench", "key", "neutralino", "window", "prove"]
platform: "unspecified"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L63"
related: ["docs/agents/learnings/bench/2026-08-17-bench-neutralino-macos-document-focus-makefirstresponder.md"]
---

## Historical source

> - 2026-08-17 [bench] A key NSWindow is not proof that its WKWebView has document focus: stock Neutralino 6.9.0 makes the window key but never makes the WebView first responder, so KEL-64 must fail closed instead of fabricating a click or accepting an unfocused beacon. (evidence: neutralinojs v6.9.0 `lib/webview/webview.h`, `/tmp/keld-kel64-neutralino-direct-url-one-20260817.json`)
