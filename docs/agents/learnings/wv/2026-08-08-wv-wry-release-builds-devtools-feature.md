---
name: "2026-08-08-wv-wry-release-builds-devtools-feature"
description: "wry release builds need the devtools feature because devtools methods are cfg-gated."
area: "wv"
tags: ["wv", "wry", "release", "builds", "devtools"]
platform: "unspecified"
recorded: "2026-08-08"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L31"
related: []
---

## Historical source

> - 2026-08-08 [wv] wry compiles open_devtools/close_devtools only under cfg(any(debug_assertions, feature="devtools")) — keld-wv pins the `devtools` feature or release builds fail. (evidence: competitors/wry/src/lib.rs `pub fn open_devtools`, crates/keld-wv/Cargo.toml)
