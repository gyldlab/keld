# KELD VeriSentry — source-authenticated CI admission rollout

**Status: source-only verifier/publisher merged on main (PR #1253); App-required enforcement remains OFF while manual-maintenance workflow and recovery are separately tested.**

This document is for the KEL-169 reviewer and release owner. It is not permission
for an agent to install a new credential, relax protections, or merge PR #1253.

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
   incomplete (>299) file lists, and refuses ALL changes to CI authority,
   including direct and transitive Linux/Windows WebView executable harnesses,
   C interposers, and public audit verifier scripts. An authorized maintenance
   exception is **not** active because GitHub review revocation does not
   automatically trigger a protected verifier rerun.
   It never checks out or executes untrusted PR code.
2. `publish` needs `provenance` and runs even if verification fails. It is the
   **only job** referencing the environment-scoped `VERISENTRY_APP_PRIVATE_KEY`.
   A fully pinned `actions/create-github-app-token` action mints a repository-
   scoped installation token, which `actions/github-script` uses to create the
   `KELD VeriSentry` App check **for the GitHub test-merge commit bound to the current PR base/head**. A failed,
   skipped, cancelled, or missing verification result publishes a **failure**
   when the PR head/base is still current. API/secret problems leave **no PASS**.
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
(stable GitHub ID checked, including rerun actor); an independent trusted
reviewer, different from the PR author, has approved the exact current head;
and explicit PR number, base SHA, head SHA and test-merge SHA all match GitHub's
live state. The latest-push and CODEOWNERS GitHub branch rules remain active.

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
if the PR has since closed or moved. It refuses an older decision overwriting
a newer one, a rerun by another actor, a stale input or a different PR's
maintenance marker. This manual override is a narrow escape route, not a
blanket acceptance of candidate-owned CI.

GitHub review dismissal still blocks normal merging through native branch
protection; maintenance authorization is *not automatically revoked* on review
dismissal. If a maintainer rescinds an exceptional release decision, they must
run `revoke` promptly. Prove approve/revoke and check-source enforcement in
real hosted probes before making VeriSentry required. Monish/Amisha's
intentionally delegated review-bypass allowances remain unchanged.

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

## Two-stage rollout to avoid a bootstrap deadlock

**Stage 1 — source only:** After exact-head CI and authorized independent
review are green, merge this PR to protected `main` through the existing three
required checks. The new App check is NOT required yet. This deploys trusted
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
