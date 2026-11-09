---
name: "2026-08-24-bench-derive-provenance-immutable-git-objects"
description: "Derive provenance from immutable Git objects and status; refuse dirty-tree publication."
area: "bench"
tags: ["bench", "derive", "provenance", "immutable", "git", "windows"]
platform: "windows"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L134"
related: []
---

## Historical source

> - 2026-08-24 [bench] An evidence document that records its own provenance must DERIVE it, never author it: hardcoding `bench_tree_state: "clean"` and hashing the WORKING-TREE harness shipped three keld-benches documents whose recorded harness sha256 did not exist at the recorded commit (one named a file that was still untracked there). `git show <sha>:<path>` and `git status --porcelain <path>` make both checkable in two lines, and an emitter that refuses to run on a dirty tree catches it before publication — provenance that cannot be checked is worse than absent because it reads as verified. (evidence: keld-benches `windows/bench/Emit-Kel25Documents.ps1` Assert-HarnessAtCommit, PR gyldlab/keld-benches#12 commits bab6c97/abd092d)
