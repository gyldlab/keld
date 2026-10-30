---
name: "2026-08-21-guard-claim-privileged-operations-developer-permissions"
description: "Do not claim privileged operations or developer permissions before their call sites exist."
area: "guard"
tags: ["guard", "claim", "privileged", "operations", "developer"]
platform: "unspecified"
recorded: "2026-08-21"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L107"
related: []
---

## Historical source

> - 2026-08-21 [guard] Crate/lib + arch/03 must not claim every privileged op / live dev-permissive+recorder until those call sites exist; v0 live evaluate is media (empty default manifest) + host fs via dispatch_privileged; keld-core depends but does not call evaluate. (evidence: KEL-93, crates/keld-guard/src/lib.rs, docs/architecture/03-security.md §3)
