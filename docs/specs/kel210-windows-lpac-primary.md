# Spec: Windows LPAC primary-role integration

Status: draft
Linear: KEL-210 · Owner: GYLDLAB · Updated: 2026-09-09

This draft is not implementation approval. It is the concrete successor contract
needed to connect the existing Windows LPAC mechanism to the existing supervised
primary. The current default pipe rejects a zero-capability AppContainer created
through the LPAC preparation path before HELLO, while the same client image with its
ordinary token can open the pipe
and an intended owner client authenticates. Merely switching process creation would
break startup; changing the default pipe ACL would weaken a different contract.

## 1. Goal & non-goals

The Windows primary runs through the existing LPAC preparation owner and the existing
supervisor. Its authenticated link works, its declared code/runtime inputs are
available, and direct authority outside the role's prepared resources is denied.
Initial launch, crash successor, accepted Quit, host death and resource cleanup remain
separate acceptance criteria. A mechanism-applied process is not labeled `Strict`
until the guard's required artifact/profile-matched admission evidence exists.

Non-goals:

- No second supervisor, generic .NET broker, app-created principal or broad capability.
- No automatic uncontained fallback, TCP fallback, global package ACE, or weakened
  current-user-only pipe mode.
- No implementation of the KEL-107 archive/cardinality/admission owner in this task.
- No widened dev-stage ACL, new manifest/profile selector, or shipping activation as
  part of the first transport qualification slice.
- No assertion that arbitrary Node addons or Electron apps work under LPAC.

## 2. Spec refs

- Architecture 01 process ownership, 02 authenticated kipc, 03 strict/legacy distinction,
  06 prebuilt host and app-link bootstrap.
- KEL-78 Windows T3: zero capabilities, All Application Packages opt-out, exact stdio/
  transport handle allowlist, token observation before resume and independent OS proof.
- KEL-75: sole generation/principal owner and T6 consumption of per-OS admission.
- KEL-101: exact one-TokenUser-ACE default mode and child-opened client connection.
- KEL-102: immutable preflight before application resources and one dispatch owner.
- KEL-107: unresolved shared pre-spawn admission and contradictory-proof semantics.

KEL-101's default contract is preserved. An explicitly selected LPAC client policy is
a new, separately reviewed mode. The specification and implementation must name that
mode; they must not silently alter the existing constructor's postconditions.

## 3. Acceptance criteria

1. Given the current default listener, a test client created through the LPAC
   preparation path, with observed AppContainer status and zero capabilities,
   receives `ERROR_ACCESS_DENIED` before HELLO; the same image outside LPAC opens, an
   intended token-bearing client authenticates and a fresh instance behaves identically.
   This is transport prerequisite evidence, not the desired product end state.
   The opt-out configuration flag is not an independently observed LPAC verdict.
2. Given an explicitly selected role LPAC transport policy, descriptor readback shows
   exactly two explicit allow ACEs: current TokenUser and the one prepared generation's
   package SID, each with the declared rights. The old default still has one ACE.
   This descriptor does not authenticate a child: same-user peers can open the pipe,
   and the existing HELLO token remains the authentication boundary.
3. Given that selected pipe and actual intended Bun LPAC token, the unchanged v2 HELLO
   and echo/lifecycle operations work. A different LPAC package is denied, while a
   same-user peer with a wrong token receives no authenticated response.
4. In product paths requesting Strict, absent, mismatched or rejected strict
   admission/preparation prevents child execution and any ordinary `Command` fallback.
   T0/T1/T2 mechanism-qualification fixtures may launch owned test children before
   shared admission lands; they never publish a Strict verdict or activate product
   wiring. Their passes cannot replace the complete admission evidence catalog.
5. Given prepared startup, independently inspected token/handle facts precede resume:
   AppContainer true, zero capabilities, independently established LPAC opt-out,
   and only the specified objects. The existing
   `all_application_packages_opt_out_configured` field records configuration, not an
   independent OS measurement. The actual Bun image/code/profile facts match the
   selected evidence identity.
6. Given admitted code/runtime resources, the primary can load them and use its exact
   log/link/private-data objects; it cannot read host-only data, connect to an owned
   network positive-control listener, open a host process or obtain unrelated inherited
   objects. Addon/native execution receives no JS-shim exemption.
7. Given a child crash, revocation and reap precede a fresh generation/profile/link/token.
   The successor has the same authority restrictions. Old authority cannot address it.
8. Given accepted Quit, cancellation or host death, the existing owner stops the tree,
   joins capture, releases link/resource/profile leases and does not admit a successor.
   A new complete launch succeeds after cleanup. Deleting profile registration alone
   is not revocation of a live token.
9. Given a normal shipping dev stage, its existing owner-only boot/manifest validation
   remains unchanged. Preparing readable role inputs does not grant the LPAC process
   access to the host executable, boot descriptor or permission authority.

## 4. Design

### Atomic model and ownership

| Atom | Owner / input -> output | Failure and first falsifier | Edges |
|---|---|---|---|
| Identity | runtime generation owner; role -> generation/principal | SID/name used as principal; stale generation accepted | Consumed by authentication, not replaced by package identity |
| Authentication | IPC bootstrap; selected pipe/token -> accepted stream | Intended LPAC cannot reach pipe; wrong peer/token succeeds | Depends on transport policy and resume |
| Authorization/resources | core verified inputs + runtime preparation -> exact readable/private objects | Broad stage grant or unchecked mutable code | Must preserve boot preflight and guard ownership |
| Containment | runtime LPAC; prepared child -> token/handle proof -> resume | Plain spawn fallback or nonzero capabilities | Independent of successful HELLO and Job cleanup |
| Lifetime | supervisor/Job/core; generation lease -> revoke/reap/close | Live old token/process or successor after terminal shutdown | Profile/transport/resource leases end only after reap |
| Evidence | guard admission owner; image/profile/catalog -> allowed state | Synthetic or conflicting proof calls process Strict | KEL-107 remains the sole unresolved owner |

### Reuse and rejected alternatives

The current Windows `RolePreparer` constructs a plain `Command`. `PreparedCommand`
has only Direct/LinuxStrict variants, and shared `spawn_prepared` returns
`std::process::Child`. The LPAC implementation already owns process/thread handles,
token observation, resume, wait and terminate; that object is not a `Child`.

Extend the shared prepared-child seam with a narrow Windows adapter under the existing
runtime owner. Keep one supervisor loop and one generation lease. Reuse the existing
capture and termination semantics rather than duplicate them in a Windows launcher.
Stable Rust 1.97.1 does not expose `CommandExt::spawn_with_attributes`: the installed
version's official rustdoc identifies it as nightly-only. A toolchain change or
`RUSTC_BOOTSTRAP` is not proposed.

Core supplies verified resource descriptions through runtime configuration. Runtime
owns LPAC creation and resource/profile leases; IPC owns the selected pipe's ACL,
namespace, cancellation and HELLO behavior. Core does not implement ACL or wire logic.

### First qualification slice: explicit LPAC pipe mode

The proposed IPC policy is an explicit alternative to the existing current-user-only
mode, not a replacement for it. Its access set is current TokenUser plus exactly one
generation-specific package SID. The candidate client mask remains `0x0012_019B`,
excluding pipe-instance creation. It grants no Everyone/Users/All Application Packages/
All Restricted Application Packages or network capability. Mandatory-integrity label
requirements and any AppContainer namespace requirement must be independently
qualified; lowering the label is not assumed necessary. Start with the existing
label, vary the package ACE and label independently, and retain the least broad
policy that passes intended/wrong-package and ordinary-client controls.

Candidate descriptor and namespace choices are **not yet proved by AC1**. AC1 proves
the existing composition fails, not whether DACL, mandatory integrity or namespace is
the only cause. The first implementation task is limited to qualifying the explicit
mode with the intended and wrong LPAC profiles, normal-owner control, readback, HELLO,
denial and cleanup. If the existing endpoint shape cannot serve the qualified mode,
stop for a concrete namespace/API contract amendment; do not silently fall back.

Rejected for initial qualification: a globally broader default ACL, direct network
capabilities and a loopback transport fallback. An inherited preopened client handle
is a possible separately reviewed alternative, but the current client contract opens
by name and Bun consumption of such a handle is not established; inheritance is not
selected merely because `extra_handles` exists in the LPAC primitive.

### Later product composition (gated)

One generation owns its profile, exact input/resource leases and one IPC endpoint.
Resource preparation must derive from verified retained sources and preserve byte
identity. It must not simply grant read-execute over the whole protected dev stage.
The exact role-code staging layout and immutable preflight interaction require review
before that task begins; no new boot/profile schema is approved by this draft.

Missing/rejected containment cannot select Direct. Explicit legacy behavior remains
the existing separately declared contract. Complete `Strict` publication depends on
KEL-107's approved shared admission contract and a complete exact-artifact OS catalog;
this task cannot invent permissive facts or merge conflicting proof rows.

Wire framing, v2 HELLO and permission-manifest syntax changes: none selected. Public
API: one explicitly scoped pipe policy and one prepared-child configuration/adapter
will require exact signature review. No arbitrary app-supplied SID/profile is accepted.

## 5. Boundaries

Implement after approval in the existing IPC Windows pipe/bootstrap owner, runtime
LPAC/prepared-child/generation owner and core composition. Keep unsafe in the existing
sanctioned runtime/IPC owners with local proofs. Do not route it through core or a new
crate merely to avoid an ownership gate. No manifest, wire, CI or root-instruction
changes are part of T1 without a named approved requirement.

## 6. Tasks

- [x] T0a: current-policy composition test; record the real denial and normal controls.
- [ ] T0b: review this contract, isolate the exact pipe-mode decision and record approval.
- [ ] T1: explicit LPAC pipe-mode qualification only; preserve the existing default;
      real intended/wrong-package/normal-user/HELLO/readback/cancellation evidence.
- [ ] T2: shared Windows prepared-child adapter and generation-owned leases, with
      synthetic and actual Bun initial/restart/cancel/host-death proof. No product wiring.
- [ ] T3: approved exact resource staging/preflight and shared admission integration;
      ordinary Windows primary consumes it only after predecessor artifacts pass.
- [ ] T4: product hostile-operation and successor catalog, then separately qualify the
      exact artifact/profile as Strict; no blanket compatibility claim.

No partial slice makes the ordinary primary claim containment or changes shipping
privileged filesystem reachability. KEL-130/KEL-102 own that separate chain.

## 7. Test plan

`crates/keld-runtime/tests/windows_lpac_app_link.rs` owns AC1. The LPAC and ordinary
clients use the same executable and endpoint; observed AppContainer and capability
facts precede resume. T0 does not certify opt-out: an attempted independent
`GetTokenInformation(TokenIsLessPrivilegedAppContainer)` query returned Windows error
87 on this device. The attempt is retained in the local evidence, not converted into
a passing oracle. T1 must qualify an independent supported oracle, including an
ordinary-AppContainer positive control; the existing boundary fixture
`windows_lpac_boundary.rs` already exercises an AAP-granted file and must be evaluated
for reuse before adding another ACL helper.
Normal owner HELLO consumption is observed before the server closes. Two fresh
listeners prevent a one-use setup result being mistaken for repeatable composition.

T1 needs actual Windows descriptor/token/namespace checks and intended/wrong-package
controls, with no broad ACL escape. T2/T3 need actual initial and successor Bun
observations, direct-denial positive controls and cleanup handles. A marker file that
LPAC denies cannot be used as absence-of-authority proof: results must flow through
the admitted log/IPC path. Every mutation/timeout/error gets a named negative control.
All process tests have bounded waits and no sleep synchronization. Windows success
does not qualify macOS/Linux.

## 8. Review gates triggered

Permission model and public API; unsafe for changes to existing native adapters.
Dependency and wire protocol are none for current T0 code and conditional for later
approved slices. Named independent evidence is required on each final diff; no
reviewer or OS pass is asserted by this draft.

## 9. Perf impact

No performance claim. LPAC/profile/resource preparation adds cold startup work and
lease lifetime; quantify only after semantic equivalence and the existing benchmark
admission predicates pass. Do not remove enforcement to meet a startup budget.

## 10. Open decisions before approval

1. Approve the bounded T1 explicit per-generation LPAC pipe policy qualification,
   preserving current-user-only defaults, with exact descriptor/namespace evidence
   before product use. The current test does not settle those external semantics.
2. Freeze the public signature/resource ownership after T1 evidence; no inherited
   handle or profile-selection schema is silently chosen here.
3. Resolve KEL-107's shared admission owner before ordinary startup or a Strict claim.
   This is a dependency, not permission to create a parallel admission implementation.
