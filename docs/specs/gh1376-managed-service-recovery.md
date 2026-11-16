# Spec: Managed service and recovery authority

- Status: draft
- Tracker: GH-1376
- Owner: Amisha, specification author. Existing runtime, security and state owners retain approval authority.
- Updated: 2026-11-16
- Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

## 1. Goal & non-goals

PROPOSED: Give managed OpenCode one accountable service owner before replacing upstream lifecycle operations.
The owner must prevent concurrent recovery and preserve uncertain effect outcomes.
Keep the complete product denominator from #1377.
This draft does not authorize implementation. B-1 and B-2 remain open.

Non-goals:

- Add another supervisor, permission authority, update agent or database.
- Implement KEL-76 while its STOP remains active.
- Change the required OpenCode commit or accept reduced functionality.
- Treat a controller handoff, endpoint or PID as process custody.
- Claim native-trusted code satisfies a broker-constrained profile.

## 2. Spec refs

FACT: The following owners govern this proposal at the header commit.

- `docs/architecture/02-ipc.md`: IPC identity, contracts and wire review.
- `docs/architecture/03-security.md`: canonical authorization and profile boundaries.
- `docs/architecture/06-runtime-and-tooling.md`: Supervisor and platform custody.
- `docs/specs/kel75-principalized-bun-child-roles.md:244`: role lifetime and shutdown.

FACT: #1379 selects the service ownership question. #1384 owns recovery behavior.
The [existing review handoff](https://github.com/gyldlab/keld/issues/1376#issuecomment-6902783193) records the current deficiencies.
This specification narrows that handoff to one service/recovery seam.
It leaves PTY implementation, browser composition and package updates with their existing owners.

FACT: The required OpenCode source commit is `4423f42836aaaf2933bc7b94b35aca4e5bcce7c3`.
External source references below use that commit. Retrieval date: 2026-11-16.

## 3. Acceptance criteria

### AC-1: Activation has one accountable owner

- **Given:** Two authorized clients request the same installed identity and canonical data root.
- **When:** Both request activation before either observes a service endpoint.
- **Then:** Exactly one service generation obtains management admission. Its writer admission separately satisfies AC-7.
  Both clients discover that generation after readiness succeeds.
  A losing request returns a typed conflict or the admitted service reference.
- **Negative control:**
  - **Single fault:** Remove the activation owner's exclusive admission check.
  - **At-risk precondition:** Both requests pass authentication, policy and artifact validation.
    Observe management admission at its owner before downstream writer arbitration.
  - **Expected RED:** The management witness records two admitted management generations, even if writer arbitration later refuses one.

### AC-2: Connection authority does not administer

- **Given:** A client can connect and use a healthy disposable service.
- **When:** The client requests Stop, Replace, Update or recovery admission without the corresponding management grant.
- **Then:** The owner refuses before changing state. The error identifies the missing grant class without exposing credentials.
  The client's permitted use operation still succeeds.
- **Negative control:**
  - **Single fault:** Remove the management check for one valid Stop request.
  - **At-risk precondition:** The service and request are valid. No other policy refuses the request.
  - **Expected RED:** The connection-only client stops the service.

### AC-3: Recovery waits for predecessor custody

- **Given:** A predecessor owns a session claim and a live tool descendant.
- **When:** Its endpoint disappears and a successor requests recovery admission.
- **Then:** Recovery remains blocked until the custody owner proves termination or complete enforceable fencing.
  A typed refusal identifies the unresolved custody. Endpoint loss and elapsed time do not satisfy the proof.
- **Negative control:**
  - **Single fault:** Replace the custody check with an endpoint-health check.
  - **At-risk precondition:** The successor has valid management authority. The predecessor's effect witness remains live.
  - **Expected RED:** Both generations execute the same claimed operation.

### AC-4: Desktop exit does not stop independent work

- **Given:** The installed service owns authorized background work with an independent progress witness.
- **When:** A window closes, desktop Quit commits, or the desktop crashes.
- **Then:** The service continues the work. Its generation and resource owner remain unchanged.
  Authorized service Stop blocks new admission and begins bounded cleanup.
  A vetoed desktop Quit performs no committed shutdown action.
- **Negative control:**
  - **Single fault:** Route accepted desktop Quit to service Stop.
  - **At-risk precondition:** No other shutdown condition exists. Background work remains authorized.
  - **Expected RED:** The witness stops before an authorized service Stop.

### AC-5: Retired bindings cannot deliver into successors

- **Given:** A request from one client binding completes after that binding retires.
- **When:** Its result reaches a replacement binding that uses the same application-visible resource name.
- **Then:** Delivery refuses the stale generation. The replacement receives no stale state or credential.
  An authorized outcome query can still retrieve a previously committed result.
- **Negative control:**
  - **Single fault:** Remove the receiving-binding generation comparison.
  - **At-risk precondition:** The result otherwise passes identity and schema validation.
  - **Expected RED:** The replacement's observer receives the predecessor's result.

### AC-6: Uncertain effects are not silently repeated

- **Given:** An independent sink records an irreversible effect. The service loses control before committing completion.
- **When:** A successor recovers the claim or a client retries the operation.
- **Then:** Recovery exposes an indeterminate outcome and a reconciliation action.
  It repeats the effect only after authoritative deduplication or deliberate authorization for a new attempt.
  A recorded successful outcome remains successful after client loss.
- **Negative control:**
  - **Single fault:** Automatically classify the indeterminate operation as replayable.
  - **At-risk precondition:** The sink has no deduplication. Recovery reaches the unfinished claim.
  - **Expected RED:** The sink records a second effect without reconciliation or new authorization.

### AC-7: Replacement cannot revive retired write authority

- **Given:** A predecessor still holds a database handle when replacement begins.
- **When:** The successor requests write admission and the predecessor attempts another write.
- **Then:** The owner admits at most one writer generation under the approved state contract.
  If effective fencing is unavailable, successor admission waits for verified predecessor termination.
  A stale binary cannot regain admission through saved registration or credentials.
- **Negative control:**
  - **Single fault:** Treat registration replacement alone as write-authority revocation.
  - **At-risk precondition:** The predecessor can reach its open database handle. Both writes target valid records.
  - **Expected RED:** A retired generation commits a write after successor admission.

### AC-8: Cleanup has an observable terminal result

- **Given:** Service Stop has valid authority and owns live children, streams and pending outcomes.
- **When:** Cleanup exceeds the approved drain or termination deadline.
- **Then:** The owner reports unresolved resources and blocks conflicting replacement.
  It preserves known outcomes and retains cleanup responsibility.
  A timeout does not become a success or proof of absence.
- **Negative control:**
  - **Single fault:** Mark cleanup complete when its deadline expires.
  - **At-risk precondition:** A retained process-identity witness proves one owned descendant remains alive.
  - **Expected RED:** Replacement is admitted while the unresolved resource remains live.

## 4. Design

### D-1: Extend canonical ownership instead of nesting managers

FACT: `RoleOwner` provides Primary and AppBound. Both stop with the host application session.
References: `crates/keld-runtime/src/role.rs:248` and `crates/keld-runtime/src/registry.rs:187`.
FACT: `RoleRecoveryGate` controls role restart after application readiness.
Reference: `crates/keld-runtime/src/role.rs:172`.
INFERRED: That gate does not itself authorize an OpenCode database sweep or prove descendant termination.

FACT: Upstream client management signals a prior process and waits for its PID to stop.
Source: [service.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/client/src/promise/service.ts#L153-L165).
FACT: The core states, "Recovery is at-least-once" and assumes previous claim owners are dead.
Source: [restart.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/session/execution/restart.ts#L47-L62).

PROPOSED: The canonical runtime owner controls the installed service's activation, custody, replacement and Stop.
That owner delegates recovery admission to the existing core only after custody and data-root checks agree.
The adapter replaces upstream managed recruitment and termination at this boundary.
It must not retain a second management loop for the same managed service.
Independent external services keep their own owner and a distinct management mode.
Connecting to an external service cannot silently adopt it.

| Alternative | Disposition and decisive reason |
| --- | --- |
| Canonical installed ownership | Recommended, conditional on B-1/B-2. Reuses runtime policy while preserving independent background work. |
| Fully external management | Credible fallback. Must include clean-machine provisioning, update, recovery and removal under one identified owner. |
| Desktop AppBound backend | Reject for the full denominator. Its lifetime ends with the desktop application session. |
| Unchanged upstream manager nested below KELD | Reject. Independent replacement and recovery decisions can create competing owners. |
| SDK inside privileged desktop host | Reject as the default. It couples crash/lifetime domains without solving independent recovery. |

No alternative wins on unmeasured startup, memory or implementation cost.
Reopen this decision if canonical ownership cannot satisfy native custody with acceptable maintenance cost.
Q-1 selects the platform mechanism before approval.

### D-2: Bind authority to installed identity and generations

PROPOSED: The canonical owner derives installed identity, data-root identity and admitted artifact identity.
Untrusted paths, endpoint strings, saved PIDs and renderer identifiers are not authority.
The authorization owner distinguishes Connect, Use and each management operation.
Management includes activation, adoption, Stop, replacement, update and recovery admission.

The owner captures effective configuration and grants before admission.
The effect owner rechecks revocation and generation before dispatch.
Changed command inputs require new authorization at the existing effect boundary.
The runtime owns process identities and descendant cleanup.
The state owner owns writer admission, schema compatibility and committed outcomes.
The bridge owns client-binding generation checks and delivery retirement.
The updater owns package selection and compatibility under #1385.
It requests activation from the runtime owner.

These owners exchange decisions. They do not duplicate each other's policy.
The service identity must distinguish installation, data root and managed/external mode.
Adoption requires an authenticated instance witness and an approved custody transfer.
Bearer credentials alone do not authorize adoption or management.

Types, manifest identifiers, wire fields and Rust signatures: none allocated by this draft.
Q-2 must resolve the exact typed contract before implementation.
The public API, permission and wire owners review that contract together.

### D-3: Admit recovery only after physical or effective exclusion

PROPOSED transition order:

1. Authorize the management request and bind its inputs.
2. Serialize activation for the installed identity and data root.
3. Retire conflicting client and effect bindings.
4. Prove predecessor custody ended or enforceably excludes all conflicting effects.
5. Admit the selected artifact and writer generation under the state contract.
6. Classify unfinished outcomes before invoking the core recovery entry point.
7. Publish the new service binding only after readiness succeeds.

Each fallible step preserves a recoverable owner record.
A failed activation never restores stale credentials or silently replays uncertain work.
Retries can discover the already-admitted result using the same operation identity.
An admission timeout returns pending or indeterminate status when effects may have occurred.

Native-trusted code can retain ambient process and database access.
Logical generation revocation cannot fence those effects by itself.
For that profile, replacement requires verified termination of all conflicting custody.
For a constrained profile, demonstrate effective fencing at every affected effect owner.
Incomplete fencing blocks replacement. It cannot become a profile downgrade during acceptance.

Windows retained handles and Job/ConPTY custody require Windows evidence.
Unix process and PTY custody require separate macOS and Linux evidence.
UNKNOWN: The current proposal has no qualified installed activation mechanism on all three systems.
Keep that limitation in B-2 and U-1.

### D-4: Preserve outcomes across cancellation and replacement

PROPOSED outcome classes: not admitted, pending, succeeded, failed, cancelled with partial effects, and indeterminate.
Cancellation is a request. An acknowledgement must identify which effects remain possible.
Preserve a successful result if cancellation arrives after its commit.
Do not infer no effect from connection loss, process loss or a response timeout.

The core retains session and operation semantics.
The adapter carries outcome identity and admission evidence across the managed boundary.
It does not implement a second job journal.
The state owner must define durable acknowledgement and retention before Q-3 closes.
Neither exclusive ownership nor SQLite transaction atomicity proves exactly-once external effects.

Migration includes recruitment, discovery, Stop, replacement, startup recovery, writer admission and bridge retirement.
Keep the permanent compatibility facade for upstream callers.
Remove temporary interception only after all managed callers use the canonical owner.
Required external and remote connection flows remain distinct and functional.

## 5. Boundaries

This PR changes only this draft specification.
Implementation candidates are the canonical runtime owner and the OpenCode managed-service adapter.
The existing core recovery entry point and state owner require coordinated changes after approval.

Must not touch runtime source, public contracts, guard policy, KEL-76 implementation, credentials or unrelated owner branches.
Do not replace canonical architecture text before the proposed contract is approved.

## 6. Tasks

### T-1: Resolve the owner contract for approval

- **Blocked by:** Q-1, Q-2, Q-3

Resolve all three questions with the named canonical owners.
Bind their decisions to exact source and native proof plans.
Obtain human specification approval. Retain open implementation blockers.

### T-2: Implement the activation and recovery admission slice

- **Blocked by:** T-1, B-1, B-2

Build an isolated platform slice under the approved runtime owner.
Replace upstream management for that mode without retaining a parallel manager.
Run AC-1 through AC-4 and AC-7 through AC-8 within that isolated fixture.
Do not expose managed activation or recovery to product callers in this task.
T-4 owns exposure after the remaining safety criteria pass.

### T-3: Integrate uncertain outcomes and retired delivery

- **Blocked by:** T-2, B-3

First use the core/state owner's B-3 qualification.
Then connect the outcome owner and canonical bridge to the admission contract.
Run AC-5 and repeat AC-6 through the integrated boundary, including both cancellation/completion orderings.
Preserve all managed and external connection journeys from #1377.

### T-4: Qualify each claimed platform and full product journeys

- **Blocked by:** T-3, U-1

Run all acceptance criteria on each claimed operating system.
Use independent resource and effect witnesses.
Expose managed activation and recovery only after AC-1 through AC-8 pass on that platform.
Complete the retained #1377 journeys before claiming full integration readiness.
Do not use an unrelated application's package or a unit test as product qualification.

## 7. Test plan & evidence

PROPOSED: Use deterministic barriers at admission, effect dispatch and completion persistence.
Avoid sleeps as the test oracle. Every wait has an explicit deadline and diagnostic evidence.
Observe native resources by retained identity rather than PID reuse assumptions.
Keep source versions, host identity, commands, logs and artifact digests with each result.

| Criterion | Required fixture and fault |
| --- | --- |
| AC-1 | Two synchronized activators. Remove exclusive admission in the control build. |
| AC-2 | Allowed Use followed by denied Stop. Remove only Stop's management check. |
| AC-3 | Suspend predecessor with a live descendant. Remove endpoint and registration separately. |
| AC-4 | Background witness across close, vetoed Quit, committed Quit and crash. Then authorize Stop. |
| AC-5 | Hold a result while replacing its client binding. Also query a committed result. |
| AC-6 | Cut after sink acknowledgement and before completion commit. Compare deduplicated and non-deduplicated sinks. |
| AC-7 | Keep the predecessor handle open. Attempt its write before and after successor admission. |
| AC-8 | Keep one descendant alive past cleanup deadlines. Verify refusal and retained cleanup ownership. |

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

## 8. Review gates triggered

Public API, permission model and wire protocol review apply to the proposed contract.
The exact implementation may also trigger unsafe or dependency review.
This documentation change admits no dependency, wire field or capability.
Named owner reviews and human specification approval remain required.
The draft PR is do-not-merge until its approval conditions are satisfied.

## 9. Perf impact

No measured performance claim and no runtime change in this PR.
Implementation must measure activation, readiness, memory, process count and replacement latency.
Record complete product totals separately from the canonical framework metric census.
Queue, retry, retention and cleanup bounds remain part of Q-2/Q-3.

## 10. Open questions, blockers & unknowns

### Q-1: Which installed activation mechanism owns lifetime on each platform?

Compare canonical runtime extension with complete external management.
Recommend canonical ownership if native custody and independent desktop lifetime can be demonstrated.
Name the activation, arbitration, provisioning, update and removal mechanism for each platform.
Runtime and platform owners must approve the selection. This question blocks specification approval.

### Q-2: What exact contract carries admission and bounded failure?

Specify typed identities, grants, operation records, generation comparisons and owner-death behavior.
Define queue, retry, activation, drain and cleanup bounds with measurable failure outcomes.
Prefer existing runtime and bridge primitives where their contracts suffice.
Runtime, guard and bridge owners must approve the signatures. This question blocks specification approval.

### Q-3: What durable outcome contract prevents blind replay?

Define operation identity, input digest, deduplication scope, retention and durable acknowledgement.
Define reconciliation for irreversible effects whose completion is unknown.
Select a failure model that distinguishes process crash from power loss.
State and recovery owners must approve the contract. This question blocks specification approval.

### B-1: Protected implementation approval remains absent

- **Status:** open
- **Owner:** Existing runtime, security, bridge and OpenCode integration owners coordinated through GH-1376.
- **Impact:** Blocks T-2 and T-3. This draft cannot create implementation authority or clear KEL-76's STOP.
- **Check:** Approved exact specification, named gate evidence and explicit clearance for every affected stopped owner.

### B-2: Independent custody has no complete native qualification

- **Status:** open
- **Owner:** Runtime/Supervisor and platform owners under #1379, KEL-75 and KEL-78.
- **Impact:** Blocks installed service activation, predecessor exclusion and replacement on each unqualified platform.
- **Check:** Pass AC-1, AC-3, AC-4, AC-7 and AC-8 with real native resource witnesses.

### B-3: Pinned recovery permits repeated external effects

- **Status:** open
- **Owner:** OpenCode recovery and state owners under #1384 and #1381.
- **Impact:** Blocks product recovery exposure, the no-blind-replay integration claim and T-3.
- **Check:** The core/state owner first passes AC-6 in its isolated outcome fixture after an effect-before-completion failure cut.
  T-3 repeats AC-6 through the integrated boundary. That later repetition cannot substitute for the prior qualification.

### U-1: Platform activation and custody costs remain unmeasured

UNKNOWN: Native feasibility, packaging impact and operating cost for the selected installed owner.
Resolve through the smallest platform custody prototypes after Q-1 and the relevant implementation approval.
If one platform fails, reopen D-1 and compare complete external management without reducing the feature denominator.
