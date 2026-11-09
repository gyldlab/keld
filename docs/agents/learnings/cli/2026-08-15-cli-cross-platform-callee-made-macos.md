---
name: "2026-08-15-cli-cross-platform-callee-made-macos"
description: "A cross-platform callee made the macOS-only caller gate stale; check callee dispatch before adding cfg."
area: "cli"
tags: ["cli", "cross", "platform", "callee", "made", "multiple"]
platform: "unspecified"
recorded: "2026-08-15"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L59"
related: ["docs/agents/learnings/cli/2026-08-13-cli-reject-closed-keld-create-flags.md", "docs/agents/learnings/wv/2026-08-15-wv-synchronous-webview2-script-injection-blocks.md"]
---

## Historical source

> - 2026-08-15 [cli] `keld dev`'s window step was gated `#[cfg(target_os = "macos")]` in `dev.rs` even though `keld_core::run_hello_window_html` (the function it would have called) was already cross-platform and dispatches per-OS internally (`keld_wv::hello::run`) — pure staleness left over from before the Windows WebView2 backend went live (KEL-27, then KEL-65). Before adding a platform cfg gate around a call, check whether the callee already handles the platform split; a redundant outer gate silently rots when a new backend lands. (evidence: KEL-29, `crates/keld-cli/src/dev.rs` `open_dev_window`, `crates/keld-core/src/hello.rs`)
