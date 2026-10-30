---
name: "2026-09-04-ci-ubuntu-apparmor-deny-bubblewrap-loopback"
description: "Ubuntu AppArmor can deny Bubblewrap loopback; mirror the approved CI sysctl without retries or skips."
area: "ci"
tags: ["ci", "ubuntu", "apparmor", "deny", "bubblewrap", "linux"]
platform: "linux"
recorded: "2026-09-04"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L156"
related: []
---

## Historical source

> - 2026-09-04 [ci/linux] Ubuntu 24.04 AppArmor may deny Bubblewrap loopback setup; mirror upstream's userns CI sysctl, never retry or skip. (evidence: PR #146, bubblewrap `f97804f`)
