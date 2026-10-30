---
name: "2026-08-13-cli-tao-eventloop-run-never-returns"
description: "tao EventLoop::run never returns; use run_return so window shutdown can reap the Bun child."
area: "cli"
tags: ["cli", "tao", "eventloop", "run", "never"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L35"
related: ["docs/agents/learnings/bridge/2026-08-15-bridge-bun-unix-socket-writes-partial.md"]
---

## Historical source

> - 2026-08-13 [cli] tao `EventLoop::run` never returns (process exit); hello backends use `run_return` so Drop/reap can run after the last window closes — `keld dev` keeps the supervised Bun echo child alive across the hello window via that path (KEL-30). (evidence: crates/keld-wv/src/wkwebview/mod.rs `run_until_closed`, crates/keld-core/src/hello_session.rs, KEL-30)
