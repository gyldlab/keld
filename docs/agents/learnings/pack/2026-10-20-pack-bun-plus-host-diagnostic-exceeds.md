---
name: "2026-10-20-pack-bun-plus-host-diagnostic-exceeds"
description: "The Bun-plus-host diagnostic exceeds installer budget; 1.3.14-only zstd results are historical."
area: "pack"
tags: ["pack", "bun", "plus", "host", "diagnostic"]
platform: "unspecified"
recorded: "2026-10-20"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L181"
related: ["docs/agents/learnings/pack/2026-10-20-pack-code-signatures-preserve-link-targets.md"]
---

## Historical source

> - 2026-10-20 [pack] Bun 1.4.2 arm64 (61,884,464 B) alone is 19,106,102 B at `zstd -19`; with the 12,648,784 B `keld-host` a `tar | zstd -19` diagnostic of the KEL-137 fixture tree is 21,176,773 B, already above the 20,000,000 B installer budget (architecture 01 §5.1); no shipped `.dmg` or other delivered format was measured. Supersedes the 2026-08-13 entry's "zstd can" conclusion for the current Bun pin; its 1.3.14 Bun-only figures stand as history. (evidence: `tar --no-xattrs --no-mac-metadata | zstd -19` over the fixture, zstd 1.5.7, 2026-10-20; Linear KEL-129 comment `ad1f7903`, KEL-141 `130b40c2`)
