---
name: "2026-10-06-shell-elevated-powershell-run-user-controlled"
description: "Elevated PowerShell can run user-controlled PATH or modules; pin both paths and reject profiler variables."
area: "shell"
tags: ["shell", "elevated", "powershell", "run", "user", "windows"]
platform: "windows"
recorded: "2026-10-06"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L171"
related: ["docs/agents/learnings/test/2026-10-14-test-failed-host-fixtures-leak-inherited.md"]
---

## Historical source

> - 2026-10-06 [shell/windows] PowerShell 5.1 runs `<name>.ps1` from PATH before auto-loading a bare cmdlet's module (pinning only `PSModulePath` does not stop it), and UAC elevation inherits the user's PATH with its user-writable entries: a hash-verified elevated script can run same-user-planted code. Pin PATH to System32 + `WindowsPowerShell\v1.0` and `PSModulePath` to its `Modules` (.NET members only; with PATH alone a module on the user's PSModulePath still shadows `New-Object`/`Get-FileHash`) as the first statement of every elevated entry point, even a `Get-FileHash` one-liner; resolve git etc. once to an absolute path whose ancestors only admins, SYSTEM or TrustedInstaller can write, with no reparse point; run `powershell.exe -NoProfile -NonInteractive`. Check: a `New-Object.ps1` on a prepended PATH dir and a module exporting `New-Object` on a prepended PSModulePath dir print only without the pins. Refuse `COR_ENABLE_PROFILING`, `COR_PROFILER*`, `COMPlus_*`, also in HKCU `Environment`/`Volatile Environment`, before each elevation. (evidence: KEL-270 comments `8a5fc164`/`546b1a44`/`ae92b7e0`)
