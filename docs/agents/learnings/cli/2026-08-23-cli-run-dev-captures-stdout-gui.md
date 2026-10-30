---
name: "2026-08-23-cli-run-dev-captures-stdout-gui"
description: "run_dev captures stdout before GUI launch; later Bun actions need an independent ordered witness."
area: "cli"
tags: ["cli", "run", "dev", "captures", "stdout", "windows"]
platform: "windows"
recorded: "2026-08-23"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L116"
related: []
---

## Historical source

> - 2026-08-23 [cli] `run_dev` writes one captured Bun stdout snapshot before opening the GUI and does not live-forward later child logs; a product proof of a later Bun action needs an ordered independent witness, so absent terminal stdout is not by itself an IPC failure. (evidence: KEL-100 Windows product proof, crates/keld-cli/src/dev.rs `run_dev`)
