# KELD VeriSentry — source-authenticated CI admission rollout

**Status: draft / NO-GO for merge admission until hosted hostile probes pass.**

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
- Intended Actions execution policy: target exactly
  `.github/workflows/ci-provenance.yml`, allow only `pull_request_target`;
  it was created **Disabled** and must remain so until qualification.
- Environment: `verisentry-publisher`, with a `main` deployment branch rule.
  Required reviewers are OFF for automatic admission; administrator deployment
  bypass is OFF. This is NOT a replacement for code-owner protection of `main`.

## Two-job trust boundary

1. `provenance` executes protected-default-branch workflow bytes using only a
   read-only `GITHUB_TOKEN`. It checks the immutable base/head comparison, refuses
   incomplete (>299) file lists, and refuses changes to CI authority unless an
   authenticated maintainer review supplies the exact maintenance receipt.
   It never checks out or executes untrusted PR code.
2. `publish` needs `provenance` and runs even if verification fails. It is the
   **only job** referencing the environment-scoped `VERISENTRY_APP_PRIVATE_KEY`.
   A fully pinned `actions/create-github-app-token` action mints a repository-
   scoped installation token, which `actions/github-script` uses to create the
   `KELD VeriSentry` App check **for the GitHub test-merge commit bound to the current PR base/head**. A failed,
   skipped, cancelled, or missing verification result publishes a **failure**
   when the PR head/base is still current. API/secret problems leave **no PASS**.
   If the PR moved, closed, changed base, or lacks a valid test merge,
   the publisher refuses to create a stale check. Job failure alone is not sufficient; the App's check is the
   required source for merge admission.

## Authorized CI-maintenance receipt

A change to a protected CI file requires an **APPROVED GitHub PR review** from
`0monish` or `amishabenramani`. The review must be the reviewer's latest review,
not dismissed or self-authored, on the exact PR head, with the **entire review
body** set to this line (replace the two values with immutable 40-hex SHAs):

```text
KELD-VERISENTRY-CI-MAINTENANCE base=<BASE_SHA> head=<HEAD_SHA>
```

The checker re-fetches the PR after reviewing the approval evidence. A review
for an earlier base or head is never transferable. This programmatic approval
is additive to existing protected-branch, CODEOWNERS, and independent review
rules; Monish/Amisha's intentionally authorized merge bypass is untouched.

## Credential isolation

**Do not put a `.pem` key in the repository, a PR, a workflow `env` expression,
logs, artifacts, caches, or chat.** After the workflow and protected-main branch
admission are independently reviewed, a trusted maintainer can load the private
key **directly from a local file via stdin** into the GitHub Actions environment
secret, not a repository-wide secret. Example (replace the placeholder path):

```sh
gh secret set VERISENTRY_APP_PRIVATE_KEY --repo gyldlab/keld \
  --env verisentry-publisher < /trusted/private/location/APP_KEY.pem
```

This `gh` command must be run from the trusted maintainer's machine. Do not
print or log the key. Confirm environment permissions and execution policy
before provisioning. A PR-authored workflow must never have access to this
secret. Local downloaded copies must be secured/rotated appropriately;
filesystem permissions do not isolate a key from agents running as the same
OS user. Only App installation tokens, never the private key, may reach the
GitHub REST API, and tokens expire/revoke.

## Mandatory live qualification BEFORE enforcing a required check

- Confirm the new workflow is present on protected `main` and its
  `pull_request_target` event policy explicitly allows **only this workflow**.
- Verify job source is the default branch, `GITHUB_REF` is `main`, and the
  environment's main-only restriction is effective for fork PRs.
- Run controlled PRs for benign change, protected workflow gutted, new
  same-name Actions job, SHA-like branch, stale head/base, A→B→A, fork, and
  299/300 file boundary. Confirm App check `success` only for truly admitted
  PRs and `failure` or **absent** for every hostile case. A skipped/neutral
  App check must never satisfy the required merge gate.
- Check the published check's **App ID 5550363** and `head_sha` (the current test-merge SHA), not only its
  name or green color. Verify no PR-owned job can get the App key or publish
  with that App ID. Also verify legitimate CI changes using the review receipt.
- ONLY after live evidence, update branch protection to add required check
  `KELD VeriSentry` pinned to source App ID 5550363 **alongside**, not instead
  of, the existing three required checks. Do not modify the existing delegated
  bypass actors. Finally verify clean and hostile PR merge admission and
  rollback plan. If any proof is missing, do not enforce or merge.

## Rollback

Disable the new required check first (preserving all existing checks), disable
publisher workflow/event policy, revoke the App credential in GitHub Apps, and
rotate environment secret if disclosure is suspected. Existing CI stays intact.
