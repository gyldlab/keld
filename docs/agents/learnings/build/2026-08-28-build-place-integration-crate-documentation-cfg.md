---
name: "2026-08-28-build-place-integration-crate-documentation-cfg"
description: "Place integration crate documentation before cfg attributes so non-target CI retains required docs."
area: "build"
tags: ["build", "place", "integration", "crate", "documentation", "windows"]
platform: "windows"
recorded: "2026-08-28"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L142"
related: ["docs/agents/learnings/runtime/2026-08-28-runtime-proc-setpcontrol-parent-death-detection.md", "docs/agents/learnings/runtime/2026-08-29-runtime-macos-pipes-inherit-descriptors-unless.md"]
---

## Historical source

> - 2026-08-28 [build] Put integration-crate `//!` docs before a crate-level platform `#![cfg(...)]`; on non-target CI the cfg removes following doc attributes and `-D missing-docs` fails the intentionally empty crate. (evidence: KEL-78 PR #108 Windows CI; standalone rustc doc-before-cfg negative control)
