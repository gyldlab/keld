---
name: "2026-09-10-shell-inherited-shell-descriptors-distort-child"
description: "Inherited shell descriptors distort child lsof counts; close them at invocation before attribution."
area: "shell"
tags: ["shell", "inherited", "descriptors", "distort"]
platform: "unspecified"
recorded: "2026-09-10"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L163"
related: []
---

## Historical source

> - 2026-09-10 [shell] The Cursor-managed shell holds Unix sockets on fds 3 and 4 with `FD_CLOEXEC=False` and children under test inherit them, so an `lsof` oracle counting all Unix descriptors on a pid charges the parent's sockets to the child. `3>&- 4>&-` on the invocation proves provenance; it is not the fix. Cost three sessions before root cause. (evidence: KEL-222, which owns the repair)
