---
name: "2026-08-19-ci-dynamic-nextest-routing-pass-empty"
description: "Dynamic nextest routing must pass empty valid packages while retaining tests for consuming packages."
area: "ci"
tags: ["ci", "dynamic", "nextest", "routing", "pass", "macos"]
platform: "macos"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L91"
related: []
---

## Historical source

> - 2026-08-19 [ci] `cargo nextest run -p <selected-package>` exits 4 for a valid zero-test package while the workspace suite does not; dynamic package routing must set `--no-tests=pass` and retain consumer-package tests, not drop empty packages or weaken test failures. (evidence: KEL-81 macOS CI job 95978373698, `cargo nextest run --help`)
