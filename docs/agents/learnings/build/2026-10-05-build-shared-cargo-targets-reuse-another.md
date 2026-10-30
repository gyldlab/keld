---
name: "2026-10-05-build-shared-cargo-targets-reuse-another"
description: "Shared Cargo targets can reuse another worktree's build; isolate targets and verify changed crates rebuild."
area: "build"
tags: ["build", "shared", "cargo", "targets", "reuse"]
platform: "unspecified"
recorded: "2026-10-05"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L170"
related: []
---

## Historical source

> - 2026-10-05 [build] A `CARGO_TARGET_DIR` shared by git worktrees let `just ci` test another worktree's build (false green): Cargo fingerprints are workspace-relative and mtime-based, so a newer build from another worktree is reused and this tree's older-dated or new files are not compiled. Use a per-worktree target; require `Checking`/`Compiling <crate> v… (<this worktree path>)` for each crate changed since that target's last build plus the expected test-count delta. (evidence: PR #383 Tests; rule: docs/specs/kel245-local-agent-workspace.md:242)
