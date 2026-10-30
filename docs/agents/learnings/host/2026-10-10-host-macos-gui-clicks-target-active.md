---
name: "2026-10-10-host-macos-gui-clicks-target-active"
description: "macOS GUI clicks must target the active topmost host window; nextest serialization is process-local."
area: "host"
tags: ["host", "macos", "gui", "clicks", "target"]
platform: "macos"
recorded: "2026-10-10"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L175"
related: ["docs/agents/learnings/test/2026-10-10-test-swift-observers-main-run-loop.md"]
---

## Historical source

> - 2026-10-10 [host/macos] The `macos-no-flag-gui` nextest group serialises GUI rows only inside one nextest process. A second worktree's concurrent run can activate a keld-host window over the same default frame, and wry's default `acceptsFirstMouse:` NO spends a click on an inactive host window on activation, so a real-pointer row must click only once the host is the active app and its window is the topmost layer-0 hit target. (evidence: GH-652; unfixed row 2/20 failed beside a concurrent second nextest, 10/10 under a cover hook)
