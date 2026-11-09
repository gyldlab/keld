---
name: "2026-10-24-ipc-inspector-callback-counts-miss-retained"
description: "Inspector callback counts miss retained slots; object markers prove slot release, not heap bytes."
area: "ipc"
tags: ["ipc", "inspector", "callback", "counts", "miss"]
platform: "unspecified"
recorded: "2026-10-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L184"
related: []
---

## Historical source

> - 2026-10-24 [ipc/test] Bun 1.4.2 emits Inspector heap graph v3 even though locked bun-types 1.4.0 comments say v2. Callback counts can stay correct while an append-only read-head retains consumed facts. Own object-valued markers plus Array Index edges distinguish retained slots; plain-object anchors keep facts alive so this does not depend on collection timing. This is a slot observer, not a heap-byte bound. (evidence: GH-1142, `packages/@keld/kipc/test/mirror-retention.ts`, `worker_link::mirror_releases_consumed_fact_array_references`; upstream WebKit `2e2aa229` schema)
