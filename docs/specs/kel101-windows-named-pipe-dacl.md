# Spec: Windows named-pipe app-link with current-user DACL

Status: draft
Linear: KEL-101 · Owner: GYLDLAB · Updated: 2026-08-23

## 1. Goal & non-goals

Replace the live Windows loopback-TCP app-link with one host-owned named pipe,
`\\.\pipe\keld-<independent-random>`, whose explicit DACL admits the host's
current user and whose kipc v2 `HELLO` still proves possession of the fresh
32-byte session token. The observable result is: a foreign user cannot open the
pipe, a same-user process without the token cannot dispatch a frame, and the
intended Bun child can complete `HELLO` and an echo call.

Non-goals:

- No implementation, Cargo change, `unsafe` exception, dependency addition, or
  wire-version change is authorized by this draft.
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
   DACL has exactly one allow ACE for the host `TokenUser` SID with mask
   `0x0012_019B` (`PipeAccessRights::ReadWrite`), and it has no
   `FILE_CREATE_PIPE_INSTANCE`/`FILE_APPEND_DATA` bit (`0x4`), inherited ACE,
   or allow ACE for Everyone, Anonymous, Users, or Administrators.
3. Given a different ordinary Windows user, when it calls `CreateFileW` with
   the client mask, then Windows returns `ERROR_ACCESS_DENIED`; no host session
   is accepted, and the intended child can subsequently authenticate.
4. Given a same-user client with pipe name but no token, when it sends an empty,
   truncated, or foreign `HELLO`, then it receives no host `HELLO` or echo reply;
   the host records only redacted `KELD-IPC-007`, disconnects that client, and
   continues accepting until the legitimate child succeeds or admission expires.
5. Given the intended Bun child and complete link, when it connects with
   `node:net` and sends the matching v2 `HELLO`, then the host replies only after
   verification and one echo succeeds. Header bytes, protocol version 2, and
   32-byte raw `HELLO` are unchanged.
6. Given cancellation, host shutdown, deadline, or a silent partial `HELLO`,
   when the operation completes, then no frame dispatch occurs, the worker joins
   without a sleep, and no peer/log/error receives the token or its hex form.
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

Today `keld_core::EchoServer` mints a token, binds one `127.0.0.1:0`
`TcpListener`, and accepts one Windows session. The Unix path already delegates
owner-only endpoint/token lifecycle and retry-after-bad-HELLO semantics to
`keld_ipc::BootstrapListener`. The Windows one-accept listener is not a reusable
admission boundary because one bad peer consumes it.

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
3. ACE mask is `0x0012_019B`: `FILE_READ_DATA`, `FILE_WRITE_DATA`, read/write
   EA, read/write attributes, `READ_CONTROL`, and `SYNCHRONIZE`. It excludes
   `FILE_CREATE_PIPE_INSTANCE`. Microsoft warns that `FILE_GENERIC_WRITE`
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
If cancellation/deadline wins, close that connection before handshake. Each `HELLO` uses the shorter of remaining generation deadline and
`APP_LINK_IO_DEADLINE`. The Windows adapter must supply overlapped read/write
waits with the existing overall started-frame deadline; `std::io::Read` alone is
not claimed to provide a named-pipe timeout.

On bad `HELLO`, send no rejection frame, record only
`BootstrapRejection::HelloAuth`, call `DisconnectNamedPipe`, and re-enter
overlapped `ConnectNamedPipe` on the same instance. Windows requires disconnect
before reconnecting an instance ([DisconnectNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-disconnectnamedpipe)).
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

### Local compatibility evidence (not security proof)

On 2026-08-23, a temporary Windows 11 build 26200 probe used Bun 1.4.1 and
`node:net` to open a .NET `NamedPipeServerStream`, write `KI`, and close. A
protected DACL granting the current `TokenUser` SID produced:

| ACE mask | Result |
| --- | --- |
| `0x0010_0003` (`ReadData | WriteData | Synchronize`) | no server accept; Bun exit 2 |
| `0x0012_019B` (`PipeAccessRights::ReadWrite`) | accepted `KI`; Bun exit 0 |
| `0x0012_019F` (`ReadWrite | CreateNewInstance`) | accepted `KI`; Bun exit 0 |

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

- Later approved implementation: shared Windows `BootstrapListener` in
  `crates/keld-ipc`; consuming `keld-core` echo link; Windows CLI template/client
  and diagnostics; Windows-only integration fixtures; the claim-update docs above.
- This specification-only PR must not touch `crates/**`, Cargo manifests/lockfile,
  `docs/architecture/**`, CI, KEL-102 materials, guard policy, Unix transport,
  frame layout, or protocol version.
- Reuse `SessionToken`, `format_app_link`/`parse_app_link`, handshake state machine,
  `BootstrapRejectionObserver`, and `APP_LINK_IO_DEADLINE`; do not copy them into
  a Windows parser or authorization path.

## 6. Tasks (each ≈ one PR; ordered; no placeholders)

- [ ] T1: obtain human approval for the FFI/safe-wrapper choice in Open Question 1.
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
| 4–5 | Real-pipe raw client sends empty/31-byte/foreign HELLO, asserts EOF/no host HELLO and redacted observer `KELD-IPC-007`, then valid Bun child proves exact echo. Reply-before-verify and stop-after-one-bad-peer mutations must fail. |
| 6 | Tests cancel pending accept and silent partial HELLO, and exercise deadline. They await events/join, not sleeps; `CancelIoEx` completion is observed before free. Link deadline is `KELD-IPC-006`. |
| 7 | Valid session ends, stale link cannot create dispatchable session, successor endpoint/token differ. Deleting consumed latch is the negative control. |
| 8 | `GetHandleInformation` verifies no inherit flag; Bun child proves it opens client endpoint. Setting the flag is negative control. |
| 9–10 | TS/Rust paired tests pin pipe vs decimal parsing and captured errors/observer records: endpoint/token never leak; malformed forms are `007`; open failures are `001`. |

The Bun preflight above runs on each pinned Bun upgrade but never substitutes for
Keld integration tests. Tests use random names, readiness events, isolated
processes, and timeouts only as kill switches.

## 8. Review gates triggered

- unsafe: **human decision required** — current `keld-ipc` rules allow unsafe only
  for a future shared-memory module. Named-pipe/DACL code needs an approved safe
  wrapper or narrowly documented exception with `// SAFETY:` proofs.
- Public API: **yes** — platform-generic bootstrap transport and endpoint selector.
- Permission model: **yes** — DACL scope is Windows OS-principal policy.
- Dependency addition: **human decision required** if the approved safe wrapper is
  not already workspace-pinned.
- Wire protocol: **none** — any HELLO/frame extension is out of scope and blocked.

## 9. Perf impact

None claimed. This is a security migration. Any claim of connection improvement
needs an attributed Windows end-to-end benchmark against equivalent loopback
HELLO/echo semantics; a speedup is not required to land correctness.

## 10. Open questions

1. **Blocking:** Which reviewed safe Windows named-pipe/DACL abstraction may live
   in `keld-ipc` without violating its current `forbid(unsafe_code)` rule? Choose
   either a minimal safe dependency (dependency review) or a narrowly documented
   exception; direct ad-hoc FFI in `keld-core` is forbidden.
2. **Blocking test infrastructure:** Who owns and where runs the distinct-user
   Windows credential fixture? A same-user test or `AccessCheck` simulation cannot
   replace an actual `CreateFileW` DACL-denial observation.
3. **Future hardening:** Should a later transport revision add a logon SID
   constraint to exclude another terminal-services session of the same user? This
   draft explicitly makes no such claim.
4. **Migration completion:** Which approved ticket removes decimal diagnostic
   endpoints, and what evidence proves no supported producer remains?
