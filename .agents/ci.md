# CI and KeldBot playbook

Load for Actions, routing, checks, dependencies, KeldBot or protection.
`.github/workflows/ci.yml` and router/checkers own behavior; this owns invariants.

## Required workflow and routing

- Run CI workflows for every PR and push. Required workflows MUST NOT use
  workflow-level `paths`/`paths-ignore`; route jobs so skips remain visible.
- Job-level `if` may use `needs`, `github`, `vars`, and `inputs`, never `matrix` (GitHub
  evaluates the job condition before matrix expansion).
- Every lane names its observable contract and changed inputs. `gitleaks` is unconditional
  because every byte is input; it is the checksum-pinned CLI job, not the org-licensed
  Action. `CI required` always runs, consumes every routed lane plus gitleaks, and
  rejects missing, cancelled, failed, or selected-as-skipped evidence.
- Mermaid routing selects changed/added/deleted/malformed diagram blocks and
  parser/renderer/config/pin/router/workflow inputs, not prose-only edits. Local
  and hosted share applicability; unknown comparisons render all. `just ci-full`
  covers the corpus. Zero candidates need no Docker; `CI required` rejects a
  selected Mermaid job missing/skipped/cancelled/failed.
- Build closures come from current metadata where available. Unknown/shared/workspace
  graph/comparison-base inputs fail safe by enabling every possibly affected lane,
  including Ubuntu WebKitGTK apt.
- Workflow/router edits exercise every conditional lane. Each Ubuntu leg building
  GTK-linked crates, fallbacks included, installs WebKitGTK behind a 15-minute step
  timeout and tests the full selection. MSRV stays on macOS without apt.
- Router metadata/parse failure fails the router job before outputs. It MUST NOT emit an
  empty/skipped-green selection.
- Test relevant/unrelated/unknown/empty PR and push diffs. Verify branch
  protection against current official GitHub docs before change.
- CI routing is an independently reviewed shared-file concern and lands in its own
  Linear-scoped PR unless CI is already the owning issue.

## Failure preservation

- Merge-critical workflows/jobs/steps MUST NOT use `continue-on-error`, inherited custom
  shells, retries, skips, or wrappers that suppress an exit status.
- Exception requiring CI/security review: up to three read-only, pre-verdict
  GitHub PR-preview reads for null/404/stale parents. Each pins PR/base/head;
  after delay recheck queue/ref. Admit only exact preview parents/staged tree.
  Wrong tree, changed identity, other API error or exhaustion denies. NEVER
  retry failed jobs, tests, checks, verdicts or App writes.
- Actions are immutable-SHA pinned. Checkout credentials are disabled unless a named
  operation genuinely needs write authority.
- Required results validate router applicability and job status; GitHub
  treats skipped required checks as successful.
- Never weaken a test/check to fit a change. Diagnose per root atomic protocol and
  `.agents/testing.md`.

## KeldBot

- PR title: `type(scope): imperative subject`, no trailing period. Allowed types are
  `feat fix docs test chore ci style build`; adding one also adds its `type:*` label and
  updates title-lint in the same PR.
- PR body has six non-empty headings: Summary · Spec refs · Review gates · Tests ·
  Platforms · Perf impact. `No boundary change` is valid Spec refs. Omit empty optional
  sections and strip submitted template comments.
- `gatekeeper` and `title-lint` are required. `size-label` is informational. All rerun on
  edited/synchronize and self-resolve after the fix.
- `pull_request_target` supports fork metadata. KeldBot MUST NOT checkout PR
  code or execute interpolated PR text. Re-derive threat model before changes.
- Bot labels: `needs-template-fix`, `needs-conventional-title`, `size/*`, `type:*`.
  Human-only review labels: the five `gate:*` and twelve `crate:*`; do not claim KeldBot
  inferred them.

## Verification

Run `just ci-router-test`, `just hygiene`, the workflow YAML parser check, and the full
root gate. Report actual GitHub `CI required` result at the exact final head.
