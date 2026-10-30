---
name: "2026-08-18-compat-bun-quit-fixture-close-kipc"
description: "A Bun Quit fixture must close kipc and exit; otherwise the host remains waiting on the socket."
area: "compat"
tags: ["compat", "bun", "quit", "fixture", "close"]
platform: "unspecified"
recorded: "2026-08-18"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L75"
related: ["docs/agents/learnings/compat/2026-08-18-compat-drain-queued-reader-output-asserting.md", "docs/agents/learnings/ts/2026-08-18-ts-bun-emits-unhandledrejection-macrotask-await.md"]
---

## Historical source

> - 2026-08-18 [compat] A Bun fixture that `await`s `app.quit()` but keeps a kipc read loop open will not exit; the host `wait()` hangs. Close the socket and `process.exit(0)` after the Quit reply. (evidence: KEL-72, `crates/keld-compat/tests/electron_lifecycle.rs`)
