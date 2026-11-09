---
name: "2026-08-24-runtime-recovered-crashes-remain-successful-crash"
description: "Recovered crashes remain successful when the crash count at the ready marker satisfies the contract."
area: "runtime"
tags: ["runtime", "recovered", "crashes", "remain", "successful"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L124"
related: ["docs/agents/learnings/runtime/2026-08-17-runtime-windows-cmd-joins-commands-ampersand.md", "docs/agents/learnings/runtime/2026-08-24-runtime-crash-counts-race-stdout-readiness.md", "docs/agents/learnings/runtime/2026-08-24-runtime-crash-loop-threshold-hide-dead.md"]
---

## Historical source

> - 2026-08-24 [runtime] "Any crash fails the run" is not a valid reading of a dead app: KEL-70 AC1/AC3 requires a crash the supervisor recovered from to stay a success, so the discriminator is the crash count at the ready marker, not the crash itself. (evidence: KEL-105, `crates/keld-cli/tests/cli_kel29.rs` `run_dev_echo_recovers_from_one_real_bun_crash`)
