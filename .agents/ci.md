# CI and KeldBot playbook

Load only for GitHub Actions, the change router, required checks, CI dependencies,
KeldBot, or branch-protection work. `.github/workflows/ci.yml` and the router/checkers
are source of truth; this file owns the design invariants.

## Required workflow and routing

- CI workflows MUST be created for every PR and push. Required workflows MUST NOT use
  workflow-level `paths`/`paths-ignore`; route at job boundaries so skipped jobs report.
- Job-level `if` may use `needs`, `github`, `vars`, and `inputs`, never `matrix` (GitHub
  evaluates the job condition before matrix expansion).
- Every lane names its observable contract and changed inputs. `gitleaks` is unconditional
  because every byte is input; it is the checksum-pinned CLI job, not the org-licensed
  Action. `CI required` always runs, consumes every routed lane plus gitleaks, and
  rejects missing, cancelled, failed, or selected-as-skipped evidence.
- Mermaid is separate from broad documentation routing: changed/added/deleted/malformed
  diagram blocks and parser/renderer/config/pin/router/workflow inputs select it; prose-only
  changes do not. Local and hosted routing share the same applicability owner. Unknown
  comparisons fail safe to full rendering. `just ci-full` retains explicit full-corpus
  assurance. Zero-candidate rendering performs no Docker probe; `CI required` rejects a
  selected Mermaid job that is skipped, missing, cancelled, or failed.
- Build closures come from current metadata where available. Unknown/shared/workspace
  graph/comparison-base inputs fail safe by enabling every possibly affected lane,
  including Ubuntu WebKitGTK apt.
- Workflow/router edits exercise every conditional lane. Each Ubuntu leg building
  GTK-linked crates, fallbacks included, installs WebKitGTK behind a 15-minute step
  timeout and tests the full selection. MSRV stays on macOS without apt.
- Router metadata/parse failure fails the router job before outputs. It MUST NOT emit an
  empty/skipped-green selection.
- Contract tests cover relevant, unrelated, unknown, empty, PR, and push diffs. Verify
  branch-protection behavior against current official GitHub documentation before change.
- CI routing is an independently reviewed shared-file concern and lands in its own
  Linear-scoped PR unless CI is already the owning issue.

## Failure preservation

- Merge-critical workflows/jobs/steps MUST NOT use `continue-on-error`, inherited custom
  shells, retries, skips, or wrappers that suppress an exit status.
- Actions are immutable-SHA pinned. Checkout credentials are disabled unless a named
  operation genuinely needs write authority.
- A required result must validate router applicability as well as job status; GitHub
  considers a skipped required check successful.
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
- KeldBot uses `pull_request_target` so fork metadata checks work. It MUST NOT checkout
  PR code or interpolate PR-controlled text into shell/script execution. Re-derive the
  threat model before changing that boundary.
- Bot labels: `needs-template-fix`, `needs-conventional-title`, `size/*`, `type:*`.
  Human-only review labels: the five `gate:*` and twelve `crate:*`; do not claim KeldBot
  inferred them.

## Verification

Run `just ci-router-test`, `just hygiene`, the workflow YAML parser check, and the full
root gate. Report actual GitHub `CI required` result at the exact final head.
