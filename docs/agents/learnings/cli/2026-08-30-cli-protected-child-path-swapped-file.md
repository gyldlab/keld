---
name: "2026-08-30-cli-protected-child-path-swapped-file"
description: "A protected child path can be swapped through FILE_DELETE_CHILD; hold directory handles and recheck ACL and digest."
area: "cli"
tags: ["cli", "protected", "child", "path", "swapped", "windows"]
platform: "windows"
recorded: "2026-08-30"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L150"
related: ["docs/agents/learnings/runtime/2026-08-29-runtime-timed-fixed-size-reads-retain.md"]
---

## Historical source

> - 2026-08-30 [cli/windows] A protected child DACL does not pin its launch pathname when a parent grants `FILE_DELETE_CHILD`: another principal can rename the child and recreate attacker bytes before path-based `CreateProcess`. Retain directory handles without `FILE_SHARE_DELETE` across every accepted stage component through host exit, then recheck the locked DACL and executable digest. (evidence: KEL-96/T4 hostile parent replacement probe; `windows_stage_namespace_is_pinned_until_the_host_owner_releases_it`)
