---
name: "2026-10-27-ipc-source-failures-replace-earlier-replies"
description: "Source failures must not replace earlier replies; old acknowledgments advance watermarks without clearing newer state."
area: "ipc"
tags: ["ipc", "source", "failures", "replace", "earlier"]
platform: "unspecified"
recorded: "2026-10-27"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L189"
related: ["docs/agents/learnings/ipc/2026-10-27-ipc-link-close-separate-call-admission.md"]
---

## Historical source

> - 2026-10-27 [ipc/replies] A later Source failure can precede an earlier ReplyApplied Promise continuation. Receipt bookkeeping uses nonthrowing Source-owned eligibility so it cannot replace that answer; a live old-sequence ack still advances the dispatched watermark without clearing newer pending state. (evidence: GH1149 candidate `0f7916a8`, `worker_link::source_earlier_reply_applied_survives_later_bad_fact_in_fenced_drain`; throwing-get and old-ack negative controls fail)
