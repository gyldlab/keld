---
name: "2026-08-23-runtime-lifecycle-drain-revoke-virtual-ports"
description: "Lifecycle drain must revoke virtual ports and retain Revoked events through the public feed."
area: "runtime"
tags: ["runtime", "lifecycle", "drain", "revoke", "virtual"]
platform: "unspecified"
recorded: "2026-08-23"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L113"
related: ["docs/agents/learnings/runtime/2026-08-19-runtime-latch-admission-completion-failure-deadline.md", "docs/agents/learnings/runtime/2026-08-23-runtime-test-lifecycle-events-registry-feed.md"]
---

## Historical source

> - 2026-08-23 [runtime] A host lifecycle drain must both revoke virtual-port state and retain the `Revoked` event for the public role feed; exposing the mutable port table lets route operations bypass that single synchronization boundary. (evidence: KEL-75 CodeRabbit CLI review, `crates/keld-runtime/src/registry.rs` `role_port_operations_sync_revocation_and_preserve_lifecycle_event`)
