# Agent learning-record migration

Status: proposed KEL-264 capacity/retrieval repair. Independent instruction review is pending.
This branch targets the PR1319 profile branch. The published parent is unchanged.
No product boundary changes.

## Cause and correction

The published log had 66,011 bytes against a 65,280-byte limit.
Its 168 entries shared one file. A valid new record also failed instruction
classification because only the old single file could be evidence.

The candidate keeps the root lookup index at 998 bytes.
It stores all 168 source lines in categorized records. The largest record has
1,712 bytes. Every record fits the 2,048-byte default. No exception was needed.
The manifest registers every file. The old index allowance decreases to 2,048 bytes.
The directory AGENTS.md owns the record contract. Root and router point to that owner.

Records contain more stored metadata: 161,071 bytes across all cards.
This is not a claim of lower repository size or faster execution.
Names, metadata and selected facts are separate retrieval steps. Related links do
not cause automatic reads. Historical entries remain historical until revalidated.

## Preservation

Migration source: `244fb285125c2ee9e14c81a1c7b02cd357c7f5b1`.
Source SHA-256: `aa1c76e9079a06e4125d5fa74d5e395c19949266f35c9c68c830e463be5d05fc`.

The migration ledger reconstructs the complete original source, including its
header and line separators. Fixed source hash and count checks reject lost,
changed, duplicated or added historical entries. The two previously shortened
entries use their complete published text. Original profile worktrees remain frozen.

## Instruction cost and alternatives

Root: 13309 → 13307 bytes.
Router: 4054 → 4081 bytes.
New path-local owner: 3334 bytes,
with a 3,584-byte registered allowance. Root and existing automatic-chain caps stay unchanged.

The source uses 18,186 comparison tokens.
Per-record metadata and body loads implement bounded retrieval. The prior log already
required query/slice access. No measured task-context reduction is claimed.
These counts use tiktoken 0.12.0 / o200k_base. They are not model billing measurements.
Actual Codex prompt traces cover root, all ten crate owners and the new directory.
They check full root/nested content and absence of migrated entry bodies.
Other clients and semantic agent behavior remain unqualified.

A larger old-file cap, ordinal continuation files and a new external memory store
were rejected. They do not meet the selected discovery and preservation requirements.
The change reuses Git, rg, the existing manifest and the existing gate recipes.

## Language and limits

New descriptions are concise discovery text. Original evidence stays quoted.
The owner applies ASD-STE100 Issue 9 writing rules. The checker validates structure
and description length. It does not certify dictionary or semantic compliance.
The [official standard](https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf)
supports the selected sentence rules. Complete live PDF/dictionary retrieval was
unavailable; publisher-indexed sections supplied the scoped primary evidence.

A complete indivisible fact can use a measured, tracker-linked manifest exception.
The long-record control exercises this path. Truncation and arbitrary continuation
are not acceptance mechanisms.

## Validation and remaining gates

- Source reconstruction: 168 records pass.
- New real-filesystem contract tests: 13 pass.
- Instruction classifier tests: 16 pass; new evidence-class test failed before the fix.
- Atomic protocol: 40 pass. LLMS generator: 8 pass, generated corpus check passes.
- Format and warning-denied workspace Clippy pass.
- Existing closeout/hook/workspace tests pass; nine platform-specific workspace cases skip.
- Actual metadata-only reads leave selected record bodies unread.

`ci_inputs.py --check` and one router test currently fail on six stale reader
fingerprints after the owning recipe/checker changes. The fail-safe remains active.
Candidate values are prepared, but renewal waits for the user's independent reviewer.
No check, assertion, input edge or reviewed-scope exclusion is weakened.

Full current-head CI, fresh native workspace acceptance and independent review are
not claimed. Existing SDK/product, memory, Docker and Drawio gates remain separate.
After review, renew the verified bindings, run exact-head gates, and integrate the
reviewed migration into the profile branch. Do not mark the parent ready from this report.

## Rollback

Before integration, close the draft and retain the branch and evidence.
After an authorized integration, revert the migration as one unit. The fixed source
ledger retains every original line. Reverting does not resolve the original capacity defect.
