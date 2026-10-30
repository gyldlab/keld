---
name: "2026-08-13-cli-cli-verbs-keld-command-failures"
description: "Non-live CLI verbs need KELD-CLI-045, not generic unknown-command failures."
area: "cli"
tags: ["cli", "verbs", "keld", "command"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L39"
related: ["docs/agents/learnings/cli/2026-08-13-cli-reject-closed-keld-create-flags.md"]
---

## Historical source

> - 2026-08-13 [cli] Spec-named verbs that are not live (`build`/`migrate`/`gen`/`ext`) must be `KELD-CLI-045` exit 2, not a bare `unknown command` exit 1 — README agents otherwise have no tracking issue. (evidence: crates/keld-cli/src/verb.rs, KEL-29)
