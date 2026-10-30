---
name: "2026-08-13-ci-create-cargo-bin-directory-running"
description: "Create the Cargo bin directory before running the Windows install action."
area: "ci"
tags: ["ci", "create", "cargo", "bin", "directory", "windows"]
platform: "windows"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L45"
related: []
---

## Historical source

> - 2026-08-13 [ci] taiki-e/install-action on windows-latest fails if `%CARGO_HOME%\bin` is missing (tar: Cannot open: No such file). Create the dir before install. (evidence: GHA 31708711822)
