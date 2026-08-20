<!--
Intake form. Policy lives in AGENTS.md — do not restate it here.
Strip every HTML comment from the submitted PR body (this block and the per-heading hints). Do not delete them from this file — hygiene reads them. Leftover comments are paid on every later `gh pr view`.

Branch: `agent/kel-<n>-<slug>` from `origin/main` (AGENTS.md § Commits & PRs).

Required headings below: keep the names. Fill them. Omit any optional heading that would be empty — do not write N/A.
Optional (only if they have content): `## Linear` (KEL-n), `## Rollback`, `## Screenshots`.
Do not paste review-bot release notes into this body.
-->

## Summary

<!-- 1–3 bullets. Why, not a file list. -->

## Spec refs

<!-- Architecture/spec paths + sections, or exactly: No boundary change -->

## Review gates

<!-- The five in AGENTS.md: `none`, or name those that apply (`unsafe`, public API, permission model, dependency addition, wire protocol). A false `none` on unsafe / permission / wire is a bad merge. -->

## Tests

<!-- Commands actually run, with counts if known. Never "should work". Use: cargo fmt --all --check · cargo clippy --workspace --all-targets -- -D warnings · cargo nextest run --workspace --profile ci. Mermaid diffs also run just mermaid-test && just mermaid-check && just mermaid-render-check. -->

## Platforms

<!-- Exercised vs not. Silence is a lie. -->

## Perf impact

<!-- none | measured | waiver. A fake percent is a bad merge. -->
