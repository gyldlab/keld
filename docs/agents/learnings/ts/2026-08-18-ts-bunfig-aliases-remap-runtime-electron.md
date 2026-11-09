---
name: "2026-08-18-ts-bunfig-aliases-remap-runtime-electron"
description: "bunfig aliases do not remap runtime Electron imports; TypeScript paths only affect resolution."
area: "ts"
tags: ["ts", "bunfig", "aliases", "remap", "runtime"]
platform: "unspecified"
recorded: "2026-08-18"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L73"
related: ["docs/agents/learnings/compat/2026-08-18-compat-drain-queued-reader-output-asserting.md"]
---

## Historical source

> - 2026-08-18 [ts] Bun 1.3.14 `bunfig.toml` `[alias]` does not remap runtime `import "electron"` — `import.meta.resolve("electron")` still returns `~/.bun/install/cache/electron`; `tsconfig.json` `compilerOptions.paths` does remap it. (evidence: KEL-72, `packages/@keld/electron/fixtures`)
