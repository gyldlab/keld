---
name: "2026-08-24-runtime-bun-version-report-canary-inspect"
description: "bun --version can report a canary; inspect --revision and restore stable releases."
area: "runtime"
tags: ["runtime", "bun", "version", "report", "canary"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L132"
related: ["docs/agents/learnings/ipc/2026-08-25-ipc-bun-unix-option-connects-windows.md", "docs/agents/learnings/ipc/2026-08-31-ipc-normalize-win32-pipe-close-errors.md", "docs/agents/learnings/runtime/2026-08-19-runtime-corpus-digest-length-framing-concatenated.md"]
---

## Historical source

> - 2026-08-24 [runtime] `bun --version` can print an unreleased version (`1.4.1` for `1.4.1-canary.1+abe2ad4f0`); the onboarding README's `bun --revision` record exposes it, and `bun upgrade --stable` restores the release. (evidence: KEL-77, KEL-101 DACL probe)
