---
name: "2026-09-10-test-soak-peak-sampler-interval-count"
description: "A soak needs peak, sampler, interval, and count; validate the sampler against known unbounded growth."
area: "test"
tags: ["test", "soak", "peak", "sampler", "interval"]
platform: "unspecified"
recorded: "2026-09-10"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L164"
related: []
---

## Historical source

> - 2026-09-10 [test] A soak reporting only pass/fail proves nothing about boundedness: a `real:<host>` row needs the measured peak, the sampling tool, the interval and the sample count, and the sampler must first be validated against a known allocation to show it can observe unbounded growth at all. (evidence: KEL-134 comment `749c3aa6`, `continuous_output_soak_keeps_memory_bounded`)
