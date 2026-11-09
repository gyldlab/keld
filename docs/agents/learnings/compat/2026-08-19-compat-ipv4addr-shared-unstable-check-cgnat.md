---
name: "2026-08-19-compat-ipv4addr-shared-unstable-check-cgnat"
description: "Ipv4Addr::is_shared is unstable; check CGNAT octets directly."
area: "compat"
tags: ["compat", "ipv4addr", "shared", "unstable", "check"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L89"
related: ["docs/agents/learnings/compat/2026-08-19-compat-exclude-special-ipv6-ranges-public.md", "docs/agents/learnings/compat/2026-08-19-compat-trim-trailing-dots-github-raw.md"]
---

## Historical source

> - 2026-08-19 [compat] `Ipv4Addr::is_shared` is still unstable (`feature(ip)` / E0658 on 1.97.1); CGNAT `100.64.0.0/10` must be octet-checked. (evidence: PR #34, `cargo clippy -p keld-compat`)
