# KEL75 role identity, grants and lifecycle

Part of: [index](index.md)
Owns: sections 2–4

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

## 3. Acceptance criteria

### AC-1: Spawn binds a fresh principal generation

- **Given:** the Host has an admitted declared role instance.
- **When:** the Host spawns that role.
- **Then:** it mints a fresh endpoint, 32-byte possession secret and principal generation before spawn; a valid `HELLO` binds that exact generation.
- **Negative control:** reuse one preceding generation across the next spawn.
  - **Single fault:** provision the previous generation identity.
  - **At-risk precondition:** a declared role is ready to spawn.
  - **Expected RED:** the generation and `HELLO` binding observer rejects the reused identity.

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

## 4. Design: roles and lifecycle

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

GH452 T2a (owner-delegated 2026-11-01, comment 6549729612) adds a separate, non-authority
development-launch fact record through the existing private macOS Guardian argument
and qualified Bun constant/preload path. Its exact shape, bound, capture and absence
rules are owned by `gh452-app-session-facts/contract.md` T2a (design §4). `KELD_APP_LINK` remains the sole
bootstrap environment variable; no extra link, identity, grant or restart owner is added.

GH452 T2b (delegated PR1362 comment 6550291308) extends that same private data owner
with explicit compiled version metadata and native macOS directory values, under
`gh452-app-session-facts/contract.md` T2b (design §4). The schema-2 record, including nested paths, is closed
object data and is frozen once for all Bun generations; schema 1 remains readable.
Absent input is also selected explicitly by the Host's Bun constant, not an application
global. These additions do not qualify an installed source graph, grant access to any
directory, change role identity or add a second startup/restart owner.

GH452 T2c (`gh452-app-session-facts/contract.md` T2c (design §4)) adds the Host process's first native
preferred-language identifier under private facts schema 3. Schemas 1/2 remain
readable with their original closed fields. Locale is immutable non-authority data,
not an environment input, negotiated renderer locale or new restart owner.

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
