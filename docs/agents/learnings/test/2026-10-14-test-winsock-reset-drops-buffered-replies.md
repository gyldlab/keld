---
name: "2026-10-14-test-winsock-reset-drops-buffered-replies"
description: "Winsock reset drops buffered replies; read peer output before OS census and sample only guaranteed windows."
area: "test"
tags: ["test", "winsock", "reset", "drops", "buffered", "windows"]
platform: "windows"
recorded: "2026-10-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L178"
related: ["docs/agents/learnings/test/2026-10-14-test-failed-host-fixtures-leak-inherited.md"]
---

## Historical source

> - 2026-10-14 [test/windows] Winsock discards a stream's already-buffered bytes when the peer's reset arrives, so a test that lets ~300 ms pass (one PowerShell census) between a control line and the peer's last lines reads 10054 instead of `QUIT_REPLY`/`LINK_EOF` once the host reaps the exited role; Linux would still deliver them. Read every line the peer sends before any out-of-band observation, and take OS censuses only inside a window the product guarantees (h12's D wait), never between two control lines. (evidence: KEL-270 S6c2a, `crates/keld-host/tests/no_flag/windows/profiles/handover.rs`, evidence runs new-second-h11 and flake-idle-h11)
