---
name: "2026-08-19-compat-trim-trailing-dots-github-raw"
description: "Trim trailing dots on GitHub raw hosts and reject nonnumeric host ports as aliases."
area: "compat"
tags: ["compat", "trim", "trailing", "dots", "github"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L90"
related: ["docs/agents/learnings/compat/2026-08-19-compat-ipv4addr-shared-unstable-check-cgnat.md"]
---

## Historical source

> - 2026-08-19 [compat] GitHub raw CDN live-ref checks must trim a trailing FQDN dot; `raw.githubusercontent.com./…/main/<40-hex>` bypassed the host equality and counted as a pin. Unbracketed `host:abc` was kept as the hostname when the port was non-numeric. (evidence: PR #34, `rejects_trailing_dot_github_raw_cdn_live_ref`, `rejects_malformed_https_ports`)
