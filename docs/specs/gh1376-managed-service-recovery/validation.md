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
Do not expose managed activation or recovery to product callers in this task.
T-4 owns exposure after the remaining safety criteria pass.

### T-3: Integrate uncertain outcomes and retired delivery

- **Blocked by:** T-2, B-3

First use the core/state owner's B-3 qualification.
Then connect the outcome owner and canonical bridge to the admission contract.
Run AC-5 and repeat AC-6 through the integrated boundary, including both cancellation/completion orderings.
Run AC-11 and AC-13 with the core state owner. Keep T-3 isolated and non-product.
Run AC-15 and AC-18 through actual recovery, subagent and background-wake paths.
Preserve all managed and external connection journeys from #1377.

### T-4: Qualify each claimed platform and full product journeys

- **Blocked by:** T-3

Run all acceptance criteria on each claimed operating system.
Use independent resource and effect witnesses.
B-2 and U-1 gate completion and product exposure, not entry into this qualification task.
Expose managed activation and recovery only after AC-1 through AC-20 pass on that platform.
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
| AC-9 | Race first creation and prepared dispatch separately. Remove dispatch comparison. Verify typed loser and unrelated healthy work. |
| AC-10 | Saturate count and bytes independently. Route Stop through exhausted ordinary execution permits. Observe control progress and terminal fidelity. |
| AC-11 | Compact around unknown outcomes. Retry expired identities. Restore an old snapshot against an independent effect sink. |
| AC-12 | Stop then trigger root activation without Start. Remove stopped-state enforcement. Test disabled registration separately. |
| AC-13 | Hold recovery after reading DispatchIntent. Commit success, then resume recovery. Remove only the transition comparison. |
| AC-14 | Barrier revocation before dispatch and reverse the order. Remove only the current grant comparison. |
| AC-15 | Resume the affected execution with a fresh tool ID. Bypass its unresolved-outcome hold. Verify unrelated progress. |
| AC-16 | Disabled and approval-required registrations. Inject a detached fallback and observe root creation. |
| AC-17 | Hold runtime custody reconciliation. Inject early unregister and observe ordered owner events. |
| AC-18 | Restore a pre-effect snapshot with surviving sink history. Bypass state-owner restore reconciliation. |
| AC-19 | Query and Cancel without their grants. Mutate one action at a time, with authorized positive controls. |
| AC-20 | Retry retired artifact admission with saved registration. Mutate that admission and verify fresh compatible rollback still works. |

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

INFERRED: The reference still lacks typed concurrent first creation and conditional recovery-state updates.
The inspected schedules may expose uniqueness exceptions or overwrite success. They are unrun hypotheses, not deployed product findings.
AC-9 and AC-13 require the actual implementation to distinguish these schedules. E-3 does not establish their acceptance.

## 9. Perf impact

No measured performance claim and no runtime change in this PR.
Implementation must measure activation, readiness, memory, process count and replacement latency.
Record complete product totals separately from the canonical framework metric census.
D-6/D-7 provide experimental bounds. Q-2/Q-3 retain exact bindings and quotas. No product cost improvement is established.

## 10. Open questions, blockers & unknowns

### Q-1: Which installed activation mechanism owns lifetime on each platform?

PROPOSED answer: D-5 selects concrete per-user activation candidates and retains complete external management as the alternative.
Runtime and package owners must still approve exact registration, endpoint identity, custody and lifecycle bytes.
This question remains open until those bindings are reviewable. No user is asked to invent the technical mechanism.

### Q-2: What exact contract carries admission and bounded failure?

PROPOSED answer: D-6 selects identity, authority, atomic dispatch and bounded-failure semantics.
Exact generated signatures, codecs, application authentication and native buffer/resource bounds remain unresolved.
Runtime, guard and bridge owners must approve those bindings before implementation. This question blocks specification approval.

### Q-3: What durable outcome contract prevents blind replay?

PROPOSED answer: D-7 selects one authoritative core outcome policy, explicit uncertainty and crash-versus-power acknowledgement classes.
State owners must still approve the schema, byte quota, completion reserve, pruning and restore contracts.
This question blocks specification approval. Reference-policy results cannot substitute for actual core qualification.

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
