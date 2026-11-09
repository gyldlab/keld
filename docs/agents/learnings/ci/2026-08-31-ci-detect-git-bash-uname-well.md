---
name: "2026-08-31-ci-detect-git-bash-uname-well"
description: "Detect Git Bash through uname as well as MSYSTEM and structurally test cygpath setup."
area: "ci"
tags: ["ci", "detect", "git", "bash", "uname", "windows"]
platform: "windows"
recorded: "2026-08-31"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L153"
related: ["docs/agents/learnings/ci/2026-08-23-ci-normalize-windows-drive-unc-metadata.md"]
---

## Historical source

> - 2026-08-31 [ci/windows] PowerShell/`just` can launch Git for Windows Bash with `MSYSTEM` unset even though `uname -s` reports `MINGW*`; gating `cygpath` or Docker-bind permission setup only on `MSYSTEM` silently skips both. Detect either signal, and structurally test executable function definitions/calls rather than inert token presence. (evidence: KEL-149 follow-up, CodeRabbit PR #131, real Docker Desktop `EACCES mkdir /out/*-assets` negative observation)
