---
name: "2026-10-31-tooling-session-closeout-receipt-rejections"
description: "session_closeout.py rejects a baseline source_ref that does not resolve to the receipt head, a url field, and tracked rows without owner."
area: "tooling"
tags: ["closeout", "receipt", "baseline", "source-ref", "remote-receipt", "handoff"]
platform: "all"
recorded: "2026-10-31"
status: "verified"
verified: "2026-10-31"
source: "https://github.com/gyldlab/keld/blob/231a212a7f8441d54f5fe24793d6fd020f54699e/tools/session_closeout.py#L128"
related: []
---

## Fact

`just session-closeout <receipt>` rejected a research-session receipt three times before it validated as `handoff`. The validator at the cited commit checks these rules in this order.

1. If the baseline carries `git_common_dir` and `source_ref`, the ref must start with `refs/heads/` and `git rev-parse <ref>` must equal the receipt `head` (lines 128 and 132). On a detached checkout with a stale local `main`, drop both optional fields together instead.
2. Finding rows accept only `id`, `summary`, `status`, `evidence` plus `issue`, `remote_receipt`, `owner`, `next_action` (lines 228 and 229). The remote link field is `remote_receipt`; a `url` key fails as `unknown fields`.
3. A `tracked` or `implemented` row needs `issue` (`KEL-n` or `GH-n`), a `remote_receipt` URL on linear.app whose path contains `/issue/KEL-n`, and non-empty `owner` and `next_action` (lines 95 and 238).
4. Outcome `handoff` needs at least one unresolved row, for example a `blocked` finding (line 329). Outcome `complete` forbids new untracked files.

Negative control: the same receipt with `url` in place of `remote_receipt` fails with `unknown fields`; renaming the key passes. Evidence: prompt-tracker PR https://github.com/0monish/prompt-tracker/pull/58, file `session-closeout/p06-closeout-20261031.json`.
