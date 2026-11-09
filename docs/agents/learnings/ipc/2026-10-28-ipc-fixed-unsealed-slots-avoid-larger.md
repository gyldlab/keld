---
name: "2026-10-28-ipc-fixed-unsealed-slots-avoid-larger"
description: "Fixed unsealed slots avoid larger sparse-map backing under the pinned Bun profile, not whole-app RSS."
area: "ipc"
tags: ["ipc", "fixed", "unsealed", "slots", "avoid", "macos"]
platform: "macos"
recorded: "2026-10-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L191"
related: ["docs/agents/learnings/ipc/2026-10-28-ipc-captured-native-messages-existing-private.md", "docs/agents/learnings/runtime/2026-10-28-runtime-windows-short-path-aliases-evaluate.md"]
---

## Historical source

> - 2026-10-28 [ipc] On the qualified Bun 1.4.4 macOS arm64 profile, repeated native shift/refill retained a larger backing at unchanged maximum array length; sealing fixed slots instead converted the dense array to a larger sparse map. The private unsealed null-prototype fixed-slot carrier kept the same 163840-byte usable backing across 358528 puts/takes, including a preinstalled inherited numeric setter. These are pinned representation observations, not whole-app/RSS bounds. (evidence: GH-1149 acceptance comment 6450886868; packages/@keld/kipc/src/transport.ts stateSlots)
