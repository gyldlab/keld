---
name: "2026-10-27-ipc-link-close-separate-call-admission"
description: "Link.close must separate call-admission shutdown from Source disposal after deferred dispatch drains."
area: "ipc"
tags: ["ipc", "link", "close", "separate", "call"]
platform: "unspecified"
recorded: "2026-10-27"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L188"
related: ["docs/agents/learnings/ipc/2026-10-27-ipc-source-failures-replace-earlier-replies.md"]
---

## Historical source

> - 2026-10-27 [ipc/lifecycle] Clearing WindowSource in explicit Link.close loses already-applied facts whose listener dispatch is deferred after a blocking return. Closing call admission and after-drain Source disposal are separate steps. (evidence: GH1149 candidate `0f7916a8`, `worker_link::source_close_before_deferred_dispatch_drains_before_end`; restored eager-end negative control fails)
