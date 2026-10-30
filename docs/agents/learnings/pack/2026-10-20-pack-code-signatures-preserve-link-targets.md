---
name: "2026-10-20-pack-code-signatures-preserve-link-targets"
description: "Code signatures preserve link targets but not executable mode; package rules must enforce executability."
area: "pack"
tags: ["pack", "code", "signatures", "preserve", "link", "macos"]
platform: "macos"
recorded: "2026-10-20"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L180"
related: ["docs/agents/learnings/pack/2026-10-20-pack-bun-plus-host-diagnostic-exceeds.md"]
---

## Historical source

> - 2026-10-20 [pack] `codesign --verify --deep --strict` seals file content and symlink *targets* (`files2[...].symlink`) but accepts POSIX mode changes: `chmod 0644` on the signed main executable or on the nested Bun still verifies (the host is then no longer `test -x`; Bun exec fails 126), while re-pointing, externalizing, dangling or file-substituting the link fails verification. Keld's package rule, not the seal, must own executability. Observed with the `codesign` CLI on macOS 26.5.1 arm64; Apple documents the symlink coverage (TN2206), not the mode behaviour. (evidence: KEL-137 fixture mutation run 2026-10-20, Linear KEL-137 comment `4a9b3cd1`; raw log `docs/handoff/evidence/mvp-20261006-p29/oracle-mutations-log.txt` in 0monish/prompt-tracker)
