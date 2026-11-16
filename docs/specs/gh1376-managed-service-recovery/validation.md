# Validation and implementation gates

Part of: [index](index.md)
Owns: Tasks, evidence, performance, open decisions and qualification gates.

## 6. Tasks

### T-1: Resolve the owner contract for approval

- **Blocked by:** none

Resolve all three questions with the named canonical owners.
Bind their decisions to exact source and native proof plans.
T-1 cannot complete until Q-1, Q-2 and Q-3 are resolved and human specification approval is recorded.
Retain open implementation blockers. T-2 still requires completed T-1 and B-1 clearance.

### T-2: Implement the activation and recovery admission slice

- **Blocked by:** T-1, B-1

Build an isolated platform slice under the approved runtime owner. B-2 does not block construction of its own fixture.
Replace upstream management for that mode without retaining a parallel manager.
Run AC-1 through AC-4, AC-7 through AC-10 and AC-12 within that isolated fixture.
Run AC-14, AC-16, AC-17, AC-19 and AC-20 at their owning boundaries.
Run AC-21, AC-25, AC-26, AC-30 and AC-31 for authentication, decoding, enrollment and management ordering.
Run AC-32 with a wrong-image root and an independent first-message witness.
Run AC-33 and AC-34 at the effect-owner release gate and capable-owner registry.
Do not expose managed activation or recovery to product callers in this task.
T-4 owns exposure after the remaining safety criteria pass.

### T-3: Integrate uncertain outcomes and retired delivery

- **Blocked by:** T-2, B-3

First use the core/state owner's B-3 qualification.
Then connect the outcome owner and canonical bridge to the admission contract.
Run AC-5 and repeat AC-6 through the integrated boundary, including both cancellation/completion orderings.
Run AC-11 and AC-13 with the core state owner. Keep T-3 isolated and non-product.
Run AC-15 and AC-18 through actual recovery, subagent and background-wake paths.
Include fork/import dependencies, delayed child completion and initial schedule-check creation in AC-15.
Run AC-22 through AC-24 and AC-27 through AC-29 with the actual core state owner.
Repeat AC-30 and AC-31 through the integrated management and background-execution paths.
Preserve all managed and external connection journeys from #1377.

### T-4: Qualify each claimed platform and full product journeys

- **Blocked by:** T-3

Run all acceptance criteria on each claimed operating system.
Use independent resource and effect witnesses.
B-2 and U-1 gate completion and product exposure, not entry into this qualification task.
Expose managed activation and recovery only after AC-1 through AC-34 pass on that platform.
Qualify the exact integrated artifact. Earlier primitive or reference-policy results cannot clear these gates.
Complete the retained #1377 journeys before claiming full integration readiness.
Consume #1382's terminal handoff proof for replacement, early adoption, expiry and failed replacement.
Hold a pending handoff during early adoption. Bypassing completion must leave observable stale state in the control build.
Retain its healthy terminal-continuity control and explicit failure cleanup. This task does not create a second terminal owner.
Do not use an unrelated application's package or a unit test as product qualification.

## 7. Test plan & evidence

PROPOSED: Use deterministic barriers at admission, effect dispatch and completion persistence.
Avoid sleeps as the test oracle. Every wait has an explicit deadline and diagnostic evidence.
Observe native resources by retained identity rather than PID reuse assumptions.
Keep source versions, host identity, commands, logs and artifact digests with each result.

| Criterion | Required fixture and fault |
| --- | --- |
| AC-1 | Race same and distinct installations sharing one physical root. Mutate arbitration to an installation-specific key. Observe before writer checks. |
| AC-2 | Parameterize all seven management operations and wrong-mode grants. Disable one applicable check per run. Keep an allowed Use control. |
| AC-3 | Suspend predecessor with a live descendant. Remove endpoint and registration separately. |
| AC-4 | Background witness across close, vetoed Quit, committed Quit and crash. Then authorize Stop. |
| AC-5 | Hold a result while replacing its client binding. Also query a committed result. |
| AC-6 | Cut after effect and before completion. Attempt fresh tool-call continuation. Compare certified and absent sink deduplication. |
| AC-7 | Keep the predecessor handle open. Attempt its write before and after successor admission. |
| AC-8 | Keep one descendant alive past cleanup deadlines. Verify refusal and retained cleanup ownership. |
| AC-9 | Race creation and claim separately. Bypass the complete state/version claim predicate. Observe token issuance before release checks. |
| AC-10 | Saturate count and bytes independently. Route Stop through exhausted ordinary execution permits. Observe control progress and terminal fidelity. |
| AC-11 | Compact around unknown outcomes. Retry expired identities. Restore an old snapshot against an independent effect sink. |
| AC-12 | Stop then trigger root activation without Start. Remove stopped-state enforcement. Test disabled registration separately. |
| AC-13 | One helper serves stale recovery and healthy unresolved work. Remove its comparison, then test a separate no-op mutation. |
| AC-14 | Commit intent, revoke before effect-owner consume, then reverse the order. Remove only the current grant comparison. |
| AC-15 | Fresh IDs, automatic fork/import, delayed child completion and schedule-check/run identities. Mutate one causal hold binding. |
| AC-16 | Disabled and approval-required registrations. Inject a detached fallback and observe root creation. |
| AC-17 | Hold runtime custody reconciliation. Inject early unregister and observe ordered owner events. |
| AC-18 | Restore a pre-effect snapshot with surviving sink history. Bypass state-owner restore reconciliation. |
| AC-19 | Query and Cancel without their grants. Mutate one action at a time, with authorized positive controls. |
| AC-20 | Retry retired artifact admission with saved registration. Mutate that admission and verify fresh compatible rollback still works. |
| AC-21 | Admit a valid host and reject a same-account wrong image. Mutate only the application-binding check. |
| AC-22 | Compact detail and request session deletion while retaining retryable upstream work. Mutate binding retention. |
| AC-23 | Saturate actual encoded bytes and reserved receipt bytes independently. Remove only reservation accounting. |
| AC-24 | Lose all owner continuity with a surviving independent effect sink. Bypass only continuity admission. |
| AC-25 | Decode exact golden vectors and single-fault malformed frames. Mutate one canonical-form or length check. |
| AC-26 | Restart the root with complete, incomplete and retired-artifact enrollment tuples. Mutate only tuple completeness. |
| AC-27 | Keep only effect_conflict in memory. Restart the core and inspect authenticated conflict evidence before independent hold/release gates. |
| AC-28 | Race cancellation and claim in both orders. Bypass the complete Prepared/version/null-token cancellation predicate. |
| AC-29 | Complete normal background-job cleanup, then query the retained outcome. Substitute an ephemeral Job-only output pointer. |
| AC-30 | Hold Start across Stop and retry the captured attempt after reply loss. Replace its expected revision with the current revision. |
| AC-31 | Hold the next background effect across desktop Quit. Incorrectly require the retired attachment. Separately revoke its grant. |
| AC-32 | Connect a valid client to a wrong-image root. Inject sensitive first-message content before the authenticated response. |
| AC-33 | Repeat actual-owner consume with the same committed claim. Mutate only Consumed-state handling. Count effect initiations. |
| AC-34 | Race closure and owner enrollment in both orders. Inject an older open snapshot. Observe owner enablement before operation checks. |

### E-1: Existing native custody counterexamples

- **Run:** [Prior exact-head evidence](https://github.com/gyldlab/keld/issues/1376#issuecomment-6902783193), `cargo test --offline --locked -p keld-runtime --lib retained_ack:: -- --nocapture --test-threads=1`.
- **n:** 16 tests passed, including two tests that intentionally demonstrate containment limits.
- **Environment:** Native macOS, header commit, Bun 1.4.4. Executed 2026-11-15.
- **Limits:** This is runtime fixture evidence. It does not qualify installed ownership, full descendant containment or this proposed seam.

### E-2: Proposed seam qualification

- **Run:** UNRUN. The acceptance fixtures require an approved contract and implementation.
- **n:** 0 implementation qualification runs.
- **Environment:** Required separately on native Windows, macOS and Linux at the eventual implementation commit.
- **Limits:** Draft contracts, source analysis and state-machine unit tests cannot satisfy these criteria.

### E-3: Bounded reference-policy challenge

- **Run:** [Q-1/Q-2/Q-3 evidence](https://github.com/gyldlab/keld/pull/1554#issuecomment-6910090717) and [native completion](https://github.com/gyldlab/keld/issues/1376#issuecomment-6911773247).
- **n:** Revised reference: eight positive cases and two intended mutant failures per recorded Linux/macOS suite. Separate race: original two effects, revised one.
- **Environment:** Python subprocesses and file-backed SQLite, native macOS arm64 and cloud Linux. Not KELD/OpenCode production code.
- **Limits:** The fixed epoch, payload-only digest, shared transport and retained expired row do not qualify admission, hostile writers, pruning or power loss.

FACT: E-3 does not test typed concurrent first creation or conditional recovery-state updates.
The separate E-4 experiment exercises those proposed SQL policies and records its own oracle correction.
AC-9 and AC-13 still require the actual implementation to distinguish these schedules.
Neither reference experiment establishes their production acceptance.

### E-4: SQL admission policy and corrected recovery oracle

- **Run:** [Exact v2 source and oracle correction](https://github.com/gyldlab/keld/issues/1376#issuecomment-6914234042), `python3 q3-probe-v2.py`.
- **n:** Two positive policy schedules passed. Two intentionally faulty policies violated their intended invariants.
  A separate no-op mutation failed the shared-helper unresolved-recovery assertion with exit 1.
- **Environment:** Cloud Linux x86_64, Python 3.12.14, SQLite 3.53.1. Executed 2026-11-16.
  First creation used two spawned processes and file-backed SQLite. Recovery used two connections and a fixed adverse order.
- **Limits:** SQL policy model only. No actual KELD/OpenCode, provider, custody, disk-full, power-loss or migration qualification.

FACT: The earlier model allowed a no-op recovery mutation to pass because its healthy control used different SQL.
V2 routes both schedules through the same transition helper. The no-op now fails the healthy unresolved-work assertion.
The earlier first-creation observation remains bounded evidence. The earlier recovery result did not establish a discriminating oracle.
V2 source SHA256: `3a293e55e1e2107fddc3bbd82c3f61b9a368946036cdf597bb2d4fcd377da8b2`.
No-op mutant SHA256: `2b522e3d11cb7f5d095b8acddf6b2c4e2010ffaa574ba55943bdd3ab5e24d622`.
All fixture children exited. Temporary database and mutant state was removed.

### E-5: Takeover reference reruns and acceptance-oracle challenge

- **Run:** [Takeover sources and receipt](https://github.com/gyldlab/keld/issues/1376#issuecomment-6914554873).
  Commands: `node admission_model.mjs`, `python3 first_create_reference.py` and the byte-exact E-4 source.
- **n:** The nominal admission model visits 372 states and 652 transitions. Four deliberate faults produce counterexamples.
  Two first-creation policies pass. Removing creation-key lookup exposes the intended storage exception.
  E-4's two positive policies and two faulty policies reproduce. A separate same-helper no-op mutation exits 1.
- **Environment:** Cloud Linux, Python 3.12.14 and SQLite 3.53.1. Executed 2026-11-16.
- **Limits:** Reference logic only. No shared release registry, causal graph, native authentication, custody or actual core qualification.

FACT: The original AC-9 state-only mutation can remain green because the expected-version comparison still rejects the loser.
FACT: The original AC-28 state-only mutation leaves the null-token comparison false after a successful claim.
Bounded SQLite schedules reproduced both masking cases. The revised controls target the complete owning eligibility predicates.
These are specification-oracle corrections. They do not report a deployed runtime defect.

## 9. Perf impact

No measured performance claim and no runtime change in this PR.
Implementation must measure activation, readiness, memory, process count and replacement latency.
Record complete product totals separately from the canonical framework metric census.
D-8 through D-10 provide proposed limits and record their remaining qualification obligations.
Q-2/Q-3 retain codec allocation, owner approval and storage-continuity dependencies. No product cost improvement is established.

## 10. Open questions, blockers & unknowns

### Q-1: Which installed activation mechanism owns lifetime on each platform?

PROPOSED answer: D-8 details registration, platform authentication, lifecycle ordering and Linux owner-launched host enrollment.
The selected Linux candidate retains bounded kernel enrollment across root replacement and creates a fresh application link.
Runtime and package owners must resolve complete native-trusted descendant custody, especially detached macOS descendants.
Windows transport-to-process binding and Linux retained enrollment still need their specified native feasibility evidence.
Full CLI authority handoff must preserve the #1377 denominator. Unauthenticated activation cannot authorize forwarded commands.
This question remains open at those mechanism and owner-approval boundaries. B-2 separately owns integrated native qualification.

### Q-2: What exact contract carries admission and bounded failure?

PROPOSED answer: D-9 specifies action grants, private execution authority, typed interfaces and bounded release ordering.
It separates attachment retirement from admitted background work and binds management retries to captured control revisions.
The existing generator remains the owner. Shared channel and receiver allocation require its designated writer and exact codec vectors.
Platform application identity consumes Q-1. Native resource measurements remain U-1, not a prerequisite for writing their fixture.
Runtime, guard and bridge owners must approve the final bindings. This question still blocks specification approval.

### Q-3: What durable outcome contract prevents blind replay?

PROPOSED answer: D-10 specifies the existing core's schema, atomic transitions, conflict holds, retention and logical completion reserve.
Runtime, package and state owners must bind durable desired state, control revision, management attempts and removal holds.
Their exact storage location and restart protocol remain unresolved. The outcome schema does not implicitly supply those records.
State owners must resolve the restore barrier outside overwritten state and its coordinated migration/rollback protocol.
The KEL-53 compatibility decision must explicitly admit the proposed schema change before implementation.
Every actual database writer and effect caller needs the specified owner binding. A new parallel journal is not permitted.
This question blocks specification approval. B-3 separately requires actual core qualification before integration.

### B-1: Protected implementation approval remains absent

- **Status:** open
- **Owner:** Existing runtime, security, bridge and OpenCode integration owners coordinated through GH-1376.
- **Impact:** Blocks T-2 and T-3. This draft cannot create implementation authority or clear KEL-76's STOP.
- **Check:** Approved exact specification, named gate evidence and explicit clearance for every affected stopped owner.

### B-2: Independent custody has no complete native qualification

- **Status:** open
- **Owner:** Runtime/Supervisor and platform owners under #1379, KEL-75 and KEL-78.
- **Impact:** Blocks product exposure and T-4 completion on each unqualified platform, not approved isolated fixture construction.
- **Check:** Pass AC-1, AC-3, AC-4, AC-7, AC-8 and AC-12 on the exact integrated artifact with native witnesses.

### B-3: Pinned recovery permits repeated external effects

- **Status:** open
- **Owner:** OpenCode recovery and state owners under #1384 and #1381.
- **Impact:** Blocks product recovery exposure, the no-blind-replay integration claim and T-3.
- **Check:** The core/state owner first passes AC-6 in its isolated outcome fixture after an effect-before-completion failure cut.
  T-3 repeats AC-6 through the integrated boundary. That later repetition cannot substitute for the prior qualification.

### U-1: Platform activation and custody costs remain unmeasured

UNKNOWN: Native feasibility, packaging impact and operating cost for the selected installed owner.
Resolve through approved isolated prototypes and integrated qualification. U-1 gates T-4 completion, not entry into qualification.
If one platform fails, reopen D-1 and compare complete external management without reducing the feature denominator.
