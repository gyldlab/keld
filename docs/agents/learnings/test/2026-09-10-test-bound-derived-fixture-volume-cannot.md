---
name: "2026-09-10-test-bound-derived-fixture-volume-cannot"
description: "A bound derived from fixture volume cannot catch its own growth; scale the tested constant independently."
area: "test"
tags: ["test", "bound", "derived", "fixture", "volume"]
platform: "unspecified"
recorded: "2026-09-10"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L162"
related: ["docs/agents/learnings/test/2026-09-10-test-mutation-tests-out-tree-clone.md"]
---

## Historical source

> - 2026-09-10 [test] An assertion whose bound and whose fixture volume both derive from one constant cannot see that constant move: doubling Keld's three `CAPTURE_*` retention constants (ceiling 327,680 -> 655,360 B) passes 680/680 while the soak prints `peak_retained=654352`. Such a test pins the mechanism, never the value — deleting the mechanism is not a sufficient negative control, also scale the constant. (evidence: KEL-225, `crates/keld-runtime/src/lib.rs:270-296`)
