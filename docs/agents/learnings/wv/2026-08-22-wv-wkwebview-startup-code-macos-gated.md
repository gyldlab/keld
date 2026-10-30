---
name: "2026-08-22-wv-wkwebview-startup-code-macos-gated"
description: "WKWebView startup code must be macOS-gated to avoid dead-code failures on other targets."
area: "wv"
tags: ["wv", "wkwebview", "startup", "code", "macos", "multiple"]
platform: "unspecified"
recorded: "2026-08-22"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L109"
related: ["docs/agents/learnings/process/2026-08-22-process-source-grep-cannot-prove-callback.md", "docs/agents/learnings/wv/2026-08-14-wv-webview2-bounds-attach-measure-child.md"]
---

## Historical source

> - 2026-08-22 [wv] `startup` is WKWebView-only; gate `mod startup` to `#[cfg(target_os = "macos")]` or Linux/Windows CI promotes `dead_code` to errors. (evidence: KEL-62, CodeRabbit #10, `crates/keld-wv/src/lib.rs`)
