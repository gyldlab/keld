# Spec: principalized Bun child roles and virtual-port routing

Status: draft
Linear: KEL-75 · Owner: GYLDLAB · Updated: 2026-08-19

## 1. Goal & non-goals

Keld needs a host-owned family of independently supervised Bun roles while retaining
one authority root, default-deny dispatch, and an app-agnostic runtime core. Each role
instance must receive a new host-minted principal generation and a private authenticated
link. The host, rather than a role, creates roles, virtual ports, windows and privileged
resources. This specification makes the later Electron `utilityProcess` and
`MessageChannelMain` facades consumers of that generic model.

Non-goals:

- Implementing `keld-runtime`, `@keld/api`, `@keld/electron`, shared memory, or an OS
  sandbox in this change.
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

The architecture files describe destination behavior. The live v0 slice remains one
CLI-owned, token-authenticated echo child and does not implement this specification.
KEL-70 remains the completed single-primary-child slice; it does not acquire named-role
or virtual-port behavior through this specification. KEL-72 remains an app-lifecycle
shim slice; it does not acquire a process model, privileged child channel or role grant.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a declared role instance, when the host spawns it, then the host creates a
   fresh endpoint, 32-byte possession secret and principal generation before spawn;
   the link is bound to that exact generation only after a valid `HELLO`.
2. Given a role restart, when the old role tries its token, endpoint or virtual-port
   capability after revocation, then the host rejects it; a newly spawned generation
   authenticates with a different token and principal instance under the same declared
   policy.
3. Given two live roles with different principals, when role A sends a call, a port
   transfer, a cancellation or a stale reference naming role B, then role B receives no
   call and neither role's authority changes. A valid role-B workflow remains live.
4. Given an app-bound and a window-bound role, when their owning window closes, then
   only the window-bound role and its routes drain and stop; the app-bound role remains
   live until its application session stops. When the host dies, every child is reaped
   by the platform lifecycle mechanism; numeric PID reuse cannot target a later process.
5. Given a virtual port pair, when the sender transfers one end, then ownership moves
   once to the host-approved target principal; duplicate, self, source, closed and
   stale-generation transfers fail without delivery. FIFO ordering is preserved per
   live port generation, and closing either end gives the peer exactly one disconnect
   observation.
6. Given a role whose strict profile cannot be admitted, when strict execution is
   requested, then the host does not start an unconfined role. A separately declared
   legacy profile remains authenticated and guarded but reports that the strict claim is
   unavailable.
7. Given an Electron facade fixture against the pinned oracle, when it exercises the
   selected `utilityProcess` and `MessagePortMain` behaviors, then the facade's
   observable events, transfer validation, queue/start behavior and disconnect behavior
   match the recorded conformance entry. An operation without an oracle remains unknown.

## 4. Design

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

The host creates an opaque `RoleInstance` for every spawn. It comprises a declared role
reference plus a monotonic host-only generation; it is not a PID, bootstrap token,
socket name or frame field. The configuration supplies the entrypoint through the host's
trusted bundle resolution. `KELD_APP_LINK` remains the sole child bootstrap variable;
it conveys only `<endpoint>#<64 hex chars>`, never role/principal/grant metadata.

The child may receive stdout/stderr sinks only for supervisor-managed logging. Those
fixed-direction log streams are not authority handles. All other inherited descriptors
and handles are closed by default. The endpoint is private, one successful accept
consumes its bootstrap generation, and platform peer authentication is an additional
admission check rather than a replacement for the possession secret.

### Permissions and lifecycle

`keld.config.ts` declares an entry and lifecycle owner; `keld.permissions.jsonc`
declares capability ceilings. The generated role-policy record is a subset of the app
ceiling by default. An exception must be represented as an explicit, reviewable
role-specific grant with its own capability diff; it cannot arise from role name,
`utilityProcess` options, a caller payload or an environment value. Both schema changes
are versioned permission/public-API review gates.

The supervisor owns the process handle and reap operation. It never kills by a bare
PID after an exit. On exit, handshake failure, protocol abuse, drain deadline or host
shutdown, it first revokes the role generation, its link, grants, virtual-port
capabilities and optional mapping handles; it then settles/reroutes observable work as
the caller contract requires. A restart always starts at provisioning with fresh
identity. `app-bound` describes ownership by the host's logical app session—not a child
process tree—so a primary-role restart does not silently grant it control over another
role's lifetime.

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
- This specification change touches only the listed architecture documents and this
  file. It must not add runtime code, a new dependency, a wire version, an async runtime
  or a package-specific branch.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1: Implement one host-owned `primary` role with a fresh link/principal
  generation, authenticated handshake, reaping and a black-box restart test. No ports,
  extra roles, sandbox or shared memory.
- [ ] T2: Add one `app-bound` role and host-owned lifecycle registry; prove independent
  crash/restart isolation and stale-generation rejection.
- [ ] T3: Add a bounded host-owned virtual-port pair between two authenticated roles;
  prove transfer, close and cross-principal negative cases before Electron facade code.
- [ ] T4: Add `window-bound` role lifecycle and real host-window-close integration.
- [ ] T5: Add the `utilityProcess`/`MessageChannelMain` compatibility facade through
  pinned Electron conformance entries, one operation family at a time.
- [ ] T6: Integrate KEL-78's approved per-OS sandbox admission and addon-worker proof;
  only then permit a strict-profile release claim.
- [ ] T7: Consider a role-private bulk mapping only after an attributed Keld end-to-end
  benchmark and hostile handle-inheritance proof; inline bounded `RAW` remains required.

## 7. Test plan

| Acceptance | Future fixture | Independent oracle |
|---|---|---|
| 1–3 | subprocess role fixture that prints its assigned test phase only after handshake | host log/exit status plus rejected stale/foreign calls |
| 4 | host integration fixture with a real temporary window owner and child processes | observed role exits and unaffected app-bound role call |
| 5 | virtual-port contract fixture | ordered received sequence, exactly-once disconnect, and no foreign delivery |
| 6 | platform-specific hostile sandbox fixture | real OS-visible fs/net/handle attempts; untested OS stays unverified |
| 7 | pinned Electron differential fixture | Electron's documented behavior and checked-in oracle result |

Fixtures use temporary owner-only endpoints, bind ports to `0`, wait for explicit
readiness records, and use timeouts only as kill switches. Crash/lifetime assertions run
the risky child out of process. The test author performs a temporary negative control by
removing generation revocation or route-target validation and records the failing test.

## 8. Review gates triggered

- unsafe: none in this specification; later platform sandbox and shared-memory changes
  require review.
- public API: yes — role configuration, generated policy and virtual-port contract.
- permission model: yes — role-specific grant subset/elevation semantics.
- dependency: none in this specification.
- wire protocol: no new frame in this specification; later virtual-port channel or
  handshake metadata changes require versioned wire review.

## 9. Perf impact

No current runtime performance impact. T1–T5 measure only after semantic correctness:
role spawn/restart latency, p99 bounded routed-port latency, queued-byte maxima, CPU and
RSS. Shared memory is not a baseline or an acceptance condition; see the committed
P13 new-run evidence and `docs/research/48-p13-new-run-audit.md`.

## 10. Open questions

- Human approval is required before implementation begins.
- KEL-78 must select and prove the platform lifecycle/reaping mechanism used to ensure
  host death cannot orphan role descendants on each supported OS.
- KEL-74 must freeze the versioned compatibility-record format that stores the Electron
  oracle revision, fixture artifact hash and operation-level result.
