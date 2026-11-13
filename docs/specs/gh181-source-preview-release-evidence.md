# Spec: Source-preview release scope and evidence admission
Status: draft
Linear: GH-181 · Owner: @0monish (scope and exact-content approval pending) · Updated: 2026-11-06
Source baseline: 0eb3ce203187f35c5276a4237189c906a5220d5a
Publication: evidence-readiness draft; creates no release, tag or distribution.

## 1. Goal & non-goals

Define the evidence that a maintainer must review before offering an honest source-preview release. The smallest proposed
scope is one immutable source revision with a tested create/doctor/dev quick-start on explicitly claimed OS/architectures.
Binary distribution is a separately selected scope with its existing packaging/authentication prerequisites.

Non-goals: publish a release or tag, run signing/notarization, choose a new trust root, promise unsupported platforms, mark
preview security as complete, assign another owner or merge code. A source preview is not an installed app or no-Rust binary
package. This draft does not broaden the user's documentation authorization into release publication.

## 2. Spec refs

- [GH181 release scope and authenticated-channel decisions](https://github.com/gyldlab/keld/issues/181).
- [GH181 source-preview recommendation](https://github.com/gyldlab/keld/issues/181#issuecomment-5691560250).
- [GH181 unresolved release-owner ranking](https://github.com/gyldlab/keld/issues/181#issuecomment-6471208023).
- [Project foundation GH167](https://github.com/gyldlab/keld/issues/167); its linked security, contributor and acceptance work remains independently owned.
- [Architecture 06 packaging/update owners](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md).
- [Windows expected-identity container](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel254-expected-identity-container.md).
- [Current development guide](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/onboarding/05-development-guide.md).
- [Current product status source](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/engineering/product-status.tsv).

The 2026-11-06 issue read still contains an unresolved ranking between KEL53 feed/verifier ownership, KEL103 release workflow
and this public release tracker. This draft preserves that decision; it does not silently declare the recommendation
approved. The Windows container consumes authenticated unsigned packaging-input Host bytes before app identity embedding;
this does not make a generic source-preview archive an authenticated binary distribution channel.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1 — Frozen explicit scope

Given a release candidate, its reviewed scope names source-preview or binary distribution, exact source commit, claimed
OS/architectures, artifact kinds, supported entry path and known exclusions. Removing a scope field or using all-platform
language when only one OS qualified fails readiness. Approval is bound to that scope and revision, not a moving branch.

### AC2 — Source completeness

Given a source-preview archive/checkout, an independent clean environment obtains the declared files, locks and dependencies
through the public recipe. Uncommitted local patches, missing required generated source or private absolute paths fail the
quick-start. Changes to any admitted input invalidate the old evidence rather than inheriting its result.

### AC3 — Actual developer workflow

Given each claimed native platform, the documented build, create, doctor and dev commands produce the original generated app,
real native window and supervised runtime. A successful compile, mock window or already-running unrelated app fails the
native observable. Record exact tools/runtime/engine versions and prerequisites with the tested command transcript.

### AC4 — Lifecycle and recovery limits

Given the original generated app, native close and the claimed shutdown/relaunch/recovery paths are exercised with the
existing process/stage owner. A missing shutdown result, surviving required descendant or unknown cleanup state blocks any
claim that depends on it. If a known limitation is accepted for a narrower preview, that exact supported route and limitation
must be in the maintainer-approved scope and notes; it cannot be silently called fixed.

### AC5 — Security gate by declared scope

Given blocking security/governance findings for the chosen scope, readiness includes their actual resolution or an explicitly
approved scope exclusion that removes the affected claim and path. A documentation edit cannot convert an exploitable
blocking defect into accepted readiness. Preserve the security owner's disposition and reporting process; do not expose
private vulnerability detail in public release material.

### AC6 — Notes match evidence

Given the candidate revision, release notes and quick-start distinguish implemented, tested, target and known-limited
behavior. Every claimed platform/workflow points to the exact retained evidence. A planned API, stale PR head or draft spec
presented as shipped behavior fails the notes check. Performance and compatibility claims consume their own admitted evidence.

### AC7 — Binary admission remains separate

Given a requested binary artifact, its actual package/update owners must provide build provenance, checksum/authentication,
app identity, executable modes/links, applicable signature decisions and clean-machine install/launch/rollback evidence.
A source-only run, matching unauthenticated checksum or dev-stage executable never passes that binary gate.

### AC8 — Windows host identity order

Given a claimed Windows packaging-input Host, its selected authenticated release source is validated before ExpectedAppIdentity
is embedded, and the app publisher's signature follows embedding. A missing/mismatched digest, unauthenticated channel or
substituted input fails before embedding. This criterion consumes the container spec; it does not choose its unresolved feed
publisher/trust-root owner or implement a new verifier.

### AC9 — Repeatable support and rollback

Given the declared candidate and its documentation, an independent operator can reproduce the supported entry path and the
scope-appropriate failure/rollback procedure. A source checkout may revert/rebuild an identified previous revision; an
installed updater must use its reviewed package rollback contract. Mixing those procedures or omitting the limitation fails.
No rollback operation is executed on user data by this draft.

### AC10 — Final authorization and immutable publication

Given a fully reviewed candidate, publication occurs only after the maintainer explicitly approves that exact scope and
artifacts. Record tag-to-commit/artifact identity and perform readback after publication. A changed candidate, missing artifact,
or prior documentation authorization cannot satisfy this gate. Until then, candidate notes and this spec stay draft.

## 4. Design

### 4.1 Owner and decision model

No runtime boundary change. GH181 owns public release scope and evidence assembly. Existing KEL103/KEL137/KEL141 packaging
owners and the KEL53 feed/verifier contract retain their accepted responsibilities; the outstanding ownership ranking is
recorded as unresolved. This draft chooses no new release service, key, trust root, manifest parser or deployment workflow.

The source-preview recommendation reduces the initially claimed distribution surface, but is still a proposal for the
maintainer. It does not erase foundation blockers or approve known unsafe behavior. A release is warranted by useful verified
scope, never by an application checklist or the existence of a tag command.

### 4.2 Evidence matrix and independent atoms

| Atom | Existing owner | Input/output and first falsifier |
|---|---|---|
| Scope | GH181 maintainer | Approved OS/artifact/entry path to claims; an unclaimed OS in notes fails |
| Source | Repository/build owners | Immutable source/locks to rebuildable candidate; omitted patch fails |
| Native behavior | Claimed platform owners | Actual create/doctor/dev/close path to retained evidence; mock-only result fails |
| Security | Existing security/permission owners | Applicable findings to reviewed disposition; unresolved in-scope blocker fails |
| Distribution | Existing package/feed owners | Verified artifact/trust/identity to usable binary; unauthenticated bytes fail |
| Publication | Maintainer | Exact approved candidate to immutable release; changed source invalidates approval |

A passed source build cannot pass native behavior or binary authenticity. A scope change restarts only affected acceptance
and review, retaining prior evidence as historical. Failed attempts remain attached to the candidate's investigation.

### 4.3 Source-preview candidate contents

Use existing repository docs and the documented command owner. Prepare a candidate source revision, changelog derived from
landed differences, exact prerequisites, tested quick-start, support boundaries and known issues. Identify all dependencies
whose acquisition is required; source preview does not imply offline or no-Rust operation. Attach only artifacts that their
own acceptance actually qualified. No installer placeholder or untested binary belongs to a source-only candidate.

### 4.4 Distribution escalation

Before adding binaries, reconcile the authenticated-channel owner decision on GH181 with the actual KEL53/KEL103 records.
Then consume the selected existing verifier and packaging workflow. Workflow permissions, immutable dependency pins, software
bill of materials and provenance, platform signing and revocation follow that owner review. This is an escalation condition,
not a new implementation schedule or a substitute signing architecture.

Runtime seam: none. No changed handles, callbacks, principal identities, wire or permissions. Migration unit is candidate
source + scope + public recipe + retained per-platform results + reviewed notes + eventual artifact references. Existing
quick-start documentation remains canonical; a release note links it instead of maintaining a competing command guide.

## 5. Boundaries

Publish only this draft specification and associated issue/PR handoff. Later release-note and workflow work must use existing
repository owners, read current claims and be explicitly scoped. Do not write tags, GitHub releases, npm packages, signing
keys, secrets, trust-root configs, shared schemas, native packaging source or production policy in this documentation task.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Obtain maintainer scope selection and reconcile foundation blockers plus authenticated-channel ownership where binary scope needs it; freeze the candidate identity.
- [ ] T2 Produce the public source recipe and notes from current landed behavior, then independently execute create/doctor/dev and claimed lifecycle on each selected real OS.
- [ ] T3 Resolve failed in-scope checks with the existing owners or submit an explicit narrower candidate for scope review; repeat every affected test against final bytes.
- [ ] T4 Assemble exact-revision evidence, known limits, support/rollback and any separately qualified binary receipts for independent release review.
- [ ] T5 Request explicit final publication authorization only for that concrete candidate; after authorization, publish and verify identities using the existing release path.

## 7. Test plan

AC1/2/6 use candidate-source and documentation checks plus clean recipe replay, including stale revision/missing input controls.
AC3/4 are real native workflow/lifetime checks; inherited CI compilation is not that proof. AC5 consumes security-owner
resolution evidence. AC7/8 use existing package/feed/identity negative controls and real clean-machine binary acceptance only
if binary scope is selected. AC9 independently exercises the appropriate recovery procedure in disposable resources.
AC10 checks exact reviewed identity before action and reads back the published artifact only after explicit authorization.

No release, native workflow, package/signing or rollback test ran in this documentation task. Ten-section/limit/source checks,
independent review and exact GitHub readback verify the draft only. Local product fmt/Clippy/workspace tests, hooks and machine
closeout are unrun; publication remains a future separately authorized action.

## 8. Review gates triggered

No production API/permission/wire/dependency/unsafe change in this draft. Actual release workflow or packaging changes retain
their applicable gates, named security/architecture evidence and maintainer scope/publication decision. This spec does not
amend the standing authority of any release or package owner.

## 9. Perf impact

None. Release notes may use only separately qualified, attributable measurements and admitted compatibility evidence. A
source-preview label creates no size, speed or percentage claim.

## 10. Open questions

1. Maintainer must choose source-preview versus binary distribution, exact claimed OS/architectures and useful runnable scope. Recommendation: start with the smallest qualified source-preview scope.
2. GH181/KEL53/KEL103 must reconcile the recorded authenticated-channel ownership ranking before binary publication. No new trust choice is made here.
3. Native owners must supply final-revision clean-environment and lifecycle evidence; applicable security/foundation blockers remain independently open.
4. Exact-content approval and final release authorization are separate. Neither is supplied by this draft or the request to create specifications.
