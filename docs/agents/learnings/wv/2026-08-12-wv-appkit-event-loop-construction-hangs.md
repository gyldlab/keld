---
name: "2026-08-12-wv-appkit-event-loop-construction-hangs"
description: "AppKit event-loop construction hangs unit tests; smoke-test markup and use just hello for GUI checks."
area: "wv"
tags: ["wv", "appkit", "event", "loop", "construction", "macos"]
platform: "macos"
recorded: "2026-08-12"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L32"
related: ["docs/agents/learnings/process/2026-08-13-process-hello-size-rss-evidence-tracked.md"]
---

## Historical source

> - 2026-08-12 [wv/macos] Unit tests must not call `WkWebViewEngine::new` / tao `EventLoop::new` (starts AppKit). Smoke title/HTML/spec instead; `just hello` is the GUI check. (evidence: KEL-26)
