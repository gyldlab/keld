---
name: "2026-08-19-ci-job-level-matrix-os-condition"
description: "A job-level matrix.os condition can invalidate the entire workflow before compiler and secret scans start."
area: "ci"
tags: ["ci", "job", "level", "matrix", "os"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L92"
related: []
---

## Historical source

> - 2026-08-19 [ci] A job-level `matrix.os` condition invalidates the whole workflow, so rustc and gitleaks never start (rule: `.agents/ci.md`). (evidence: PR #36 run 32244879790)
