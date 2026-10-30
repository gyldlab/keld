---
name: "2026-08-19-runtime-corpus-digest-length-framing-concatenated"
description: "Corpus digest needs length framing; concatenated fields make ab,c collide with a,bc."
area: "runtime"
tags: ["runtime", "corpus", "digest", "length", "framing"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L100"
related: ["docs/agents/learnings/runtime/2026-08-19-runtime-bun-kill-exit-behavior-differs.md", "docs/agents/learnings/runtime/2026-08-24-runtime-bun-version-report-canary-inspect.md"]
---

## Historical source

> - 2026-08-19 [runtime] Dropping path-NUL-length framing from the KEL-77 corpus digest still passes a one-byte fixture flip; `("ab","c")` vs `("a","bc")` is the test that fails (concatenated SHA-256 of `abc`). (evidence: KEL-77 T2 negative control, `corpus_digest_rename_plus_content_shuffle_does_not_collide`)
