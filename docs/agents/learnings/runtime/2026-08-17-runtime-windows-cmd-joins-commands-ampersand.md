---
name: "2026-08-17-runtime-windows-cmd-joins-commands-ampersand"
description: "Windows cmd joins commands with ampersand, while Unix uses semicolons; build scripts per OS."
area: "runtime"
tags: ["runtime", "windows", "cmd", "joins", "commands"]
platform: "windows"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L67"
related: ["docs/agents/learnings/runtime/2026-08-24-runtime-recovered-crashes-remain-successful-crash.md"]
---

## Historical source

> - 2026-08-17 [runtime] Windows `cmd /C "step1; step2"` does not treat `;` as a statement separator — cmd only splits on `&`; a `;`-joined test script silently runs as one `echo` argument and never reaches `exit N`, so a crash-loop test built from `sh -c`/`cmd /C` pairs needs an OS-aware join (`; ` on Unix, ` & ` on Windows), not one script string for both. (evidence: KEL-70, `crates/keld-runtime/src/lib.rs` `joined_steps`, first `crash_loop_breaker_trips_after_max_crashes` run reporting `Stopped` instead of `CrashLoop` on Windows)
