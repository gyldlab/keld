# KELD VeriSentry — retired enforcement and diagnostic telemetry

**Current state (1 November 2026):** native VeriSentry merge enforcement is
**retired**. Ruleset **26261486** is **disabled**, not awaiting activation.
The [owner decision](https://github.com/gyldlab/keld/issues/1341#issuecomment-6548280246)
keeps the trusted verifier and App checks as diagnostic telemetry only. Existing
required CI and review protections remain in force. This document preserves the
implementation and historical evidence; it is not a deployment checklist.

Ordinary KELD pull requests do not depend on activating a merge queue or migrating
`ROBOKELD_TOKEN`. Stateless CI input routing landed separately in
[PR #1350](https://github.com/gyldlab/keld/pull/1350): schema v2 has no recorded reader
digests or rebind step. Retirement does **not** solve the broader
[KEL-169](https://linear.app/gyldlab-keld/issue/KEL-169) risk that a candidate workflow
can mint its own CI result; do not describe telemetry as enforced provenance.

## Identity and protected-main configuration

| Item | Exact authority |
| --- | --- |
| Repository | `gyldlab/keld` (public, GitHub Free organization) |
| Trusted default-branch source | PR [#1253](https://github.com/gyldlab/keld/pull/1253), merged as `c820452ebbcec975f786e69f175b6d74b9b726ee` |
| Dedicated GitHub App | **KELD VeriSentry**, App ID **5550363**, installation ID **177084226**, selected `keld` only |
| GitHub Actions App | App ID **15368**; source of the three existing required checks, not the dedicated verifier |
| Diagnostic check identity | `KELD VeriSentry` from **App ID 5550363**, not a matching Actions job name; **not required for merge** |
| Credential | Encrypted `VERISENTRY_APP_PRIVATE_KEY` in `verisentry-publisher` GitHub Actions environment, branch rule **main** |
| Actions execution policy | Repository policy **10173**, active, `.github/workflows/ci-provenance.yml` with only `pull_request_target` allowed; refresh is a **separate** protected-main `push`/`schedule`/trusted `workflow_dispatch` workflow |
| Native App ruleset | [**26261486**](https://github.com/gyldlab/keld/rules/26261486), `KELD VeriSentry trusted App admission`, branch `main`, **retired/disabled** |
| Stored actors in the disabled ruleset | `0monish` ID **155816356** and `amishabenramani` ID **130687844**, both `User` with `bypass_mode: pull_request` (never exempt/direct-push) |

The existing protected `main` status checks `gatekeeper`, `title-lint` and
`CI required` are still required with source **GitHub Actions App ID 15368**.
Strict base freshness, one approving review, CODEOWNERS, last-push approval,
stale-review dismissal, conversation resolution and admin enforcement remain
configured. Force pushes and branch deletion remain prohibited. Existing GitHub
review-bypass allowances and the active PR-required ruleset are unchanged; this
retirement does not authorize bypassing any remaining check or review requirement.

## Verifier identity and the limit of diagnostic checks

A candidate PR can create its own workflow and job named `KELD VeriSentry`.
Both a genuine and a forged **name** may appear green. GitHub's expected-source
rule distinguishes the **GitHub App identity** when that rule is enforced.
The historical source-isolation probes below distinguish App 15368 from App
5550363, but the retired production rule no longer makes that distinction a
merge-admission condition. Authentic check publication alone does not prove that
GitHub will admit or reject the final merge.

`.github/workflows/ci-provenance.yml` runs trusted **default-branch** bytes with
`pull_request_target` and never checks out or executes candidate PR code. Its
read-only verification job binds the live PR to immutable base/head SHAs, a
complete GitHub Compare Commits file list of **fewer than 300** entries, and a
fail-closed protected-path classifier. Protected paths include all workflows,
tooling, formatter/compile configuration and the direct/transitive Linux,
Windows and public-audit CI oracles. Changes to protected CI authority
unconditionally FAIL; no mutable reviewer-comment exemption is in the
production verifier.

A separate publisher job owns the environment-scoped App signing key. Its
repository-scoped installation token requests **Checks write and Pull requests
read only**. It creates the App's `KELD VeriSentry` success/failure check for
the current GitHub-generated PR **test-merge SHA**, never an arbitrary
candidate-supplied SHA. It refuses stale, closed, moved, invalid and
unmergeable PRs, incomplete evidence and API failures rather than publishing a
false PASS. The key is never stored in Git, a candidate PR, caches or logs.

## Retained telemetry refresh implementation

The source-only `pull_request_target` workflow runs when the PR changes, not
necessarily when the base `main` branch advances. GitHub required checks can
also become too old after seven days. That limit is not an additional ordinary
merge requirement for the now-unrequired VeriSentry check. A protected-default-main-only
`.github/workflows/ci-verisentry-refresh.yml` runs on **every protected main
push** (to refresh changed test-merge SHAs promptly) and also **daily at
02:17 UTC** (to renew aging checks). No server or private repository is needed.
Authorized Monish/Amisha may also use `workflow_dispatch` on `main` for an
urgent refresh. Other actors, non-main refs, rerun attempts, and a workflow
SHA that no longer matches the **current protected main HEAD** are refused.
The source/main SHA is checked at run start and immediately before **every**
App check creation to stop stale scheduled runs from signing with a superseded
security policy. The job checks principal and ref at job admission before
minting the App key. It reads GitHub API metadata only, without `actions/checkout`,
PR file execution, caches, shell steps or candidate-controlled workflows.

The refresh holds a repository-scoped App token with Checks write,
Pull requests read and **Contents read** (required for the immutable Compare
Commits API). The direct event publisher's token remains more narrowly scoped
without Contents read. It reuses the automatic verifier's protected-path
classifier byte-for-byte, enforced by hosted Bun negative tests. It verifies
PR number, exact base/head, current synthetic test-merge SHA, full 1–299 file
comparison, rename/status syntax and protected CI authority, then refetches
live PR state **before publishing**. Missing, unmergeable, stale, 300+ and
API-failing PRs never get a fabricated PASS. Any duplicated synthetic merge
SHA is a security-review error, not a shared PASS across PR identities.

Existing same-source App checks younger than four days are reused. Older
checks are renewed from fresh evidence before GitHub's seven-day validity
boundary. As GitHub may return transient `mergeable: null`, the workflow
re-fetches the same PR up to three times (maximum 1.5 seconds explicit
wait) without inventing a source-head PASS.
Unresolved mergeability or missing Compare Commits evidence remains an error
for retry, never a sticky App FAIL against a benign PR. A prior App FAIL is **not silently upgraded** to PASS by a periodic
refresh; a fresh PR event or authorized review is required. Every new check
is verified as App ID 5550363 on the current test-merge SHA.

The first trusted-source dry-run over **56 open `main`-targeting PRs** on
31 October 2026 (not all 152 open PRs across other base branches) classified
**35 PASS and 21 DENY**. It loaded no App key and wrote no checks. This is historical
verifier evidence, not approval to activate enforcement. The retained `push`
trigger refreshes base SHA changes, and the daily schedule renews diagnostic
results without PR-body edits.

To defeat public PR open/close churn, batching is **stable by immutable PR
number**, never by a changing rank or bucket count. With up to 120 open main
PRs, every run processes all. Above 120, exactly five buckets are used:
`PR.number % 5`. On each UTC day, the scheduler selects
`floor(UTC_epoch_days) % 5`, so a given PR cannot be starved by adding or
closing other PRs. A trusted Monish/Amisha `workflow_dispatch` can provide
`batch_index=0` through `4` to backfill all five buckets without waiting.
For example, **501 PRs** give about **100 per bucket**; a 20-day adversarial
480/481 population-churn test showed PR #480 still refreshed every five days.
A cross-bucket duplicate head/base parent pair is denied even if another
PR's synthetic merge hint is temporarily `null`. The 30-minute GitHub-hosted
job timeout and bounded three mergeability attempts keep normal work within
one run. Monitor capacity when open main PRs approach **600**, especially if
many IDs land in one modulo bucket, and adjust scheduling well before a
seven-day check expires. GitHub cron may be delayed or dropped; use the
trusted manual dispatch for immediate recovery, not a PR-authored fake check.

## Historical hosted proof receipts (31 October 2026)

These receipts prove only their stated verifier or isolated-rule observations.
They do not qualify the retired native production gate's positive merge path.

Temporary security PRs were opened against `main`, then closed and their
branches deleted. Genuine App ID **5550363** source and exact check SHA were
verified through GitHub's Checks API, not just color or job name.

| Hostile/benign probe | Expected and verified App result |
| --- | --- |
| [#1326](https://github.com/gyldlab/keld/pull/1326) normal docs | [PASS](https://github.com/gyldlab/keld/runs/124444395064) |
| [#1327](https://github.com/gyldlab/keld/pull/1327) protected workflow + forged green job | [FAIL from VeriSentry](https://github.com/gyldlab/keld/runs/124444413451), even though candidate Actions App ID 15368 emitted success |
| [#1328](https://github.com/gyldlab/keld/pull/1328) 299 changed files | [PASS](https://github.com/gyldlab/keld/runs/124444441233) |
| [#1329](https://github.com/gyldlab/keld/pull/1329) 300 changed files | [FAIL](https://github.com/gyldlab/keld/runs/124444421429) |
| #1327 attempted to replace the candidate-owned `ci-provenance.yml` verifier | [FAIL](https://github.com/gyldlab/keld/runs/124445133963), proving protected-main bytes still won |
| #1326 force-pushed A→B→A | [stale B failed](https://github.com/gyldlab/keld/actions/runs/41169908971); [restored A passed](https://github.com/gyldlab/keld/actions/runs/41169909694) |
| [#1334](https://github.com/gyldlab/keld/pull/1334) genuine existing `amishabenramani/keld` fork PR | [PASS](https://github.com/gyldlab/keld/runs/124462984811) from source App ID 5550363 on fork PR's synthetic test-merge SHA; protected main workflow and environment ran normally |

**Additional native required-source canary (no main change):** a temporary
active ruleset required `KELD VeriSentry` from App ID 5550363 on a disposable
branch. [#1331](https://github.com/gyldlab/keld/pull/1331) published a green
**GitHub Actions App 15368** same-name job; GitHub still reported the PR
blocked. An authorized `0monish` PR-only admin-bypass merge was exercised into
that temporary branch with safe [#1332](https://github.com/gyldlab/keld/pull/1332);
protected `main` SHA remained unchanged. The canary ruleset and all its
branches were deleted afterward. The production ruleset remains **disabled**.

GitHub refused a literal 40-hex-only branch name with HTTP 422; that
specific SHA-like-branch event cannot be tested as such. The genuine external
fork test succeeded **without fork-write access** by opening a temporary draft
PR from an already-existing unchanged public Amisha fork branch: [#1334](https://github.com/gyldlab/keld/pull/1334).
Protected-main [run 41175779113](https://github.com/gyldlab/keld/actions/runs/41175779113)
verified and published [check 124462984811](https://github.com/gyldlab/keld/runs/124462984811)
from App ID 5550363, `conclusion=success`, on GitHub test merge
`bdff52b5587862bf9789889a861d9e70c094b6e5` (not candidate fork HEAD).
It was closed **without merge**, and the fork branch was not modified or
deleted. No additional public fork, server or local worktree was created.

## Why enforcement was retired

The [GH #1341 native-rule investigation](https://github.com/gyldlab/keld/issues/1341#issuecomment-6541527912)
found that ordinary merge attempts evaluated a final commit different from the
previously signed synthetic test-merge commit, despite identical parent commits
and file trees. A genuine App success on the preview therefore did not establish
successful non-bypass admission. The owner retired the requirement rather than
publish a reusable success on the PR's HEAD commit or keep legitimate development
blocked by an unqualified gate.

[PR #1344](https://github.com/gyldlab/keld/pull/1344), the HEAD-publication prototype,
and [PR #1345](https://github.com/gyldlab/keld/pull/1345), the staged merge-queue
candidate, are **closed without merge** under the simplified retirement decision.
Their branches, review findings and test evidence are retained as research, not
release dependencies. Do not deploy the HEAD-PASS prototype: a commit-scoped check
can be reused by another PR or after its base changes.

The earlier [PR #1330](https://github.com/gyldlab/keld/pull/1330) manual-approval
prototype also remains closed and unmerged. Its adversarial proof found an
APPROVE/REVOKE race in which a delayed old approval could overwrite a newer App
failure. That rejection remains relevant; retirement does not validate the design.

## Ordinary maintenance after retirement

Use the existing required CI and independent review process for application,
documentation and CI-maintenance PRs. A diagnostic VeriSentry denial on a protected
path is not an additional required gate while rule 26261486 is disabled. It is also
not permission to ignore a failing required check, unresolved review or security
finding. Do not request an admin merge merely to bypass retired enforcement.

Keep ruleset 26261486 disabled. Do not silently replace it with a required HEAD
check, activate a queue, change credential scope, or remove existing protections.
The stored bypass actors in its disabled definition are historical configuration,
not a new exception to the remaining protections.

A future signed merge gate requires a **separately authorized and reviewed** design.
Exact staged-commit identity, genuine App5550363 positive and hostile-negative
admission, credential isolation, and requeue/replay evidence would be requirements
of that future scope. They are not unfinished activation steps for this retirement
or prerequisites for unrelated KELD merges.

## Retained credentials and incident handling

No credential migration or rotation is required merely to keep the gate retired.
Retain existing bot credentials until their consumers have a separately reviewed
migration; do not delete `ROBOKELD_TOKEN` to close this task. The App signing key
remains in the restricted `verisentry-publisher` environment for telemetry.

If a key is exposed, the authorized owner must revoke and rotate that key and
requalify the affected publisher. Never print its contents or store it in Git,
PR text, logs or caches. Retirement does not relax credential handling.

## Primary GitHub verification references

- [Ruleset availability on GitHub Free public repositories](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/managing-rulesets-for-a-repository)
- [Creating branch rulesets and PR-only bypass permissions](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/creating-rulesets-for-a-repository)
- [Required status check App source identity](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets)
- [GitHub Check Runs REST API and required App scopes](https://docs.github.com/en/rest/checks/runs)
- [GitHub seven-day required-check freshness and test-merge precedence](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks)
- [GitHub scheduled default-branch workflow events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule)
- [GitHub protected-main push workflow events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#push)
- [Trusted `pull_request_target` source semantics](https://docs.github.com/en/actions/reference/security/securely-using_pull_request_target)
- [App installation token inputs](https://github.com/actions/create-github-app-token/blob/main/README.md)
- [GitHub Actions concurrency behavior and cancellation risks](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)
