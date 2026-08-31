# Spec: Windows named-pipe app-link with current-user DACL

Status: approved
Linear: KEL-101 · Owner: GYLDLAB · Updated: 2026-08-31

Approval provenance: direct operator decisions in KEL-101 comments
`eaed1777-00e2-480a-b512-fd22e564c18d` (T1 binding, unsafe boundary, and
foreign-user fixture) and `e0f20ee7-a55b-479b-b673-07e3f2676fb2` (exact T2
slice). T1's governance change does not itself claim T2 or T4 behavior.

## 1. Goal & non-goals

Replace the live Windows loopback-TCP app-link with one host-owned named pipe,
`\\.\pipe\keld-<independent-random>`, whose explicit DACL admits the host's
current user and whose kipc v2 `HELLO` still proves possession of the fresh
32-byte session token. The observable result is: a foreign user cannot open the
pipe, a same-user process without the token cannot dispatch a frame, and the
intended Bun child can complete `HELLO` and an echo call.

Non-goals:

- No implementation, Cargo change, dependency addition, or wire-version change
  is part of T1. The approved T2 slice may implement only the boundary frozen
  below after T1 lands and receives its own review.
- No role/principal identity is carried by the pipe name, PID, DACL,
  environment, or frame. KEL-75 remains the owner of post-admission role binding.
- No Windows sandbox, AppContainer/LPAC policy, job-object policy,
  cross-session guarantee, shared memory, multi-client service, or credit window.
- No fallback to loopback TCP after pipe creation or admission fails.
- No claim that the DACL protects against an administrator or a malicious
  same-user process. The token remains mandatory for the latter.

## 2. Spec refs

- `docs/architecture/02-ipc.md` §§1, 2, 7 — app-link topology, v2 `HELLO`,
  deadlines, and destination Windows transport.
- `docs/architecture/03-security.md` §§1, 4 — host-minted identity and
  guard-before-handler.
- `docs/architecture/06-runtime-and-tooling.md` §1 — canonical
  `KELD_APP_LINK=<endpoint>#<64 hex chars>` bootstrap contract.
- `docs/specs/kel75-principalized-bun-child-roles.md` §§3–4, 7 — reusable
  bootstrap admission, redacted rejection, and no inherited authority handle.
- KEL-60 — v2 token defense that must not regress.

Microsoft's OS contract is authoritative: a named-pipe security descriptor
controls both ends, and Windows compares a connecting client's access token with
its DACL ([Named Pipe Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)).
`CreateNamedPipeW` accepts that descriptor, supports a one-instance pipe and
remote-client rejection, and deletes an instance after its last handle closes
([CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew)).

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a Windows host provisioning one generation, when it creates the
   app-link, then it owns exactly one `\\.\pipe\keld-<64 lowercase hex>`
   instance, created before child spawn from a nonce independent of the token;
   the child receives only `KELD_APP_LINK`.
2. Given that instance, when its descriptor is read back, then its protected
   DACL has exactly one allow ACE for the host `TokenUser` SID with final
   serialized mask `0x0012_019B`: the future adapter explicitly ORs the
   `PipeAccessRights::ReadWrite`-equivalent base (`0x0002_019B`) with
   `SYNCHRONIZE` (`0x0010_0000`) before descriptor construction, and must not
   assume a wrapper adds that right. It reads back and compares that final mask
   exactly; it has no `FILE_CREATE_PIPE_INSTANCE`/`FILE_APPEND_DATA` bit (`0x4`),
   inherited ACE, or allow ACE for Everyone, Anonymous, Users, or Administrators.
3. Given a different ordinary Windows user, when it calls `CreateFileW` with
   the client mask, then Windows returns `ERROR_ACCESS_DENIED`; no host session
   is accepted, and the intended child can subsequently authenticate.
4. Given a same-user client with pipe name but no token, when it sends a complete
   empty, wrong-length, or foreign `HELLO`, then it receives no host `HELLO`, `ERR`,
   or echo reply; the host records exactly one redacted `HelloAuth`/
   `KELD-IPC-007` record, disconnects that client, and continues accepting until the
   legitimate child succeeds or admission expires. Every other pre-authentication
   failure has the exact existing-code host record in §4's admission mapping: EOF or
   non-timeout I/O is `KELD-IPC-001`; a started partial frame that reaches
   `APP_LINK_IO_DEADLINE` is `KELD-IPC-006`; a malformed header is
   `KELD-IPC-002`; an oversized envelope is `KELD-IPC-004`; and a well-formed
   non-`HELLO`/wrong-reserved-fields frame is `KELD-IPC-005`. None is `HelloAuth`.
   They receive no host frame, clean up, and re-accept while generation time remains.
5. Given the intended Bun child and complete link, when it connects with
   `node:net` and sends the matching v2 `HELLO`, then the host replies only after
   verification and one echo succeeds. Header bytes, protocol version 2, and
   32-byte raw `HELLO` are unchanged.
6. Given cancellation, host shutdown, deadline, or a silent partial `HELLO`,
   when the operation completes, then no frame dispatch occurs, the worker joins
   without a sleep, and no peer/log/error receives the token or its hex form. In
   particular, if `APP_LINK_IO_DEADLINE` expires before the generation deadline,
   the host records `KELD-IPC-006`, disconnects and re-accepts on the **same** pipe
   instance, and a valid child can still authenticate; if the generation deadline
   wins, it is terminal and no reconnect occurs.
7. Given valid admission followed by session completion, child crash, or host
   shutdown, when the server handle closes, then it never re-listens after valid
   `HELLO`; a stale locator cannot form another session and a successor has a
   different endpoint and token.
8. Given the server handle before spawn, when inspected, then
   `HANDLE_FLAG_INHERIT` is clear. The child opens its own client handle by name;
   it never inherits the server handle.
9. Given a legacy `<decimal-port>#<token>` diagnostic link, when a current
   client consumes it during migration, then it remains an explicit compatibility
   path only. A new Windows host never mints a port or falls back to TCP; an
   unrecognized endpoint is `KELD-IPC-007`.
10. Given a transport open failure, deadline, or token failure, then it maps to
    `KELD-IPC-001`, `KELD-IPC-006`, or `KELD-IPC-007` respectively. No new wire
    error exists and no error echoes a token or the full `KELD_APP_LINK` string.

## 4. Design

### First-principles and reuse decision

After KEL-75/T8, `keld_core::EchoServer` and the Windows primary-generation
coordinator both delegate token minting, `127.0.0.1:0` lifecycle,
retry-after-bad-HELLO semantics and consumed-locator cleanup to the same
`keld_ipc::BootstrapListener`. That live backend is still explicitly
unprivileged loopback. This specification replaces its Windows transport with
the named-pipe/DACL backend; it does not add another bootstrap owner.

`keld-ipc` owns the Windows bootstrap primitive, name/DACL construction,
cancellation, and deadline mapping. `keld-core`/the future `keld-host` owns the
generation and supplies the rejection observer; it must not create a second pipe
or ACL. The child owns only a client handle it opens after spawn. The DACL proves
the caller's current-user SID; the `HELLO` token proves possession; neither is a
Keld role identity. KEL-75 binds host-minted identity only after authentication.

| Boundary | Proves | Does not prove |
| --- | --- | --- |
| Pipe DACL | Client access token satisfies current-user ACE | Intended child, role, token possession, or sandbox |
| `PIPE_REJECT_REMOTE_CLIENTS` | Remote client rejection | Same-user local or another same-user terminal-session denial |
| v2 `HELLO` | Possession of generation secret | Principal identity |
| KEL-75 coordinator | Host-minted link metadata | OS containment |

Extend the shared `BootstrapListener` under `cfg(windows)`; do not create an
`EchoServer`-local alternative. Unix behavior stays unchanged. This is a cold
security-correctness change and makes no performance claim.

### Approved binding and unsafe boundary

T1 selects the smallest reuse-compatible Windows boundary:

- Reuse workspace-pinned `windows-permissions = 0.2.4` for the current
  `TokenUser` SID, self-relative protected security-descriptor construction,
  and handle-based DACL readback. KEL-78 already introduced and reviewed this
  safe wrapper; T2 must not add a second SID/DACL parser or builder.
- Use workspace-pinned Microsoft `windows-sys = 0.61.2` directly only in
  `crates/keld-ipc/src/windows_named_pipe.rs` for named-pipe creation,
  overlapped connect/read/write, completion waits/cancellation, disconnect,
  handle flags, and handle ownership. Rust std exposes none of those complete
  contracts, and `windows-permissions` does not wrap the pipe/overlapped state
  machine.
- `bootstrap.rs` remains the platform-neutral owner and delegates its Windows
  transport mechanics to that module. Wire framing, codec, token, handshake,
  and admission classification remain safe shared code; the ABI module must not
  parse or authorize a kipc frame.
- The ABI module denies `unsafe_op_in_unsafe_fn`; every unsafe block carries a
  local pointer/buffer/handle lifetime proof. T2 adds target-only direct
  dependencies from `keld-ipc` to the two existing workspace pins and therefore
  still triggers dependency, unsafe, public-API, and permission-model review.

Rejected alternatives: std cannot express exact named-pipe security and
overlapped cancellation; `windows-permissions` alone lacks the pipe state
machine; an `EchoServer`-local adapter would duplicate the live
`BootstrapListener`; raw ACL construction would duplicate the reviewed safe
wrapper; and an async/thread-per-operation transport violates the hot-path and
bounded-cancellation contracts. No compatibility fallback is required for new
hosts: decimal loopback remains a client-only diagnostic migration path until
T5 and a pipe failure never downgrades to TCP.

### Pipe identity and DACL

1. Mint independent 32-byte `PipeNonce` and `SessionToken` values. Endpoint is
   `\\.\pipe\keld-<nonce in 64 lowercase hex>`; bootstrap remains
   `format_app_link(endpoint, token)`. The name contains no `#` and nonce is
   never derived from the secret.
2. Use `OpenProcessToken(..., TOKEN_QUERY)` and `GetTokenInformation(TokenUser)`;
   copy that SID and build a protected explicit DACL with one `AccessAllowed` ACE.
   Do not use a null/default descriptor. The default named-pipe descriptor can
   grant broader access, including Everyone/anonymous
   ([SECURITY_ATTRIBUTES](https://learn.microsoft.com/en-us/windows/win32/api/wtypesbase/ns-wtypesbase-security_attributes)).
3. The future adapter explicitly serializes the `PipeAccessRights::ReadWrite`-
   equivalent base (`0x0002_019B`) OR `SYNCHRONIZE` (`0x0010_0000`) as the final
   `0x0012_019B` ACE mask: `FILE_READ_DATA`, `FILE_WRITE_DATA`, read/write EA,
   read/write attributes, `READ_CONTROL`, and `SYNCHRONIZE`. Descriptor read-back
   compares that final mask; the contract does not rely on a wrapper adding
   `SYNCHRONIZE`. It excludes `FILE_CREATE_PIPE_INSTANCE`. Microsoft warns that
   `FILE_GENERIC_WRITE`
   includes the overlapping append/create-instance bit; individual rights are
   required ([Named Pipe Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)).
4. Create one byte-stream instance with
   `PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED`,
   `PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS`,
   and `nMaxInstances = 1`. Do not request `WRITE_DAC`, `WRITE_OWNER`, or
   `ACCESS_SYSTEM_SECURITY`; `PIPE_NOWAIT` is forbidden.
5. Set `SECURITY_ATTRIBUTES.bInheritHandle = FALSE` and fail host setup if
   `GetHandleInformation` shows `HANDLE_FLAG_INHERIT`. Microsoft requires both
   an inheritable handle and `CreateProcess(..., TRUE)` for inheritance
   ([Handle Inheritance](https://learn.microsoft.com/en-us/windows/win32/sysinfo/handle-inheritance),
   [GetHandleInformation](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-gethandleinformation)).

This is a **current-user** boundary, not a logon-session boundary. Microsoft
recommends a logon SID when a service must exclude remote or different terminal
services sessions. Keld v0 instead rejects remote clients at the pipe mode and
uses `HELLO` to distinguish same-user processes; the shipped claim must say so.

### Admission, deadlines, cancellation, and cleanup

Call `ConnectNamedPipe` as overlapped I/O with a manual-reset completion event.
Wait in priority order on host cancellation, generation deadline, then connect
completion. If connect is pending, call `CancelIoEx(handle, &overlapped)` and
observe `GetOverlappedResult` before freeing `OVERLAPPED` state. Cancellation is
not complete merely because `CancelIoEx` returns: it can race a normal I/O
completion ([CancelIoEx](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex)).

Treat `ERROR_PIPE_CONNECTED` as a successful connection; Microsoft documents the
[create/connect race](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe).
If cancellation/deadline wins, close that connection before handshake. Each `HELLO`
uses the shorter of remaining generation deadline and `APP_LINK_IO_DEADLINE`. The
Windows adapter must supply overlapped read/write waits with the existing overall
started-frame deadline; `std::io::Read` alone is not claimed to provide a named-pipe
timeout.

Until a valid `HELLO` consumes bootstrap, every handshake result other than host
cancellation or elapsed generation deadline is non-terminal. Each rejected connection
produces exactly one redacted host `BootstrapRejection` record: no endpoint, token,
raw bytes, OS-error detail, or second Windows-only logging taxonomy. The future shared
observer extends its current `HelloAuth` record with the existing `IpcError` classes
and their existing codes: `Io` (`KELD-IPC-001`), `Header` (`KELD-IPC-002`),
`PayloadTooLarge` (`KELD-IPC-004`), `Protocol` (`KELD-IPC-005`), and `Timeout`
(`KELD-IPC-006`). `HelloAuth` remains `KELD-IPC-007`. This is a future public-API
change under the already-declared public-API review gate, not a new error value or
wire-message type.

Before a successful `HELLO`, the host never writes `HELLO`, `ERR`, or an echo reply.
Thus the wire result for every rejected connector is close-without-reply. A connector
which waits to read after that host close observes its local existing
`IpcError::Io`/`KELD-IPC-001`; a connector that caused physical EOF has no response to
observe. The redacted host record, not a new pre-auth wire error, distinguishes the
failure classes:

| Pre-authentication failure | Host observer record | Connector-visible result |
| --- | --- | --- |
| EOF before a complete header/payload, or non-timeout handshake read/write I/O | `Io` / `KELD-IPC-001` | close without reply; a subsequent client read is local `KELD-IPC-001` |
| Started partial header/payload remains open until the shorter handshake deadline expires | `Timeout` / `KELD-IPC-006` | close without reply; a subsequent client read is local `KELD-IPC-001` |
| Bad magic, version, or kind in the frame header | `Header` / `KELD-IPC-002` | close without reply; a subsequent client read is local `KELD-IPC-001` |
| Decoded envelope length exceeds `MAX_FRAME_LEN` | `PayloadTooLarge` / `KELD-IPC-004` | close without reply; a subsequent client read is local `KELD-IPC-001` |
| Valid envelope but non-`HELLO`, or `HELLO` with nonzero reserved channel/correlation | `Protocol` / `KELD-IPC-005` | close without reply; a subsequent client read is local `KELD-IPC-001` |
| Empty, wrong-length, or foreign 32-byte `HELLO` token | `HelloAuth` / `KELD-IPC-007` | close without reply; a subsequent client read is local `KELD-IPC-001` |

For each non-terminal result, the adapter completes or calls `CancelIoEx` on every
pending overlapped read/write and observes each completion with `GetOverlappedResult`
before it reuses that `OVERLAPPED` state. It then calls `DisconnectNamedPipe` and
issues a new overlapped `ConnectNamedPipe` on the same instance. If cancellation or
the generation deadline wins during that cleanup, it does not reconnect and instead
closes the instance. Windows requires disconnect before reconnecting an instance
([DisconnectNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-disconnectnamedpipe)).
On valid `HELLO`, consume bootstrap: the connected server handle becomes the
session and is never disconnected/relisted. On any terminal lifecycle event,
close it. The object then disappears when its last handle closes; a successor
always mints a new nonce, token, DACL, and pipe before child spawn.

### Migration and errors

The wire and bootstrap syntax do not change:

```text
KELD_APP_LINK=<endpoint>#<64 lowercase hex token>
```

The Windows child selector accepts only an exact `\\.\pipe\keld-` endpoint or a
decimal legacy diagnostic port. Pipe endpoints use Bun `node:net`
`createConnection({ path: endpoint })`. New hosts produce pipe form only; they
never downgrade. Unknown forms fail locally as `KELD-IPC-007` without echoing the
supplied string.

| Condition | Classification | Disclosure |
| --- | --- | --- |
| `CreateNamedPipeW`/`CreateFileW`/close error, including DACL denial | `KELD-IPC-001` | Never token or full link |
| Connect, HELLO, started-read, or started-write deadline | `KELD-IPC-006` | Never token/raw prefix |
| Invalid token | Peer sees close/`KELD-IPC-001`; host observer sees `KELD-IPC-007` | Host does not send HELLO first |
| Invalid endpoint/token text before connect | `KELD-IPC-007` | State syntax only |

### Required Electron migration conformance entry

Before accepting the migration compatibility tests in §7 AC 9–10, record this
conformance entry. It is a process-boundary entry, not a claim that Electron defines
Keld's endpoint or token protocol.

| Entry | Electron documentation oracle | Keld migration contract | Status and falsifiable test |
| --- | --- | --- | --- |
| `KEL101.electron-main-node-app-link` | [Electron Process Model: main process](https://www.electronjs.org/docs/latest/tutorial/process-model#the-main-process) says Electron's main process runs in a Node.js environment with Node APIs, while renderer code has no direct Node API access. | A migrated Electron-main entry may use its Node-capable main-process equivalent to open the host-provided `KELD_APP_LINK` through `node:net`; no renderer or preload receives the endpoint, token, or pipe handle. Electron supplies no equivalent host-issued app-link, so Keld's endpoint/token is an explicit `▲` migration contract, not an Electron behavior match. | Planned until T3. A Windows main-process migration fixture proves pipe and legacy decimal parsing from the main entry, and a renderer/preload fixture proves the link value is absent there. It fails if the test exposes `KELD_APP_LINK` outside the child main entry or calls the parser before this entry is recorded. |

### Local compatibility evidence (not security proof)

On 2026-08-23, a temporary Windows 11 build 26200 probe used
Bun `1.4.1-canary.1+abe2ad4f0` and `node:net` to open a .NET
`NamedPipeServerStream`, write `KI`, and close. A
protected DACL granting the current `TokenUser` SID produced:

| ACE mask | Result |
| --- | --- |
| `0x0010_0003` (`ReadData + WriteData + Synchronize`) | no server accept; Bun exit 2 |
| `0x0012_019B` (`ReadWrite + Synchronize`) | accepted `KI`; Bun exit 0 |
| `0x0012_019F` (`ReadWrite + CreateNewInstance + Synchronize`) | accepted `KI`; Bun exit 0 |

This establishes why `0x0012_019B` is selected and why create-instance access
is unnecessary for that pinned client. It does not prove foreign-user denial,
production Rust behavior, cleanup, or a shipped Keld transport.

### v0 and destination claim update plan

Until implementation plus §7 Windows evidence land, architecture 02 remains
correct: Windows v0 is loopback TCP and named pipe/DACL is destination work. This
draft changes no architecture wording.

The implementation PR updates `docs/architecture/02-ipc.md` §1 and
`docs/onboarding/04-wire-formats-and-contracts.md` only after all Windows tests
pass. Their LIVE wording must say: “Windows app-link is a host-owned
one-instance named pipe with explicit current-user DACL, remote-client rejection,
and mandatory v2 HELLO token,” and must retain the same-user limitation. KEL-60
remains Done for token possession only; KEL-101 closes the OS-object divergence.

## 5. Boundaries

- Approved T2 implementation: the shared Windows `BootstrapListener` transport
  and focused Windows fixtures in `crates/keld-ipc` only. T2 does not migrate a
  product consumer or update architecture LIVE text.
- Later T3 atomically migrates `keld-core` echo, Windows diagnostics, and the Bun
  template/client; T4 owns the foreign-user product evidence and claim-update
  docs above.
- This T1 governance PR may change only this specification, the root and
  `keld-ipc` instruction owners, their measured instruction inventory if needed,
  generated documentation, and compiler-only `forbid(unsafe_code)` attributes
  on the shared framing/codec/handshake modules. It must not change executable
  Rust/TypeScript behavior, Cargo manifests/lockfile, architecture LIVE text,
  CI, KEL-102 materials, guard policy, Unix transport, frame layout, or protocol
  version.
- Reuse `SessionToken`, `format_app_link`/`parse_app_link`, handshake state machine,
  `BootstrapRejectionObserver`, and `APP_LINK_IO_DEADLINE`; do not copy them into
  a Windows parser or authorization path.

## 6. Tasks (each ≈ one PR; ordered; no placeholders)

- [x] T1: approve the reused safe DACL wrapper plus narrow raw pipe/overlapped
  ABI owner, foreign-user fixture, review gates, and rollback boundary.
- [ ] T2: add Windows shared bootstrap, exact DACL, overlapped state machine,
  cancellation, redacted rejection, and inheritance proof with focused tests.
- [ ] T3: atomically migrate echo server, diagnostics, and Bun template/client;
  retain only explicit decimal-port diagnostic consumption and prove bad-then-good HELLO.
- [ ] T4: run different-user Windows fixture and full Windows suite; only then
  update LIVE wording. If it cannot run in PR CI, make it a required release/weekly gate.
- [ ] T5: create a separately scoped ticket to remove decimal compatibility once
  evidence shows no supported producer remains.

## 7. Test plan

| AC | Test and independent oracle |
| --- | --- |
| 1–2 | Windows `keld-ipc` integration test uses `GetSecurityInfo`, enumerates ACEs, and byte-compares SID/mask. Removing the protected explicit DACL is the negative control. |
| 3 | Pre-provisioned non-admin foreign-user helper calls `CreateFileW` with `0x0012_019B` and must observe `ERROR_ACCESS_DENIED`; `ConnectionRefused`/not-found are not DACL proof. It then permits a valid same-user child run. No fork or normal PR uses its credential. |
| 4–5 | Real-pipe raw clients cover each row in §4's admission mapping: EOF/non-timeout I/O, partial-frame timeout, bad header, oversized envelope, wrong `HELLO` state, and empty/31-byte/foreign token. Each asserts no host `HELLO`/`ERR`/echo reply, the exact one redacted host observer record, cleanup, and same-instance re-accept before a valid Bun child proves exact echo. A client read after host close is `KELD-IPC-001`; a client that caused EOF has no reply to observe. Reply-before-verify, collapse-to-`HelloAuth`, and stop-after-one-bad-peer mutations must fail. |
| 6 | Separate deadline tests are mandatory. `per_handshake_io_deadline_reaccepts_same_instance_then_authenticates` leaves a started partial `HELLO` open until `APP_LINK_IO_DEADLINE` expires while the generation deadline is still in the future; it asserts redacted `Timeout`/`KELD-IPC-006`, completed-or-cancelled-and-observed overlapped I/O, the same pipe instance's next `ConnectNamedPipe`, then valid child authentication. `generation_deadline_is_terminal` independently expires the generation deadline and asserts `DeadlineElapsed`, closed instance, and no reconnect. Tests cancel pending accept and silent partial `HELLO`; they await readiness/completion events and joins, never sleeps. Treating per-handshake timeout as terminal, or treating generation expiry as reconnectable, must fail its respective test. |
| 7 | Valid session ends, stale link cannot create dispatchable session, successor endpoint/token differ. Deleting consumed latch is the negative control. |
| 8 | `GetHandleInformation` verifies no inherit flag; Bun child proves it opens client endpoint. Setting the flag is negative control. |
| 9–10 | First record `KEL101.electron-main-node-app-link` above against its Electron documentation oracle. Only then do TS/Rust paired migration tests pin main-entry pipe vs decimal parsing and captured errors/observer records: endpoint/token never leak; malformed forms are `007`; open failures are `001`. |

The Bun preflight above runs on each pinned Bun upgrade but never substitutes for
Keld integration tests. Tests use random names, readiness events, isolated
processes, and timeouts only as kill switches.

## 8. Review gates triggered

- Unsafe: **yes** — T1's exact root/IPC instruction diff and T2's implementation
  both require named independent exact-diff review. Only `windows_named_pipe`
  may own the Win32
  pipe/overlapped ABI, with local `// SAFETY:` proofs and
  `unsafe_op_in_unsafe_fn` denied. SID/DACL construction and readback reuse
  `windows-permissions`.
- Public API: **yes** — T1 freezes the platform-generic bootstrap transport and
  endpoint selector; T2's exact API still requires independent review.
- Permission model: **yes** — T1 freezes current-user DACL scope and T2's exact
  descriptor/denial implementation still requires independent review.
- Dependency addition: **yes; T2 independent review required** — `keld-ipc`
  will add target-only direct uses of existing workspace pins
  `windows-permissions 0.2.4` and `windows-sys 0.61.2`; T1 changes no manifest
  or lockfile.
- Wire protocol: **none** — any HELLO/frame extension is out of scope and blocked.

## 9. Perf impact

None claimed. This is a security migration. Any claim of connection improvement
needs an attributed Windows end-to-end benchmark against equivalent loopback
HELLO/echo semantics; a speedup is not required to land correctness.

## 10. Open questions

None. T1 selected the bindings and unsafe boundary above, and RAMANI's
operator-managed `KeldDaclTest` ordinary non-admin account owns T4's real
foreign-user observation; no credential is stored in the repository, issue, or
CI. A future logon-SID constraint and T5 decimal-endpoint removal require their
own approved tickets and evidence and do not block T2.
