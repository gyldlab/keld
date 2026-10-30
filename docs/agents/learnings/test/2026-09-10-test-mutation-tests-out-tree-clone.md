---
name: "2026-09-10-test-mutation-tests-out-tree-clone"
description: "Mutation tests need an out-of-tree clone with Git metadata and isolated target; verify tree and baseline first."
area: "test"
tags: ["test", "mutation", "tests", "out", "tree"]
platform: "unspecified"
recorded: "2026-09-10"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L161"
related: ["docs/agents/learnings/test/2026-09-10-test-bound-derived-fixture-volume-cannot.md"]
---

## Historical source

> - 2026-09-10 [test] Mutation-test landed code out of tree: `git clone --no-hardlinks` to a path outside the repo, with `CARGO_TARGET_DIR` outside it too. Clone `.git` — a copy without it makes tests running `git rev-parse HEAD` fail exit 128, which reads as a mutation result, and forces a hand-undo instead of `git restore`. Verify the copy's `HEAD` tree hash equals the real one so results transfer, and take an unmutated baseline first so every red is attributable. (evidence: KEL-225/223/224 runs at `9ad7993`)
