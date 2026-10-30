---
name: "2026-08-13-pack-bun-gzip-exceeds-installer-budget"
description: "Bun gzip exceeds the installer budget; zstd fits the diagnostic, while the DMG does not."
area: "pack"
tags: ["pack", "bun", "gzip", "exceeds", "installer"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L49"
related: []
---

## Historical source

> - 2026-08-13 [pack] bun 1.3.14 this-Mac is 63,096,576 B; gzip -9 = 23,548,666 (over the ≤20 MB bun-installer budget before host); zstd -19 = 16,838,595. UDZO/zlib DMG of host+Bun cannot hit architecture 01 §5; zstd can. (evidence: `stat`/`gzip`/`zstd` on `$(command -v bun)`, budget-scoreboard Win conditions)
