---
name: "2026-08-21-ci-gui-dependency-discovery-follow-normal"
description: "GUI dependency discovery must follow normal Cargo edges, not dev or build dependencies."
area: "ci"
tags: ["ci", "gui", "dependency", "discovery", "follow"]
platform: "unspecified"
recorded: "2026-08-21"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L106"
related: ["docs/agents/learnings/host/2026-08-21-host-hostownedhellosession-core-owned-separate-keld.md", "docs/agents/learnings/ipc/2026-08-17-ipc-second-echo-call-repeats-hello.md"]
---

## Historical source

> - 2026-08-21 [ci] Adding `keld-runtime` under `keld-core` puts runtime in the host dependency closure (`gui=true` for runtime-only paths); host-dirs must walk only normal cargo deps (`kind == null`), not `dev`/`build`, or runtime's test-only `keld-compat` edge falsely enables GUI smoke. (evidence: tools/ci_changes.sh `host_dependency_dirs`, KEL-30, PR #48)
