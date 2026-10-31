# Spec standard

Load to write, change or review a `docs/specs/` spec. Copy source:
[`spec-template.md`](../docs/agents/spec-template.md). `just spec-structure` enforces
the rest; § Writing and § Authoring checklist are for review.

## Size

A spec file MUST stay within all three caps:

| Cap | Value | Why |
|---|---|---|
| Lines | 500 | Owner rule |
| Bytes | 40 KiB | 500 lines at the census p80 of 82 bytes |
| Line | 400 chars | Over prose p99 (303) and table p90 (380); fences count |

A table is an index. Put AC, decision and blocker text under headings.

## Layout

A spec is one file `docs/specs/<id>-<slug>.md` or, over a cap, one flat directory, never
both.
An "Acceptance criteria" heading elsewhere fails.

```text
docs/specs/<id>-<slug>/
  index.md   map: header, sections 1 and 8, Parts, Current blockers
  <part>.md  sections 2-7, 9, 10, in part-table order
```

- `<id>` is `kel<n>` or `gh<n>`. The header states `Status`, `Tracker` (equal to `<id>`),
  `Owner`, `Updated` and `Pin` (a full SHA in branch history).
- The index stays within 120 lines and 8 KiB. Its `## ` headings are exactly
  `1. Goal & non-goals`, `Parts`, `Current blockers`, `8. Review gates triggered`.
- `## Parts` is a table `| Part | Owns | Load when |` that lists every part.
- `## Current blockers` lists exactly the open `B-n`.
- A part starts with `# <title>`, `Part of: [index](index.md)`, `Owns: <scope>`.
  A part `##` is a canonical section, with optional `: <subtitle>`.

Agents MUST load the index, then only the parts the task needs.

## Hierarchy and stable IDs

Sections keep the template order. An ID is defined once, by a heading that starts with
the ID, and any part may reference it.

| ID | Meaning | Section | Required fields |
|---|---|---|---|
| `AC-n` | Acceptance criterion | 3 | Given, When, Then, Negative control (Single fault, At-risk precondition, Expected RED) |
| `D-n` | Decision | 4 | |
| `T-n` | Task | 6 | Blocked by (`none` or IDs) |
| `E-n` | Evidence | 7 | Run, n, Environment, Limits |
| `B-n` | Blocker | 10 | Status (open/closed), Owner, Impact, Check |
| `Q-n` | Question | 10 | |
| `U-n` | Unknown | 10 | |

A spec defines at least one `AC-n` and one `T-n`, never in a fence, comment or indented
code. Each required field and `Owner` MUST hold real content: a slot, `TBD` or `UNKNOWN`
fails, and so does a bare `none` in `Then` or a negative control. Only a `migrated:` row
or a blocker `Owner` passes `UNKNOWN`.

Qualify another spec's ID with its tracker: `#1311 AC-2`, `KEL-53 B-4`. IDs never change
meaning; retire one by closing it, not by reuse.

## Writing

Normative text (rules, ACs, tasks, blockers) follows ASD-STE100 Issue 9 Part 1. Source:
<https://doi.org/10.5281/zenodo.23666006>, retrieved 2026-10-31: "Rule 5.1 Write short
sentences. Use a maximum of 20 words in each sentence."

- 5.1: at most 20 words in an instruction. 6.3: at most 25 in a description.
- 5.2: one instruction per sentence, unless the actions are simultaneous.
- 6.5, 6.6: one topic per paragraph, at most six sentences.
- 3.6: active voice. A description MAY use passive only when the agent is unknown.
- 2.1: nouns of at most three words. 8.1: no semicolons. 4.3: lists for complex text.
- 1.11, 9.4: one term for one thing. GR-3 (advice): replace an ambiguous pronoun with
  its noun.
- 8.5-8.7 word count: parenthetical text, identifiers, quotes, numbers and hyphenated
  words each count as one.

Use RFC 2119 words only in normative statements. The STE dictionary is not adopted. Keld
terms are technical nouns under rule 1.5.

## Citations

| Claim | Form |
|---|---|
| Code | `` `path:line` `` at the spec `Pin`, or `` `path:line@<full-sha>` `` |
| Other spec or PR | `` #n `path:line@<full-sha>` `` |
| External | URL, verbatim quote, retrieval date |
| Measured | `E-n` |
| Review finding | PR comment link |

A relative link stays in `docs/`, `.agents/` or a `.md` file, through no symlink; cite
code instead.
Label every factual claim FACT, INFERRED or UNKNOWN.

## Authoring checklist

Review the whole spec at the exact head before push, not the diff. Fix every instance of
a class. Check that:

1. Each negative control can turn RED: a single fault, a reachable precondition, no
   earlier refusal or later defense that hides it, an observable at the rule's owner.
2. Each cross-owner sentence matches that owner's text at a pinned SHA.
3. An outcome that depends on an open `Q-n`, `B-n` or `U-n` is conditional and names it,
   and a blocker states its full impact.
4. No task exposes a capability before its safety atoms, and each task runs its ACs.
5. Each platform claim is quoted from a primary source, measured, or labelled.
6. No rule, term, count or status disagrees across parts. Grep changed terms.
7. Paired events hold in both orders, and an AC forces the adverse order.
8. Each citation matches its pin, and each quote is verbatim.
9. A reply or PR body claims no more than the spec.
10. An upstream event is not assumed to be OS completion.
11. Each documented host obligation has a caller.
12. An "every" rule is tested on every kind, not on one.
13. Old evidence does not qualify a new code path.
14. Task dependencies are explicit, never implied by forward order.
15. No state commits before a fallible step without recovery.
16. The oracle branches where the rule branches.
17. Each queue, retry, buffer and wait has a bound.
18. A check on a normalized spelling also covers the raw reopen.
19. A same-world signal is not treated as authority.
20. A raw path cannot bypass a private narrowing.
21. Each digest covers every input it vouches for.

## Ratchet

`docs/specs/.spec-baseline.tsv` lists landed files that fail a cap or the structure. An
over-cap value MUST equal the measurement. It MUST NOT exceed its value at `BASELINE_SEAL`
or at the merge base with `KELD_CI_BASE_REF` or `origin/main`, so a shrink is permanent.
A missing base fails. A new file MUST NOT enter. Remove a row when its file complies, and
lower `BASELINE_ROWS`. Print rows with
`target/spec-structure/spec-structure baseline-rows .` after `just spec-structure`.

A landed spec with gaps uses `migrated:<id.field,...>`, which names each `UNKNOWN` field
and can only lose names. A directory keeps its flat row, sizes `-`.
Raising `BASELINE_ROWS` or moving the seal needs repository-owner approval.

Agents MUST NOT split a spec to hide text, or lower a value by deleting a contract.

## Not enforced

Quote accuracy, labels, reworded non-values, cited line content, root-level and
extensionless citations, `@sha` commits absent from the clone, text split across lines,
raw HTML links, `<details>` text, renumbered IDs.
