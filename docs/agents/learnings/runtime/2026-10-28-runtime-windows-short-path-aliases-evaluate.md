---
name: "2026-10-28-runtime-windows-short-path-aliases-evaluate"
description: "Windows short-path aliases can evaluate sidecars twice; realpath the stage before deriving preload and entry paths."
area: "runtime"
tags: ["runtime", "windows", "short", "path", "aliases"]
platform: "windows"
recorded: "2026-10-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L190"
related: ["docs/agents/learnings/ipc/2026-10-24-ipc-mimalloc-rounds-huge-allocations-os.md", "docs/agents/learnings/ipc/2026-10-28-ipc-captured-native-messages-existing-private.md", "docs/agents/learnings/ipc/2026-10-28-ipc-fixed-unsealed-slots-avoid-larger.md"]
---

## Historical source

> - 2026-10-28 [runtime/windows] Bun 1.4.4 can evaluate one physical sidecar twice when CLI preload uses an 8.3 temporary-directory alias and entry imports use its long path; the first namespace captured native operations and the second captured later app replacements. Native realpath normalization of the stage before deriving cwd/entry/preload paths passed the actual Windows runner while the omitted-preload control still failed. (evidence: GH-1149, PR #1181, hosted Windows job 122413162037; packages/@keld/api/src/window-state-phase.test.ts)
