---
name: "2026-08-18-wv-media-authorization-belongs-minted-webviewid"
description: "Media authorization belongs to a minted WebviewId; missing identity must fail closed."
area: "wv"
tags: ["wv", "media", "authorization", "belongs", "minted"]
platform: "unspecified"
recorded: "2026-08-18"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L79"
related: ["docs/agents/learnings/compat/2026-08-18-compat-bound-drain-wait-initial-write.md"]
---

## Historical source

> - 2026-08-18 [wv] Camera/mic must not evaluate as AppProcess; a minted WebviewId is the principal, and missing identity is KELD-GUARD007 fail-closed. (evidence: CodeRabbit on PR #27, KEL-73, crates/keld-wv/src/media.rs)
