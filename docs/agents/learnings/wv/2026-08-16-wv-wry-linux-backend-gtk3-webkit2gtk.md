---
name: "2026-08-16-wv-wry-linux-backend-gtk3-webkit2gtk"
description: "wry's Linux backend uses GTK3 and WebKit2GTK 4.1; pass explicit environment to exact-self exec."
area: "wv"
tags: ["wv", "wry", "linux", "backend", "gtk3"]
platform: "linux"
recorded: "2026-08-16"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L60"
related: ["docs/agents/learnings/process/2026-08-16-process-wsl-version-prove-compositor-exists.md"]
---

## Historical source

> - 2026-08-16 [wv/linux] wry 0.56.1's Linux backend is GTK3 + WebKit2GTK 4.1 (`gtk` 0.18, `webkit2gtk` 2.0.2), not GTK4/`webkitgtk-6.0` — architecture 05's "webkit6/gtk4" phrasing describes the *destination* direct-bindings rewrite, not what wry-interim actually links. apt packages to build it: `libgtk-3-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev pkg-config build-essential`. KEL-132 removed live environment mutation: process entry passes explicit `argv`/`envp` to exact-self `execve`, and engine construction fails closed if unprepared. clippy `doc_markdown` flags bare "WebKitGTK" in doc comments; backtick it. (evidence: KEL-28, KEL-132, crates/keld-wv/Cargo.toml, crates/keld-wv/src/webkitgtk/mod.rs)
