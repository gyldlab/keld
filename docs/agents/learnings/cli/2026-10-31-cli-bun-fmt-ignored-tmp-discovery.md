---
name: "2026-10-31-cli-bun-fmt-ignored-tmp-discovery"
description: "Formatter discovery differs under ignored TMP; neutralize TMP to reproduce the check and retain unknown original stdout."
area: "cli"
tags: ["bun-fmt", "cli", "differs", "discovery", "formatter", "ignored", "tmpdir"]
platform: "unspecified"
recorded: "2026-10-31"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L199"
related: []
---

## Historical source

> - 2026-10-31 [cli/work-run] With Bun 1.4.4+38afee6c8, identical generated-project files and formatter argv had different check outcomes beneath ignored `.keld-work` scratch and a neutral short temporary root: the ignored healthy/malformed cases both exited 0 without a reported file census; neutral scratch reported five healthy files and rejected the planted malformed file with exit 1. Explicit neutral `TMPDIR`/`TEMP`/`TMP` restored the unchanged formatter regression during qualification. The original failed child stdout/census remain unknown; no zero-file discovery count was established. Failed-run and two-location control evidence: [GH569 handoff](https://github.com/gyldlab/keld/issues/569#issuecomment-6531966267), with [official ignore/check semantics](https://bun.com/docs/format).
