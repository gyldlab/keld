---
name: "2026-08-25-git-branch-recreation-reset-upstream-push"
description: "Branch recreation can reset upstream; push an explicit destination ref and inspect full output."
area: "git"
tags: ["git", "branch", "recreation", "reset", "upstream"]
platform: "unspecified"
recorded: "2026-08-25"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L136"
related: ["docs/agents/learnings/ci/2026-08-24-ci-package-pinned-typescript-compiler-frozen.md"]
---

## Historical source

> - 2026-08-25 [git] `git checkout -B <branch> origin/main` (default `branch.autoSetupMerge`) resets an existing branch's upstream to `origin`/`refs/heads/main` (`branch.<name>.remote`/`.merge` show it; `--no-track` prevents it); a bare `git push --force-with-lease` then aborts (`simple`), targets `main` (`upstream`/`tracking`) or pushes every same-named branch (`matching`), and filtered output hides it. Push `git push --force-with-lease origin HEAD:refs/heads/<branch>` and read the full output. (evidence: KEL-111/KEL-112; throwaway-repo reproduction)
