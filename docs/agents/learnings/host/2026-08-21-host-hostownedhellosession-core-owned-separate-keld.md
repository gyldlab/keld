---
name: "2026-08-21-host-hostownedhellosession-core-owned-separate-keld"
description: "HostOwnedHelloSession is core-owned; a separate keld-host process remains required for process ownership."
area: "host"
tags: ["host", "hostownedhellosession", "core", "owned", "separate"]
platform: "unspecified"
recorded: "2026-08-21"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L108"
related: ["docs/agents/learnings/ci/2026-08-21-ci-gui-dependency-discovery-follow-normal.md", "docs/agents/learnings/docs/2026-08-27-docs-powershell-get-content-hash-decoded.md"]
---

## Historical source

> - 2026-08-21 [host] KEL-30 “host-owned” hello echo is crate ownership (`keld-core::HostOwnedHelloSession`); `keld dev` still runs that session in the CLI process — Unique #1 needs no-flag `keld-host` process ownership, gated on approved research-83 + kel96 spec. (evidence: KEL-96, research `83-host-vs-cli-ownership.md` @ `5118d6b`, `crates/keld-cli/src/dev.rs`)
