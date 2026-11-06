# Spec: Lifecycle recorder and offline differential replay
Status: draft
Linear: GH-495 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: future implementation proposal; documentation only, do not merge.

## 1. Goal & non-goals

Produce attributable Electron lifecycle recordings and replay the same main-process fixtures
under Keld. Each result names its runtime, fixture, host, golden and expected contract. A missing
golden or missing mapped execution cannot become a passing compatibility claim.

The Electron recorder is shared with renderer/preload family fixtures. Keld replay in this slice
is main-process only. Non-goals: implementing BrowserWindow/renderer replay, repairing quit
behavior, creating product corpus receipts, enabling GH585, or refreshing frozen lifecycle v0.

## 2. Spec refs

- [GH495](https://github.com/gyldlab/keld/issues/495), [GH494](https://github.com/gyldlab/keld/issues/494), [GH411](https://github.com/gyldlab/keld/issues/411) and [GH445 lifecycle cell owner](https://github.com/gyldlab/keld/issues/445).
- [GH532](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh532-first-proof-evidence-rules.md), [GH566](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh566-corpus-manifest-owner.md) and [KEL74](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel74-compat-evidence-schema.md).
- [Existing lifecycle integration target](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-compat/tests/electron_lifecycle.rs) is a reuse source, not new recorder evidence.
- [Architecture 06](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md) owns child supervision; [architecture 01](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/01-overview.md) owns budgets.
- [Pinned Electron app documentation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md) and [environment variables](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/environment-variables.md).
- [Owner-attributed Node-mode observation](https://github.com/gyldlab/keld/issues/495#issuecomment-6457989043); no local reproduction is claimed.
- Companion [GH494 draft](gh494-two-arm-oracle-harness.md) owns identity values, packaging, comparison and the proposed differential digest class.

No runtime boundary change in this publication. Future recording consumes the approved GH494
amendment; this file does not independently define a corpus shape or a second receipt validator.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a missing, mismatched or wrong-platform Electron archive, record mode fails before
   publishing a golden; a verified installed archive launches directly and reports Electron
   `44.4.5`, the selected architecture and main-process API identity.
2. Given `ELECTRON_RUN_AS_NODE=1` in the parent environment, the child-only policy removes it
   or refuses recording; a deliberately unfiltered control fails startup identity. Parent
   environment and unrelated OS settings remain unchanged.
3. Given the lifecycle fixture set, record mode captures quit order, before-ready behavior,
   window-all-closed control and first-launch activate relative to ready. Listeners exist
   before the first await. Recordings state the actual first-launch observation.
4. Given a packaged CommonJS preload and static page, the shared recorder retains renderer
   rows with their logical process identity. This does not admit a Keld renderer replay claim.
5. Given a committed lifecycle golden for the actual host lane, replay runs offline and compares
   every mapped main-process fixture. An unqualified or absent lane contributes unknown coverage,
   never a silent skip, copied macOS pass or guessed golden.
6. Given a row-order mutation or undeclared varying field, replay fails deterministically with
   the selected existing diagnostic and offending cell/field. A one-byte manifest mutation
   fails its raw-byte receipt binding even when the fixture-set artifact digest is unchanged.
7. Given a completed replay, records retain `legacy_sandbox_off`, declared engine token at
   tested revision and Electron tag commit; the receipt also binds exact archive, manifest,
   fixture set and golden bytes. An uncited matched cell remains `unknown`.
8. Given one family fixture that cannot link a Keld named export, its arm fails with a KELD
   diagnostic and every independent fixture still receives an observation or explicit failure.
   A link failure cannot vanish behind an empty transcript or a green aggregate.
9. Given timeout, output overflow, parse rejection, abnormal exit or unwinding, each spawned
   child and owned descendant is terminated as necessary and reaped before return. A negative
   control that removes the cleanup path must expose a surviving process and fail.
10. Given a proposed golden refresh, review sees raw observations, runtime identity, fixture
    digest, normalization receipts, exact expected status and independent negative controls.
    The PR route only replays; no recorder runs automatically or uses Keld output as the oracle.

## 4. Design

### 4.1 Current behavior and reuse

Existing keld-compat lifecycle tests and corpus admission provide process/test patterns and
KEL74 integration. They are not evidence that the new Electron recorder, cross-process capture
or complete teardown paths work. GH411 still lacks the repeated-run receipt required by GH494.

Reuse GH494's single comparator and promoted fixture digest, GH566's registered corpus owner,
and KEL74 serialization. Do not create a separate reporter/parser/scorer in this target.
GH1162 supplies future Bun admission provenance; a human `(pass)` line alone is insufficient.

Compatibility fallback: existing lifecycle-v0 files and their historical evidence remain frozen.
New differential recordings use a new registration only after GH494/GH566 approval.

### 4.2 Command modes and process lifecycle

Propose one recorder implementation with explicit `record` and `replay` modes.
Record mode requires caller-selected verified archive installation, fixture set and output
directory. It never installs Electron into a package manifest, resolves `latest`, or uses npx.
Replay mode accepts only reviewed committed goldens and requires no Electron executable.

The parent owns process handles, bounded stdout/stderr readers and the final exit status.
Use a private per-run directory. Do not accept paths that overwrite fixture inputs or another
run. A successful receipt is finalized only after output collection and reap succeed.
Partial or failed captures remain diagnostic artifacts, never committed accepted goldens.

Proposed limits for first qualification: 60 seconds per arm, 8 MiB combined captured output
and 100,000 rows. Timeout is a failure ceiling, not an ordering oracle. Platform qualification
must accept or revise these values before approval; ordinary retries remain zero.
Reader backpressure cannot leave a full pipe preventing termination.

Termination ownership covers the process tree that this command created. Platform-specific
process-group/job containment and kill/reap behavior must be tested on each claimed OS.
A primary child exit does not prove descendants are gone. Never terminate unrelated processes
by executable name or borrow a broad machine cleanup command.

### 4.3 Environment and effective identity

Snapshot a deliberate child environment at spawn. The reviewed policy removes
`ELECTRON_RUN_AS_NODE`; allowlist any additional Electron/Node override needed by a fixture.
Reject unsupported behavior-changing overrides rather than blindly retaining ambient flags.
Record only nonsecret effective settings; do not dump the parent environment.

After spawning, require a startup witness from the fixture: exact Electron version, role and
required main-process APIs. A valid archive hash cannot detect Node-mode execution by itself.
Do not replace the selected executable with a PATH lookup after verification.

For Bun replay, preserve current GH566 admission until the GH1162 amendment is approved.
Its child-only `AGENT=0` proposal addresses visibility and is not identity or provenance proof.
Do not silently apply the old blanket `CLAUDECODE`/AI_AGENT note as a complete current policy.

macOS preparation follows the specific pinned GH494 workflow settings on an authorized recorder
account, restoring prior state afterward. No native preparation was performed for this draft.

### 4.4 Fixture inventory and contract split

| Fixture | Required observation | Contract authority |
|---|---|---|
| Ordinary quit | before-quit, window close path, will-quit and final quit/exit ordering | Cited app contract, GH445/F01 owner |
| Before-ready | listener installation and readiness boundary | Cited app/ESM contract |
| Window-all-closed control | natural last-window close versus an active app quit | Cited app contract; runtime fix remains its owner |
| First launch | whether activate occurs, and its position relative to ready | Record the pin; do not invent undocumented total order |
| Recorder renderer control | preload and static-page rows appear with renderer identity | Recorder capability only, no renderer conformance score |

The first-launch sequence is observed-only wherever the pinned docs do not specify the relation.
A fixture can contain several independently mapped cells with different citation strengths.
Do not turn its entire transcript into a single documented pass.

Each lifecycle recording uses a fresh process and controlled fixture state. A previous macOS
application instance, restored window state or inherited Node mode invalidates recording.
Use event handshakes and explicit operations; waiting longer is not evidence of stable ordering.

### 4.5 Evidence bindings

GH494 defines the proposed new differential shape. Here `artifact.sha256` is its fixture-set
digest, identical to that new denominator's `corpus_sha256`. The exact raw manifest digest is
a separate receipt field. These are deliberately different identities with distinct controls.

The proposed differential recording receipt is an extension owned by GH566, reviewed with
GH494. It binds corpus/shape, raw manifest hash, fixture-set hash, archive hash, tag commit,
npm identity receipt, host platform/architecture, recorder revision, effective options,
golden/raw-transcript hashes, normalization receipt IDs, exit status and successful cleanup.
It also names whether it is an Electron recording or Keld replay; one cannot substitute for the other.

Use `sha256:` evidence references to the committed receipt bytes. No circular hash graph:
receipt fields name input and observation hashes, while records name the receipt; receipt does
not contain its own digest or a digest of those records. A hash establishes byte identity,
not trustworthy production by itself.

GH663's v1 showcase receipt is a distinct owner-supported variant under the same GH566 validator.
It is not copied wholesale into this differential mode, and it retains manifest-byte artifacts.
No KEL74 schema change, invented authority profile or product percentage is introduced.

### 4.6 Failures and reporting

Separate operation observation, harness execution and evidence verdict. A process may fail while
an uncited cell remains unknown; the failed run must still be red and attributable.
A passing regression test for a pending cell records its expected `fail` under GH532.
Cited matching behavior may record `pass` only after exact mapped execution and negative control.

Use the existing error vocabulary where it precisely fits. Diagnostics name corpus, fixture,
arm, platform, stage and remedy without dumping secrets. Missing golden on an undeclared lane
is unknown coverage; missing evidence on a declared required lane is a failed qualification.

Runtime seam: a test parent supervises subprocesses and owns output buffers until reap. No
production transport, OS grant, callback or principal changes. Types and document fields are
test-only and require the GH566 amendment before use.

## 5. Boundaries

Future implementation: keld-compat integration/support code, new differential fixture directories,
GH494/GH566-owned receipt/shape amendment and narrowly reviewed developer command wiring.
This publication adds this file only. Preserve frozen v0, existing v1 manifests, runtime app/quit
semantics, renderer replay, product receipts, dependency manifests and CI schedules.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Land GH411 evidence and obtain exact GH494/GH566 plus this spec approval, including
  run limits, existing diagnostics and real macOS owner.
- [ ] T2 Build verified record mode with environment/runtime witnesses and cleanup controls;
  capture a plain main fixture and renderer-capable recorder control on macOS arm64.
- [ ] T3 Add the four lifecycle fixtures, citations and first-launch observation; commit the
  complete raw/golden/receipt bindings through the one owner.
- [ ] T4 Add offline Keld main-process replay with per-fixture failures and independent sequence,
  digest, unknown and declared-status controls.
- [ ] T5 Qualify the PR replay route and publish exact platform coverage; leave additional
  Electron recorder platforms and scheduled runs to GH585.

## 7. Test plan

| AC | Test and decisive negative control |
|---|---|
| 1 | Known verified archive; wrong digest/platform/version or missing executable |
| 2 | Parent Node-mode variable; intentionally preserve it and require startup rejection |
| 3 | Four separate lifecycle fixtures; delete activate fixture or move listener after await |
| 4 | CJS preload/static page; disable renderer capture and detect missing required rows |
| 5 | Offline host replay; absent lane emits unknown, declared required golden absence fails |
| 6 | Swap before-quit/will-quit; undeclared PID field; one raw manifest byte mutation |
| 7 | Exact receipt/record joins; wrong archive, engine, oracle, fixture or golden hash |
| 8 | Missing named export beside a healthy fixture; assert both outcomes remain visible |
| 9 | Force each terminal path and prove process-tree census is empty afterward |
| 10 | Wrong-arm golden, missing receipt and recorder in PR route are independently rejected |

All are planned, not executed. Negative-control receipts must show the unmodified check fails
and a healthy follow-up still runs. Use deterministic fixtures and causal handshakes, not sleeps.

## 8. Review gates triggered

This draft is documentation only. Future implementation requires exact human spec approval,
GH411 and GH494 prerequisites, GH566 document-format approval, and real platform acceptance.
Dependency review applies only if an npm/Cargo entry is added. CI routing changes need the
designated single writer. No wire or permission contract change is planned.

## 9. Perf impact

No measurements or production cost claim. Qualification records per-arm elapsed time, peak
captured bytes and process count under the same fixture. Preserve finite limits and cleanup;
do not trade away a required lane or increase retries to make measurements green.

## 10. Open questions (block implementation)

- Missing GH411 receipt and raw checksum/npm qualification remain blocking inputs.
- Approve GH494's differential shape and the GH566 recording-receipt extension.
- Approve finite limits and per-platform child-tree containment; select existing diagnostics.
- First-launch ordering and additional recorder OS support remain unknown until real runs.
