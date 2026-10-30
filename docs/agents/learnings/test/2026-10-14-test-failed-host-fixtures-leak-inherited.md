---
name: "2026-10-14-test-failed-host-fixtures-leak-inherited"
description: "Failed host fixtures leak inherited stdout; kill surviving hosts on drop and capture diagnostics."
area: "test"
tags: ["test", "failed", "host", "fixtures", "leak", "windows"]
platform: "windows"
recorded: "2026-10-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L177"
related: ["docs/agents/learnings/shell/2026-10-06-shell-elevated-powershell-run-user-controlled.md", "docs/agents/learnings/test/2026-10-14-test-winsock-reset-drops-buffered-replies.md"]
---

## Historical source

> - 2026-10-14 [test/windows] Extends the 2026-08-24 grandchild-pipe entry to the runner side: a host left alive by a failed assertion inherits the test binary's redirected stdout (Rust `Command` inherits every inheritable handle), so PowerShell's native-command wait never sees EOF and a failing cell hangs the whole run script until the leaked host dies. Give the host fixture a kill-on-drop that runs only after `try_wait` says it is alive, and dump its captured output there as diagnostics. (evidence: KEL-270 S6c2a, `crates/keld-host/tests/no_flag/windows/profiles/sessions.rs` `SeamedHost::drop`, evidence run failure-first-base-h12)
