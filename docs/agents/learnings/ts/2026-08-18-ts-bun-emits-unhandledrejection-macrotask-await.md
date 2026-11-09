---
name: "2026-08-18-ts-bun-emits-unhandledrejection-macrotask-await"
description: "Bun emits unhandledRejection on a macrotask; await setImmediate before checking the count."
area: "ts"
tags: ["ts", "bun", "emits", "unhandledrejection", "macrotask"]
platform: "unspecified"
recorded: "2026-08-18"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L76"
related: ["docs/agents/learnings/compat/2026-08-18-compat-bun-quit-fixture-close-kipc.md", "docs/agents/learnings/ts/2026-08-18-ts-bun-tests-share-module-cache.md"]
---

## Historical source

> - 2026-08-18 [ts] Bun 1.3.14 delivers `unhandledRejection` on a macrotask; two `await Promise.resolve()` ticks still report COUNT=0. Await `setImmediate` before reading the count. (evidence: KEL-72 `packages/@keld/electron/fixtures/app_ready.ts`)
