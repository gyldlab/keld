# Spec: Reproducible macOS product benchmark evidence
Status: draft
Linear: GH-180 · Owner: @0monish (exact-content approval pending) · Updated: 2026-11-06
Source baseline: keld 0eb3ce203187f35c5276a4237189c906a5220d5a; keld-benches a3d56a33b4f4a876bb1e5830dbada8113abf583c
Publication: grouped evidence-readiness draft; no measurement or release claim.

## 1. Goal & non-goals

Make a selected macOS product measurement independently reproducible from public fixture/build provenance, actual process
identity, complete raw attempts and the existing statistic owner. A reviewer must be able to identify the exact measured
subject and recompute its result without private notes or reconstructed historic numbers.

Non-goals: build another runner; rename a host-only or Rust-to-Rust measurement as a full app measurement; manufacture missing
raw samples; collect user files; change a frozen schema, budget or engine comparison rule; optimize runtime code or publish a
superiority claim. Historical evidence remains historical even if the current validator becomes stronger.

## 2. Spec refs

- [GH180 ownership and prior evidence](https://github.com/gyldlab/keld/issues/180).
- [KEL64 attribution spec](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel64-macos-startup-attribution.md).
- [Architecture 01 budgets](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/01-overview.md).
- [Public harness contract](https://github.com/gyldlab/keld-benches/blob/a3d56a33b4f4a876bb1e5830dbada8113abf583c/HARNESS-CONTRACT.md).
- [Current result/schema ownership](https://github.com/gyldlab/keld-benches/blob/a3d56a33b4f4a876bb1e5830dbada8113abf583c/schema/README.md).
- [Existing macOS runner lineage](https://github.com/gyldlab/keld-benches/blob/a3d56a33b4f4a876bb1e5830dbada8113abf583c/macos/bench/README.md).
- [Current public measurement inventory](https://github.com/gyldlab/keld-benches/blob/a3d56a33b4f4a876bb1e5830dbada8113abf583c/MEASUREMENTS.md).
- [Existing macOS Rust IPC raw evidence, PR21](https://github.com/gyldlab/keld-benches/pull/21).

Retrieved 2026-11-06: the macOS slot explicitly reuses the preserved KEL64 runner and forbids a replacement. The current
schema owner already has v3 block/session corpora and retained-sidecar checks; GH180's historical schema-gap note is not a
reason to invent v4. The macOS Keld hello product row remains marked not measured in this pinned public inventory. That is a
statement about this row, not a census of every unpublished branch or the user's current physical measurements.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1 — Rebuildable subject

Given a public OS-qualified fixture and selected Keld revision, a clean reproduction creates the exact claimed artifact
class from a complete recorded recipe, locks and toolchain. Bind produced executable/runtime/fixture hashes and declared
configuration to the measured live subject. Replacing the executable after build or omitting an interpreted harness module
must fail provenance eligibility. Source identity alone is not a claim of bit-identical native rebuilds.

### AC2 — Metric and class identity

Given the selected registered metric, records use its declared unit, oracle, cache class, sampling and eligibility policy.
Host-only hello, developer flow, packaged app and installed app remain distinct subjects. Substituting a developer-stage
launch for installed-app startup or pooling cache classes fails admission.

### AC3 — Complete process census

Given an actual app run, the metric's existing census owner identifies Host, supervised Bun and all required engine helpers
with appropriate lifetime/birth facts. Missing or unreadable members stay explicit uncertainty rather than zero usage.
Removing a required helper or double-counting one member fails the census control. PID alone cannot identify a reused process.

### AC4 — Independent paint and clock

Given a startup measurement, only the existing independent paint-opportunity oracle and monotonic clock define the interval.
A log line, load event, window handle or diagnostic stage trace cannot replace it. Broken/absent beacon or loss of the required
foreground condition makes the attempt invalid, retained with a reason. It must not become a fast successful sample.

### AC5 — Attempts and statistics

Given all attempted sessions, accepted/rejected attempts, raw observations, chosen cache class and exclusions are retained.
The existing registry's required valid sample policy and statistic are applied without treating within-session iterations as
independent sessions. Dropping a slow failure, duplicating a valid session or pooling incompatible arms fails eligibility.

### AC6 — Raw binding and recomputation

Given a claimed result, an independent checkout can retrieve each referenced sidecar, verify bytes/digest and recompute the
statistic and uncertainty using the existing owner. A one-byte change, missing sidecar, changed module, reordered digest
chain where order is contractual, or summary without raw data fails. No old median is reconstructed from prose.

### AC7 — Comparable claims

Given a comparison, workload, artifact class, cache policy, engine grouping, OS/hardware, sample policy and metric are
compatible under the harness owner. An incompatible arm is reported separately. A Rust-to-Rust IPC result is not promoted to
Bun, renderer or full-product latency; one OS result is not promoted to another. A mismatched comparison never yields a
budget/superiority verdict.

### AC8 — Safe completion

Given a run's actual owned processes and temporary resources, cleanup follows the existing generation-bound owner and proves
retirement before the next eligible attempt. A still-live member, stale identity or unknown liveness fails cleanup eligibility;
no unrelated PID is signaled. A caller timeout does not prove retirement or authorize broad deletion.

### AC9 — Public reproducibility

Given the final public evidence, the fixture, recipe, raw data, schemas and commands suffice without private research access.
Public evidence contains repository-relative paths and non-sensitive environment facts; redact or re-run sensitive captures
without rewriting historical digests. A required local absolute path or inaccessible private blob fails this criterion.

### AC10 — Honest publication

Given completed measurement execution, publication eligibility is separately computed by the unchanged policy owner. An
exit-zero result with failed provenance, insufficient samples or unknown census is retained but ineligible. Historical data
never gains new qualification through a summary edit. Current source checks and documentation CI establish no native result.

## 4. Design

### 4.1 One measurement owner

No runtime boundary change. KEL64 owns macOS paint/census/cleanup observations; KEL90 owns the cross-OS runner interface;
KEL129 owns the applicable attributable-budget work; keld-benches owns public OS-qualified fixtures, result schemas and
metric registry. Consume their accepted interfaces and existing feature lineage. If a runner must move to macos/bench, its
owner performs that integration with unchanged semantics. This spec does not create a second runner or model its behavior.

### 4.2 Six independent atoms

| Atom | Input to output | Failure and independent falsifier |
|---|---|---|
| Census | Live app identities to required measured process set | Remove a required helper; reject incomplete attribution |
| Work | Committed app fixture to actual requested behavior | Replace the app with a blank/host-only fixture; app arm fails |
| Queue/copy | Existing attributed instrumentation to bounded stage facts | A byte-length counter without observed copy provenance cannot claim a physical copy |
| Clock | Owner start event and independent paint oracle to interval | Substitute load completion or wall-clock adjustment; reject |
| Statistic | Complete raw attempt/session set to summary/uncertainty | Duplicate a session or omit a rejected attempt; policy fails |
| Artifact | Source/build/module/live-image identities to reproducible subject | Swap artifact after manifest creation; binding fails |

Passing one row cannot conceal another's unknown. A native stage trace diagnoses the result; it does not redefine the
independent score oracle. Follow the existing schema version appropriate to the accepted result mode; current v3 already
supports independent block/session sidecars. New metadata or metrics require that owner's review, not a sibling schema.

### 4.3 Execution and evidence path

First identify the preserved runner revision and its owner; inspect unmerged lineage instead of reconstructing it. Freeze the
selected fixture, source/build recipe and one registered metric/class. Execute the owner's controls before a new real-Mac
run. Store complete immutable results in the existing OS-qualified benches locations. The reviewer recomputes the exact
published result and validates provenance before the owning scoreboard is updated. Preserve historical results unchanged.

A clean reproduction is a fresh environment/session executing the public recipe; an independent reviewer need not possess
identical hardware, but cannot pool unlike hardware as one result. Any bit-identical build claim requires separate evidence.
No measurement is requested or run by this documentation publication.

### 4.4 Runtime seam and migration unit

Runtime seam: none. No execution owner, OS grant, handle, callback lifetime, wire or capture policy changes. The consumer unit
is fixture + build recipe + selected live images + existing observer/validator + raw attempts + result references. A temporary
adapter, if needed solely for runner path alignment, belongs to the existing macOS runner owner and is removed when its
standard interface is integrated. No replacement score path remains.

## 5. Boundaries

This PR writes only this spec. Later work belongs to gyldlab/keld-benches under macos/<framework>/<fixture>, its existing
macOS runner lineage, schema owner and immutable result directories. Keld's summary scoreboard may link the accepted result
only through its owner. Do not edit framework runtime, schemas, budgets, private research, other OS fixtures or active native
worktrees in this documentation task.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Reconcile the existing KEL64/KEL90/KEL129 runner lineage, source recipe and one metric/class; record every missing prerequisite and exact native owner.
- [ ] T2 Integrate only required fixture/provenance/interface gaps in the existing benches owners, proving wrong-artifact, missing-module and missing-sidecar controls.
- [ ] T3 Execute the preserved harness controls and complete new real-macOS run with full attempt/census/artifact evidence; preserve invalid attempts as invalid.
- [ ] T4 Have an independent reviewer reproduce and recompute the result, then publish only eligible OS-qualified claims and links; retain all historical evidence unchanged.

## 7. Test plan

AC1/2/6/9 use the existing public schema/semantic checker and a clean recipe execution, with artifact/module/raw substitutions.
AC3/4/8 require real macOS process, foreground, paint-oracle and cleanup observations; a mock proves none of those. AC5/7/10
exercise the existing sample/eligibility policy with omitted, duplicated and incompatible evidence. Retain failing controls,
raw sessions and resulting exact revisions; synchronization uses observable conditions, never sleep-to-success.

Document verification checks ten sections, file limits, source references, independent review and exact GitHub readback.
Local product fmt/Clippy/workspace tests, native benchmarks, hooks and machine closeout are unrun in this documentation lane.

## 8. Review gates triggered

No production boundary change in this draft. Later harness/registry/provenance changes require their existing owners and
applicable public API/dependency gates. A benchmark result needs independent source, statistic and actual-OS evidence review;
this document grants neither result eligibility nor execution/merge authority.

## 9. Perf impact

None measured here. Measurement overhead, invalidation and diagnostic instrumentation follow the accepted metric contract.
No proposed optimization or percentage claim. An optimization starts only from an attributable measured stage and its owner.

## 10. Open questions

1. KEL64/KEL129 must freeze the current preserved runner revision and obtain the actual Mac measurement slot; public main's slot is not the runner.
2. Select the first registered metric and exact artifact/cache class. Reuse the registry's sample policy; do not invent a lower threshold.
3. Any remaining reproducible-artifact or provenance integration gap must be completed by the existing owner before eligibility.
4. Exact-content approval of this draft remains pending. Its existence neither schedules native work nor establishes release readiness.
