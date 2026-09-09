# Spec: Windows LPAC primary-role integration

Status: T1 limited qualification approved; later product slices remain draft
Linear: KEL-210 · Owner: GYLDLAB · Updated: 2026-09-09

On 2026-09-09 the repository owner approved the limited Windows testing scope
(Linear approval/claim dfaecc67-da02-49e2-8157-8612ad7370d8). This approves T1
qualification only; it is not approval of later product implementation. The successor
contract connects the existing Windows LPAC mechanism to the existing supervised
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
- [x] T0b: approve the limited T1 qualification scope; owner approval is recorded above.
      Later implementation signatures and product integration are not approved by it.
- [x] T1: test-only explicit package pipe-policy qualification; default unchanged;
      intended/wrong-package/ordinary client, real Bun HELLO/echo, descriptor readback,
      cancellation and fresh-launch evidence obtained. No shipping constructor added.
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

1. The bounded T1 qualification is approved and exercised below. Product use still
   needs an exact approved constructor/ownership contract and the later tasks.
2. Freeze the public signature/resource ownership after T1 evidence; no inherited
   handle or profile-selection schema is silently chosen here.
3. Resolve KEL-107's shared admission owner before ordinary startup or a Strict claim.
   This is a dependency, not permission to create a parallel admission implementation.

## 11. Approved T1 qualification evidence - 2026-09-09

The native Windows test uses only owned objects. It qualifies the existing global
endpoint shape and exact client mask; it does not implement a public LPAC constructor.

| Descriptor cell | Intended configured-LPAC client | Different package | Ordinary same-image owner |
|---|---|---|---|
| Current user only, medium label | Denied | Denied | Opens |
| Current user + exact package, medium label | Opens | Denied | Opens |
| Current user only, low label | Denied | Denied | Opens |
| Current user + exact package, low label | Opens | Denied | Opens |

All 24 cases (four cells, three clients, two fresh repetitions) matched exact descriptor
readback. The matrix uses protected DACLs and `S:AI` mandatory-label metadata. The OS
added `AI` on the first unprotected-SACL attempt; requesting a protected SACL instead
failed with error 1314. No privilege was enabled. The final fixture fixes the observed
metadata explicitly and checks every ACE and label through exact readback.

For authenticated transport, the test binds the real bootstrap listener, verifies its
original one-user DACL, and changes only that owned object's DACL through an owner
handle before starting clients. Exact readback requires one additional intended package
ACE and preserves the original label. This explicit test setup is not the proposed
shipping constructor and must not become a runtime ACL-modification fallback.

The unchanged checked-in Bun template client and existing Rust bootstrap/echo session
then run with actual Bun artifact SHA-256
`15277c59ccd6c6c20f8dc9716c2b59c1776320d606b6a8658f70be8799519ca4`.
Wrong-token attempts reach the existing verifier and are rejected; the intended token
completes HELLO and two distinct typed echo round trips. Candidate cancellation closes
the endpoint and joins the worker before the admission deadline. A new complete setup
passes again. All owned child processes are reaped before profile/resource release.

The existing `windows_lpac_boundary.rs` supplies the independent functional opt-out
oracle. Original preparation passes; temporarily disabling only the opt-out bit causes
`all_packages_denied=false` and fails the real AAP-granted-file assertion; restoring
exact source bytes restores the pass. This functional observation is independent of
the configured flag. The direct token-query error 87 remains a real limitation and is
not hidden behind a successful fallback query.

Decision supported by this evidence: retain the existing global endpoint shape and
integrity label for the next constructor design, with an explicit exact-package DACL
mode. Neither a lower label nor inherited-client-handle transport is required by these
observations. Published defaults remain unchanged. Evidence does not cover product
resource staging, successor generation ownership, host-death teardown or complete
Strict admission; T2/T3/T4 and KEL-107 continue to own those predicates.

Evidence directory: `target/kel210-evidence/t1`. Primary references, retrieved
2026-09-09: [AppContainer access model](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer),
[named-pipe access rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights),
[CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew),
[ConnectNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe),
and [SetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setsecurityinfo).
Platform documentation supplies the API contracts; the tests supply the device-specific
observations above. No unrun operating system is qualified.
