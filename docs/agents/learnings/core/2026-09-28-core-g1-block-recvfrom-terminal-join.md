---
name: "2026-09-28-core-g1-block-recvfrom-terminal-join"
description: "G1 can block in recvfrom at terminal join; stop each generation before link close."
area: "core"
tags: ["core", "g1", "block", "recvfrom", "terminal", "macos"]
platform: "macos"
recorded: "2026-09-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L169"
related: []
---

## Historical source

> - 2026-09-28 [core/macos] G1 reader with live peer stayed in recvfrom at terminal join. Per-generation stop before link close preserved G2; bounded child exposed failures. (GitHub #336)
