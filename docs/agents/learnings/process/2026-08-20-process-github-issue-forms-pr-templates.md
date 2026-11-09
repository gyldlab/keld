---
name: "2026-08-20-process-github-issue-forms-pr-templates"
description: "GitHub issue forms are not PR templates; branch naming and comment stripping need explicit handling."
area: "process"
tags: ["process", "github", "issue", "forms", "pr"]
platform: "unspecified"
recorded: "2026-08-20"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L103"
related: []
---

## Historical source

> - 2026-08-20 [process] GitHub YAML issue forms are issues-only; PR templates are Markdown. Multiple PR templates have no chooser (query param only). HTML comments persist in `gh pr view` unless stripped. Feature branches are `agent/kel-<n>-<slug>` from `origin/main`, not Linear's `gyldlab/kel-…`. (evidence: docs.github.com issue-forms + PR-template pages; `gh pr view` #34/#41; KEL-84)
