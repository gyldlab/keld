---
name: "2026-08-24-ci-package-pinned-typescript-compiler-frozen"
description: "Use the package-pinned TypeScript compiler with a frozen lockfile, not unpinned bunx."
area: "ci"
tags: ["ci", "package", "pinned", "typescript", "compiler"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L129"
related: ["docs/agents/learnings/git/2026-08-25-git-branch-recreation-reset-upstream-push.md"]
---

## Historical source

> - 2026-08-24 [ci] Unpinned `bunx tsc` resolved TypeScript 7 (the native port) at job time; typecheck lanes use the packages' exact `typescript` devDependency pin and a frozen lockfile. (evidence: KEL-111, `bunx tsc --version` 7.0.2 on 2026-08-24)
