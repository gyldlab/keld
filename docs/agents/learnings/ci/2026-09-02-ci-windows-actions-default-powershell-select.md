---
name: "2026-09-02-ci-windows-actions-default-powershell-select"
description: "Windows Actions default to PowerShell; select Bash for POSIX steps and test the exact condition."
area: "ci"
tags: ["ci", "windows", "actions", "default", "powershell"]
platform: "windows"
recorded: "2026-09-02"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L155"
related: []
---

## Historical source

> - 2026-09-02 [ci/windows] An unqualified GitHub Actions `run` step on `windows-latest` uses PowerShell; POSIX `mkdir -p` invokes `New-Item` and fails when the Rust cache already restored the directory. Set `shell: bash` for POSIX blocks and contract-test the shell, unique step, and exact condition. (evidence: CodeRabbit PR #136, GHA 33622370474, `tools/ci_hygiene.rs`)
