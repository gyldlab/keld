---
name: "2026-08-24-bench-stable-rss-sample-omit-engine"
description: "A stable RSS sample can omit engine processes; require stable process classes and a nonzero engine floor."
area: "bench"
tags: ["bench", "stable", "rss", "sample", "omit", "windows"]
platform: "windows"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L133"
related: []
---

## Historical source

> - 2026-08-24 [bench/windows] A process-tree RSS sample can be byte-STABLE while the tree is still INCOMPLETE: one run in thirty settled after the titled `keld dev` window was already visible but before any `msedgewebview2` child existed, giving a 55,336 KiB "stable" tree with zero engine processes against a ~372,000 KiB median. A bounded-drift rule alone accepts it. Require an identical process-class census across the drift window plus a non-zero engine floor; adding that cut host-lane stdev 986 -> 69 KiB and engine-lane stdev 59,139 -> 3,376 KiB, so churn had been contaminating many samples, not one. `schema/result.v1.schema.json` already lists "membership churn" as a reject reason. (evidence: keld-benches `windows/bench/Measure-KeldIdleRss.ps1` Wait-BoundedDrift + its negative controls, PR gyldlab/keld-benches#12)
