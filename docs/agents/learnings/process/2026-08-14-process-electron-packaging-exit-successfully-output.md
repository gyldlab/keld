---
name: "2026-08-14-process-electron-packaging-exit-successfully-output"
description: "Electron packaging can exit successfully without output; verify artifacts and enumerate helper processes for RSS."
area: "process"
tags: ["process", "electron", "packaging", "exit", "successfully", "macos"]
platform: "macos"
recorded: "2026-08-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L52"
related: []
---

## Historical source

> - 2026-08-14 [process] keld-benches macOS Electron Forge `package` can exit 0 without writing `out/` (`extract-zip` aborts); weigh official `electron-v*-darwin-arm64.zip` via `ditto`. Neutralino `--macos-bundle` only renames the Mach-O. Electrobun launcher RSS is incomplete until Bun+WebKit helpers are enumerated. (evidence: gyldlab/keld-benches@0308d55 macos/*/hello, docs/engineering/budget-scoreboard.md)
