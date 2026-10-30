---
name: "2026-08-17-cli-error-scanner-owns-registry-coverage"
description: "The error scanner owns registry coverage; add every new KELD code to docs in the same change."
area: "cli"
tags: ["cli", "error", "scanner", "owns", "registry"]
platform: "unspecified"
recorded: "2026-08-17"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L68"
related: []
---

## Historical source

> - 2026-08-17 [cli] `SCAN_REL` owns error-registry coverage; emitted and registered codes are checked bidirectionally, and a new `KELD-*` code ships with its `docs/engineering/keld-error-codes.md` heading in the same PR. (evidence: KEL-211, `crates/keld-cli/tests/error_registry.rs`, `docs/architecture/07-agent-experience.md` §2, `docs/onboarding/06-documentation-map.md` error-registry section)
