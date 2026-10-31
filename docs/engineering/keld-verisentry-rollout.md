# KELD VeriSentry — source-authenticated CI admission

**Release state (31 October 2026):** trusted workflow is merged into `main`;
production App-required ruleset is created but **disabled** pending backfill and
final deployment qualification. No new server or private repository is needed.

## Identity and protected-main configuration

| Item | Exact authority |
| --- | --- |
| Repository | `gyldlab/keld` (public, GitHub Free organization) |
| Trusted default-branch source | PR [#1253](https://github.com/gyldlab/keld/pull/1253), merged as `c820452ebbcec975f786e69f175b6d74b9b726ee` |
| Dedicated GitHub App | **KELD VeriSentry**, App ID **5550363**, installation ID **177084226**, selected `keld` only |
| GitHub Actions App (untrusted for this gate) | App ID **15368** |
| Expected required check | `KELD VeriSentry` from **App ID 5550363**, not any matching check name |
| Credential | Encrypted `VERISENTRY_APP_PRIVATE_KEY` in `verisentry-publisher` GitHub Actions environment, branch rule **main** |
| Actions execution policy | Repository policy **10173**, active, `.github/workflows/ci-provenance.yml` with only `pull_request_target` allowed; refresh is a **separate** protected-main `push`/`schedule`/trusted `workflow_dispatch` workflow |
| Native App ruleset | [**26261486**](https://github.com/gyldlab/keld/rules/26261486), `KELD VeriSentry trusted App admission`, branch `main`, **disabled until final release** |
| Native bypass actors | `0monish` ID **155816356** and `amishabenramani` ID **130687844**, both `User` with `bypass_mode: pull_request` (never exempt/direct-push) |

The existing protected `main` status checks `gatekeeper`, `title-lint` and
`CI required` are still required with source **GitHub Actions App ID 15368**.
Existing CODEOWNERS, last-push review, existing GitHub review-bypass allowances,
active PR-required ruleset and normal CI remain unchanged.

## Why App identity is a security boundary

A candidate PR can create its own workflow and job named `KELD VeriSentry`.
Both a genuine and a forged **name** may appear green. GitHub's expected-source
rule distinguishes the **GitHub App identity**. An attacker-controlled Actions
job runs as App ID 15368 and must never satisfy a required check whose source
is VeriSentry App ID 5550363.

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

## Trusted periodic backfill and aging protection

The source-only `pull_request_target` workflow runs when the PR changes, not
necessarily when the base `main` branch advances. GitHub required checks can
also become too old after seven days. A protected-default-main-only
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
**35 PASS and 21 DENY**. It loaded no App key and wrote no checks. Before
activation, run this workflow on protected `main` and confirm signed-check
coverage for current mergeable PRs. The `push` trigger refreshes base SHA
changes, and the daily schedule renews aging results without PR-body edits.

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

## Real hosted proof receipts

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

**Additional native required-source canary (no main change):** a temporary
active ruleset required `KELD VeriSentry` from App ID 5550363 on a disposable
branch. [#1331](https://github.com/gyldlab/keld/pull/1331) published a green
**GitHub Actions App 15368** same-name job; GitHub still reported the PR
blocked. An authorized `0monish` PR-only admin-bypass merge was exercised into
that temporary branch with safe [#1332](https://github.com/gyldlab/keld/pull/1332);
protected `main` SHA remained unchanged. The canary ruleset and all its
branches were deleted afterward. The production ruleset remains **disabled**.

GitHub refused a literal 40-hex-only branch name with HTTP 422. A genuinely
distinct-fork hosted probe remains unverified because the Amisha fork's
connected integration returned write HTTP 403. Never mark these as passing
hosted test cases.

## Legitimate CI maintenance without a mutable App PASS

**Use the native ruleset's audited PR-only bypass**, not an additional
secret-owning manual workflow. Rule [26261486](https://github.com/gyldlab/keld/rules/26261486)
allows exactly `0monish` and `amishabenramani` to request an audited PR-only
ruleset bypass for a legitimate protected-CI maintenance merge. Ordinary
contributors cannot do so; no bypass applies to direct pushes. Existing
branch review, CODEOWNERS and other required CI rules still apply. **Warning:**
GitHub's PR-only bypass is an exceptional admin-merge operation and can elect
to bypass other branch protections in that PR, not *only* the VeriSentry
check. Before any bypass, document the independent reviewer, current PR
base/head and why it is a legitimate CI maintenance change; verify
`gatekeeper`, `title-lint`, `CI required` and security review evidence.
Inspect GitHub's rule suite bypass audit and PR merge actor/commit afterward.
Never use bypass for routine green application changes.

An earlier Stage-2 prototype, [#1330](https://github.com/gyldlab/keld/pull/1330),
is deliberately **closed and unmerged**. Independent adversarial proof showed
that concurrent GitHub `workflow_dispatch` APPROVE and REVOKE runs can race:
a delayed old approval could overwrite a newer App FAIL because the Checks
API has no documented atomic conditional PATCH. GitHub repository-wide Actions
concurrency also permits collisions from PR-owned workflows. This was a
confirmed security reason **not** to deploy that prototype. The native
ruleset replaces it using GitHub's own actor identity and bypass audit trail.
Do not reintroduce mutable manual release checks or broaden the bypass list.

## Before activating the required rule on main

1. Preserve the current main protection snapshot and ruleset definition.
   Require the **complete** `bypass_actors` array to contain **exactly** two
   `User` entries: `0monish` (ID `155816356`) and `amishabenramani`
   (ID `130687844`), both with `bypass_mode: pull_request`. Reject every
   additional actor, actor type or bypass mode. Confirm required context
   `KELD VeriSentry` and `integration_id: 5550363` exactly, and check
   enforcement remains `disabled` until qualification.
2. Existing open PRs predate the workflow and may have no VeriSentry check.
   Run the protected-main scheduled/manual refresh **once** after its exact
   source and security tests land. Backfill only from trusted default-main
   verifier bytes, not PR code. Re-fetch head/base/test merge before the App
   check; reject unknown/300+ file lists, protected authority and failed APIs.
   Exclude closed/moved PRs; leave unmergeable PRs blocked, never fake PASS.
   Confirm the protected-main `push` trigger, next daily schedule, first-run
   protection and four-day renewal threshold; test stale-workflow rerun
   rejection against a newer main commit.
3. Qualify distinct-fork isolation when a write-enabled fork is available.
   Reconfirm default-main source, restricted environment, stale PR handling,
   duplicate PR head/merge identity, the forged-same-name scenario and a
   live PR-only authorized bypass. Do not claim absent evidence as verified.
4. Activate **only** ruleset 26261486, keeping existing three checks and
   review rules intact. Immediately inspect actual merge admission of a clean
   App-verified PR, a denied protected PR and an Actions-only fake-green PR.
   Any failure or unexpected mass PR block means **disable the new ruleset**,
   leaving existing main protections unchanged.
5. After successful end-to-end qualification, the owner generates a new App
   private key. Replace the environment-scoped GitHub secret from the local
   file via stdin, revoke the old key, then run fresh benign/hostile checks.

## Safe rollback and key rotation

To roll back this gate, set ruleset **26261486** enforcement to `disabled`.
Do **not** turn off the preexisting main required checks or relax CODEOWNERS.
The automatic verifier may remain deployed but no longer blocks merges unless
its App check is required. If a key is exposed, revoke and rotate the GitHub
App key and replace `VERISENTRY_APP_PRIVATE_KEY` in the restricted environment.
Never print the `.pem` contents or save them in the repository.

Example rotation by the authorized owner, with a freshly generated key:

```sh
gh secret set VERISENTRY_APP_PRIVATE_KEY --repo gyldlab/keld \
  --env verisentry-publisher < /trusted/private/location/NEW_APP_KEY.pem
```

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
