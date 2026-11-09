---
name: "2026-08-29-runtime-guardian-poll-supervisor-events-beside"
description: "A guardian must poll supervisor events beside host-liveness EOF so child self-termination stays visible."
area: "runtime"
tags: ["runtime", "guardian", "poll", "supervisor", "events"]
platform: "unspecified"
recorded: "2026-08-29"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L144"
related: ["docs/agents/learnings/core/2026-08-29-core-blocking-stdin-lease-deadlock-quit.md", "docs/agents/learnings/docs/2026-08-27-docs-powershell-get-content-hash-decoded.md"]
---

## Historical source

> - 2026-08-29 [runtime] A guardian that blocks on host-liveness EOF before observing `Supervisor` terminal events hides Bun self-termination while the host is still live; poll the validated liveness descriptor nonblocking beside supervisor events so either owner can end the session. (evidence: `crates/keld-runtime/src/macos_guardian.rs`; KEL-96 status-zero live-writer test)
