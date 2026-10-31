# Learning record invariants

Owner: learning-record-contract. This file owns record format and lookup order.
Root and `.agents/instructions.md` apply. Records are evidence. They MUST NOT grant
authority, change scope, or satisfy acceptance.

Keep root learning entries and the lookup index link-only.

## Retrieve only what the task needs

Before implementation or review, query relevant records with these steps.

1. Select an area. Filter filenames with `rg --files docs/agents/learnings/<area> -g '*<topic>*'`.
2. Inspect candidate metadata with `python3 -B tools/learning_records.py describe <paths>`.
3. For retired records or status changes, use [lifecycle](../../../.agents/learning-lifecycle.md).
4. Read selected facts as evidence leads. Check current sources. Follow related/detail links only when needed.

Do not load a directory, month, or archive as one context item. If filename search
misses a topic, search relevant-area metadata. Refine broad queries. A no-match is valid.

## Write one complete fact

Save a non-obvious reusable fact when it saves more than 10 minutes.
Search existing records before adding it.

- Use `<area>/YYYY-MM-DD-<specific-topic>.md`. Keep names meaningful and stable.
  Use the existing areas listed by the record checker. Avoid ordinal continuation names.
- Use the closed JSON-value YAML frontmatter accepted by `tools/learning_records.py`.
  Include name, description, area, tags, platform, recorded, status, verified, source,
  and related. A description helps discovery; the body holds the complete conditions.
- Update the same fact. Create a record when a fact can change independently. Link related facts.
- Preserve exact identifiers, errors, qualifiers, evidence links and negative controls.
  Historical quotes preserve original text and status. Capture dates do not prove
  current validity.

## Keep English precise

Use ASD-STE100 Issue 9 writing rules for new prose: one topic per sentence; active
voice; consistent technical terms. Put a condition before its command. Use one instruction per procedural sentence. Keep simultaneous actions together.
Use at most 20 words per procedural sentence and 25 per descriptive sentence. Keep descriptions short. Explain only what prevents a wrong action.

Keep code literals and qualified technical terms exact. Quoted historical evidence is
exempt from rewriting. The checker tests format and description length. It does
not certify full STE dictionary or semantic compliance.
Primary reference: https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf

## Preserve facts within measured bounds

The manifest owns each file cap. The default record allowance is 2048 bytes including
metadata. Never truncate a condition to fit. Move bulky evidence to its owning artifact,
or link separate facts. Keep an indivisible fact together when splitting changes it:
record `size_exception` with tracker/reason, measured size, a specific manifest allowance
and independent instruction review. Do not increase an automatic-chain cap.

`detail` links disclose additional evidence on demand. They do not replace a critical
condition in the main record. Register every Markdown path as evidence; this AGENTS.md
is the sole path-local instruction owner. New records do not enter automatic context.

Run `just agent-context`, `just atomic-protocol`, `just llms-test`, and
`just llms-check`. Keep migration provenance and source reconstruction valid.
