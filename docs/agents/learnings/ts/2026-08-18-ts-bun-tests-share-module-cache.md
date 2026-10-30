---
name: "2026-08-18-ts-bun-tests-share-module-cache"
description: "Bun tests share a module cache; isolate cross-file stubs in a fixture child process."
area: "ts"
tags: ["ts", "bun", "tests", "share", "module"]
platform: "unspecified"
recorded: "2026-08-18"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L77"
related: ["docs/agents/learnings/ts/2026-08-18-ts-bun-emits-unhandledrejection-macrotask-await.md"]
---

## Historical source

> - 2026-08-18 [ts] `bun test` shares a process-wide module cache across files; stubbing `LifecycleLink.connect` in one `*.test.ts` leaks into `link.test.ts`. Spawn a fixture child instead. (evidence: KEL-72 `packages/@keld/electron/src/app.test.ts`, bun 1.3.14)
