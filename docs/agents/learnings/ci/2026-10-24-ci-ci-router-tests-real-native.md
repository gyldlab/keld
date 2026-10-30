---
name: "2026-10-24-ci-ci-router-tests-real-native"
description: "CI router tests need the real Native consumer to retain hygiene without inventing Rust readers."
area: "ci"
tags: ["ci", "router", "tests", "real"]
platform: "unspecified"
recorded: "2026-10-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L182"
related: []
---

## Historical source

> - 2026-10-24 [ci] After native formatter routing moved into the shared CI consumer, the router test's synthetic repo with `consumers=[]` still selected TypeScript but omitted hygiene. Reusing only the production Native consumer with a tracked, bound copy of the real selector restored PR/push hygiene without inventing Rust readers; empty/prose/Rust negatives stayed false and detected stale-reader widening. (evidence: PR #1121, failed exact53af CI run40044174663, independent synthetic-owner controls)
