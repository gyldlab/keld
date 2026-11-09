---
name: "2026-08-28-runtime-latest-exit-status-hide-earlier"
description: "Latest exit status can hide earlier crashes; retain termination history and classify before revocation."
area: "runtime"
tags: ["runtime", "latest", "exit", "status", "hide"]
platform: "unspecified"
recorded: "2026-08-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L140"
related: []
---

## Historical source

> - 2026-08-28 [runtime] A latest-only termination record lets a final exit 0 hide an earlier post-ready crash, while classifying after lease revocation can mislabel a host-caused EOF exit; retain separate crash/all-termination views and classify before revocation. (evidence: KEL-116 isolated reviews, `crates/keld-runtime/src/lib.rs`, `crates/keld-core/src/hello_session.rs`)
