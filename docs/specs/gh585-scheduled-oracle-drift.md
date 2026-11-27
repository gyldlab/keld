# Spec: Parked Electron oracle drift lane
Status: draft
Linear: GH-585 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: parked design only; no schedule activated, do not merge.

## 1. Goal & non-goals

Define how a future scheduled recorder detects drift without silently rewriting the oracle.
A failed comparison must produce an attributable failed job and retained evidence. A reviewer
decides whether behavior is unknown, a baseline needs a justified change, or the recorder failed.

GH585 remains parked. Its pull condition is a second recorded family or a proposed Electron pin
bump, followed by approval. This draft does not change a milestone, workflow or scheduler.
Non-goals: product corpus smoke, broad upstream Electron suite execution, automatic re-recording,
retry-based green results, user messaging, or making this lane a first-proof dependency.

## 2. Spec refs

- [GH585](https://github.com/gyldlab/keld/issues/585) owns the parked lane and failure sink.
- [GH495 recorder](https://github.com/gyldlab/keld/issues/495), [GH494 harness](https://github.com/gyldlab/keld/issues/494) and [GH411 receipt prerequisite](https://github.com/gyldlab/keld/issues/411).
- [GH532 evidence rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh532-first-proof-evidence-rules.md) and [GH566 corpus owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh566-corpus-manifest-owner.md).
- [Current CI workflow](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/.github/workflows/ci.yml) and [CI routing contract](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/.agents/ci.md).
- [Pinned Electron test workflow](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/.github/workflows/pipeline-segment-electron-test.yml).
- [Architecture 01](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/01-overview.md) and [06](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md) govern budgets and supervised processes.

No boundary change or workflow edit in this publication. Upstream source demonstrates macOS
preparation and Linux virtual-display usage, not successful Keld hosted qualification.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given no approved pull condition, GH585 stays parked and no automatic schedule is enabled.
   Activation requires GH495 plus exact spec and workflow-owner approval.
2. Given a scheduled invocation, each lane resolves one immutable Keld source and corpus pin,
   verifies its platform archive and runs the approved recorder once under qualified display/
   environment preparation. No lane silently upgrades the pin or downloads an unverified binary.
3. Given a deliberately altered committed golden, the named oracle-drift job fails and retains
   raw comparison evidence tied to run/job, source, fixture, golden, receipt and runtime identity.
   No commit, PR update or baseline rewrite occurs.
4. Given any retry or rerun count above zero at workflow, command or harness level, routing
   review fails. A transient failure remains a failed attempt with its original evidence.
5. Given failure, timeout or cancellation, child cleanup completes within the approved bound
   and diagnostic upload is attempted independently. Missing diagnostic upload remains visible;
   it cannot turn the original failure into success.
6. Given a proposed new baseline, only a reviewed ordinary PR may update fixture/golden/receipt/
   cell bindings or mark the affected cell unknown. Green scheduled output is never auto-admitted.
7. Given an unqualified platform or display setup, the lane reports qualification failure/unknown
   coverage, never a recorded pass. Activation cannot claim a three-OS lane until all three run.
8. Given ordinary first-proof or offline replay PRs, no scheduled recorder is invoked and no
   dependency on this parked lane is introduced. Any future workflow/router edit has its own
   exact changed-path and required-check verification.

## 4. Design

### 4.1 Trigger and ownership

The issue's two pull conditions are alternatives: reviewed goldens exist for a second family,
or a pin bump is proposed. Neither condition authorizes execution by itself.
After the owner pulls the issue and approves this design, propose a weekly Monday 00:00 UTC
schedule plus a manual qualification entry point. Cadence is a reviewable proposal, not active.

The designated CI single writer owns the future workflow and router edits. GH495 owns recording,
GH494 owns comparison and identities, GH566 owns artifact validation, and family owners decide
cell behavior. The lane supplies evidence and cannot approve its own drift correction.

Reuse the current repository CI ownership/routing machinery. Do not copy its required PR gate
into a recurring workflow or invent a second path classifier. Exact integration is a reviewed
change to the existing owner; ordinary PR offline replay stays separate.

### 4.2 Immutable inputs and platform preparation

Each attempt records immutable Keld commit, corpus identity, manifest and fixture digests,
golden/receipt hashes, Electron tag commit/archive digest and Bun/recorder revisions.
Resolve once before launching lanes. A live branch name alone is not an evidence identity.

The ordinary drift mode re-runs the same approved Electron pin. A pin-bump experiment is an
explicitly labeled candidate invocation; old and candidate identities never share one receipt.
It emits differences for review without changing the approved pin.

| Lane | Preparation to qualify before activation | Current status |
|---|---|---|
| macOS arm64 | Dedicated recorder account; crash-dialog and persistence settings from pinned workflow; restore state | Unexecuted |
| Linux x64 | Verified archive and actual virtual display; relevant sandbox/display permissions | Unexecuted |
| Windows x64 | Verified archive, child-only environment, desktop/process-tree cleanup qualification | Unexecuted |

The pinned upstream workflow runs main tests under xvfb-run on Linux, and uses
CrashReporter DialogType=server plus Electron ApplePersistenceIgnoreState=YES on macOS.
These are source facts only. Its retries and unrelated machine setup are not copied.
The same workflow contains enableRerun=3 on several non-ASAN paths; this lane uses zero.

Use authorized isolated hosted environments. Do not change a user's interactive desktop,
disable a sandbox broadly or introduce unrelated system grants to make a recorder work.

### 4.3 Named failure sink and artifact lifecycle

Proposed workflow display name: `Electron oracle drift`. Proposed per-lane job:
`oracle-drift (<platform>-<arch>)`. The named sink is that failed GitHub Actions job,
its step summary and a linked artifact `oracle-drift-<run-id>-<job-id>`.
The summary links GH585 and the owning family issue; it sends no custom message or issue comment.

Retain the artifact for a proposed 14 days. It includes immutable input manifest, effective
nonsecret configuration, raw output, parsed transcript, exact mismatch report and cleanup status.
Bound output using GH495's approved limits; record truncation as failure rather than hiding it.
Do not upload secrets or broad environment dumps.

If artifact upload fails, preserve a concise failure summary and mark upload failure separately.
A red comparison remains red. The successful upload path is not a substitute for comparator
failure or cleanup evidence. No success report is written before required artifacts are finalized.

Allow only read permissions and narrowly needed artifact upload. No repository-content write
credential, automated commit, auto-PR or baseline update is needed for this lane.
A read-only permissions regression test must reject future unnecessary write scope.

### 4.4 Exactly one attempt and no automatic repair

Each scheduled attempt invokes each fixture once per declared lane. Workflow, shell, network
wrapper and recorder retry/rerun settings must all be zero; a hidden retry inside a helper also
violates the contract. Do not borrow Electron's suite rerun argument.

Manual investigation is a new, separately attributable run, never evidence replacing a failed
attempt in place. Preserve the original attempt and its outcome. A reviewer may diagnose flake,
environment drift, fixture error, product regression or an upstream change; the label alone
does not relax a contract.

Concurrency is one active attempt per immutable corpus/pin/platform. Do not cancel a live run
merely to obtain a newer green attempt. An intentional operator cancellation records cancellation,
executes cleanup and preserves incomplete status. The approval must select and verify the
workflow's actual concurrency behavior rather than rely on an untested YAML sketch.

### 4.5 Reviewed resolution and trust

A correction PR includes old/new raw observations, source identities, fixture differences,
normalizer justification, all affected cells and negative controls. Where the contract is still
unknown, retain unknown. A corpus owner may update the golden only with independent Electron
evidence and its approved rationale, never with Keld output.

Hash matches establish byte identity. They do not prove the correct runtime ran or confer
strict authority. The actual recorder startup witness, mapped execution and admitted receipt
remain required. Existing authority labels and score semantics are unchanged.

Runtime seam: isolated job parent owns subprocesses, display session, buffers and cleanup.
No production grants, bridge channels or callback-lifetime changes. Compatibility fallback:
none for the new lane; existing offline tests keep their original meaning.

## 5. Boundaries

Future work: a designated writer's workflow/router change, GH495 command invocation and bounded
artifact reporting. This publication changes this file only.
Do not activate a cron job, change GH585 milestone, auto-update repository content, run corpus-app
product smoke, edit runtime behavior, or make first-proof completion depend on this parked design.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Confirm the pull condition, obtain exact spec approval and designate the CI writer;
  approve cadence, artifact retention and all-platform qualification ownership.
- [ ] T2 Qualify one-shot record/compare/cleanup on all three actual hosted lanes with the
  approved archive/environment/display identities and visible failed-attempt controls.
- [ ] T3 Implement the read-only failure sink and artifact lifecycle, with altered-golden,
  upload-failure, cancellation and hidden-retry negative controls.
- [ ] T4 Independently review the routing/schedule change, then enable only the approved lane;
  document the ordinary reviewed PR process for any subsequent drift resolution.

## 7. Test plan

| AC | Proof and single decisive mutation |
|---|---|
| 1 | Approval/pull-condition review; enable schedule while issue remains parked |
| 2 | Identity-bound one-shot invocation; replace pin/archive or omit runtime witness |
| 3 | Deliberately alter one golden row; observe failed job/artifact and no repository write |
| 4 | Set any retry/rerun to one, including helper-level retry |
| 5 | Force timeout/cancel/upload failure; census child cleanup and preserve failed status |
| 6 | Attempt automatic baseline commit or accept Keld-generated output |
| 7 | Remove display/qualification witness; prevent cross-platform pass substitution |
| 8 | Existing PR router regression fixtures; scheduled invocation must not appear |

No workflow, schedule, native recorder or automated test ran for this draft.
Qualification uses actual hosted lanes and explicit child/display cleanup; source presence alone
does not meet the criteria.

## 8. Review gates triggered

Publication: none. Activation requires exact human spec approval, the issue's pull condition,
GH495 completion, designated CI single writer and independent routing/permissions review.
Dependency review applies if a new action or package is selected. No wire/public runtime
API/permission-model change is proposed.

## 9. Perf impact

Recurring hosted resource use is unmeasured. Qualification records lane wall time, process count,
peak output/artifact bytes and retention cost before cadence is approved. Finite deadlines and
zero retries are part of correctness; do not omit a platform to reduce measured cost silently.

## 10. Open questions (block activation)

- The issue is parked; confirm a real pull condition and owner approval.
- Approve weekly cadence, 14-day artifact retention and exact concurrency behavior.
- Qualify Linux display, Windows desktop/process lifetime and macOS preparation on real lanes.
- Confirm the existing CI owner's integration without adding this lane to first-proof gates.
