---
name: "2026-09-11-test-process-substitution-hides-git-ls"
description: "Process substitution hides git ls-files failures; use pipefail with NUL paths and narrow evidence exclusions."
area: "test"
tags: ["test", "process", "substitution", "hides", "git", "linux"]
platform: "linux"
recorded: "2026-09-11"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L165"
related: []
---

## Historical source

> - 2026-09-11 [test/linux] Process substitution hides a failing `git ls-files -z`; use a `pipefail`-checked NUL pipeline. An in-repo evidence exception must exclude only its normalized subtree and reject Git admin dirs. (evidence: CodeRabbit #227)
