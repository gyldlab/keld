# kipc — Keld's Typed IPC Plane

IPC is where every competitor cut corners (Electron structured-clone JSON, Tauri
serde-JSON invoke, Electrobun localhost WebSockets, Deno deleting the boundary
entirely). Keld treats the IPC plane as a core product. Design goals: typed end-to-end,
binary, copy-accounted, backpressured, capability-checked, and fast enough that the
compat layer built on it beats Electron's native IPC.

## 1. Topology: two links, three principals

```text
webview_j ⇄ host       "wv-link"       engine bridge (control) + measured engine-specific bulk lane
host ⇄ app role_i      "app-link[i]"   UDS/named pipe (control) + optional measured shm lane
webview_j ⇄ app role_i                routed via host (both link classes), never direct
```

The generated [Current/Target/Evidence ledger](../engineering/product-status.md) owns
implementation status and evidence; this document owns the protocol and trust contract.

The host mediates everything. That's what makes capability checks, auditing, crash
isolation, and Electron-compat routing possible. Binary framing removes avoidable
serialization overhead; shared memory is added only to an attributed bulk bottleneck.
Mediation and browser-engine copies are measured, not described away.

Principal identity is authenticated **link metadata**, not a frame payload field. The
destination supervisor mints a role principal, creates the listener/token, spawns that
role and binds the accepted link to the principal before dispatch. A frame cannot select,
forge or upgrade its role. KEL-70's live generic supervisor does not yet perform this
binding; v0's token proves possession only.

**Destination role-instance contract (KEL-75):** a role instance is the host-owned
pair `(declared role, fresh generation)`. It is never inferred from a PID, listener
name, token, environment value or decoded frame. The host creates an endpoint/token
before spawn, admits an authenticated link, then binds that link as trusted metadata.
Revocation invalidates grants, routed virtual-port capabilities and optional mapping
handles before successor provisioning. `KELD_APP_LINK` carries only endpoint plus
possession secret; it never carries role or principal identity.

**v0 app-link (KEL-60/KEL-70/KEL-30/KEL-101):** one host-owned primary link
(`keld-core::EchoServer` / `HostOwnedHelloSession`) uses the shared
`keld-ipc::BootstrapListener`: a domain socket inside an owner-only (`0o700`)
session directory on Unix and one host-owned
`\\.\pipe\keld-<64 lowercase hex>` instance on Windows. The Windows pipe
rejects remote clients and has one protected allow ACE for the host's current
`TokenUser` SID with mask `0x0012_019B`. Both transports
require a 32-byte session token in the v2 `HELLO` payload,
minted by the host and passed to the child in `KELD_APP_LINK` as
`<endpoint>#<64 hex chars>`. Invalid endpoint/token text is `KELD-IPC-007`
before connect. On the wire, a wrong `HELLO` shape is `KELD-IPC-005`, while
`KELD-IPC-007` is reserved for an exactly shaped foreign token (§2). The
shipping `keld dev` path keeps the listener and supervised Bun live for the hello
window duration; `keld-cli` diagnostics (`ipc-echo` / `ipc-client`) re-export the
same listener. Decimal loopback endpoints remain explicit client-only diagnostic
compatibility; no shipping Windows app-link producer binds or falls back to TCP.
Real Windows KEL-101/T4 evidence observed `ERROR_ACCESS_DENIED` from a distinct
ordinary-user token before admission, followed by an authorized same-user echo
and a fresh successor generation. The DACL does not exclude an administrator or
a malicious process running as the same user; the HELLO token remains mandatory.

**macOS/Windows no-flag primary (KEL-96 T1a-T4):** the staged `keld-host` process
mints and authenticates one one-use platform bootstrap per Bun generation, then
gives the accepted stream to one logical private router with serialized writes.
Each active stream routes
both echo channel 1 and KEL-72 lifecycle channel 3; there is no lifecycle-side
listener or second HELLO within a generation. Initial `Ready` follows native
renderer navigation; a replacement generation receives `Ready` after its fresh
HELLO while the same native window remains live. The old endpoint/token/stream
is revoked before the platform owner accepts its successor.
Quit atomically quiesces dispatch and records accepted-shutdown attribution,
writes its correlated reply, closes the link, stops and reaps the supervised Bun
child, and only then wakes the platform UI loop to exit. macOS composes the
guardian/process-group owner; Windows consumes T8's `PrimaryRoleSupervisor` over the
current-user-DACL named pipe.
The locator is consumed at successful authentication. `keld dev` now uses the
staged no-flag host on macOS and Windows: the CLI owns no app-link endpoint, token, stream,
reader, writer, router, or Bun supervisor. Its separate stdin-v1 writer is only
a host-liveness lease; it never carries a frame, principal, path, digest, or
permission. Linux `keld dev` fails closed until its KEL-96/T4 no-flag row.

## 2. Wire protocol (control plane)

Little-endian framed binary, versioned at handshake:

```
frame  := header(16B) payload
header := magic:u16 'KI' | ver:u8 | kind:u8 | flags:u16 | channel:u16 | corr:u32 | len:u32
kind   := HELLO | CALL | REPLY | ERR | EVENT | STREAM_* | GRANT | PING
payload:= postcard-encoded schema type (structured) | raw bytes (flags.RAW)
```

- **ERR payload (v0):** a postcard `CallError { code: String, message: String }`
  (`crates/keld-ipc/src/call_error.rs`), written by `write_call_error` on the
  channel and correlation id of the `CALL` being answered. `code` is the
  registered `KELD-*` code owned by the crate that failed — `DenyReason::code()`
  for a guard denial, the broker's own code (e.g. `KELD-NATIVE-001`) for a
  post-allow OS failure — and `message` is that error's full `Display` text,
  which already contains the imperative fix sentence (07 §2). Peers match on
  `code` and do not parse it back out of `message`; every privileged channel
  uses this one payload rather than a per-channel `ERR` encoding (the binding
  rule lives in `crates/keld-ipc/AGENTS.md`). An `ERR` answers one call and
  leaves the session up — that is what distinguishes it from `IpcError`, which
  is a transport or session fault and tears the link down. Before KEL-102 this
  payload was one bare postcard `String` per broker, which forced peers to
  string-parse the code and had already drifted (one writer shipped a payload
  carrying no code at all). The two shapes are mutually undecodable — a bare
  `String` is self-terminating, so it leaves no second field, and a `CallError`
  always leaves trailing bytes a `String` decode rejects — so a mixed rollout
  fails deterministically rather than mis-reporting: `KELD-IPC-003` from
  `keld_ipc::codec::decode`, surfaced by `@keld/electron` as `KELD-IPC-005`
  ("not a CallError"). The generated hello scaffold
  (`crates/keld-cli/templates/hello/src/kipc.ts`) speaks only the ungated echo
  channel and does not decode `ERR` payloads.
- **HELLO payload (v2):** exactly 32 bytes — the session token minted by the host
  (KEL-60). It is raw bytes, not postcard. Before token comparison, the receiver
  requires `kind=HELLO`, zero flags/channel/correlation, and declared payload
  length 32. A shape mismatch is `KELD-IPC-005`; `KELD-IPC-007` is reserved for
  an exactly shaped `HELLO` carrying a foreign token. A peer that declares 32
  bytes but closes or stalls before sending them fails as I/O (`KELD-IPC-001`)
  or deadline (`KELD-IPC-006`), not semantic shape or token identity. The client
  writes `HELLO` first. The server reads and verifies before writing its own
  `HELLO`, so a connector that does not already possess the token never learns
  it from the wire. This proves possession of the session token; it is not a
  principal id (peers still do not self-identify). KEL-75's
  reusable listener continues accepting after an invalid `HELLO` until its bounded
  deadline. Its platform `BootstrapListener` primitive is live and used by the host-owned
  echo server (`keld-core`) plus the T1b Unix and T8 Windows primary-generation
  coordinator. T1b/T8 add cancellable admission,
  generation-wide deadline handling, host-only redacted `KELD-IPC-007` rejection
  observation, and consumed-locator cleanup after bind. Multi-role dispatch, role
  grants and privileged Windows dispatch remain destination work. The Windows
  named-pipe/DACL bootstrap and its shipping consumers are live (KEL-101 T2-T4);
  T3 virtual ports remain a Unix library/test surface.
  Channel-table exchange remains later work.
- **Legacy/diagnostic v0 app link is one session, not one endpoint.** On successful authentication
  `BootstrapListener` unlinks the Unix socket path or closes the Windows listener immediately
  (`crates/keld-ipc/src/bootstrap.rs`), so the accepted stream stays live but the
  locator is gone. A supervised app child that dies and is restarted therefore
  cannot re-enter the session: its `connect` fails at the OS
  (`ErrorKind::NotFound`, asserted by `authenticated_bind_unlinks_stale_locator`),
  not at the handshake. `keld dev` does not paper over that. It reports the death
  rather than pretending the app is alive: teardown reads the supervisor's
  `CrashLedger`, which records unrequested self-termination independently of exit
  status, and the window path exits 1 with `KELD-CORE-033` quoting the nested
  `KELD-RUNTIME-*` cause (KEL-105/KEL-116 option (a), SURFACE). A completed
  windowless echo explicitly accepts status-zero self-termination after its reply;
  it does not change the ledger fact or weaken the window policy. The
  fresh-generation macOS product path is the KEL-96/T3 owner described above;
  it does not change this retained diagnostic/legacy contract.
- **v0 session:** one `HELLO` per connection, then N `CALL`/`REPLY` pairs until
  stream EOF. `echo_call` is the one-shot helper (deadline + handshake + one
  CALL). Further CALLs on the same stream use `echo_invoke` and must not send a
  second `HELLO`. Correlation id `0` stays reserved for `HELLO`. After `HELLO`,
  a host-owned persistent echo reader uses the same reader-only short-poll rule
  as lifecycle: idle polls retry, the writer keeps its five-second deadline,
  and a started frame still must finish within that deadline. This is the session
  loop a persistent Bun child needs; it is not a 10k-call latency bench (KEL-30
  AC3 / KEL-39 remain parked).
- **v0 host lifecycle (KEL-72):** `LIFECYCLE_CHANNEL` is `ChannelId(3)`. The host
  sends `Event` frames (`Ready`, `LastWindowClosed`); the app process sends a
  `Call` `Quit` and the host replies, then the serve loop returns. Handshake
  still uses `APP_LINK_IO_DEADLINE` (5s `SO_RCVTIMEO`/`SO_SNDTIMEO`). After
  `HELLO`, cleanup is reader-only: `set_app_link_read_deadline` replaces the
  handshake recv timeout with a short poll — not `set_app_link_deadlines(None)`,
  which would also clear `SO_SNDTIMEO`. Writer `SO_SNDTIMEO` stays
  `APP_LINK_IO_DEADLINE`. The persistent reader retries idle timeouts
  (`read_frame_interruptible`) so a quiet `whenReady` wait is not
  `KELD-IPC-006` and so Drop can join — Win32 `TcpStream::shutdown` on a cloned
  handle does not wake a blocking local `read` (rust-lang/rust#121594). After
  the first byte of a frame, the remainder (header rest and payload) must
  finish within `APP_LINK_IO_DEADLINE` or the reader returns `KELD-IPC-006`.
  Non-blocking sockets are unsupported (`WouldBlock` is poll expiry). This is
  not a frame-layout change (protocol version stays 2). `@keld/electron` maps
  these onto `app.whenReady` / `app.quit` / `window-all-closed` — the wire names
  are host-lifecycle, not Electron-isms.
- **Codec**: postcard (serde, compact, no_std-friendly) for structured payloads —
  expected to be materially cheaper than JSON for typical shapes (no committed Keld
  measurement yet; the kipc bench lane owns that number). A JSON fallback codec is
  planned for `--inspect-ipc` debugging (human dump) — none is live today — and would
  never sit on the hot path.
- **Correlation ids** give request/reply without per-call allocations; channels are
  u16 handles resolved at handshake from schema names (string names never travel per-call).
- **Receiver semantics (KEL-133, live):** one `keld-ipc` validator owns which
  kind/flags/channel/correlation/declared-length combination each session state
  admits. The host selects a static `ReceivePolicy` (no frame chooses its
  policy); reserved combinations — structured `CALL` with correlation `0`,
  `FLAG_RAW`, unknown flag bits, wrong channels, wrong `HELLO` shapes — fail
  `KELD-IPC-005` before payload allocation, with `KELD-IPC-007` reserved for an
  exactly shaped foreign token. Every live Rust receiver, `@keld/electron`, and
  the hello scaffold consume the same rules; Rust and Bun replay one canonical
  hostile-vector corpus (`crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv`,
  one owner, one digest).
- **Cancellation**: `STREAM_CANCEL`/`CALL_CANCEL` carry corr id; handlers observe an
  `AbortSignal` in JS, a `CancelToken` in Rust.
- **Backpressure (destination):** per-channel credit windows granted in `GRANT`
  frames (SPSC credit counting); senders suspend (JS: promise; Rust:
  state-machine pause) at zero credit. The design goal is no unbounded queues
  anywhere — Electron's frame-starving chatty-IPC failure becomes structurally
  impossible once credit windows ship. **v0:** `FrameKind::Grant` exists in the
  wire schema but has no live sender/receiver; bounded inline `CALL`/`REPLY`
  and the current drain-driven writer are the v0 backpressure surface; the
  readiness-driven reader remains destination work (see §7).

## 3. Bulk plane (measured copies, optional shared memory)

- **app-link[i]**: a pair of SPSC shared-memory ring buffers (one per direction) MAY be created by
  the host, passed to Bun at spawn (memfd/`shm_open`/named section + fd/handle
  inheritance). JS reads/writes via `ArrayBuffer` views over the mapping (Bun `mmap` /
  N-API external buffers — bun:ffi remains experimental, so the stable binding path is a
  tiny N-API glue shipped with `@keld/api`). Control frame carries {ring offset, len,
  generation}; payload bytes are never re-serialized after mapping. Inline `RAW` frames
  over the socket remain the mandatory baseline. A role receives a mapping only after
  its own end-to-end benchmark and hostile handle-inheritance proof justify it; one role
  cannot map another role's ring. The P13 new-run did not justify a shared-memory
  baseline: at 16 MiB memfd was near UDS and at 1 MiB explicit-copy memfd was slower.
- **wv-link**: engines don't expose shm to page JS reliably (SAB needs COOP/COEP and
  still doesn't cross to native). Bulk therefore rides the custom scheme: `keld://c/{channel}`
  request/response with streaming bodies (WKURLSchemeHandler / WebView2
  WebResourceRequested / WebKitGTK 2.40+ streams), which engines serve off the UI
  thread and can hand to us as counted buffers. postMessage stays control-only
  (string-typed on WebView2 — see `docs/research/library/host-platforms/06-webview-reality.md`).
- Renderer→app-role file-ish transfers (the Electron `send(bigBuffer)` pattern) are
  routed through the host with bounded credit. A platform adapter MAY forward into that
  role's ring, but the actual engine path reports its copies; choosing a binary codec
  does not itself make a transfer zero-copy.

## 4. Schema-first contracts

Contracts are TypeScript-native (no new IDL to learn) and compiled to both sides:

```ts
// app/contracts/notes.k.ts
import { channel, stream, z } from "@keld/schema";

export const notes = {
  save:   channel({ input: z.object({ id: z.string().max(64), body: z.string() }),
                    output: z.object({ bytes: z.number() }) }),
  export: stream({ input: z.object({ id: z.string() }), chunk: z.instanceof(Uint8Array) }),
};
```

`keld gen` emits: TS client/server stubs (typed `notes.save(...)` promise API — the
`bindings.x()` ergonomics Deno proved, but typed), Rust `serde` types + handler traits
for native plugins, a channel table for the handshake, and — critically — **the
permission stubs**: every channel declares required capabilities, so the manifest
generator and the guard enforce the same source of truth.

Validation runs where trust changes: host validates schema + capability on every frame
from webviews; app-link frames validate schema in debug, length/window checks always
(the app process is semi-trusted: it's the developer's own code, but treating it as
compromised keeps the host's threat model uniform).

## 5. Electron-compat mapping (why this design carries the shim)

- `ipcRenderer.send/on` ⇄ EVENT frames on a compat channel namespace (`el:*`).
- `ipcRenderer.invoke`/`ipcMain.handle` ⇄ CALL/REPLY with structured-clone-compatible
  codec: a `postcard`-encoded SCV (structured-clone value) sum type covering Electron's
  value domain (incl. Buffer/TypedArray → bulk lane refs, Date, Map/Set, Error).
- `webContents.send` ⇄ host-routed EVENT to a specific webview principal.
- `MessagePort`s ⇄ host-owned virtual-port pairs with dedicated bounded channel routes
  and credit windows. A port capability binds to one authenticated role or webview
  generation; transfer is one-shot, receiver-bound and host-authorized. Browser and Bun
  endpoints never connect directly. Exact Electron `start`, queued-message, close-event
  and transferable-validation behavior is a pinned-oracle conformance requirement.
- `utilityProcess.fork` ⇄ an `@keld/electron` request for a host-declared app-bound or
  window-bound Bun role, not an arbitrary child-process escape hatch. Its PID is
  diagnostic only and never authorizes, identifies, reaps or routes a Keld role.
- Semantics preserved: ordering per channel, `event.sender`/`senderFrame` identity
  (host mints principal ids), sync `sendSync` supported but rate-limited + dev-warned
  (it's a blocking CALL with a deadline; Electron apps abuse it, so it must exist).

## 6. Hot-path implementation rules (Bun-rewrite discipline)

- Reader/writer are explicit state machines (`Idle → Header → Payload → Dispatch`)
  driven by readiness events; zero allocations steady-state (pooled frame buffers,
  arena for decode); no Tokio, no async fn in the frame path.
- Per-link single-producer queues into the main thread's wakeup primitive for UI-bound
  messages; everything else completes on I/O threads.
- Frame header parse is branch-lean; struct layouts audited (`#[repr]`, size asserts in
  tests — the Bun port's "56-byte Path" lesson).
- Benchmarks in `bench/ipc/`: small-call RTT, 64 KB/1 MB/64 MB bulk, 1M-msg soak with
  backpressure, restart-storm. Budgets: RTT p99 ≤ 100 µs; bulk ≥ 1 GB/s; zero drops.

## 7. Failure & lifecycle semantics

- App-role crash: the destination host parks only that role's channels, buffers nothing
  (credit hits zero), emits a role-qualified `runtime-crashed` event and restarts per
  policy. Its in-flight calls reject with a registered role-qualified `KELD-*` error;
  other principals continue. KEL-70 currently proves generic child restart only.
- Window close: the destination host revokes that window generation and virtual-port
  routes, then drains only roles declared `window-bound` to it. App-bound roles remain
  live until the host application session stops.
- Webview navigation: principal generation rotates; pending replies to the old principal
  drop; guard re-evaluates capabilities using origin/resource policy context.
- Every individual I/O wait on either peer carries a deadline, and every live
  production receiver reads through the shared KEL-133 validated path, which
  carries one frame-wide stall clock: the first byte of a frame starts it and
  partial reads cannot renew it, so a byte-trickling peer expires with
  `KELD-IPC-006` instead of holding the session open (the gap research 115
  recorded is closed; the legacy `read_frame` helper stays per-receive-bounded
  for raw test fixtures only). Exactness is per reader: the pollable
  interruptible reader observes an absolute deadline within one
  `APP_LINK_READER_POLL`; the plain validated reader cannot shrink the socket
  timeout, so its bound is the stall limit plus one `SO_RCVTIMEO`. Bootstrap admission additionally carries the
  host-minted generation deadline across accept, bad-peer reaccept, and every
  handshake read — byte drip cannot renew it, and it is enforced at connect
  wait, per-peer window, and post-authentication check independently.
  v0 app-link
  applies `APP_LINK_IO_DEADLINE` (5s `SO_RCVTIMEO`/`SO_SNDTIMEO`) during
  authentication and on writes; expiry is `KELD-IPC-006`. That is an OS socket
  timeout, not an async timer. **Persistent-session exception (KEL-30/KEL-72):**
  after a successful echo or lifecycle `HELLO`, cleanup is
  `set_app_link_read_deadline` with a short poll (reader-only). Writer
  `SO_SNDTIMEO` stays `APP_LINK_IO_DEADLINE`. Idle timeouts retry so a quiet
  persistent child or `whenReady` wait is not `KELD-IPC-006` and so Drop can join (Win32
  clone-shutdown does not wake a local blocking `read`; rust-lang/rust#121594).
  A started frame that stalls still expires at `APP_LINK_IO_DEADLINE`
  (`KELD-IPC-006`); per-`recv` `SO_RCVTIMEO` is not that overall deadline.
  Non-blocking sockets are unsupported. `read_frame` still cannot retry after
  Timeout. The readiness-driven reader (and credit windows) remain later work.
