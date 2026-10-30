---
name: "2026-08-14-wv-omitting-wry-permission-handler-grants"
description: "Omitting wry's permission handler grants camera and microphone access without origin context."
area: "wv"
tags: ["wv", "omitting", "wry", "permission", "handler", "macos"]
platform: "macos"
recorded: "2026-08-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L53"
related: []
---

## Historical source

> - 2026-08-14 [wv/macos] wry 0.56 still auto-grants camera/mic when `with_permission_handler` is omitted; the handler is `Fn(PermissionKind)` with no origin, so v0 evaluates requested resource `*`. (evidence: wry-0.56.1 `wry_web_view_ui_delegate.rs`, KEL-59)
