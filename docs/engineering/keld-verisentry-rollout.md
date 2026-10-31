# KELD VeriSentry — source-authenticated CI admission rollout

**Status: source-only verifier/publisher merged on main (PR #1253); App-required enforcement remains OFF while manual-maintenance workflow and recovery are separately tested.**

This is an evidence-led release runbook. Its examples do not authorize silently
relaxing branch protection, bypassing failed checks, leaking keys or issuing
manual App release decisions without independent approval.

## Fixed identities and roles

- Repository: `gyldlab/keld`, protected default branch `main`.
- VeriSentry GitHub App ID: **5550363**; installation ID: **177084226**.
- App installation: selected repository `keld`, with Checks write, Contents read,
  Pull requests read, and implicit Metadata read.
- Required check to be registered only after live qualification:
  `KELD VeriSentry`, **expected source App ID 5550363**, not the shared GitHub
  Actions App ID 15368. Preserve `gatekeeper`, `title-lint`, and `CI required`.
- Active Actions execution policy `10173`: targets only
  `.github/workflows/ci-provenance.yml` and permits only `pull_request_target`.
  It does not, by itself, enable or require a GitHub App security check.
- Environment: `verisentry-publisher`, with a `main` deployment branch rule.
  Required reviewers are OFF for automatic admission; administrator deployment
  bypass is OFF. This is NOT a replacement for code-owner protection of `main`.

## Two-job trust boundary

1. `provenance` executes protected-default-branch workflow bytes using only a
   read-only `GITHUB_TOKEN`. It checks the immutable base/head comparison, refuses
   incomplete (300+) file lists, and refuses ALL changes to CI authority,
   including direct/transitive Linux/Windows WebView executable harnesses,
   C interposers and public audit verifier scripts. A future trusted manual
   release is additive to this automatic default-deny verification.
   It never checks out or executes untrusted PR code.
2. Automatic `publish` needs `provenance` and runs even if verification fails.
   It is the only **automatic** job referencing environment-scoped
   `VERISENTRY_APP_PRIVATE_KEY`; the separately reviewed manual maintenance
   job uses the same protected environment. A pinned App token action and
   `actions/github-script` create or update a `KELD VeriSentry` App check for
   the current GitHub test-merge SHA, bound to the exact PR/base/head. Automatic
   failure, skip, cancellation or missing evidence publishes a **failure**
   when the PR head/base remains current; API/secret failures cannot mint PASS.
   An existing **valid App-authenticated manual release/revocation receipt**
   tied to that exact PR/base/head/merge remains authoritative across metadata
   edits; mixed, malformed or cross-PR release receipts fail closed.
   The publisher retries a pending GitHub test-merge SHA a bounded number of
   times, validating current head/base on every read. If the PR moved, closed,
   changed base, or still lacks a valid test merge,
   the publisher refuses to create a stale check. Job failure alone is not sufficient; the App's check is the
   required source for merge admission.

## CI authority maintenance: default deny + trusted manual exception

The **automatic** verifier rejects changes to `.github/`, `tools/`, build
configs and directly/transitively executed CI oracles. Legitimate maintenance
of those files must not become impossible after the App status is required.

Stage-2 `.github/workflows/ci-verisentry-maintenance.yml` adds a **manual**
GitHub `workflow_dispatch` exception without introducing a server. It executes
only protected-default-branch code (`--ref main`), never checks out PR code, and
may issue an App-authenticated manual authorization **only** if all of these
are true: the authenticated first-attempt trigger actor is Monish or Amisha
(stable GitHub ID checked, including rerun actor); a **different** trusted
reviewer, who is also not the PR author, has approved the exact current head;
the PR has 1–299 fully enumerated changed files including an actual protected
CI-authority path; and PR number, base SHA, head SHA and test-merge SHA match
GitHub's live state. Compare Commits requires temporary Contents **read** as
well as Checks write and Pull requests read. Native latest-push and CODEOWNERS
review rules remain active. An App-authenticated automatic **FAIL** must already
exist on the current test-merge SHA; manual dispatch never mints a PASS without
prior trusted verification.

For a reviewed, current, legitimate protected CI change the maintainer may run:

```sh
gh workflow run ci-verisentry-maintenance.yml --repo gyldlab/keld --ref main \
  -f pull_request=1234 -f expected_base=BASE_40HEX \
  -f expected_head=HEAD_40HEX -f expected_merge=MERGE_40HEX \
  -f decision=approve
```

Values must come from freshly inspected GitHub PR metadata, not an arbitrary
PR comment. This is an auditable **independent release decision**, not a
mutable review receipt: the App check records the authenticated actor, PR,
all three SHAs and the Actions run ID. The same trusted workflow with
`decision=revoke` overwrites the matching App check with **failure**, even
if the PR has since closed or moved. It refuses an older decision when a newer one is visible, a rerun by another
actor, stale inputs or a different PR's maintenance marker. The publisher
updates App-owned check runs instead of continuously creating new ones, so an
untrusted PR title-edit flood cannot consume the same-name check quota. This manual override is a narrow escape route, not a
blanket acceptance of candidate-owned CI.

GitHub review dismissal still blocks normal merging through native branch
protection; maintenance authorization is *not automatically revoked* on review
dismissal. If a maintainer rescinds an exceptional release decision, they must
run `revoke` promptly. **Do not dispatch opposing manual decisions concurrently:**
GitHub Checks updates are not an atomic compare-and-swap register, and
untrusted PR-owned workflows could cancel an Actions concurrency group with a
predictable name. The manual workflow deliberately has no such concurrency
group; only authenticated trusted operators may submit a dispatch, and a run
ID watermark plus fresh PR/review rechecks reject stale decisions when visible.
An untrusted PR cannot trigger the manual decision itself. Prove the complete
approve/revoke/check-source sequence in real hosted runs before making VeriSentry
required. Monish/Amisha's delegated review-bypass settings remain unchanged.

## Credential isolation and post-test rotation

**The GitHub App private key was provisioned on 31 October 2026 as encrypted
`verisentry-publisher` environment secret `VERISENTRY_APP_PRIVATE_KEY`; this
is not a repository-wide secret.** No key bytes were printed or committed.
A non-required real test-merge check ([App ID 5550363 receipt](https://github.com/gyldlab/keld/runs/124432341055))
proved GitHub App authentication and Checks write; the temporary test token
was revoked. This alone does not prove a protected-workflow execution.

**Never put a `.pem` key in the repository, a PR, workflow code, logs,
artifacts, caches, or chat.** After successful end-to-end tests, the owner
will generate a fresh App key, replace the existing environment secret by
reading that new file directly through stdin, then revoke the old key in
GitHub App credentials and remove stale local copies. Example for the owner
(replace the placeholder path):

```sh
gh secret set VERISENTRY_APP_PRIVATE_KEY --repo gyldlab/keld \
  --env verisentry-publisher < /trusted/private/location/APP_KEY.pem
```

This `gh` command must be run from the trusted maintainer's machine. Do not
print or log the key. Confirm environment permissions and execution policy
during rotation. A PR-authored workflow must never have access to this
secret. Local downloaded copies must be secured/rotated appropriately;
filesystem permissions do not isolate a key from agents running as the same
OS user. Only App installation tokens, never the private key, may reach the
GitHub REST API, and tokens expire/revoke.

## Source rollout and hosted evidence

Source-only PR [#1253](https://github.com/gyldlab/keld/pull/1253) merged into
protected `main` at `c820452ebbcec975f786e69f175b6d74b9b726ee` after
latest three-OS CI and independent review. Live temporary PRs all used
protected-default-branch `pull_request_target` bytes and the same App ID
5550363, with direct GitHub check-run receipt links:

- [Benign #1326, App PASS](https://github.com/gyldlab/keld/runs/124444395064)
- [Protected same-name forgery #1327, App FAIL](https://github.com/gyldlab/keld/runs/124444413451) despite a PR-owned Actions `KELD VeriSentry` PASS by App ID 15368
- [299 changed files #1328, App PASS](https://github.com/gyldlab/keld/runs/124444441233)
- [300 changed files #1329, App FAIL](https://github.com/gyldlab/keld/runs/124444421429)
- [Gutted candidate-owned workflow, App FAIL](https://github.com/gyldlab/keld/runs/124445133963)
- A→B→A: [stale B FAIL](https://github.com/gyldlab/keld/actions/runs/41169908971), [restored A PASS](https://github.com/gyldlab/keld/actions/runs/41169909694)

All disposable PRs were closed and temporary branches deleted. GitHub itself
rejected a SHA-only branch name with HTTP 422. Distinct-fork qualification remains
unverified because the available connected fork write permission returned 403;
do not describe that probe as passing. The signed-check expected-source
**merge admission** test also remains mandatory before activating required
branch protection. Preserve existing checks during this rollout.

## Two-stage rollout to avoid a bootstrap deadlock

**Stage 1 — source only (completed):** Exact-head CI and authorized independent
review were green, and PR #1253 was merged to protected `main` through the
existing required checks. The new App check is NOT required yet. This deploys trusted
base-owned verifier code so real hostile PR tests become possible; it is not
production security admission. Do not bypass failed source checks.

**Stage 2 — active admission:** Execute the full live matrix below, including
an exact-head approved maintenance exception, denied self/replayed dispatches,
subsequent revocation and stale App-check exclusion. Do not require this
check until the manual trusted exception is proven live: otherwise future CI
maintenance can be locked out. Only enable required status once
both hostile tests and the maintenance/recovery protocol pass.

## Mandatory live qualification BEFORE enforcing a required check

- Confirm both the automatic and the separately maintained manual-decision
  workflows are present on protected `main` and the automatic workflow's
  `pull_request_target` event policy explicitly allows **only this workflow**.
- Verify job source is the default branch, `GITHUB_REF` is `main`, and the
  environment's main-only restriction is effective for fork PRs.
- Run controlled PRs for benign change, protected workflow gutted, new
  same-name Actions job, SHA-like branch, stale head/base, A→B→A, fork, and
  299/300 file boundary. Confirm App check `success` only for truly admitted
  PRs and `failure` or **absent** for every hostile case. A skipped/neutral
  App check must never satisfy the required merge gate.
- Exercise a legitimate protected-CI PR with a distinct reviewer: the
  automatic verifier must FAIL, the audited authorized manual `approve` must
  update exactly its App check to PASS, and manual `revoke` must return it to
  FAIL (including after close/head drift). Wrong actors, reruns, stale inputs,
  missing reviews and duplicate PR receipts must not mint PASS.
- Check the published check's **App ID 5550363** and `head_sha` (the current test-merge SHA), not only its
  name or green color. Verify no PR-owned job can get the App key or publish
  with that App ID. Verify that CI-authority changes remain rejected without a separately
  qualified trusted maintenance protocol.
- Before changing main protection, evaluate current open PRs: existing PRs
  may not have received a `pull_request_target` VeriSentry event. Backfill or
  re-trigger clean/reviewable checks with a trusted workflow first; never leave
  unrelated branches unable to merge just because a new check was enabled.
- ONLY after live evidence, update branch protection to add required check
  `KELD VeriSentry` pinned to source App ID 5550363 **alongside**, not instead
  of, the existing three required checks. Do not modify the existing delegated
  bypass actors. Finally verify clean and hostile PR merge admission and
  rollback plan. If any proof is missing, do not enforce or merge.

## Rollback

Disable the new required check first (preserving all existing checks), disable
publisher workflow/event policy, revoke the App credential in GitHub Apps, and
rotate environment secret if disclosure is suspected. Existing CI stays intact.

## Primary GitHub references for reviewers

- Trusted default-branch workflow semantics and elevated event risks:
  https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target
- Required status check source and head versus test-merge precedence:
  https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks
- GitHub App authentication inside a GitHub Actions workflow:
  https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/making-authenticated-api-requests-with-a-github-app-in-a-github-actions-workflow
- REST Checks API and permissions:
  https://docs.github.com/en/rest/checks/runs
- `pull_request_target` and environment branch evaluation:
  https://github.blog/changelog/2025-11-07-actions-pull_request_target-and-environment-branch-protections-changes/
- Workflow execution event allowlist and December 2026 default policy:
  https://docs.github.com/en/actions/how-tos/administer/control-workflow-execution

- Manual `workflow_dispatch` runs originate from the default branch:
  https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#onworkflow_dispatch
- Reruns preserve original `github.actor`; verify `github.triggering_actor` and
  require first attempt for privileged maintenance decisions:
  https://docs.github.com/en/actions/reference/workflows-and-actions/variables#default-environment-variables
