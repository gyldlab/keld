---
name: "2026-09-09-guard-wildcard-url-grants-match-scheme"
description: "Wildcard URL grants can match only a scheme; judge destination access from the expanded grant boundary."
area: "guard"
tags: ["guard", "wildcard", "url", "grants", "match"]
platform: "unspecified"
recorded: "2026-09-09"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L157"
related: ["docs/agents/learnings/test/2026-09-09-test-double-escaped-json-fixtures-grant.md"]
---

## Historical source

> - 2026-09-09 [guard] A `/**` prefix matcher strips the URL authority delimiter: `https://**` leaves prefix `https:/`, whose boundary byte is the second slash of `//`. `https:/**` widens identically without ending in `//**`, and a resource-side rule keyed on `://` cannot see it (RFC 3986 reads a path, WHATWG reads a host). Judge the *grant*: scheme + separators only = no destination. Traps: the colon must be in scheme position or `$APPDATA/cache/https:/**` goes inert; `C:` is both scheme and drive, so `a:/**` stays a path glob. (evidence: KEL-208, `names_no_destination`)
