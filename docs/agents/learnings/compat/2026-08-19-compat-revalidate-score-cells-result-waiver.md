---
name: "2026-08-19-compat-revalidate-score-cells-result-waiver"
description: "Revalidate score cells, result-waiver pairing, and evidence URIs on hand-built public structs."
area: "compat"
tags: ["compat", "revalidate", "score", "cells", "result"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L84"
related: ["docs/agents/learnings/compat/2026-08-19-compat-branch-based-blob-raw-refs.md"]
---

## Historical source

> - 2026-08-19 [compat] `score` must re-check empty/duplicate cells, result/waiver pairing, and evidence URIs; parse-only gates do not bind hand-built structs, so empty `cells` was `complete` `0/0`, one Pass in a duplicated cell list was 2/2, and Pass+unexpired-waiver was 100%. (evidence: KEL-74, PR #34, `crates/keld-compat/src/evidence.rs`)
