---
name: "2026-08-23-process-prompt-tracker-categories-synchronized-schema"
description: "Prompt Tracker categories require synchronized schema, taxonomy labels, and local catalog entries."
area: "process"
tags: ["process", "prompt", "tracker", "categories", "synchronized"]
platform: "unspecified"
recorded: "2026-08-23"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L117"
related: []
---

## Historical source

> - 2026-08-23 [process] In the Prompt Tracker checkout (`0monish/prompt-tracker`, typically `keld-agent-prompts`), new categories must be added to `src/data/schema.ts` (`PROMPT_CATEGORIES`), `src/data/taxonomy.ts` (`CATEGORY_LABELS`), and `scripts/local-catalog.ts`, or typecheck/seed fail. (evidence: KEL-109, `NEW/audit/01`)
