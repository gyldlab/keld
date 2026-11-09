---
name: "2026-08-25-bench-windows-working-totals-vary-session"
description: "Windows working-set totals vary by session; score the gated process and use private bytes for reproducibility."
area: "bench"
tags: ["bench", "windows", "working", "totals", "vary"]
platform: "windows"
recorded: "2026-08-25"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L131"
related: []
---

## Historical source

> - 2026-08-25 [bench] A Windows working set is not a property of the program: the SAME sha-verified keld.exe running the SAME unchanged fixture under the SAME 30-round protocol on the SAME machine on AC power measured its Bun child at 50,828 KiB one day and 22,968 KiB the next (2.22x), while `runtime_private_kib` moved 0.1% (89,016 vs 88,916) and the within-session spread stayed under 0.2%. Ruled out: binary, fixture, canonical payload (22,972 with vs 22,896 without), and capture time (the child's working set is flat to 0.3% over 20 s). The scored host values reproduced at 1.00x and the paired headline replicated 0.848386 -> 0.847364, so score the process you gate on and treat any working-set SUM as session-local; private bytes is the reproducible counter for the Bun child. (evidence: KEL-25, gyldlab/keld-benches@b30d145 vs a matched 2026-08-26 repeat)
