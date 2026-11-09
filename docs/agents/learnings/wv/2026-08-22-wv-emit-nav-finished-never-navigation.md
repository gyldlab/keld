---
name: "2026-08-22-wv-emit-nav-finished-never-navigation"
description: "Emit nav_finished=never when navigation completion never arrives so hangs leave evidence."
area: "wv"
tags: ["wv", "emit", "nav", "finished", "never"]
platform: "unspecified"
recorded: "2026-08-22"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L111"
related: ["docs/agents/learnings/process/2026-08-22-process-source-grep-cannot-prove-callback.md"]
---

## Historical source

> - 2026-08-22 [wv] `KELD_STARTUP_TRACE=1` must emit `nav_finished=never` when `PageLoadEvent::Finished` never fires, or hung navigations leave no stderr evidence. (evidence: KEL-62, CodeRabbit #10, `crates/keld-wv/src/startup.rs` `should_emit_nav_never_finished`)
