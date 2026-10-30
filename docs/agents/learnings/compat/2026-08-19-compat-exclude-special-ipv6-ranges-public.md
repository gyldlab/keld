---
name: "2026-08-19-compat-exclude-special-ipv6-ranges-public"
description: "Exclude special IPv6 ranges from public evidence; record unresolved literal and DNS residuals separately."
area: "compat"
tags: ["compat", "exclude", "special", "ipv6", "ranges"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L88"
related: ["docs/agents/learnings/compat/2026-08-19-compat-ipv4addr-shared-unstable-check-cgnat.md", "docs/agents/learnings/compat/2026-08-19-compat-normalize-trailing-fqdn-dots-ipv4.md"]
---

## Historical source

> - 2026-08-19 [compat] After the FQDN-dot / mapped / compatible / translated close, NAT64 `64:ff9b::/96`, 6to4 `2002::/16`, and CGNAT `100.64.0.0/10` still counted as public https evidence; abbreviated IPv4 and DNS-to-private are T1 residuals (literal host, no DNS). (evidence: PR #34, `64:ff9b::10.0.0.1`, `2002:a00:1::1`, `100.64.0.1`)
