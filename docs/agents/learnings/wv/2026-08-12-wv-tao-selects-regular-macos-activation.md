---
name: "2026-08-12-wv-tao-selects-regular-macos-activation"
description: "tao already selects regular macOS activation; avoid setting the policy twice."
area: "wv"
tags: ["wv", "tao", "selects", "regular", "macos"]
platform: "macos"
recorded: "2026-08-12"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L33"
related: []
---

## Historical source

> - 2026-08-12 [wv/macos] tao 0.35 `EventLoopExtMacOS` already defaults to `NSApplicationActivationPolicyRegular`; do not set it again. (evidence: tao-0.35.3 `platform/macos.rs` trait docs)
