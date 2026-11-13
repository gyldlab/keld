# Spec: Linux explicit-profile qualification

- Status: draft
- Tracker: GH-1128; shared execution authority remains KEL-78 / GH-586 / GH-588
- Owner: Monish Khandelwal; Linux runtime and shared profile owners retain their scope
- Updated: 2026-11-06
- Keld source: `0eb3ce203187f35c5276a4237189c906a5220d5a`

## 1. Goal & non-goals

Qualify a Linux primary Bun role against the approved shared explicit-profile contract.
An explicit Legacy run must disclose its ambient authority, retain authenticated Keld calls,
and complete the existing supervised lifecycle. An omitted declaration remains Unverified.
Strict admission failure must never select an ambient fallback.

This is a platform qualification plan, not a second profile policy or an implementation approval.
The result is a version-bound Linux admission, behavior and cleanup record.
A metadata label, successful Bun import or macOS receipt cannot supply that record.

Non-goals:

- Change the profile key, boot schema ownership, permission vocabulary or KEL-78 state table.
- Replace Linux's existing strict launcher, supervisor, app-link or guard.
- Weaken default containment, introduce a privileged launcher, or widen host grants.
- Claim support for every Linux distribution, architecture, installed package or Electron app.
- Take over GH-939 implementation, macOS/Windows claims, or publish a compatibility percentage.

## 2. Spec refs

Governing files at the header source pin:

- Architecture 01 sections 2 and 5: process ownership and performance budgets.
- Architecture 02: authenticated app-link; architecture 03: permission and containment boundaries.
- Architecture 06 section 1: supervised runtime and platform launch ownership.
- `docs/specs/kel78-strict-profile-sandbox.md`: profile states, strict Linux mechanism and proof catalog.
- `docs/specs/kel74-compat-evidence-schema.md`: evidence provenance and separate authority labels.
- `docs/specs/kel75-principalized-bun-child-roles.md`: generation and supervision ownership.

The exact explicit-profile amendment inspected is
`docs/specs/kel78-explicit-legacy-profile.md` in PR-1120 at
`5f66d50ff88c578ef651ebe4d121a19830355524`.
Its initially qualified scope is macOS primary only. Its state, parser, disclosure and
receipt rules are dependencies; this file proposes their Linux qualification.
GH-588 / PR-1319 owns shared implementation. GH-587 owns trusted product receipt collection.
Refresh their final approved and implemented revisions before execution.

Current source facts, not qualification results:

| Fact | Evidence |
|---|---|
| Main's one closed boot parser accepts versions 1 and 2; explicit-profile versions 3/4 are not implemented there. | [S1] |
| Linux direct startup calls `linux_strict_primary_config` and the shared gated primary supervisor. | [S2] |
| The proved runtime-file recipe is Ubuntu/Debian x86_64; other architectures are refused. | [S2] |
| Strict preparation uses `LinuxStrictProfile`; the generic branch otherwise builds `Command`. | [S3] |
| Bubblewrap receives an empty-root mount recipe, namespace flags, exact environment and two seccomp programs. | [S4] |
| The launcher reports its observed Landlock layer before target execution; failure has no Direct retry. | [S4] |
| Guard's pure admission result allows Unverified/Legacy; Strict checks its matching proof catalog. | [S5] |

Existing strict mechanisms are implementation facts. They do not establish a new declared
Strict product verdict. WebView storage profiles are unrelated to this Bun security profile.

## 3. Acceptance criteria (binary, each becomes a test)

1. **One verified selection.** Given a supported Linux compiler/Host pair and each approved
   schema/profile row, when the actual compiled input is admitted, then exactly the shared
   row is selected from the same digest-verified permissions bytes before application resources.
   Run unknown version, duplicate profile, invalid profile value, digest mismatch and
   version/declaration mismatch as separate cases. Each produces the owning typed refusal
   and zero app-child, listener and window creations. A mutant skipping only the digest
   comparison must fail the digest case. A profile-bearing unsupported mode must be refused.

2. **Omission preserves the default.** Given fresh valid profile-free input, when the Linux
   primary starts, then its effective profile is Unverified and the existing strict launch
   recipe remains in use. Host observations and actual denied outside-path/network probes
   distinguish recipe use from its label. Removing only the strict route must fail the
   mechanism observation even if the report still says Unverified.
   Removing Legacy from a fresh input must not silently retain a Legacy selection.

3. **Explicit Legacy is an observed run.** Given approved Linux Legacy admission and
   disposable owner-readable data outside the strict role root, when the real primary
   executes, then the canonical forfeiture is emitted before application execution,
   the retained Host result identifies Legacy, and the owned direct-file operation succeeds.
   Independently, an ungranted Keld privileged call is denied before its handler runs.
   Suppressing only forfeiture must fail the disclosure assertion; bypassing only the guard
   must fail the independent forbidden-handler counter. Ordinary OS permissions still apply.

4. **Strict fails closed.** Given an explicit Strict request, when a required primitive or
   proof is missing, stale, mismatched or incomplete, then the owning `KELD-GUARD` error
   names the fault and repair, no application instruction executes, and the request stays Strict.
   Exercise initial launch and a successor attempt separately.
   Injecting only a Direct retry after the refused preparation must cause the independent
   app-entry witness or spawned-process census to fail.

5. **Recovery keeps one policy snapshot.** Given a bound generation G1, when its Bun process
   crashes after the permissions file is edited on disk, then any permitted G2 uses the
   original session request and permissions digest, with a fresh generation/link/token.
   G1 credentials are refused against G2. A valid G2 request still works.
   A mutant rereading only the profile on recovery must fail the frozen-request assertion.
   A new profile takes effect only after full session retirement and fresh capture.

6. **Inputs and inherited objects are accounted for.** Given the exact approved launch recipe,
   when initial and recovered children run, then independent descriptor/environment observations
   match its declared categories and retained objects. Legacy preserves existing handle hygiene;
   it gains no extra host descriptor as a side effect of selecting the profile.
   A deliberately inherited host-only sentinel descriptor must be found and fail this row.
   Strict separately proves its allowlist. Neither a label nor an environment dump proves it.

7. **Termination and revocation are independent.** Given a running Linux primary and an owned,
   enrolled descendant, when accepted Quit, cancellation, Bun crash, CLI loss or Host death
   occurs in separate subprocess cases, then the named owner retires the appropriate generation,
   the relevant process handles reach exited state, links are unusable, and fresh launch works.
   Terminal shutdown admits no successor; crash recovery follows the existing policy.
   A mutant omitting only descendant enrollment/cleanup must fail an independent survivor census.
   If the chosen Legacy route lacks a proved Host-death mechanism, that cell blocks qualification.

8. **Evidence has trusted provenance.** Given the trusted GH-587 collector and actual Linux run,
   when a KEL-74 product record is assembled, then its runtime/artifact, OS, role generation,
   permissions digest, requested/effective profile and behavior witnesses agree.
   A receipt-shaped line printed by Bun, a macOS receipt or a G1 receipt relabelled G2 is rejected.
   Removing only the trusted-origin check must make the forged-output case fail.
   Until that collector exists, local mechanism records remain mechanism evidence.

9. **Partial support is explicit.** Given an unsupported architecture, installed route or
   unimplemented schema/mode pair, when a profile-bearing launch is requested, then it refuses
   before resources with the existing actionable category. No declaration stripping,
   renderer manufacture, numeric-version-range acceptance or automatic platform fallback occurs.
   A mutant dropping only the declaration while staging non-macOS input must fail this case.
   A qualified x86_64 dev run does not pass any other platform cell.

## 4. Design

### Decision atoms and independent oracles

| Atom / owner | Boundary and input -> output | Failure / direct oracle |
|---|---|---|
| Policy capture / shared compiler + core | Captured bytes/version -> frozen request | Mutated digest or mismatched version; pre-resource counters |
| Identity / role generation owner | Role attempt -> fresh principal | Reused generation; Host generation binding |
| Authentication / IPC bootstrap | Fresh link/token -> accepted stream | Stale or forged credential; actual rejection plus healthy peer |
| Authorization / guard | Authenticated request -> permitted handler | Ungranted call executes; independent handler counter |
| OS authority / Linux runtime | Approved recipe -> actual process capabilities | Direct operation contradicts selected recipe; OS/file/socket oracle |
| Lifecycle / supervisor and platform custodian | Owned process family -> revoked/reaped generation | Surviving owned descendant or accepted stale link |
| Provenance / GH-587 + KEL-74 | Trusted admission + behavior -> scoped record | App output relabelled Host evidence; forged-record rejection |

A file-operation pass does not prove authentication. A guard denial does not prove OS
containment. Process death does not prove grant revocation, and a startup label proves none
of those atoms. Their explicit edges are policy -> preparation, generation -> authentication,
and admission plus observed launch -> trusted receipt.

### Reuse and selected scope

Reuse the one compiler/parser, core policy snapshot, `RoleConfig`, `RolePreparer`,
`PreparedCommand`, primary supervisor and `LinuxStrictProfile`.
Do not create a Linux parser, parallel restart loop, profile-specific broker or new crate.
The existing strict launch is the baseline. Keep its ordinary default behavior.

The shared explicit-profile owner must extend supported target admission deliberately.
Until Linux support is approved and implemented, explicit profiles remain unsupported there;
publishing this plan does not make the macOS-only predecessor accept them.

A qualification fixture consumes the public production entry and actual caller chain.
It must not construct a convenient test-only Legacy label around an unverified bare Bun process.
No new successful mode is exposed merely to make a fixture pass.

For Legacy, reuse the shared known-uncontained admission rule only after the owner identifies
that actual Linux launch path. Do not manufacture strict artifact/proof identities to invoke
a richer API. Conversely, never call the known-uncontained path after Strict admission fails.
The exact core/runtime composition and lifecycle custodian remain approval dependencies in §10.

### Runtime seam and capture

Before: the Linux direct primary uses a fixed strict recipe, while main lacks explicit
profile boot support. After the gated work: the shared verified request selects an approved
Linux route, then the same supervisor and generation owner execute it.

Core owns the session policy. Runtime owns process preparation and native custody.
IPC owns endpoint authentication. Guard owns authorization and strict proof admission.
The process crash domain remains the existing primary Bun child.
The application cannot mint a role generation, replace the frozen request or self-authorize.

Strict retains explicit runtime-file mounts, fixed `/app` working directory, cleared
environment, required namespace/mount/seccomp boundary and observed Landlock state.
A kernel reporting no Landlock is not the same as a setup/enforcement failure.
Use the existing owner contract; do not reinterpret either result in a new platform shim.

Legacy forfeits Keld's direct filesystem/network/syscall confinement guarantee.
It does not grant root privileges or promise that operating-system checks will allow all work.
Freeze the working directory, environment categories, executable, code inputs and inherited
objects through the owning launch recipe before approval. Exclude credentials from published evidence.

The strict Bubblewrap parent-death mechanism cannot be assumed to protect a route that does
not execute Bubblewrap. Qualify the existing Linux lifecycle owner on the actual Legacy route.
If no existing mechanism satisfies the enrolled-family requirement, stop that slice and obtain
a reviewed owner amendment. Do not add a guardian, namespace or process supervisor by analogy.

### Types, channels and migration

Current source APIs used as anchors, not a proposed second admission interface:

```rust
RoleConfig::linux_strict(self, profile: LinuxStrictProfile) -> Self
LinuxStrictProfile::command(&self, program: &Path, args: &[OsString],
    environment: &[(OsString, OsString)]) -> Result<LinuxStrictCommand, LinuxStrictError>
admit(req: &AdmissionRequest, facts: &HostFacts,
    archive: Option<&ProofArchive>) -> Result<ProfileState, AdmissionError>
```

Any changed signature belongs to the shared owner and must be frozen in its implementation
review. This qualification file adds no new runtime API, wire frame or receipt format.

Capabilities/manifest changes: none owned here. Consume the approved shared profile key.
Boot versions 3/4 remain the shared amendment's compatibility barrier; no local renumbering.
Public kipc wire changes: none. Existing schema/diagnostic ownership remains intact.

Migration unit: compiler-produced bytes, verified core policy, initial and recovered primary,
actual Linux caller, Host disclosure and the trusted evidence consumer must move together.
No temporary dual parser or permissive adapter is selected.
The permanent Electron facade remains unchanged.

## 5. Boundaries

Implementation after approval belongs to the existing owners of:

- `crates/keld-cli/src/boot.rs` and core `app_session/boot_contract.rs`: shared compilation/preflight.
- `crates/keld-core/src/app_session.rs`: Linux primary composition and frozen policy.
- `crates/keld-runtime/src/role.rs`, `lib.rs`, `linux_strict.rs`: existing preparation/lifetime.
- Existing Linux Host/runtime integration tests and KEL-74/GH-587 evidence consumers.

This draft changes only this specification. Do not modify architecture, policy baselines,
unsafe allowlists, dependencies, global runtime pins, package signing, generated contracts,
macOS work or active GH-939 implementation. New native operations need their actual owner gate.

## 6. Tasks (each approximately one PR; ordered vertical slices)

- [ ] **T1 — approve the Linux composition.** Reconcile KEL-78 execution ownership, refresh
  shared predecessor heads, freeze one supported dev/runtime recipe and its native custodian.
  Extend shared conformance cases for AC1/2/9 without duplicating parser code.
  Exit: exact-content approval, a named lifecycle owner and reproducible refusal cases.

- [ ] **T2 — qualify actual admission and authority.** Through the approved production entry,
  prove AC1-4/6 with one valid explicit Legacy run, unchanged omission behavior and isolated
  Strict/invalid-input failures. Observe real resources and guard dispatch independently.
  Exit: original passes, named single-fault controls fail, restored source passes again.

- [ ] **T3 — qualify recovery and termination.** Reuse the existing subprocess fixtures for
  AC5/7 with actual Bun, enrolled descendants, policy edit, Host/CLI death and fresh launch.
  Exit: no surviving enrolled family or usable retired link; each unsupported cell stays blocked.

- [ ] **T4 — attach trusted Linux product evidence.** After GH-587 and runnable product callers
  are available, consume their existing collector for AC8 and repeat required admitted workflows.
  Exit: exact-input scoped record accepted, forged/cross-generation records rejected.
  Earlier mechanism evidence does not require or imply a complete product PASS.

## 7. Test plan

| Criteria | Test kind and existing owner | Independent observation |
|---|---|---|
| AC1/9 | Shared boot/parser contract and real Host refusal | Typed code, digest identity, zero resource counters |
| AC2/4 | Linux runtime boundary + Host startup subprocess | Actual process/mount effects and app-entry witness |
| AC3/6 | Actual Bun integration and guard contract | Owner-readable bytes, handler counter, descriptor census |
| AC5 | Existing role recovery fixture plus real Host | Frozen digest, fresh identity and stale-link rejection |
| AC7 | Linux no-flag/runtime subprocess fixtures | Retained process identities, exits, revoked link, fresh launch |
| AC8 | Existing corpus collector contract + product run | Trusted-origin binding and rejected forged record |

Reuse `crates/keld-runtime/tests/linux_strict_boundary.rs` and
`crates/keld-host/tests/no_flag/linux/`; inspect their final owners before extending them.
No generic new harness is justified by this plan.

Use disposable owned paths and loopback resources; wait for actual readiness/exit conditions.
Timeouts are kill switches, never readiness proofs. Proposed fixture caps are 120 seconds
per process case, 32 enrolled processes and 4096 event records. Exceeding a cap fails the case
and records the incomplete census; it does not truncate into a pass.
Keep death probes in isolated subprocesses and retain source/digest records for every mutation.

Current-documentation receipt: this documentation pass inspected exact GitHub source and
governing specs above on 2026-11-06. It selects no new external OS API.
The implementation owner must refresh official Linux/Bubblewrap contracts before changing
native behavior and record the real kernel, distribution, architecture and runtime artifacts.

Execution status: source inspection only. No Linux/native experiment, format command,
warning-denied workspace Clippy, workspace suite or local gate was run in this GitHub-only
session. Hosted PR checks are reported separately; none is an OS-qualification substitute.

## 8. Review gates triggered

Future public API and permission-model integration require named independent review of the
shared final diff. Native unsafe, dependency and wire gates are conditional on actual changes;
this plan grants none. This documentation PR changes no executable API or wire contract.
KEL-78 approval, the Linux owner and independent security evidence remain required.
Use an isolated reviewer for exact content; do not treat the author's source audit as approval.

## 9. Perf impact

No measured performance claim. Additional policy checks and evidence collection may affect
cold startup and recovery; observe architecture 01 section 5 budgets only after correctness.
Do not remove containment, disclosure or custody to improve a benchmark.
Fixture deadlines above are safety bounds, not product latency promises.

## 10. Open questions and approval dependencies

- **B1 — execution authority:** GH-1128's body contains mixed tracker wording.
  KEL-78 / GH-586 / GH-588 retain the shared owner. Confirm its live execution tracker and
  link this public planning record there before production edits; this docs-only claim does
  not arbitrate conflicting claims. First check: one accepted, non-conflicting owner handoff.
- **B2 — shared implementation:** pin approved compiler/parser/admission/receipt revisions.
  First check: real Linux profile-bearing inputs reach the approved preflight or refuse;
  the current main schema-1/2 reader is not presented as schema-3/4 support.
- **B3 — Linux Legacy custody:** the Linux runtime owner must identify and approve the exact
  existing Host/CLI-death custodian and launch recipe outside the strict Bubblewrap route.
  First check: isolated enrolled-family death and fresh-launch proof. Unknown blocks T2/T3
  qualification; it does not authorize a new supervisor.
- **B4 — native and product evidence:** a Linux operator and supported environment are required
  for T2/T3. GH-587 and runnable product callers are required only for T4's product claim.
  Until then, all acceptance rows remain unrun, and production implementation remains gated.

[S1]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session/boot_contract.rs
[S2]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session.rs
[S3]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/role.rs
[S4]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/linux_strict.rs
[S5]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-guard/src/admit.rs
