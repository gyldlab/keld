---
name: "2026-08-24-runtime-crash-loop-threshold-hide-dead"
description: "A crash-loop threshold can hide one dead app; read durable crash state rather than infer success."
area: "runtime"
tags: ["runtime", "crash", "loop", "threshold", "hide", "macos"]
platform: "macos"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L123"
related: ["docs/agents/learnings/runtime/2026-08-24-runtime-recovered-crashes-remain-successful-crash.md"]
---

## Historical source

> - 2026-08-24 [runtime] `keld dev` surfacing an app death only via the crash-loop breaker is timing-dependent, not a fix: `KELD-RUNTIME-002` needs 3 crashes inside a *sliding* 30s window, so one death — or a slower cadence — leaves a clean `Stopped` outcome over a dead app. Read durable crash state instead. Verified live on macOS: after `kill -9` the restarted generations could not re-enter the one-session echo listener (`Bun.connect` ENOENT) and crashed fast enough that the breaker did trip — so a test built on that trip passes over the real hole. (evidence: KEL-105 live run, `keld dev` exit 1 with `KELD-CORE-033`; `crates/keld-runtime/src/lib.rs` `RestartPolicy`/`crash_times.retain`)
