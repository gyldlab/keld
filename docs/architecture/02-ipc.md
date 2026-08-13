# kipc — Keld's Typed IPC Plane

IPC is where every competitor cut corners (Electron structured-clone JSON, Tauri
serde-JSON invoke, Electrobun localhost WebSockets, Deno deleting the boundary
entirely). Keld treats the IPC plane as a core product. Design goals: typed end-to-end,
binary, zero-copy bulk, backpressured, capability-checked, and fast enough that the
compat layer built on it beats Electron's native IPC.

## 1. Topology: two links, three principals

```
webview ⇄ host        "wv-link"   native bridge (control) + keld:// scheme (bulk)
host    ⇄ app process "app-link"  UDS/named pipe (control) + shm rings (bulk)
webview ⇄ app process             routed via host (both links), never direct
```

The host mediates everything. That's what makes capability checks, auditing, crash
isolation, and Electron-compat routing possible. The mediation cost is engineered away
with binary framing + shm, not avoided by deleting the boundary (Deno's mistake).

## 2. Wire protocol (control plane)

Little-endian framed binary, versioned at handshake:

```
frame  := header(16B) payload
header := magic:u16 'KI' | ver:u8 | kind:u8 | flags:u16 | channel:u16 | corr:u32 | len:u32
kind   := HELLO | CALL | REPLY | ERR | EVENT | STREAM_* | GRANT | PING
payload:= postcard-encoded schema type (structured) | raw bytes (flags.RAW)
```

- **Codec**: postcard (serde, compact, no_std-friendly) for structured payloads —
  measured order-of-magnitude cheaper than JSON for typical shapes; JSON fallback codec
  exists only for `--inspect-ipc` debugging (human dump), never on the hot path.
- **Correlation ids** give request/reply without per-call allocations; channels are
  u16 handles resolved at handshake from schema names (string names never travel per-call).
- **Cancellation**: `STREAM_CANCEL`/`CALL_CANCEL` carry corr id; handlers observe an
  `AbortSignal` in JS, a `CancelToken` in Rust.
- **Backpressure**: per-channel credit windows granted in `GRANT` frames (SPSC credit
  counting); senders suspend (JS: promise; Rust: state-machine pause) at zero credit.
  No unbounded queues anywhere — Electron's frame-starving chatty-IPC failure is
  structurally impossible.

## 3. Bulk plane (zero-copy where the platform allows)

- **app-link**: a pair of SPSC shared-memory ring buffers (one per direction) created by
  the host, passed to Bun at spawn (memfd/`shm_open`/named section + fd/handle
  inheritance). JS reads/writes via `ArrayBuffer` views over the mapping (Bun `mmap` /
  N-API external buffers — bun:ffi remains experimental, so the stable binding path is a
  tiny N-API glue shipped with `@keld/api`). Control frame carries {ring offset, len,
  generation}; payload bytes are never re-serialized. Fallback: inline `RAW` frames on
  the socket when shm is unavailable (containers, exotic sandboxes).
- **wv-link**: engines don't expose shm to page JS reliably (SAB needs COOP/COEP and
  still doesn't cross to native). Bulk therefore rides the custom scheme: `keld://c/{channel}`
  request/response with streaming bodies (WKURLSchemeHandler / WebView2
  WebResourceRequested / WebKitGTK 2.40+ streams), which engines serve off the UI
  thread and can hand to us as counted buffers. postMessage stays control-only
  (string-typed on WebView2 — see research/06).
- Renderer→app-process file-ish transfers (the Electron `send(bigBuffer)` pattern) are
  routed: webview streams over `keld://` into host, host forwards into the shm ring —
  no double JSON, one copy at the scheme boundary (engine-imposed), zero after.

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
- `MessagePort`s ⇄ dedicated channel pairs with credit windows.
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

- App process crash: host parks channels, buffers nothing (credit hits zero), emits
  `runtime-crashed` to webviews (compat shim translates to nothing — Electron apps just
  never see it — but `@keld/api` users can render "reconnecting" UI); supervisor
  restarts with backoff; channels re-handshake; in-flight CALLs reject with `E_RESTART`.
- Webview navigation: principal id rotates; pending replies to the old principal drop;
  guard re-evaluates capabilities (origin-scoped grants).
- Host never blocks on either peer; every await point has a deadline. v0 app-link
  applies a 5-second `SO_RCVTIMEO`/`SO_SNDTIMEO` on the connected stream; expiry is
  `KELD-IPC-006`. That is an OS socket timeout, not an async timer. The readiness-driven
  reader (and credit windows) remain later work.
