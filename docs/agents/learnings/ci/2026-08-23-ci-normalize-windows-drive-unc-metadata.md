---
name: "2026-08-23-ci-normalize-windows-drive-unc-metadata"
description: "Normalize Windows drive and UNC metadata at ingestion while preserving valid Unix backslash bytes."
area: "ci"
tags: ["ci", "normalize", "windows", "drive", "unc"]
platform: "windows"
recorded: "2026-08-23"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L112"
related: ["docs/agents/learnings/ci/2026-08-31-ci-detect-git-bash-uname-well.md"]
---

## Historical source

> - 2026-08-23 [ci] Git Bash reports the repo root as `D:/...` while cargo.exe metadata uses JSON `D:\\...`; normalize only Windows drive/UNC metadata filesystem fields once at ingestion, because a backslash inside an absolute Unix path is a valid filename byte and a root-relative Windows path cannot match a drive-qualified repo root. Exercise native shapes per OS plus native-jq CRLF in the portable fixture. (evidence: KEL-149, CodeRabbit #127, `tools/ci_changes.sh`, `tools/ci_changes_test.sh`)
