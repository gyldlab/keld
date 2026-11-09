---
name: "2026-08-15-wv-synchronous-webview2-script-injection-blocks"
description: "Synchronous WebView2 script injection blocks the UI thread; future injection should use async completion."
area: "wv"
tags: ["wv", "synchronous", "webview2", "script", "injection", "windows"]
platform: "windows"
recorded: "2026-08-15"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L57"
related: ["docs/agents/learnings/cli/2026-08-15-cli-cross-platform-callee-made-macos.md"]
---

## Historical source

> - 2026-08-15 [wv/windows] wry 0.56.1 `attach_ipc_handler` blocks the UI thread 96-109 ms injecting `window.ipc` via a synchronous `AddScriptToExecuteOnDocumentCreated` wait even when `ipc_handler` is `None`; WebView2 environment creation is only 3-6 ms (runtime resolution — the browser process launches at controller creation, which is the 313-506 ms floor). Any future Keld script injection must use the async completion-handler form. (evidence: KEL-65 attribution, competitors/wry/src/webview2/mod.rs:965)
