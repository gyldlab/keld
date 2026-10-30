---
name: "2026-08-19-compat-normalize-trailing-fqdn-dots-ipv4"
description: "Normalize trailing FQDN dots and IPv4-mapped forms before classifying evidence hosts as public."
area: "compat"
tags: ["compat", "normalize", "trailing", "fqdn", "dots"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L87"
related: ["docs/agents/learnings/compat/2026-08-19-compat-exclude-special-ipv6-ranges-public.md", "docs/agents/learnings/compat/2026-08-19-compat-public-scoreboard-fields-forge-completion.md"]
---

## Historical source

> - 2026-08-19 [compat] `Ipv4Addr` rejects a trailing FQDN dot and `to_ipv4_mapped` ignores deprecated `::a.b.c.d` / `::ffff:0:a.b.c.d`, so RFC1918/169.254 still parsed as public https evidence until the host is trimmed and those embeddings share the v4 check. (evidence: PR #34, `https://10.0.0.1.` and `https://[::10.0.0.1]/` at 1a334f1)
