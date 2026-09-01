# Spec: principalized Bun child roles and virtual-port routing

Status: approved
Linear: KEL-75 · Owner: GYLDLAB · Updated: 2026-08-30

## 1. Goal & non-goals

Keld extends KEL-70's live generic single-child supervisor into a host-owned family of
independently supervised Bun roles while retaining one authority root, default-deny
dispatch, and an app-agnostic runtime core. Each role instance receives a new
host-minted principal generation and private authenticated link. The host, rather than a
role, creates roles, virtual ports, windows and privileged resources. This specification
makes the later Electron `utilityProcess` and `MessageChannelMain` facades consumers of
that generic model.

Non-goals:

- Replacing KEL-70's generic `keld_runtime::Supervisor`, its restart policy, output
  capture, or crash-loop breaker. T1 extends it through a role-bootstrap boundary.
- Implementing `@keld/api`, `@keld/electron`, shared memory, or an OS sandbox in T1.
- Giving a webview a Bun socket, pipe, file descriptor, mapping handle, reconnect
  capability, or direct OS channel.
- Making a PID, listener path, token, environment variable or a wire field an identity.
- Adding VS Code role names, package-name branches, or arbitrary process spawning to
  `keld-runtime`.
- Claiming that any strict sandbox profile is implemented; KEL-78 owns its real-OS
  proof.

## 2. Spec refs

- `docs/architecture/01-overview.md` §§1, 4
- `docs/architecture/02-ipc.md` §§1, 5, 7
- `docs/architecture/03-security.md` §§1, 2, 4
- `docs/architecture/04-electron-compat.md` §§3, 4
- `docs/architecture/06-runtime-and-tooling.md` §1
- Electron oracle snapshot `competitors/electron@cbb5e25d1dca07f5001b9c2f9feec23cdd445cb6`:
  `docs/api/utility-process.md`, `docs/api/message-port-main.md`, and
  `spec/api-ipc-spec.ts`

The architecture files describe destination behavior. KEL-70 is live: its generic
supervisor spawns one CLI-owned Bun echo child, captures stdout/stderr, applies
backoff/crash-loop policy, and reaps exited children. It has no role identity, principal
binding, role-specific grant, multi-role registry, reusable bootstrap listener or
renderer-continuity proof. KEL-72 remains an app-lifecycle shim; it does not acquire a
process model, privileged child channel or role grant through this specification.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a declared role instance, when the host spawns it, then the host creates a
   fresh endpoint, 32-byte possession secret and principal generation before spawn;
   the link is bound to that exact generation only after a valid `HELLO`.
2. Given a primary-role restart, when the old role tries its complete locator or token
   after revocation, then the host rejects it; a newly provisioned generation has a
   different endpoint, token and principal instance under the same declared policy.
   `Revoked(g1)` must happen before any `Provisioned(g2)` or successor spawn. The
   portable KEL-70 child wait observes/reaps on exit, so literal revoke-before-reap is
   not claimed for natural exit; a per-OS non-reaping wait design is separate work.
3. Given two live roles with different principals, when role A sends a call, a port
   transfer, a cancellation or a stale reference naming role B, then role B receives no
   call and neither role's authority changes. A valid role-B workflow remains live.
4. Given an app-bound role and window-bound roles for two distinct host-minted window
   generations in T2/T4, when one owner window begins closing, then a close tombstone
   prevents every later prepare, spawn, bind, ready or restart transition for roles
   bound to that exact window generation. The host quiesces those roles, revokes their
   dispatch context, grant leases and virtual-port routes, drains only already-admitted
   work, then revokes the link and stops/reaps the retained process handle. The
   app-bound role and the other window's role each complete a positive call after the
   close. When the host dies, every role authority is revoked and every enrolled child
   tree is reaped by the platform lifecycle mechanism; numeric PID reuse cannot target
   or satisfy evidence for a later process.
5. Given a virtual port pair in T3, when the sender transfers one end, then ownership moves
   once to the host-approved target principal; duplicate, self, source, closed and
   stale-generation transfers fail without delivery. FIFO ordering is preserved per
   live port generation, and closing either end gives the peer exactly one disconnect
   observation.
6. Given a role whose strict profile cannot be admitted, when strict execution is
   requested, then the host does not start an unconfined role. A separately declared
   legacy profile remains authenticated and guarded but reports that the strict claim is
   unavailable.
7. Given an Electron facade fixture in T5 against the pinned oracle, when it exercises the
   selected `utilityProcess` and `MessagePortMain` behaviors, then the facade's
   observable events, transfer validation, queue/start behavior and disconnect behavior
   match the recorded conformance entry. An operation without an oracle remains unknown.
8. Given a live renderer document and a window-bound role generation, when that role
   crashes and a successor reaches its distinct post-bind `Ready` transition, then
   renderer continuity requires two externally observed beacons from the same window
   generation and document-local nonce: one before the crash and a later,
   successor-correlated beacon after `Ready(g2)`. Native-window survival, a repeated
   fixed marker, or a new document with a new nonce does not satisfy continuity.

## 4. Design

### First-principles and reuse decision

- **Ownership/trust/lifecycle:** `keld-runtime::Supervisor` owns a live child process
  and its restart/reap lifecycle; `keld-ipc` owns wire authentication and must own the
  reusable endpoint/token listener; the new coordinator owns role generation and only
  binds a principal after authentication. The webview owns neither the endpoint nor the
  role lifetime. The host remains the sole privileged-resource owner.
- **Existing options evaluated:** the live KEL-70 `Supervisor` is retained for spawn,
  output capture, crash-loop and process-handle lifetime. The CLI-local `EchoServer` is
  rejected as a reusable listener because it accepts one connection and terminates on
  invalid `HELLO`; that would let a hostile connector deny the legitimate role. The
  existing `keld-ipc` handshake is retained as the wire oracle.
- **Named unmet requirement:** KEL-70 cannot mint fresh endpoint/token generations,
  bind an authenticated link to host metadata, accept again after an invalid connector,
  or revoke old link authority before successor provisioning. T1 adds those missing
  responsibilities without duplicating supervision or wire decoding.
- **Fallback:** bounded inline kipc remains mandatory. No shared mapping, raw OS handle,
  direct webview link or compatibility facade is introduced in T1.
- **Performance:** T1 is a correctness/security boundary, not a performance rewrite;
  no speed claim is made. Any later bulk optimization needs a reproducible attributed
  end-to-end benchmark and retains inline `RAW` as fallback.

### Role declaration and identity

A role declaration is host configuration, not a child-supplied request. It contains a
stable application-local role name, its entry artifact, a lifecycle owner, restart
policy, logging policy and a reference to generated permission policy. The runtime
accepts only three initial lifecycle owners:

| Owner | Intended use | Stop condition |
|---|---|---|
| `primary` | one app entry role | host application session stop |
| `app-bound` | shared worker, PTY broker facade or agent | host application session stop |
| `window-bound` | extension, watcher or worker tied to one host window | owner window close or host application session stop |

The host creates an opaque `RoleInstance` for every spawn. It comprises a stable,
host-assigned role-declaration id plus a fresh host-only generation; two declarations
with the same lifecycle category are still different roles. It is not a PID,
bootstrap token, socket name or frame field. A `window-bound` declaration additionally
stores the exact opaque `WindowGeneration` minted for one window incarnation. Navigation
generation and document nonce rotate independently inside that window and are not part
of the role principal. Window affinity is not a reusable native window number. The
app-session registry, rather than a recreated per-role coordinator, owns the monotonic
generation counter, so coordinator recreation cannot reset identity into a live or
retired collision. The configuration supplies the entrypoint through the host's trusted
bundle resolution. `KELD_APP_LINK` remains the sole child bootstrap variable; it conveys
only `<endpoint>#<64 hex chars>`, never role/principal/grant metadata.

The child may receive stdout/stderr sinks only for supervisor-managed logging. Those
fixed-direction log streams are not authority handles. All other inherited descriptors
and handles are closed by default. The endpoint is private, one successful accept
consumes its bootstrap generation, and platform peer authentication is an additional
admission check rather than a replacement for the possession secret.

### T1b prepared-child lifecycle seam

T1b extends the existing `keld_runtime::Supervisor`; it does not wrap it with another
restart loop or consume its public events from a competing thread. The internal contract
is conceptually:

```rust
struct PreparedChild<L> {
    command: Command,
    lease: L,
}

trait GenerationLease {
    fn child_spawned(&mut self, pid: u32, attempt: u32) -> Result<(), RuntimeError>;
    fn poll(&mut self) -> Result<(), RuntimeError>;
    fn revoke(self, cause: RevocationCause) -> Result<(), RuntimeError>;
}

trait ChildPreparer {
    type Lease: GenerationLease;
    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError>;
}
```

The supervisor calls `prepare` exactly once before every OS spawn. The primary-role
preparer mints an opaque monotonic generation, binds a fresh platform
`BootstrapListener`,
creates the trusted Bun command with only its generation-specific `KELD_APP_LINK` as
role bootstrap metadata, and returns the listener as the generation lease. After the OS
spawn succeeds, the supervisor calls `child_spawned`; the lease starts cancellable
admission without blocking the restart/reap owner. The supervisor then polls the lease
while it watches the child, so bootstrap admission failure becomes a typed terminal
failure instead of a hidden side-thread result. The supervisor keeps that lease beside
the live `Child`; it releases it synchronously after natural exit and before any
successor preparation. For explicit shutdown, it revokes the lease before it kills/reaps
the `Child`. Initial or respawn preparation/spawn failure also revokes the unstarted
lease and produces a typed terminal error rather than a bare `RespawnFailed` event.

Successful `HELLO` binds the listener's still-live generation under one coordinator
state transition and emits `LinkBound`. A foreign `HELLO` emits a host-only redacted
`BootstrapRejected { code: KELD-IPC-007 }` record, closes the foreign stream, and leaves
the generation available for the legitimate child. The peer may observe only a closed
connection (`KELD-IPC-001`); T1b must not invent a wire reply that leaks authentication
state. The coordinator must close/unlink the bootstrap endpoint immediately after a
successful bind, so a stale client cannot connect to an unserved listener.

### Permissions and lifecycle

`keld.config.ts` declares an entry and lifecycle owner; `keld.permissions.jsonc`
declares capability ceilings. The generated role-policy record is a subset of the app
ceiling by default. An exception must be represented as an explicit, reviewable
role-specific grant with its own capability diff; it cannot arise from role name,
`utilityProcess` options, a caller payload or an environment value. Both schema changes
are versioned permission/public-API review gates.

KEL-70's `Supervisor` owns the process handle and reap operation; it never kills by a
bare PID after an exit. The role-bootstrap layer owns generation revocation. On
handshake failure, protocol abuse, drain deadline or host shutdown it revokes the role
generation before closing/killing the child. On natural exit, portable safe Rust
observes exit through `try_wait`, which already reaps; the required invariant is that it
revokes the old generation before it provisions or spawns any successor. A restart
always starts at provisioning with fresh identity. `app-bound` describes ownership by
the host's logical app session—not a child process tree—so a primary-role restart does
not silently grant it control over another role's lifetime.

### T4a window-bound lifecycle contract

T4a is a contract and executable trace oracle only. It adds no `RoleOwner` variant,
`RoleRegistry` slot, window callback, `WebEngine` method, renderer bridge, principal
schema or shipping restart path. The portable oracle is
`crates/keld-runtime/tests/window_bound_contract.rs`; it accepts complete observed
traces and rejects counterexamples. Its `Revoke` record means an authoritative mutation
has already occurred; it does not turn an observation queue into the authority ledger.
T4 product integration must reuse the owning runtime/registry abstraction, make bind and
revoke synchronous there, expose a separate read-only event feed, and prove that draining
or discarding that feed cannot prevent locator, link, dispatch, grant, port, mapping or
pending-call invalidation.

The host declares the role and mints every `RoleInstance`. The declaration has exactly
one lifecycle owner: `primary`, `app-bound` or `window-bound(WindowGeneration)`. A
window generation is live or closing/closed. Closing is a monotonic tombstone: once the
host linearizes `WindowClosing(w)`, no role bound to `w` may prepare, spawn, bind,
become ready or restart, even if close races backoff or a partially completed successor.
The child cannot declare or change this binding.

`LinkBound(g)` and `Ready(g)` are deliberately separate. `LinkBound` means the host
authenticated the possession secret and attached trusted role metadata. `Ready` is the
host's receipt of a generation-correlated role-runtime readiness acknowledgement on
that authenticated control path (or a reviewed host-known equivalent). It is not
inferred from `HELLO`, a native window, or KEL-96's sticky renderer-navigation readiness.
The future product slice chooses the wire/API representation, not the meaning. A
valid `Ready(g)` acknowledgement immediately enters `Running(g)` in the trace. A
post-bind/pre-ready child failure is observed/reaped, fully revoked and may restart only
under the same live-owner/session/restart-policy checks; it never emits `Ready(g1)` or a
renderer-continuity pass for g1.

| Cause / phase | Required ordered transition | Forbidden edge |
|---|---|---|
| Initial start | `Declared(role, owner) → Provisioned(g1) → Spawned(g1, handle) → LinkBound(g1) → Ready(g1) → Running(g1)` | bind before authentication; ready before bind; child-selected role/window/grant |
| Natural crash | `ObservedAndReaped(g1) → RevokeAll(g1)`; only while the application and exact owner window remain live and restart policy permits: `Provisioned(g2)` | claiming portable revoke-before-reap; successor provisioning before old link, dispatch context, grant, port/mapping and pending-call authority are revoked |
| Admission, protocol or pre-bind failure | `RevokeAll(g) → close/terminate → reap-or-retire(g)` | treating `LinkBound` as ready; leaving a listener/token/handle live; unsandboxed fallback |
| Post-bind/pre-ready child failure | `ObservedAndReaped(g1) → RevokeAll(g1)`; if owner/session/policy remain live, begin `Provisioned(g2)` | publishing `Ready(g1)`; renderer-continuity claim for g1; successor before full revocation |
| Window close | `WindowClosing(w) → quiesce(bound roles) → revoke dispatch/grant/port/mapping authority → drain admitted work → revoke/close link → terminate/reap retained handles → WindowClosed(w)` | accepting new work while draining; provisioning a successor; stopping an app-bound or different-window role |
| App shutdown | `SessionQuiescing → revoke every live generation → stop/reap every retained handle → SessionStopped` | first cleanup error abandoning later children; any successor after quiesce |
| Host death | platform reaper observes the owner-death primitive, revokes registered authority, then terminates/reaps each enrolled process tree | host RPC after death; lookup, signal or pass evidence by a recovered numeric PID |

`RevokeAll(g)` means the generation's authenticated link/locator, trusted dispatch
context, opaque grant leases, virtual-port routes, optional mapping handles and pending
calls are no longer usable. T4a does not choose the KEL-102 guard-principal or grant
representation. For an orderly window close, drain covers only correlation ids admitted
before quiesce; those exact records may complete, while every call arriving after
quiesce is rejected. A host-authorized stop/drain control may use the still-bound link,
but the host revokes that link and locator before process termination. Timeout is only
the bounded end of that drain, not permission to restart or retain authority.

Close races are part of the contract, not scheduler accidents:

- close before successor provision prevents provision;
- close after provision but before spawn revokes and retires the unstarted generation;
- close after spawn but before bind revokes and reaps that exact handle;
- close after bind or ready follows the full ordered window-close path;
- close during restart backoff latches before the next prepare call;
- crash after the close tombstone cannot schedule a successor.

Stale endpoint/token, dispatch-context, grant and port references for a retired
generation fail closed without changing a live role's authority. A positive call from
an app-bound role and from a role bound to a different live window must still complete
after the target window closes; absence of a sibling `Revoked` event alone is not the
isolation oracle.

Renderer continuity belongs to the same ordered trace but needs a real backend later.
The already-running document creates a nonce in document-local memory and emits
`Beacon(w, navigation, nonce, g1, sequence=1)`. After the successor is authenticated
and reaches `Ready(g2)`, that same document emits
`Beacon(w, same navigation, same nonce, g2, sequence>1)`. Reload/navigation rotates
the document nonce and therefore fails this continuity observation. The landed macOS
KEL-96/T3 test proves a fresh primary generation plus one retained native window and
one initial fixed-marker beacon; it does not contain the second renderer beacon and is
not T4 continuity evidence. The current backend's `LastWindowClosed` observation is
also unqualified: it carries no `WindowGeneration`, so T4 cannot reuse it unchanged as
an exact owner-close input.

Process ownership and evidence use a retained live process/job/guardian registration
whose identity is consumed exactly once on terminal cleanup. PID and native window
numbers are diagnostics only. The CI model rejects PID-only and duplicate reap records;
its direct-child kill/wait helper is only subprocess-isolation sanity, not platform
host-death or descendant proof. Each future per-OS test must use an instance-bound
witness whose EOF/status identifies the enrolled child and descendants, then prove a
clean relaunch; polling `kill(pid, 0)` or an empty window list alone cannot exclude PID
reuse.

### T8 Windows primary-generation predecessor

T8 is the human-promoted Windows predecessor required by approved KEL-96-D7. It
extends the T1b generation owner to Windows without creating another restart,
backoff, crash-loop, admission or reap policy. `keld-runtime::Supervisor` remains the
only child/process-handle owner; the same platform-neutral role coordinator owns the
monotonic generation, prepared-child lease, authenticated bind and synchronous
revocation-before-successor transition.

KEL-96-D10 explicitly permits T8 to use the current unprivileged Windows loopback
transport. `keld-ipc::BootstrapListener` therefore owns both current backends behind
one public contract: an owner-only Unix socket and Windows `127.0.0.1:0`. Both mint a
fresh `SessionToken`, continue after rejected `HELLO`, enforce the same generation and
handshake deadlines, publish the same redacted rejection taxonomy, consume the
locator after authentication and close it on revocation. Windows keeps the existing
decimal endpoint plus v2 `HELLO` bytes; T8 adds no frame, channel, parser or fallback.
KEL-101 remains the separate named-pipe/current-user-DACL security migration and is
not passed or partially claimed here.

The default role-generation admission deadline is ten seconds while one peer's kipc
I/O deadline remains five seconds. A first silent connector is therefore classified
and closed before the generation expires, and a queued legitimate Bun child can still
authenticate. Repeated rejections are coalesced to one diagnostic event per one of the
six redacted rejection classes, bounding hostile telemetry to six records per
generation without blocking lifecycle events. Repeated same-user connectors can still
consume the finite unprivileged loopback admission window; T8 makes no denial-of-service
or named-pipe/DACL claim against that active local adversary.

The primary coordinator's opt-in bound-generation feed transfers one already
authenticated platform stream plus host-only generation and attempt metadata at the
existing successful-admission transition. Callers must not run a second handshake or
derive identity from payload bytes. The ordinary `RoleRegistry` path does not enable
that feed, and its app-bound/virtual-port implementation remains Unix-only pending
KEL-97. Event delivery is observation only: revocation mutates the generation lease
even if the event receiver is not drained.

Each successor must have a generation, endpoint and token distinct from the retired
generation. Provisioning retries a bounded number of freshly OS-minted listeners if
an endpoint or cryptographic token repeats and fails typed lifecycle setup rather than
accepting a collision. A real Windows Bun fixture proves g1 authenticated echo, an
explicit g1 crash, `Revoked(g1)` before `Provisioned(g2)`, stale g1 locator closure,
g1-token rejection on the still-live g2 listener, legitimate g2 authenticated echo,
orderly revoke/reap and a clean next coordinator cycle.

T8 does not implement KEL-96/T4's no-flag Windows host, window/router integration,
CLI-death behavior or product evidence. It also does not prove Windows host-death
descendant cleanup, which remains a separately approved KEL-75/KEL-78 process/job
artifact, or any privileged Windows dispatch.

### Routed virtual ports

The host owns `VirtualPort` pairs. A port capability is bound to one principal
generation and a route target; a receiver must be an already authenticated role or a
live webview/navigation generation selected by host policy. The host performs every
transfer, validates its target and chargeable quota, then forwards bounded frames over
existing links. There is no direct renderer-to-role transport.

The generic contract is FIFO per port generation, one-shot ownership transfer,
host-visible close/disconnect, bounded queues/credits and generation invalidation. The
compatibility facade maps Electron's `MessageChannelMain`, `MessagePortMain`,
`webContents.postMessage` and `utilityProcess.postMessage` into that contract. Exact
Electron event/error shape is established by pinned-oracle conformance fixtures rather
than assumed from this generic transport prose.

### Electron facade boundary

`utilityProcess.fork` is an `@keld/electron` request to the host for an app-bound or
window-bound declared role. It is not a generic child-process escape hatch. The facade
may expose its own OS PID only as a diagnostic after host spawn; no Keld authorization,
reap or routing decision uses it. Requested `env`, working directory, executable flags,
network session and unsigned-library options are translated to reviewed Keld policy or
rejected with a documented compatibility result. `allowLoadingUnsignedLibraries` can
never silently weaken a strict profile.

## 5. Boundaries

- Implement in later slices: `keld-runtime`, `keld-core`, `keld-ipc`, `keld-guard`,
  `keld-compat`, `@keld/api`, `@keld/electron`, and generated schema packages.
- The original specification-only review is confined to the listed architecture documents
  and this file. KEL-75 implementation tasks may touch only their named crate/package
  boundaries; unrelated frame, manifest, native-service, CI, or package-specific changes
  require their owning issue and review gate. T1a is the documented exception: it added
  the reusable `keld-ipc` bootstrap listener, migrated the CLI echo consumer, and used
  the already workspace-pinned `getrandom` dependency. It did not change a wire version,
  add an async runtime, or add a package-specific branch.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1a: Extract a generic Unix bootstrap listener in `keld-ipc`, used by the live
  CLI echo server. It mints an owner-only endpoint/token, continues accepting after an
  invalid `HELLO`, and unlinks on drop. This proves one shared bootstrap primitive, not
  a role coordinator.
- [x] T1b: Reuse KEL-70's `Supervisor` for one Unix host-owned `primary` role
  coordinator through its internal prepared-child/lease seam. It provisions fresh
  identity before every spawn, binds only a successful `HELLO`, revokes before successor
  provisioning, reports redacted bootstrap rejection, and proves the black-box restart
  flow. No Windows named pipe/DACL, ports, extra roles, sandbox or shared memory.
- [x] T2: Add one `app-bound` role and host-owned lifecycle registry; prove independent
  crash/restart isolation and stale-generation rejection.
- [x] T3: Add a bounded host-owned virtual-port pair between two authenticated roles;
  prove transfer, close and cross-principal negative cases before Electron facade code.
- [x] T4a: Freeze the `window-bound` owner-generation state model, close/restart race
  ordering, opaque authority revocation, PID-independent host-death evidence contract and exact
  renderer-continuity beacon. The portable executable trace oracle is contract proof,
  not a shipping implementation or real-webview pass.
- [x] T8: Extend the one T1b generation owner to a Windows `primary`; it
  originally landed over the KEL-96-D10 unprivileged loopback interim. Reuse one cross-platform
  `keld-ipc::BootstrapListener`, expose the authenticated bound generation to its
  future host router, and prove real Bun g1→g2 rotation, stale authority rejection,
  revoke-before-successor, handle-owned shutdown and a clean next cycle. T8 is an
  independently promoted predecessor and does not wait for T5–T7.
  KEL-101/T3 later migrated that shared listener and its consumers to the
  current-user-DACL named pipe without changing this generation-owner contract.
- [ ] T4: Add `window-bound` role lifecycle and real host-window-close integration.
  Consume T4a without adding a second restart loop or treating the event queue as the
  authority ledger. Real macOS, Windows and Linux acceptance remains here (or in an
  explicitly approved successor product node), not in T4a.
- [ ] T5: Add the `utilityProcess`/`MessageChannelMain` compatibility facade through
  pinned Electron conformance entries, one operation family at a time.
- [ ] T6: Integrate KEL-78's approved per-OS sandbox admission and addon-worker proof;
  only then permit a strict-profile release claim.
- [ ] T7: Consider a role-private bulk mapping only after an attributed Keld end-to-end
  benchmark and hostile handle-inheritance proof; inline bounded `RAW` remains required.

## 7. Test plan

| Acceptance | Future fixture | Independent oracle |
|---|---|---|
| T1b subset of 1–3 | real Bun subprocess + owner-only test control socket + hostile raw kipc client | ordered `Provisioned(g1) → Spawned(g1) → LinkBound(g1) → Revoked(g1) → Provisioned(g2) → Spawned(g2) → LinkBound(g2)`; host-only redacted `KELD-IPC-007` rejection; stale g1 locator fails; legitimate g2 client binds after foreign g1-token input |
| T8 Windows subset of 1–2 | real Bun subprocess + loopback test-control socket + shared `BootstrapListener` + authenticated stream echo worker | first silent peer times out without consuming the generation; distinct g1/g2 generation, endpoint and token; real echo on both; exact `Revoked(g1)` before `Provisioned(g2)`; exact stale g1 port can be rebound; g1 token rejected once on g2 without consuming it; shutdown wait succeeds, the same fixture's instance-bound control connection closes, and a clean next coordinator cycle completes |
| 4 / T4a | portable trace model plus direct-child subprocess-isolation helper | exact owner-window tombstone; close at successor prepare/spawn/bind/ready boundaries; every opaque authority class revoked; stale/PID-only/duplicate cleanup rejected; correlated app-bound and other-window replies; admitted-work drain; app shutdown cleans every role; next helper cycle succeeds |
| 4 / T4 | per-backend host integration fixture with two real temporary window owners and hostile child processes | exact identity-qualified close delivery; bound role and descendants gone; app-bound and other-window positive call; clean relaunch |
| 5 / T3 | virtual-port contract fixture | ordered received sequence, exactly-once disconnect, and no foreign delivery |
| 6 | platform-specific hostile sandbox fixture | real OS-visible fs/net/handle attempts; untested OS stays unverified |
| 7 | pinned Electron differential fixture | Electron's documented behavior and checked-in oracle result |
| 8 / T4a | portable trace model | changed/missing/replayed document nonce or missing post-`Ready(g2)` beacon is rejected |
| 8 / T4 | per-backend document-local nonce plus port-0 beacon server | same window/navigation/document nonce before crash and after successor readiness, strictly increasing beacon sequence |

Fixtures use temporary owner-only endpoints, bind ports to `0`, wait for explicit
readiness records, and use timeouts only as kill switches. Crash/lifetime assertions run
the risky child out of process. T1 does not claim renderer continuity: current
host/webview code has no persistent window registry plus renderer acknowledgement oracle.
T4a proves only that the portable contract accepts the complete trace and rejects
counterexamples. T4 must prove the document nonce and post-restart beacon on each real
backend it claims. The test author performs a temporary negative control by reusing a
token, binding before `HELLO`, closing after one invalid `HELLO`, removing generation
revocation, dropping the close tombstone or accepting a changed/missing document nonce,
and records the named failing test.

Acceptance classification for T4a is fixed:

- `CI-only`: the trace state model, exact counterexamples and direct subprocess
  hostile-shutdown/next-cycle isolation in
  `crates/keld-runtime/tests/window_bound_contract.rs`;
- `not-applicable`: native close-event delivery, real renderer continuity and platform
  process-tree reaping. These remain future T4 product evidence; KEL-78 owns strict
  sandbox admission, and KEL-97/runtime-04 must re-check its own shipping/OS entry gate;
- no real-OS criterion is owned or passed by T4a.

Acceptance classification for T8 is separate:

- `CI-only`: shared generation-owner ordering, cross-platform compilation and
  existing Unix bootstrap/role regression coverage;
- `real OS/device`: Windows 11 with pinned Bun 1.4.0 — both authenticated
  generations complete an echo after a first silent peer, stale g1 authority fails,
  revoke precedes g2, shutdown wait succeeds, that Bun instance's control connection
  closes and a fresh same-fixture cycle succeeds;
- `not-applicable`: KEL-96/T4 no-flag host/window behavior, KEL-101 named-pipe/DACL,
  privileged dispatch, strict OS containment and Windows abnormal-host-death cleanup.

The T4a assertions are named so review can repeat the negative control instead of
accepting a prose claim:

| Deleted or inverted rule | Test that must fail |
|---|---|
| remove fresh successor generation | `restart_rotates_generation_after_full_authority_revocation` |
| reset the generation counter by recreating a coordinator | `coordinator_recreation_cannot_reset_role_generation` |
| provision before locator/link/dispatch/grant/port/mapping/pending-call revocation | `restart_rejects_successor_before_full_authority_revocation` |
| revoke before portable natural-exit reap | `natural_exit_is_observed_and_reaped_before_revocation` |
| infer ready before spawn/authenticated bind or after owner close | `ready_requires_spawn_authenticated_link_and_live_owner` |
| publish ready for a post-bind/pre-ready failure or recover before revocation | `post_bind_pre_ready_failure_revokes_before_recovery` |
| reap an admission failure before revocation or restart that terminal failure | `admission_failure_revokes_before_reap_and_is_terminal` |
| reap a post-bind protocol failure before revocation or restart that terminal failure | `protocol_failure_revokes_before_terminate_and_is_terminal` |
| accept a retired generation | `stale_generation_is_rejected_after_rotation` |
| broaden a close to app-bound/other-window roles | `closing_one_window_revokes_only_its_bound_role` |
| omit the close tombstone at prepare/spawn/bind/ready | `close_wins_at_every_successor_boundary` |
| revoke before quiesce, drain before route/grant invalidation, close before reap, or reap before link/locator revocation | `window_close_revokes_routes_before_drain_and_link_before_reap` |
| admit new work after quiesce or finish drain with admitted work outstanding | `drain_completes_admitted_work_and_rejects_new_work_after_quiesce` |
| accept a changed/missing/replayed nonce or no post-restart beacon | `continuity_requires_same_document_nonce_and_post_restart_beacon` |
| accept PID-only reap or leave host-death authority live | `host_death_requires_full_revocation_and_handle_bound_reap` |
| stop an app session with a live authority/handle or schedule a successor after quiesce | `application_shutdown_revokes_and_reaps_every_role_before_stop` |
| stop cleanup after the first failure or discard the aggregated terminal failure | `application_shutdown_continues_after_cleanup_failure` |
| run hostile shutdown in the test runner or break relaunch | `hostile_shutdown_is_subprocess_isolated_and_next_cycle_succeeds` |

T8's real-Windows negative controls temporarily remove revoke-before-successor,
reuse the retired endpoint or token, stop accepting after a foreign `HELLO`, and
disable supervisor shutdown before the clean next cycle. Each mutation must make
`windows_primary_restart_rotates_authenticated_generation_and_rejects_stale_authority`
fail and must be restored before review.

The completed node publishes `keld.execution-artifact/v1` with
`node_id=role-lifecycle-contract`, `issue_id=KEL-75`,
`approved_task_id=KEL-75/T4a`, `status=passed`, an ancestor-of-current-main
`head_sha`, the exact prompt source/body digest, CI-only T4a acceptance rows and
explicit `not-applicable` product-OS rows. That artifact records contract completion;
it cannot mark T4 implemented, satisfy a KEL-102 approval, or substitute for
KEL-97/runtime-04's own entry gate and real-OS observations.

The completed T8 node publishes a separate `keld.execution-artifact/v1` with
`node_id=windows-primary-generation`, `issue_id=KEL-75`,
`acceptance.id=KEL-75/T8`, `status=passed`, an ancestor-of-current-main
`head_sha`, the real Windows/Bun observation and explicit not-applicable rows for
KEL-96/T4, KEL-101, privileged dispatch, containment and host-death cleanup.

## 8. Review gates triggered

- unsafe: none in this specification; later platform sandbox and shared-memory changes
  require review.
- public API: yes — the original role configuration, generated policy and virtual-port
  contract, T4a's lifecycle/renderer observable, and T8's cross-platform bootstrap
  stream plus bound-generation handoff require human review.
- permission model: yes for the original role-specific grant subset/elevation contract.
  The T4a/T8 amendments add no grant representation or evaluator decision and therefore
  do not approve or implement privileged Windows dispatch or another permission model.
- dependency addition: none in this specification, T4a or T8.
- wire protocol: no new frame in this specification, T4a or T8; later virtual-port channel,
  readiness signal or handshake metadata changes require their own versioned wire
  review.

## 9. Perf impact

T1 adds a cold bootstrap listener and is not a hot-path optimization. T1–T5 measure only after semantic correctness:
role spawn/restart latency, p99 bounded routed-port latency, queued-byte maxima, CPU and
RSS. Shared memory is not a baseline or an acceptance condition; see the committed
P13 new-run evidence and `docs/research/campaigns/vscode/reports/48-p13-new-run-audit.md`.

## 10. Open questions

- KEL-78 must select and prove the platform lifecycle/reaping mechanism used to ensure
  host death cannot orphan role descendants on each supported OS.
- KEL-74 must freeze the versioned compatibility-record format that stores the Electron
  oracle revision, fixture artifact hash and operation-level result.
- KEL-101 must separately approve and prove the named-pipe/current-user-DACL
  transport before privileged Windows dispatch can replace T8's explicitly
  unprivileged loopback predecessor.
