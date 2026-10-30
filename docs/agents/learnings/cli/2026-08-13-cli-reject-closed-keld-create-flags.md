---
name: "2026-08-13-cli-reject-closed-keld-create-flags"
description: "Reject closed keld create flags and extra dev arguments before entering the window loop."
area: "cli"
tags: ["cli", "reject", "closed", "keld", "create"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L40"
related: ["docs/agents/learnings/cli/2026-08-13-cli-cli-verbs-keld-command-failures.md", "docs/agents/learnings/cli/2026-08-15-cli-cross-platform-callee-made-macos.md"]
---

## Historical source

> - 2026-08-13 [cli] `keld create --template` must be `KELD-CLI-044` (closed flag set), not `KELD-CLI-020` invalid name `--template`; extra `keld dev` tokens must fail before tao `EventLoop::run`. (evidence: crates/keld-cli/src/flags.rs, KEL-29)
