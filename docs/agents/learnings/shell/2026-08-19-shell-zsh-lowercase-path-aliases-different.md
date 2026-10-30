---
name: "2026-08-19-shell-zsh-lowercase-path-aliases-different"
description: "In zsh, lowercase path aliases PATH; use a different loop variable for filenames."
area: "shell"
tags: ["shell", "zsh", "lowercase", "path", "aliases"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L96"
related: ["docs/agents/learnings/ipc/2026-08-19-ipc-macos-wake-socket-notconnected-mean.md", "docs/agents/learnings/runtime/2026-08-19-runtime-macos-unix-socket-sun-len-private-tmp.md"]
---

## Historical source

> - 2026-08-19 [shell] In zsh, lowercase `path` is tied to `PATH`; using `path` as a loop variable can erase command lookup mid-script. Use names like `changed_file` in hygiene/review loops. (evidence: KEL-75 review hygiene command, `docs/agents/learnings.md`)
