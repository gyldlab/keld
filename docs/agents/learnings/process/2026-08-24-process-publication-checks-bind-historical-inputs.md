---
name: "2026-08-24-process-publication-checks-bind-historical-inputs"
description: "Publication checks must bind historical inputs to immutable commits, blobs, and hashes."
area: "process"
tags: ["process", "publication", "checks", "bind", "historical"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L120"
related: []
---

## Historical source

> - 2026-08-24 [process] A recompute-from-raw publication check must bind historical inputs to immutable commits, blobs, and hashes; reading mutable raw plus mutable tables can pass self-consistently after both are changed. (evidence: gyldlab/keld-benches PR #10 `Check-PublishedMeasurements.ps1` negative control)
