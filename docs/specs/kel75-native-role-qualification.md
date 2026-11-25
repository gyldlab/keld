# Spec: native application-role qualification

Status: draft
Tracker: KEL-75
Owner: GYLDLAB
Updated: 2026-11-16
Pin: 1fff0fbc8f6d214b63d865181bb0d483c6fed670

This pin records the pre-correction PR snapshot reviewed on 2026-11-16.
It is not a reconstructed baseline for the original 2026-09-17 design.
T9a must reconcile the governing contracts against its implementation baseline before approval.

## 1. Goal & non-goals

Keld must allow application logic to remain TypeScript/Bun, move selected logic to a managed native Rust role,
or eventually use a Rust-only application primary without forking or recompiling the Keld host. The observable
goal is runtime substitution behind the same host-owned identity, lifecycle, permission intent and renderer
contract. This specification deliberately proves one second execution consumer before generalizing the
framework.

Non-goals:

- no Node, Deno, Python, Go, C#, Zig, WASM or arbitrary executable support claim;
- no runtime-plugin marketplace, public execution-provider trait or competing lifecycle owner for the same role;
- no new KIPC wire version/opcode, alternate permission engine or direct renderer-to-role transport;
- no in-host Rust application code disguised as a confined backend;
- no shared-memory lane, generic resource governor, distributed transaction system or exactly-once IPC claim;
- no blocker on KEL-139/KEL-142's minimum TypeScript/Bun product path.

## 2. Spec refs

Governing contracts:

- `docs/architecture/01-overview.md` §§1–4: host authority, process ownership, principals and current Bun application family;
- `docs/architecture/02-ipc.md` §§1–7: authenticated app-link, typed payloads, validation, routing and lifecycle failure semantics;
- `docs/architecture/03-security.md`: generated host-enforced default deny and principal-bound decisions;
- `docs/architecture/04-electron-compat.md`: compatibility is a facade over generic Keld primitives;
- `docs/architecture/05-webview-and-native.md`: in-host native extensions join the host TCB;
- `docs/architecture/06-runtime-and-tooling.md` §1: existing Supervisor/ChildPreparer/GenerationLease ownership and Bun boot contract;
- `docs/specs/kel75-principalized-bun-child-roles.md`: approved role-generation, restart and revocation state machine;
- KEL-78 owns real-OS containment; KEL-98 owns shared contract/codegen qualification.

Architecture 01 and its onboarding mirror distinguish the runtime-neutral ownership
principle from current Bun/TypeScript support. Native application roles remain a draft
target; Bun remains the default and Electron-compatibility runtime. Current CLI/config
syntax, source-demo instructions and product-status evidence remain unchanged because
this draft introduces no public native declaration or implemented native-role support.
Promotion to `approved` still requires exact-head review and the B-2 document gates.

**No boundary change.** Handle ownership, crash ownership and principal minting remain
with their existing owners. This documentation does not admit a native artifact or
authorize T9b; the draft, B-1/B-2 and implementation-entry gates remain in force.

## 3. Acceptance criteria

### AC-1

- **Given:** the existing Bun primary.
- **When:** the native-role implementation is absent.
- **Then:** every current Bun lifecycle, renderer, guard and packaging acceptance continues unchanged; the native slice is additive.
- **Negative control:**
  - **Single fault:** Require a native artifact before launching the existing Bun primary.
  - **At-risk precondition:** Use a Bun-only package with no native-role artifact or toolchain.
  - **Expected RED:** The previously passing Bun startup acceptance fails at that new prerequisite.

### AC-2

- **Given:** one declared native role.
- **When:** the host starts it.
- **Then:** the existing Supervisor preparation path owns spawn/restart/backoff/reap and the existing generation owner mints endpoint/token/principal state; no second restart or identity owner exists.
- **Negative control:**
  - **Single fault:** Enable a second restart owner for the same failed role generation.
  - **At-risk precondition:** Crash one authenticated native child with a positive restart budget. Both owners reach independently eligible spawn decisions.
  - **Expected RED:** The owner/attempt trace records a second spawn decision for one retirement. Later admission deduplication or cleanup cannot hide this violation.

### AC-3

- **Given:** a child-supplied role name, principal, grant, runtime feature or trace identifier.
- **When:** it authenticates.
- **Then:** none of those fields can select or enlarge authority; host-bound declaration/generation metadata remains authoritative.
- **Negative control:**
  - **Single fault:** Use the child's claimed principal instead of the host-bound principal in authorization.
  - **At-risk precondition:** An authenticated low-authority child claims a known higher-authority principal and requests its restricted operation.
  - **Expected RED:** The denial oracle observes the unauthorized effect or accepted result.

### AC-4

- **Given:** the same versioned application contract implemented once by Bun/TypeScript and once by Rust.
- **When:** a renderer invokes the qualified operation.
- **Then:** both implementations produce the same accepted values, typed errors, permission result and ordering through the host.
- **Negative control:**
  - **Single fault:** Change one Rust result value for a shared accepted contract vector.
  - **At-risk precondition:** Invoke that vector through the same renderer and host route for both implementations.
  - **Expected RED:** The literal golden-vector comparison fails for Rust while Bun remains the positive control.

### AC-5

- **Given:** malformed, truncated, out-of-range, non-scalar or version-incompatible contract input on an authenticated link.
- **When:** the role/host decodes it.
- **Then:** the request fails with the registered typed error before the application handler observes a lossy value.
- **Negative control:**
  - **Single fault:** Bypass rejection of one out-of-range integer before handler dispatch.
  - **At-risk precondition:** Send that authenticated malformed vector with a handler-entry receipt armed.
  - **Expected RED:** The test observes handler entry or lacks the required typed decoding error.

### AC-6

- **Given:** a live generation that commits a non-idempotent side effect and loses its reply before the host observes completion.
- **When:** the generation dies.
- **Then:** Keld rejects stale replies and reports an unknown/ambiguous outcome rather than claiming rollback or automatically replaying the call.
- **Negative control:**
  - **Single fault:** Automatically replay the pending non-idempotent request on the successor generation.
  - **At-risk precondition:** Observe one durable effect commit and suppress its reply. Retire the generation, then admit a successor that accepts the same operation without application deduplication.
  - **Expected RED:** After independent acknowledgement of the replayed effect, the durable counter is two instead of one. The caller must still report ambiguity, not rollback.

The native caller-path mapping is a target, not current product support. For this slice,
the Host maps native-service retirement or an unclassified service failure to the existing
renderer `KELD-WV-011` failure prefix. For renderer callers the exact observable is an Error whose message starts with
`KELD-WV-011:`. This prefix means outcome Unknown for this native-service caller path,
including a Host-known pre-dispatch refusal; NotSent is deliberately internal-only.
No renderer-visible code or `.code` property asserts safe retry. Register this scoped
meaning in architecture 02 and the KEL-142 failure contract before T9a promotion.
The SDK must preserve the prefix unchanged and must not infer effect status from peer text.
The qualification fixture independently asserts this exact prefix and no replay. This does not extend IPC-022/023/025 semantics.
The current renderer facade exposes an Error message, not the role SDK's `.code` field.

The Host must preserve trusted emitter and call direction internally. No error-code string
alone proves an effect. A Host-local refusal before any dispatch attempt can prove NotSent.
Attempted or partial forwarding, lost reply, Worker loss, empty/malformed peer ERR and
unknown terminal state mean Unknown. Peer-chosen IPC-005, guard or Host-like codes cannot
mint non-execution proof. A correlated application error does not prove rollback.

Add a peer-code-laundering control: commit the durable effect, return a malformed ERR or
forged non-execution code, and require Unknown with no replay. An admission/write ACK is
not a transaction outcome. A later explicit retry requires either a terminal/fenced old
effect owner plus authoritative negative outcome, or concurrency-safe durable deduplication
for the same operation key. A status read of “absent” alone is insufficient.
GH1095 remains a draft dependency direction, not evidence that this mapping is implemented.

### AC-7

- **Given:** a low-authority renderer/document and a higher-authority service role.
- **When:** the renderer asks the service to perform an operation it is not allowed to request on its behalf.
- **Then:** the host's caller/delegation policy rejects the deputy escalation unless a separately reviewed privileged-service contract explicitly authorizes that delegation.
- **Negative control:**
  - **Single fault:** Remove caller/delegation binding from the service authorization decision.
  - **At-risk precondition:** A low-authority caller reaches the higher-authority service with an otherwise valid restricted request.
  - **Expected RED:** The independent effect witness records the denied operation. Direct authorized service access remains the positive control.

### AC-8

- **Given:** the native role crashes, aborts, hangs, floods output, violates protocol or exhausts its declared resource budget.
- **When:** failure handling runs.
- **Then:** the host/window/other unrelated principals remain live, old generation authority is revoked, descendants terminate with an empty owned-group census, direct owned children are reaped, and restart policy remains bounded.
- **Negative control:**
  - **Single fault:** Skip retirement of the old generation's dispatch authority.
  - **At-risk precondition:** Queue an authenticated old-generation request immediately before its dispatch-authority check. Retire that generation, then release the request to that check. Keep an unrelated role live.
  - **Expected RED:** The old-generation dispatch is accepted instead of rejected at the authority fence. Transport closure must not prevent this at-risk check. Unrelated-role liveness cannot substitute for rejection.

### AC-9

- **Given:** a Rust-only primary configuration.
- **When:** a packaged app starts and exercises one renderer operation plus clean Quit.
- **Then:** the runtime/process/artifact census contains no Bun runtime and no dummy TypeScript primary; frontend JavaScript/build tooling is reported separately.
- **Negative control:**
  - **Single fault:** Add a Bun runtime artifact to the Rust-only package.
  - **At-risk precondition:** Package the Rust-only primary with its actual runtime dependencies and execute renderer invocation and Quit.
  - **Expected RED:** The independent artifact census finds Bun even if invocation and Quit pass.

### AC-10

- **Given:** a TS-only application.
- **When:** installed and run through the supported prebuilt workflow.
- **Then:** no Rust toolchain or framework-host compilation is required; third-party native dependency requirements are diagnosed separately rather than hidden.
- **Negative control:**
  - **Single fault:** Make the prebuilt TS-only workflow invoke Cargo before launch.
  - **At-risk precondition:** Use a clean environment without Cargo, rustc or rustup and without third-party native dependencies.
  - **Expected RED:** The supported workflow fails instead of completing without framework compilation.

### AC-11

- **Given:** an unsupported OS/runtime/artifact/security-profile tuple.
- **When:** create/dev/build/doctor evaluates it.
- **Then:** Keld fails closed or reports unsupported with an actionable diagnostic; it does not silently downgrade strict containment or load native application code into the host.
- **Negative control:**
  - **Single fault:** Replace one unsupported strict-profile refusal with admission under a weaker profile.
  - **At-risk precondition:** Request that unsupported tuple through the real admission path with the selected profile observable.
  - **Expected RED:** The test observes downgraded admission instead of the unsupported/refusal result.

### AC-12

- **Given:** a performance-motivated native substitution.
- **When:** measured against the simpler correct Bun baseline.
- **Then:** equivalent functionality, security profile, complete process/engine census, tail latency and cleanup are measured before retaining the added complexity. Language choice alone is not evidence of improvement.
- **Negative control:**
  - **Single fault:** Omit one known child process from the measurement census.
  - **At-risk precondition:** Run equivalent workloads with that child alive and independently counted by the OS.
  - **Expected RED:** The qualification check detects the census mismatch and rejects the performance evidence.


The controls above specify future qualification tests. None was executed for this document correction.
Each control isolates one fault. The other obligations in each criterion still require their planned tests.

## 4. Design

### First-principles and reuse decision

**Authority:** `keld-host` remains the only long-lived general privileged process. A normal native application role is not trusted merely because it is Rust. KEL-78's real-OS profile determines ambient authority. Reviewed in-host extensions remain a distinct TCB-expanding mode.

**Identity:** the first slice has one native AppBound service and an unchanged Bun primary.
Refuse an occupied AppBound slot or multiple service declarations; never evict an existing role.
Host-owned role/generation identity and reader custody remain mandatory. Child metadata is not authority.
Keep coordinator, counters, registry, ports and pending calls within one Host session lifetime.
Do not hot-replace the coordinator while retaining its registry or generation tombstones.
Retire all session context before exposing a new session; counter restart cannot alias old context.
Before exposing a successor, require the authority/port owner to acknowledge its admission fence.
An asynchronous Revoked notification is not this acknowledgement. Fencing stops new handler entry;
it does not undo already admitted work or its effects. Drain/cancel outcomes stay explicit.

**Lifecycle:** reuse `keld-runtime::Supervisor`, `ChildPreparer`, `PreparedChild`, `GenerationLease`, `RoleGenerationOwner` and the existing role registry/coordinator contracts. The second consumer may expose Bun-specific assumptions that need moving behind the existing preparation boundary; that does not justify a public generic-runtime framework.

**I/O:** use the existing authenticated app-link and KIPC frame/receiver semantics. The native role receives only its approved bootstrap/log/containment handles. Renderer traffic stays host-mediated. Shared memory remains absent until a measured bulk consumer independently justifies it.

**Failure:** restart restores a fresh process/generation, not application transactions. Pending calls from the retired generation are invalidated. Side effects that may have committed are surfaced as ambiguous unless the application service supplies an independently durable idempotency/transaction contract.

**Existing alternatives rejected:**

- putting ordinary application Rust in `keld-host`: rejects crash/security isolation and expands the TCB;
- creating a separate Rust supervisor or localhost service framework: duplicates lifecycle/identity/policy ownership;
- defining `runtime = rust` as the abstraction: couples architecture to a source language rather than a qualified executable contract;
- immediately supporting arbitrary native languages: multiplies packaging, ABI, debugger and containment matrices before a second concrete consumer proves the seam;
- designing a new universal IDL now: KEL-98's existing schema/golden-vector owner is sufficient for the first shared operation.

Compatibility fallback: the existing Bun/TypeScript product path remains mandatory and unchanged while this slice is incomplete.

No performance claim is made by this design.

### New/changed internal types and contracts

The first declaration is private Host-owned qualification input, not public application syntax.
It names the exclusive AppBound role, packaged artifact identity, native artifact kind, existing
Echo contract identifier and exact qualified containment profile. Packaging/admission resolves
these values; child-supplied values cannot select identity, grants or profiles. Reject unknown
fields and unsupported tuples. KEL-96 owns any later serialized public schema allocation;
existing boot versions cannot be reused. No public runtime-provider or language registry is added.
The KEL-75 preparer and KEL-78 artifact admission owners jointly freeze a closed launch envelope:
exact executable/dependent-code hashes and custody through execution, ordered argv, approved cwd,
explicit environment allowlist, loader/search behavior and inherited descriptor/handle allowlist.
Do not inherit the parent environment by default. KELD_APP_LINK is the only bootstrap locator.
Revalidate the envelope on every generation. Refuse an unsupported envelope before app code runs.
Qualification plants harmless secret/loader canaries, shadow dependencies, replacement artifacts
and unapproved handles. Independently prove refusal/absence and healthy approved execution.

The implementation MUST NOT add a second environment bootstrap. `KELD_APP_LINK=<endpoint>#<token>` remains the sole child bootstrap locator unless a separately reviewed requirement proves it insufficient. Role/principal/grant/runtime metadata is negotiated or bound by the host after authentication, not trusted from environment values.

KEL-98 Echo is the first operation: preserve its existing String/u32 schema and literal vectors.
Both TS and Rust consume that canonical source. No independent hand-written schema pair is parity.
No arbitrary bytes, null, streaming, wider integers or schema evolution claim is made.
Postcard wire-format stability does not establish schema evolution compatibility.
KEL-98 owns Echo value/error vectors; existing KIPC owners retain routing/correlation and
ordering. Admit one in-flight request; reject excess work, do not add a hidden reorder queue.
The durable-counter probe is qualification-only, not a product operation or opcode. It traverses
the same Host failure mapper and renderer SDK path, not a fixture-only outcome shortcut.
Exercise old effects still in flight and durable completion followed by lost reply separately.
Retain stale readiness/reply and stale port-operation controls across generation replacement.

### Capabilities required; manifest changes

No new capability name is required for native execution itself. Existing declared role-policy intent applies to both implementations. KEL-78/KEL-102 must prove that the selected OS mechanism and guarded dispatch enforce that intent for each supported tuple.

The initial service has no privileged service-to-Host dispatch. Reject direct privilege attempts
and callbacks after the originating renderer retires. Later privileged services require caller
authorization for the declared operation and service authority for the downstream resource,
both scoped by explicit Host-owned delegation across both hops. No automatic authority union.
Later privileged product dispatch requires passed KEL-102 T5 accepted-link-to-role-guard
integration, including its T4 and exact KEL-97 `role-guard-principal` and
`stale-generation-dispatch` prerequisites. Generic link or Echo success is not proof.

### Wire/protocol changes

None for the first slice. Existing KIPC v2 bootstrap/framing/receiver semantics remain authoritative. If the second consumer proves version/feature negotiation is insufficient, the wire change is a separately versioned review gate rather than an implicit native-role exception.

### Platform notes

- macOS: native-role admission must separately qualify executable identity, Hardened Runtime/App Sandbox/entitlements as applicable, descendant cleanup and signing/package provenance. Bun evidence is not inherited.
- Windows: native-role admission must separately qualify token/LPAC/AppContainer/job/handle policy as applicable, executable/library ACLs, descendants and signing/package provenance. Bun evidence is not inherited.
- Linux: native-role admission must separately qualify the supported namespace/seccomp/Landlock/artifact combination and exact executable/library mounts. Existing Bun strict-profile evidence is not inherited.

### Resource qualification contract

The shared Windows Host-death Job contains the Host. It is not a per-service quota boundary.
Preserve its kill-on-close/release contract. Qualify a separate per-generation resource Job
under the same lifecycle owner, with compatible nesting and assignment before untrusted execution.
Disallow breakaway; exhausting or killing that Job must leave the Host and sibling functional.
Linux requires delegated cgroup-v2 controllers, membership before execution and readback,
and no child-controlled migration. Namespace visibility or post-spawn migration is not proof.
Keep direct-child wait completion separate from resource-group census.
No macOS whole-tree hard quota follows from App Sandbox, Hardened Runtime or cleanup.
An unproven resource-profile cell remains unsupported, not silently weakened.

KEL-78 must supply an exact artifact/OS/architecture/profile qualification record before product admission.
It states finite CPU units/interval/scheduler scope, memory metric/overshoot, process/thread count,
Host queue/correlation/output caps, restart budget, aggregate admission and measured Host reserve.
Choose values from measured fixture startup and workload, not invented universal defaults.
One in-flight Echo request is the first fixture concurrency limit; retain existing wire bounds.
Each resource row names its enforcement mechanism, independent observable, exhaustion negative,
and healthy Host/sibling positive. Include aggregate descendant consumption, not just one process.
Remove one enforcement control while the workload reaches
that bound: the independent measurement must fail even if kill/reap eventually succeeds.
Liveness is qualified within this stated operating envelope, not against arbitrary system OOM.

A platform may ship TS/Bun support while native-role support remains explicitly unsupported there.

## 5. Boundaries

Implement in:

- `keld-runtime` preparation/supervision/role owner only where the second consumer proves a Bun-specific assumption;
- existing host role wiring necessary to consume the shared owner;
- KEL-98-owned contract fixtures/bindings;
- KEL-78-owned containment admission/tests;
- existing packaging/prebuilt owners for native artifacts;
- docs/config only in the synchronized promotion PR.

Must not touch merely to make the design look generic:

- workspace `Cargo.toml` or add new crates;
- KIPC frame/opcode/version;
- a second permission parser/evaluator;
- renderer direct transport;
- `@keld/electron` behavior except through its existing generic role/compat mapping;
- custom browser/engine/runtime projects;
- updater implementation before its existing trigger/prerequisites.

## 6. Tasks


### T-1: T9a

- **Blocked by:** none

promote this draft with synchronized architecture/config/current-vs-target wording; freeze the smallest
host-owned native-role declaration without adding a public runtime-plugin API; resolve and register the AC-6
caller-visible ambiguity contract and SDK mapping before approval. **T9b–T9g remain planning-only and MUST NOT
begin until T9a lands with this specification at `Status: approved`.**

### T-2: T9b

- **Blocked by:** T-1

After approved T9a, build one isolated native AppBound prototype with the existing owner.
T9b entry also requires the KEL-75/KEL-78 owners to record one exact OS version,
architecture, controlled build/dependency inputs and containment/resource profile. Select a tuple with
controlled enforceable resource primitives; availability alone does not establish qualification.
This is an implementation-owner selection record, not a request for more user design approval.
Record immutable output artifact/dependency hashes at T9b completion, before T9d qualification.
This permits isolated qualification work, not product admission or a support claim.
No renderer API change.

### T-3: T9c

- **Blocked by:** T-1, T-2

same-contract hybrid fixture: TS implementation and Rust implementation separately satisfy value/error/permission/ordering/malformed-input vectors; renderer call site is unchanged.

### T-4: T9d

- **Blocked by:** T-1, T-2

KEL-78 qualifies the immutable T9b candidate with hostile file/network/process/code-load,
hang/abort/resource and descendant tests. Product admission requires this pass plus B-1.
Qualification is not circular: T9b constructs the isolated candidate; T9d qualifies it.

### T-5: T9e

- **Blocked by:** T-1, T-2, T-3, T-4

Rust-only primary is a separate qualification target, not inherited AppBound acceptance.
Before T9e completion, KEL-75/KEL-96 review primary boot/declaration/schema and lifecycle
compatibility; qualify the exact primary artifact/dependencies and each claimed OS profile.
Require no Bun process/artifact, real renderer call, guarded denial, ambiguous-effect handling
and clean Quit. T9b–d AppBound success alone cannot authorize primary product admission.

### T-6: T9f

- **Blocked by:** T-1, T-2, T-3, T-4, T-5

repeat T9b–T9e independently on each additional claimed OS/architecture/security profile; unsupported cells remain explicit.

### T-7: T9g

- **Blocked by:** T-1, T-3, T-4

only after measured consumer evidence, decide whether any additional runtime/language adapter is worth product support.

The T-1 to T-7 headings map to the existing T9a to T9g plan without changing its scope.
T9 is additive to existing KEL-75 T4–T7/T8 work; numbering does not retroactively change those passed artifacts.

## 7. Test plan & evidence

- AC-1: existing Bun product/role/compat suites plus KEL-139 minimum-spine acceptance; no native prerequisite may be introduced.
- AC-2–3: focused runtime contract/integration test with a real child; mutations adding a second restart owner, trusting child identity or reusing a generation must fail.
- AC-4–5: KEL-98 literal golden vectors and same-renderer-call differential fixtures for TS/Rust; malformed vectors include boundary integers, bytes, absence/null if admitted, U+FEFF, malformed UTF-8 and version mismatch.
- AC-6: subprocess service fixture commits a visible side effect, dies before reply, and asserts no automatic replay/false rollback. A follow-up explicit idempotent application contract may prove safe retry separately.
- AC-7: permission integration fixture attempts indirect deputy escalation; removing caller/delegation binding must make the negative test fail.
- AC-8: subprocess hostile matrix for abort/hang/output flood/protocol violation/resource limit; unrelated role/window positive call remains live and descendant census is clean.
- AC-9: clean packaged-process/artifact census with Bun executable/runtime removed; invoke/quit still pass.
- AC-10: clean-machine TS workflow with Cargo/rustc/rustup unavailable; document any third-party native dependency exception.
- AC-11: doctor/build negative cases for unsupported tuples and strict-profile admission failure; no fallback path may pass the test.
- AC-12: KEL-90 matched result.v2 measurement after semantic/security equivalence. No benchmark is required merely to land correctness T9b.

Anti-flake: use marker/IPC/process-handle conditions, temporary owner-private paths and bounded deadlines; never sleep-sync. Real OS/device claims execute on that OS and retain artifact/process evidence.

## 8. Review gates triggered

- unsafe: conditional on platform implementation; none in this draft;
- public API: yes for promotion/config/SDK wording and any application authoring surface;
- permission model: yes when native-role admission/delegation becomes live;
- dependency: only if an implementation proves a new dependency necessary;
- wire protocol: none for the planned first slice; any change becomes its own gate.

## 9. Perf impact

The design itself changes no budget. A native role can add a process, memory, wakeups, serialization and packaging bytes even if a workload runs faster. KEL-90 owns the matched full-process/end-to-end evidence before any performance-motivated substitution or shared-memory path becomes a retained product design.

## 10. Open questions, blockers & unknowns

### B-1: Exact native admission evidence

- **Status:** open
- **Owner:** KEL-78 and KEL-102 implementation owners
- **Impact:** native product admission remains blocked on exact resource/containment qualification.
  Privileged dispatch is additionally blocked on role-aware authorization integration.
- **Check:** exact numeric resource/profile record and artifact-specific OS controls pass
  independent negative controls. For privilege, also require KEL-102 T5 with its named
  KEL-97 prerequisites. The unprivileged Echo slice does not require privileged dispatch.

### B-2: Promotion and synchronized documentation

- **Status:** open
- **Owner:** KEL-75
- **Impact:** T9b–T9g remain planning-only while this spec is draft.
- **Check:** architecture/config current-versus-target text, declaration and effect contract
  receive exact-head independent review; required document gates pass before promotion.

No native execution or acceptance is claimed by this documentation correction.
