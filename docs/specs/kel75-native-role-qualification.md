# Spec: native application-role qualification

Status: draft
Linear: KEL-75 · Owner: GYLDLAB · Updated: 2026-09-17

## 1. Goal & non-goals

Keld must allow application logic to remain TypeScript/Bun, move selected logic to a managed native Rust role, or eventually use a Rust-only application primary without forking or recompiling the Keld host. The observable goal is runtime substitution behind the same host-owned identity, lifecycle, permission intent and renderer contract. This specification deliberately proves one second execution consumer before generalizing the framework.

Non-goals:

- no Node, Deno, Python, Go, C#, Zig, WASM or arbitrary executable support claim;
- no runtime-plugin marketplace, public execution-provider trait or second supervisor;
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

This draft intentionally does not change architecture 01's normative “JS owns the app” wording. Promotion to `approved` requires the synchronized architecture/config/public-contract delta that replaces that destination with runtime-neutral application roles while preserving Bun as the default and Electron-compatibility runtime.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given the existing Bun primary, when the native-role implementation is absent, then every current Bun lifecycle, renderer, guard and packaging acceptance continues unchanged; the native slice is additive.
2. Given one declared native role, when the host starts it, then the existing Supervisor preparation path owns spawn/restart/backoff/reap and the existing generation owner mints endpoint/token/principal state; no second restart or identity owner exists.
3. Given a child-supplied role name, principal, grant, runtime feature or trace identifier, when it authenticates, then none of those fields can select or enlarge authority; host-bound declaration/generation metadata remains authoritative.
4. Given the same versioned application contract implemented once by Bun/TypeScript and once by Rust, when a renderer invokes the qualified operation, then both implementations produce the same accepted values, typed errors, permission result and ordering through the host.
5. Given malformed, truncated, out-of-range, non-scalar or version-incompatible contract input on an authenticated link, when the role/host decodes it, then the request fails with the registered typed error before the application handler observes a lossy value.
6. Given a live generation that commits a non-idempotent side effect and loses its reply before the host observes completion, when the generation dies, then Keld rejects stale replies and reports an unknown/ambiguous outcome rather than claiming rollback or automatically replaying the call.
7. Given a low-authority renderer/document and a higher-authority service role, when the renderer asks the service to perform an operation it is not allowed to request on its behalf, then the host's caller/delegation policy rejects the deputy escalation unless a separately reviewed privileged-service contract explicitly authorizes that delegation.
8. Given the native role crashes, aborts, hangs, floods output, violates protocol or exhausts its declared resource budget, when failure handling runs, then the host/window/other unrelated principals remain live, old generation authority is revoked, descendants are reaped by the platform owner and restart policy remains bounded.
9. Given a Rust-only primary configuration, when a packaged app starts and exercises one renderer operation plus clean Quit, then the runtime/process/artifact census contains no Bun runtime and no dummy TypeScript primary; frontend JavaScript/build tooling is reported separately.
10. Given a TS-only application, when installed and run through the supported prebuilt workflow, then no Rust toolchain or framework-host compilation is required; third-party native dependency requirements are diagnosed separately rather than hidden.
11. Given an unsupported OS/runtime/artifact/security-profile tuple, when create/dev/build/doctor evaluates it, then Keld fails closed or reports unsupported with an actionable diagnostic; it does not silently downgrade strict containment or load native application code into the host.
12. Given a performance-motivated native substitution, when measured against the simpler correct Bun baseline, then equivalent functionality, security profile, complete process/engine census, tail latency and cleanup are measured before retaining the added complexity. Language choice alone is not evidence of improvement.

## 4. Design

### First-principles and reuse decision

**Authority:** `keld-host` remains the only long-lived general privileged process. A normal native application role is not trusted merely because it is Rust. KEL-78's real-OS profile determines ambient authority. Reviewed in-host extensions remain a distinct TCB-expanding mode.

**Identity:** a role instance remains `(host-declared role, fresh host-minted generation)`. Executable path, PID, runtime kind, environment fields, KIPC frames and tracing metadata are never principals.

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

The first implementation should extend the current internal preparation seam only as required by the native executable. Conceptually the declaration needs an implementation/artifact choice that is host-owned; exact public config syntax is intentionally not frozen by this draft.

The implementation MUST NOT add a second environment bootstrap. `KELD_APP_LINK=<endpoint>#<token>` remains the sole child bootstrap locator unless a separately reviewed requirement proves it insufficient. Role/principal/grant/runtime metadata is negotiated or bound by the host after authentication, not trusted from environment values.

KEL-98 owns the shared operation contract. The first native fixture uses the same existing typed/golden-vector source consumed by the TS implementation; no independently handwritten Rust/TS schema pair is accepted as parity evidence.

### Capabilities required; manifest changes

No new capability name is required for native execution itself. Existing declared role-policy intent applies to both implementations. KEL-78/KEL-102 must prove that the selected OS mechanism and guarded dispatch enforce that intent for each supported tuple.

Any future explicit privileged-service delegation requires a separate permission/public-contract review. Automatic authority union or blanket role-to-role trust is forbidden.

### Wire/protocol changes

None for the first slice. Existing KIPC v2 bootstrap/framing/receiver semantics remain authoritative. If the second consumer proves version/feature negotiation is insufficient, the wire change is a separately versioned review gate rather than an implicit native-role exception.

### Platform notes

- macOS: native-role admission must separately qualify executable identity, Hardened Runtime/App Sandbox/entitlements as applicable, descendant cleanup and signing/package provenance. Bun evidence is not inherited.
- Windows: native-role admission must separately qualify token/LPAC/AppContainer/job/handle policy as applicable, executable/library ACLs, descendants and signing/package provenance. Bun evidence is not inherited.
- Linux: native-role admission must separately qualify the supported namespace/seccomp/Landlock/artifact combination and exact executable/library mounts. Existing Bun strict-profile evidence is not inherited.

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

- [ ] T9a — promote this draft with synchronized architecture/config/current-vs-target wording; freeze the smallest host-owned native-role declaration without adding a public runtime-plugin API. **T9b–T9g remain planning-only and MUST NOT begin until T9a lands with this specification at `Status: approved`.**
- [ ] T9b — after approved T9a, implement one supervised Rust service role on one already-qualified OS using the existing preparation/generation/app-link owner; no renderer API change.
- [ ] T9c — same-contract hybrid fixture: TS implementation and Rust implementation separately satisfy value/error/permission/ordering/malformed-input vectors; renderer call site is unchanged.
- [ ] T9d — KEL-78 containment qualification for that exact native artifact, including hostile file/network/process/code-load/hang/abort/resource tests and descendant cleanup.
- [ ] T9e — Rust-only primary product fixture: no Bun process/artifact, real renderer call, guarded denial, recovery/ambiguous-effect case and clean Quit.
- [ ] T9f — repeat T9b–T9e independently on each additional claimed OS/architecture/security profile; unsupported cells remain explicit.
- [ ] T9g — only after measured consumer evidence, decide whether any additional runtime/language adapter is worth product support.

T9 is additive to existing KEL-75 T4–T7/T8 work; numbering does not retroactively change those passed artifacts.

## 7. Test plan

- AC1: existing Bun product/role/compat suites plus KEL-139 minimum-spine acceptance; no native prerequisite may be introduced.
- AC2–3: focused runtime contract/integration test with a real child; mutations adding a second restart owner, trusting child identity or reusing a generation must fail.
- AC4–5: KEL-98 literal golden vectors and same-renderer-call differential fixtures for TS/Rust; malformed vectors include boundary integers, bytes, absence/null if admitted, U+FEFF, malformed UTF-8 and version mismatch.
- AC6: subprocess service fixture commits a visible side effect, dies before reply, and asserts no automatic replay/false rollback. A follow-up explicit idempotent application contract may prove safe retry separately.
- AC7: permission integration fixture attempts indirect deputy escalation; removing caller/delegation binding must make the negative test fail.
- AC8: subprocess hostile matrix for abort/hang/output flood/protocol violation/resource limit; unrelated role/window positive call remains live and descendant census is clean.
- AC9: clean packaged-process/artifact census with Bun executable/runtime removed; invoke/quit still pass.
- AC10: clean-machine TS workflow with Cargo/rustc/rustup unavailable; document any third-party native dependency exception.
- AC11: doctor/build negative cases for unsupported tuples and strict-profile admission failure; no fallback path may pass the test.
- AC12: KEL-90 matched result.v2 measurement after semantic/security equivalence. No benchmark is required merely to land correctness T9b.

Anti-flake: use marker/IPC/process-handle conditions, temporary owner-private paths and bounded deadlines; never sleep-sync. Real OS/device claims execute on that OS and retain artifact/process evidence.

## 8. Review gates triggered

- unsafe: conditional on platform implementation; none in this draft;
- public API: yes for promotion/config/SDK wording and any application authoring surface;
- permission model: yes when native-role admission/delegation becomes live;
- dependency: only if an implementation proves a new dependency necessary;
- wire protocol: none for the planned first slice; any change becomes its own gate.

## 9. Perf impact

The design itself changes no budget. A native role can add a process, memory, wakeups, serialization and packaging bytes even if a workload runs faster. KEL-90 owns the matched full-process/end-to-end evidence before any performance-motivated substitution or shared-memory path becomes a retained product design.

## 10. Open questions

- Exact public configuration spelling and SDK package/crate naming are intentionally unresolved until T9a reviews the smallest consumer-facing surface against KEL-139/KEL-142 and packaging constraints.
- Which OS should host T9b first is an execution/evidence choice, not an architecture decision; select the currently available platform with the smallest qualified containment/delivery delta without inferring support elsewhere.
- Native-role resource limits remain platform/runtime-specific until a demonstrated workload requires a portable public policy.

These questions do not authorize implementation while this specification remains `draft`.