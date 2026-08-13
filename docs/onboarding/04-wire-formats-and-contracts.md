# Wire Formats and Contracts

Keld has no database. It has no ORM, no migrations, no schema.sql. What it has instead — and
what plays the same role, in that getting it wrong breaks compatibility for everyone downstream
— is a set of **durable shapes**: bytes on a socket, filenames the tooling agrees to look for,
environment variables handed to a child process, and error codes that appear in other people's
logs and issue trackers.

This document is the reference for those shapes. Everything here is read out of the code in
this repository; where a normative spec describes a shape that has no implementation, it is
labeled **specified, not implemented** and you should not assume it exists.

This is document 04 of the onboarding set;
[`02 — Architecture guide`](./02-architecture-guide.md) covers the system shape and the reasoning
behind it, and is the better place to start if you have not read it. See also
[`03 — API and CLI surface`](./03-api-and-cli-surface.md) for the developer-facing surface built on
these contracts. Normative source for the protocol:
[`docs/architecture/02-ipc.md`](../architecture/02-ipc.md).

> **Review gate.** Wire-protocol changes — kipc frames, manifest schema, update feed — require
> human sign-off, listed under a `## Review gates` heading in the PR (root
> [`AGENTS.md`](../../AGENTS.md)). `crates/keld-ipc/AGENTS.md` adds the mechanics: a change to
> the frame layout, `FrameKind`, flags, or the handshake means a version bump plus the spec §2
> edit plus the code, **in one PR**. There is no such thing as landing the code first.

---

## 1. The kipc frame header

Every message on the control plane is a 16-byte little-endian header followed by `len` bytes of
payload. The size is a protocol constant, tested independently of any Rust struct layout so that
a field reordering can never silently change the wire
(`crates/keld-ipc/src/frame.rs:155-159` asserts `HEADER_LEN == 16`).

```text
frame  := header(16B) payload(len B)
header := magic:u16 | ver:u8 | kind:u8 | flags:u16 | channel:u16 | corr:u32 | len:u32
```

| Offset | Size | Field | Type | Source of value | Notes |
|---:|---:|---|---|---|---|
| 0 | 2 | `magic` | `u16` LE | Constant `MAGIC` | `u16::from_le_bytes(*b"KI")` = `0x494B`. On the wire the bytes read `4B 49`, i.e. ASCII `K`, `I` |
| 2 | 1 | `ver` | `u8` | Constant `PROTOCOL_VERSION` | Currently `2` (v2 HELLO token; KEL-60) |
| 3 | 1 | `kind` | `u8` | `FrameKind as u8` | See §2. Values `0..=10` |
| 4 | 2 | `flags` | `u16` LE | `FrameHeader.flags` | Bitfield. Only `FLAG_RAW` (`1 << 0`) is defined |
| 6 | 2 | `channel` | `u16` LE | `ChannelId.0` | Handle, not a name. See §4 |
| 8 | 4 | `corr` | `u32` LE | `CorrelationId.0` | `0` for uncorrelated kinds |
| 12 | 4 | `len` | `u32` LE | Payload byte count | Bytes following the header |

Two details that surprise people reading `FrameHeader` for the first time:

**`magic` and `ver` are not struct fields.** `FrameHeader` carries only `kind`, `flags`,
`channel`, `corr`, and `len`. The first three bytes are written from constants at encode time and
checked against constants at decode time, which means it is *impossible* to construct a header
claiming a version other than the current one. Version negotiation, when it arrives, will need a
deliberate change here rather than a new field assignment.

```rust
// crates/keld-ipc/src/frame.rs:112-122
pub fn encode(&self) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[0..2].copy_from_slice(&MAGIC.to_le_bytes());
    out[2] = PROTOCOL_VERSION;
    out[3] = self.kind as u8;
    out[4..6].copy_from_slice(&self.flags.to_le_bytes());
    out[6..8].copy_from_slice(&self.channel.0.to_le_bytes());
    out[8..12].copy_from_slice(&self.corr.0.to_le_bytes());
    out[12..16].copy_from_slice(&self.len.to_le_bytes());
    out
}
```

**Little-endian is a protocol choice, not a host artifact.** Every multi-byte field is written with
an explicit `to_le_bytes()` and read with `from_le_bytes()`, so the encoding is identical on a
big-endian host. Do not replace these with pointer casts or `#[repr(C)]` transmutes.

### A real frame, byte for byte

An echo `CALL` carrying `EchoRequest { message: "kipc", count: 3 }` — these bytes were produced by
running the actual `encode` path, not derived by hand:

```text
header   4B 49 01 01 00 00 01 00 01 00 00 00 06 00 00 00
         └─┬─┘ │  │  └─┬─┘ └─┬─┘ └────┬────┘ └────┬────┘
         magic │  │  flags  chan     corr        len
             ver  kind=Call  =0      =1          =6
                    =1                =1(echo)

payload  04 6B 69 70 63 03
         │  └───┬────┘ │
         │      │      └─ count: u32 = 3          (postcard varint)
         │      └──────── "kipc"                  (UTF-8 bytes)
         └─────────────── string length = 4       (postcard varint)
```

And the HELLO frame that opens every session — kind `0`, everything else zero:

```text
4B 49 01 00 00 00 00 00 00 00 00 00 00 00 00 00
```

---

## 2. Frame kinds

`FrameKind` is `#[repr(u8)]` with 11 variants (`crates/keld-ipc/src/frame.rs:19-45`). All 11 are
*defined* — the roundtrip test in that file encodes and decodes every one of them — but most have
no sender and no handler anywhere in the workspace. The distinction matters: the byte values are
frozen protocol facts, while the behavior behind them is largely future work.

| Value | Variant | Purpose | Implemented? |
|---:|---|---|---|
| 0 | `Hello` | Handshake: version + channel table exchange | **Partial** — version only, no channel table (§6) |
| 1 | `Call` | Request expecting exactly one `Reply` or `Err` | **Live** on the echo channel |
| 2 | `Reply` | Successful response to a `Call` | **Live** |
| 3 | `Err` | Failed response to a `Call` | Defined only — nothing emits it |
| 4 | `Event` | Fire-and-forget notification | Defined only |
| 5 | `StreamOpen` | Opens a stream on a channel | Defined only |
| 6 | `StreamChunk` | One stream chunk; payload may reference the bulk lane | Defined only |
| 7 | `StreamClose` | Graceful end of stream | Defined only |
| 8 | `Cancel` | Cancels an in-flight `Call` or stream | Defined only |
| 9 | `Grant` | Grants flow-control credit on a channel | Defined only |
| 10 | `Ping` | Liveness probe | **Live** — echoed back with the same channel and corr |

There is no `Pong`. A liveness reply is a `Ping` frame returned with the sender's `channel` and
`corr` (`crates/keld-ipc/src/session.rs:36-38`).

### Flags

One flag is defined:

```rust
// crates/keld-ipc/src/frame.rs:68-69
/// Header flag: payload is raw bytes (bulk-lane reference or inline), not codec-encoded.
pub const FLAG_RAW: u16 = 1 << 0;
```

`FLAG_RAW` distinguishes "this payload is a postcard-encoded schema type" from "this payload is
opaque bytes — either inline, or a `{ring offset, len, generation}` reference into the shared-memory
bulk lane". Nothing sets it today; the bulk lane it exists for is §10.

---

## 3. Reading and writing frames

`crates/keld-ipc/src/link.rs` provides the framed I/O. It is deliberately minimal and explicitly
labelled "app-link control plane **v0**":

```rust
pub fn read_frame<S: Read>(stream: &mut S) -> Result<(FrameHeader, Vec<u8>), IpcError>
pub fn write_frame<S: Write>(
    stream: &mut S, kind: FrameKind, flags: u16,
    channel: ChannelId, corr: CorrelationId, payload: &[u8],
) -> Result<(), IpcError>
```

`write_frame` flushes after every frame, so a header and its payload always reach the peer
together. `read_frame` does a `read_exact` of 16 bytes, decodes, then a `read_exact` of `len`
bytes.

**Know what this is not.** `crates/keld-ipc/AGENTS.md` requires the eventual implementation to be a
state machine with no steady-state allocation, and says so bluntly: "State-machine readers/writers.
No async, no steady-state alloc (`Vec`/frame = wrong design)." The current code is blocking and
allocates a `Vec` per frame. It has the right *format* and the wrong *mechanics*, on purpose, so
that the format could be pinned down and tested before the hot-path work begins. Do not treat
`link.rs` as the model for the reader you are eventually going to write.

### What decoding does not check

`FrameHeader::decode` validates magic, version, and kind. It does not validate the
payload bytes. `read_frame` **does** reject a declared `header.len` above
[`MAX_FRAME_LEN`](../../crates/keld-ipc/src/lib.rs) (**16 MiB**) *before* allocating the
payload `Vec` (`ensure_payload_len` → `KELD-IPC-004`). A forged `u32` must not become a
multi-GiB allocation. The 4 GiB “allocate verbatim” story is stale.

What remains parked (crate `AGENTS.md`, not this slice): a `Vec` per frame instead of a
caller-owned bounded buffer. Blocking `read_exact`/`write_all` now have a 5s OS deadline
(`AppLinkDeadlines` / `KELD-IPC-006`); do not treat `link.rs` as the eventual hot-path
reader. Fuzz decode paths — malformed webview input is expected.

---

## 4. `ChannelId` and `CorrelationId`

```rust
// crates/keld-ipc/src/frame.rs:11-17
pub struct ChannelId(pub u16);
pub struct CorrelationId(pub u32);
```

**`ChannelId` is a handle, never a name.** This is one of the crate's three stated design
constraints (`crates/keld-ipc/src/lib.rs:6-9`): "Channel names never travel per-call; they resolve
to `ChannelId` handles." The string `"notes.save"` is meant to be exchanged exactly once, in the
handshake's channel table, and thereafter every frame carries two bytes. That is what keeps a call
allocation-free and what makes the p99 ≤ 100 µs budget in
[`01` §5](../architecture/01-overview.md) plausible.

Currently allocated:

| Channel | Value | Purpose | Defined at |
|---|---:|---|---|
| *(control)* | `0` | Handshake frames | Implicit — `link.rs:63` passes `ChannelId(0)`. **There is no named constant for this**; it is a convention waiting to be made explicit |
| `ECHO_CHANNEL` | `1` | The echo vertical slice | `crates/keld-ipc/src/echo.rs:10` |

The doc comment on `ECHO_CHANNEL` records its own temporariness: "resolved at handshake in later
versions". Today it is a hardcoded constant on both sides.

**`CorrelationId` pairs a `Reply` or `Err` with its `Call`.** The client picks it; the server echoes
it back unchanged. Uncorrelated kinds use `0`. Today `echo_call` hardcodes `CorrelationId(1)`
(`session.rs:60`) because exactly one call is ever in flight — a real client will need an
allocator and an in-flight table.

---

## 5. Decode errors

`HeaderError` (`crates/keld-ipc/src/frame.rs:86-107`) has exactly three cases, each carrying the
offending value so the message can name it:

| Variant | Raised when | `Display` output |
|---|---|---|
| `BadMagic(u16)` | First two bytes are not `0x494B` | `bad kipc magic: 0x____` |
| `BadVersion(u8)` | Version byte ≠ `PROTOCOL_VERSION` | `unsupported kipc version: _` |
| `BadKind(u8)` | Kind byte is outside `0..=10` | `unknown kipc frame kind: _` |

Note that `BadVersion` is a strict equality check, not a range or a negotiation — see §6. Two tests
cover the rejection paths (`rejects_bad_magic`, `rejects_unknown_kind`); there is no test for
`BadVersion` today.

`HeaderError` is wrapped by `IpcError::Header` via a `From` impl, which is how it reaches the
error-code taxonomy in §11.

---

## 6. The handshake

Both peers open the session by writing a `Hello` frame with a 32-byte session token
and then reading one:

```rust
// crates/keld-ipc/src/link.rs
pub fn handshake<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
) -> Result<(), IpcError>
```

What this actually establishes, and what it doesn't:

| Spec ([`02` §2](../architecture/02-ipc.md), `frame.rs:23`) | v2 reality |
|---|---|
| "Handshake: version + channel table exchange" | Version + **session token**. **No channel table** — both sides hardcode `ECHO_CHANNEL` |
| "versioned at handshake" | Strict equality: a peer on any version other than `2` is rejected by `decode` before `handshake` even inspects the frame. No negotiation, no range, no downgrade |
| HELLO payload | 32 raw bytes (KEL-60). Empty, truncated, or mismatched tokens are `KELD-IPC-007`. The channel table will still have to live here later |

The token is minted by the host (`getrandom::fill`) and passed to the child in
`KELD_APP_LINK` as `<endpoint>#<64 hex chars>`. Unix still also binds inside a `0o700`
session directory; Windows is still loopback TCP (named-pipe DACL is the destination).

Both peers write before either reads. A 16-byte header plus 32-byte token (48 bytes)
fits in any socket buffer. A later channel table of meaningful size will need
revisiting.

One more current-code quirk: `echo_call` calls `handshake` itself, so it is a
connect-handshake-call-close operation rather than a client you hold open. Calling it twice on one
stream would send a second HELLO.

---

## 7. The codec: postcard

Structured payloads are [postcard](https://docs.rs/postcard) over `serde`. Decode
rejects leftover bytes (`take_from_bytes` plus an empty remainder):

```rust
// crates/keld-ipc/src/codec.rs
pub fn decode<T: serde::de::DeserializeOwned>(payload: &[u8]) -> Result<T, IpcError> {
    let (value, rest) = take_from_bytes(payload).map_err(IpcError::Codec)?;
    if rest.is_empty() {
        Ok(value)
    } else {
        Err(IpcError::Codec(postcard::Error::DeserializeBadEncoding))
    }
}
pub fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, IpcError> {
    to_allocvec(value).map_err(IpcError::Codec)
}
```

Postcard encodes integers wider than `u8` as varints and prefixes sequences with a varint length,
which is why `EchoRequest { message: "kipc", count: 3 }` costs 6 bytes on the wire (§1) rather than
the 12+ a fixed-width or self-describing encoding would need.

### Why postcard and not JSON

The evaluation is in [`10-ipc-state-of-the-art.md`](../research/10-ipc-state-of-the-art.md) and the
decision is normative in [`02` §2](../architecture/02-ipc.md):

| Candidate | Verdict |
|---|---|
| `serde_json` | Baseline 1×; requires a copy plus UTF-8 validation. Debuggable, but the wrong default for a hot path |
| **postcard** | Compact, `no_std`-friendly, pairs cleanly with `RAW`/shm for bulk. **Chosen** for the control plane |
| flatbuffers / Cap'n Proto | Near zero-copy but schema tooling is heavy. Overkill for command RPC; reconsider for bulk later |
| bincode | Fast Rust↔Rust, less portable to TypeScript. Rejected for public contracts |

The framing is worth stating explicitly, because it is what every competitor got wrong: Electron
uses structured-clone/JSON, Tauri uses serde-JSON `invoke`, Electrobun uses JSON-RPC over localhost
WebSockets, and Deno Desktop deletes the boundary. Keld's position is that the mediation cost gets
*engineered away* with binary framing plus shared memory — not avoided by removing the security
boundary.

**JSON has exactly one sanctioned use**: the human-readable dump behind `keld dev --inspect-ipc`
([`06` §5](../architecture/06-runtime-and-tooling.md)). `crates/keld-ipc/AGENTS.md` states the rule
as "postcard on hot path; JSON only for `--inspect-ipc` debug." Neither the debug codec nor the flag
exists yet.

---

## 8. The echo channel — today's only contract

`crates/keld-ipc/src/echo.rs` is the entire application-level contract that exists. It is a
deliberate vertical slice: two structs, one handler, one channel constant.

```rust
// crates/keld-ipc/src/echo.rs:10-28
pub const ECHO_CHANNEL: ChannelId = ChannelId(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoRequest {
    pub message: String,
    /// Repeat count metadata (demonstrates structured fields).
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoResponse {
    pub message: String,
    pub count: u32,
}
```

`handle_echo` decodes an `EchoRequest` and re-encodes the same values as an `EchoResponse` — the
`count` field exists purely to prove that a multi-field struct survives the round trip.

### Session behavior

`serve_echo_session` (`crates/keld-ipc/src/session.rs:16-47`) is a blocking loop that fails closed:

| Received | Response |
|---|---|
| `Call` on `ECHO_CHANNEL` | `Reply` on `ECHO_CHANNEL` with the caller's `corr` |
| `Ping` (any channel) | `Ping` back with the same channel and `corr` |
| Clean EOF | Loop exits, session ends `Ok` |
| **Anything else** — including a `Call` on a different channel | `IpcError::Protocol`, session terminates |

Note what is *not* in that path: there is no capability check. The echo frame goes from
decode straight to handler. `keld-guard::evaluate` is live for MCP
`keld_permissions_explain` and for macOS webview camera/microphone capture; it is
still not called on privileged kipc frames, so the "every privileged operation
passes the guard" property in [`03` §1](../architecture/03-security.md) is not yet
true of IPC.

Coverage: `crates/keld-ipc/tests/echo_link.rs` exercises the round trip over a real socket, and
`crates/keld-cli/tests/bun_echo.rs` does it with a real Bun process in the loop.

---

## 9. Transport: the app-link

[`02` §1](../architecture/02-ipc.md) specifies two links:

```text
webview ⇄ host        "wv-link"   native bridge (control) + keld:// scheme (bulk)
host    ⇄ app process "app-link"  UDS/named pipe (control) + shm rings (bulk)
webview ⇄ app process             routed via host (both links), never direct
```

Only the app-link control plane exists, and it is implemented in `crates/keld-cli/src/echo_link.rs`
rather than in `keld-ipc` — `keld-ipc` is transport-agnostic and operates on any `Read + Write`.

| | Unix | Windows |
|---|---|---|
| **Spec** ([`02` §1](../architecture/02-ipc.md)) | Unix domain socket | **Named pipe** |
| **Code** (`echo_link.rs:12-19`) | `UnixListener` / `UnixStream` | **Loopback TCP** — `TcpListener::bind("127.0.0.1:0")` |
| Endpoint value | Path + `#` + 64 hex chars | Port + `#` + 64 hex chars |

**The Windows transport still diverges from the spec on the OS object.** Loopback TCP is
not a named pipe: it is visible to any local process that can connect to the port.
**v2 closes the empty-HELLO hole:** connecting without the session token fails
`KELD-IPC-007` before any echo handler runs (KEL-60). A named pipe with a current-user
DACL remains the destination Windows transport. Electrobun choosing localhost WebSockets
is still called out in the research corpus as one of the things Keld exists to do
better.

The Unix side cleans up its socket file on `join()` and best-effort on `Drop`
(`echo_link.rs:103-119`), which matters because a stale socket file at the same path would make the
next `bind` fail.

---

## 10. Lanes specified but not built

Everything in this section is **specified, not implemented**. The frame kinds that will carry it
already have their byte values assigned (§2), which is the extent of the current investment.

**Shared-memory bulk lane (app-link).** A pair of SPSC ring buffers, one per direction, created by
the host and passed to Bun at spawn via memfd / `shm_open` / named section with fd or handle
inheritance. JS reads and writes through `ArrayBuffer` views over the mapping. A control frame
carries `{ring offset, len, generation}` and the payload bytes are never re-serialized. Fallback:
inline `FLAG_RAW` frames on the socket where shm is unavailable — containers, exotic sandboxes.
This is the only place `unsafe` is sanctioned in `keld-ipc`, and the module does not exist yet.

**`keld://` streaming (wv-link).** Engines do not reliably expose shared memory to page JS, so bulk
data to and from webviews rides a custom scheme instead: `keld://c/{channel}` request/response with
streaming bodies, served through `WKURLSchemeHandler`, WebView2's `WebResourceRequested`, or
WebKitGTK 2.40+ streams. `postMessage` stays control-only — it is string-typed on WebView2. A
renderer→app-process transfer (Electron's `send(bigBuffer)` pattern) is routed: the webview streams
over `keld://` into the host, and the host forwards into the shm ring — one copy at the
engine-imposed scheme boundary, zero after.

**Backpressure.** Per-channel credit windows granted in `Grant` frames, SPSC credit counting;
senders suspend at zero credit (a promise in JS, a state-machine pause in Rust). The design rule is
absolute — "No unbounded queues anywhere" — which is what makes Electron's frame-starving chatty-IPC
failure structurally impossible rather than merely discouraged.

**Cancellation.** `Cancel` frames carry the `corr` of the target `Call` or stream; handlers observe
an `AbortSignal` in JS and a `CancelToken` in Rust.

**Schema-first contracts.** Channels are meant to be declared in TypeScript `.k.ts` files using
`@keld/schema`, from which `keld gen` emits TS client/server stubs, Rust `serde` types and handler
traits, the channel table for the handshake, and — the part that matters most — the **permission
stubs**, so that the manifest generator and the guard enforce the same source of truth
([`02` §4](../architecture/02-ipc.md)). Neither `@keld/schema` nor `keld gen` exists; `EchoRequest`
is hand-written.

---

## 11. Config and manifest file contracts

### The sanctioned filenames

Root [`AGENTS.md`](../../AGENTS.md) §Naming is unambiguous: these four names, and nothing else,
without a spec change.

| File | Role | Generated by (per [`04` §2](../architecture/04-electron-compat.md)) | Exists today? |
|---|---|---|---|
| `keld.config.ts` | App identity, windows, runtime choice, engine policy, dev server | `keld migrate` / `create-keld` | **Partial** — a 6-line template, unparsed |
| `keld.permissions.jsonc` | Capability manifest — see [`03` §2](../architecture/03-security.md) | `keld migrate` + the doctor recorder | **No** |
| `keld.build.ts` | Packaging: targets, signing, update feed, delta settings | `keld migrate`, from electron-builder config | **No** |
| `keld.compat.ts` | Compat switches: quirks flags, `sendSync` policy, shim logging | `keld migrate`, migrated apps only | **No** |

Two more files are *edited* rather than owned: `package.json` (scripts and `@keld/*` deps replacing
`electron*`) and `bunfig.toml` or the bundler alias config (`electron` → `@keld/electron`). The spec
is emphatic about what is absent from that list: no `src-tauri/`, no Rust files, no new IDL anywhere
in the migration path.

### What `keld.config.ts` actually is today

`keld create <name>` writes five files (`crates/keld-cli/src/template.rs:13-34`):
`keld.config.ts`, `package.json`, `index.html`, `src/main.ts`, `.gitignore`. The config is this, in
full, with `{{name}}` substituted at scaffold time:

```ts
/** Keld app config — compiled by the CLI at dev/build time. */
export default {
  name: "{{name}}",
  entry: "src/main.ts",
  renderer: "index.html",
} as const;
```

Compare that with the sketch in [`04` §2](../architecture/04-electron-compat.md), which imports
`defineConfig` from `@keld/cli` and carries `app`, `runtime`, `windows`, `web`, `compat`, and `dev`
sections. Three honest observations about the gap:

1. **Parsing is name + renderer for the hello slice.** `keld-core`
   (`title_from_config_ts` / `renderer_from_config_ts`) and `keld dev`
   (`hello_title_for_project` / `load_dev_window_html`) read those two fields.
   `find_project_root` still walks up looking for the file; `keld doctor` confirms it is
   present. Other keys are not a config schema yet.
2. **`entry` is still ignored.** `dev.rs` hardcodes `project_root.join("src/main.ts")`.
   `renderer` is consulted: the window loads that file as inline HTML (default
   `index.html`). `keld hello` still uses `HELLO_HTML`.
3. **`defineConfig` cannot exist yet**, because `@keld/cli` — like every other `@keld/*`
   package — has no code. `packages/` is an empty directory.

### The permission manifest

`keld.permissions.jsonc` is the highest-stakes contract in the system. Its shape is
normative in [`03` §2](../architecture/03-security.md). v0 code is
`parse_manifest` / `load_manifest` / `evaluate` in `keld-guard` (path scopes for
`app.<group>.<action>`). Recorder, `keld doctor --permissions`, and host IPC still
calling `evaluate` on every privileged frame are not this slice. Webview camera and
microphone capture *do* call `evaluate` (`web.camera` / `web.microphone`, KEL-59).

**v0 matcher:** `$VARS` match as **literals**; a `..` path segment is always out of
scope; symlink canonicalization is not in this slice. That is not an Allow.
**Destination** (spec 03): host resolves `$VARS`, then normalizes symlink/`..`
before matching. A channel's declared capability set — derived from its `.k.ts`
contract — must be a subset of the caller's grants; that grant-shape rule is still
the destination. `crates/keld-guard/AGENTS.md` documents the v0 exception and keeps
bypass fixtures (traversal, symlink swap, case folding, wildcard-swallow) as
permanent tests for when the destination matcher lands.

---

## 12. The spawn contract

When the host starts the developer's JS main process, everything the child needs to find its way
back arrives through the environment. This is the contract that lets the Bun process be a plain
`bun run` with no embedding and no patched runtime — [`06` §1](../architecture/06-runtime-and-tooling.md)
calls it "a versioned process contract" and is explicit that it is a contract *instead of* embedding,
because Bun has no stable embedding C API.

### What the code actually passes

```rust
// crates/keld-cli/src/dev.rs:88-95
let mut bun = Command::new("bun");
bun.arg("run")
    .arg(&bun_main)
    .current_dir(project_root)
    .env("KELD_APP_LINK", &link)
    .env("KELD_BIN", &keld_bin)
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit());
```

| Variable | Value | Consumed by |
|---|---|---|
| `KELD_APP_LINK` | `<endpoint>#<64 hex chars>` — Unix endpoint is the UDS path, Windows endpoint is the loopback port (`echo_link.rs`) | The template's `main.ts:6`; absence is a hard error; missing `#token` is `KELD-IPC-007` |
| `KELD_BIN` | `std::env::current_exe()` — the path to the running `keld` binary | `main.ts:14`, to spawn the Rust IPC client |

The child's half of the contract is to refuse to run outside it:

```ts
// crates/keld-cli/templates/hello/src/main.ts:6-12
const link = process.env.KELD_APP_LINK;
if (!link) {
  console.error(
    "KELD-CLI-010: KELD_APP_LINK is unset — run the app with `keld dev`, not `bun` directly.",
  );
  process.exit(1);
}
```

That is the error standard (§13) applied to an environment contract: it names the missing variable
and gives the exact command to use instead.

### Three different specified namings, none of which match the code

This is a real inconsistency in the repository, not a summarizing artifact, and it will need
resolving the first time anyone implements the real spawn path:

| Source | Names |
|---|---|
| [`01` §4](../architecture/01-overview.md) | `KELD_IPC_FD` / pipe name, plus a `keld.app.json` contract file |
| [`06` §1](../architecture/06-runtime-and-tooling.md) | `KELD_LINK={fd\|pipe}`, `KELD_SHM={handle}`, `KELD_CONTRACT=keld.app.json` |
| Code (`dev.rs:92-93`) | `KELD_APP_LINK`, `KELD_BIN` |

Grepping the workspace for `KELD_IPC_FD`, `KELD_LINK`, `KELD_SHM`, `KELD_CONTRACT`, or
`keld.app.json` returns nothing outside those two spec files. **`keld.app.json` does not exist in
any form** — no schema, no writer, no reader. Per root `AGENTS.md`, the two specs disagreeing with
each other is itself a bug to be fixed in the same PR as whichever one gets implemented, and picking
a winner is a wire-protocol decision subject to the review gate.

Note also that `KELD_BIN` is an artifact of the current shape rather than part of the intended
contract: it exists only because the Bun child has no `@keld/api` and must shell out to a Rust
subprocess to speak kipc. It should disappear when the JS client lands.

---

## 13. Error code taxonomy

[`07` §2](../architecture/07-agent-experience.md) makes errors a framework-wide contract, on the
premise that a stable greppable code plus an imperative fix is what lets both a human and an agent
recover without reading source. Every developer-facing error carries five things: `code`, `message`
naming the failing value, `cause`, **`fix`** as an imperative next step, and a `docs` URL.

### Codes in the code today

| Code | Meaning | Defined at |
|---|---|---|
| `KELD-IPC-001` | I/O error | `keld-ipc/src/lib.rs:51` |
| `KELD-IPC-002` | Bad frame header (wraps `HeaderError`) | `lib.rs:52` |
| `KELD-IPC-003` | Codec error (postcard) | `lib.rs:53` |
| `KELD-IPC-004` | Payload too large for a kipc frame | `lib.rs:54` |
| `KELD-IPC-005` | Protocol error — unexpected frame or state | `lib.rs:57` |
| `KELD-IPC-006` | App-link I/O deadline exceeded | `keld-ipc/src/lib.rs` (`IpcError::Timeout`) |
| `KELD-IPC-007` | HELLO session token rejected | `keld-ipc/src/lib.rs` (`IpcError::HelloAuth`) |
| `KELD-WV-001` | No webview backend for this OS | `keld-wv/src/error.rs:33` |
| `KELD-WV-002` | Window creation failed | `error.rs:38` |
| `KELD-WV-003` | Webview creation failed | `error.rs:43` |
| `KELD-WV-004` | Event loop error | `error.rs:48` |
| `KELD-WV-005` | Navigation failed | `error.rs:49` |
| `KELD-WV-006` | Script evaluation failed | `error.rs:54` |
| `KELD-WV-007` | Unknown webview id | `error.rs:59` |
| `KELD-CLI-010` | `KELD_APP_LINK` unset in the app process | `templates/hello/src/main.ts:9` |
| `KELD-CLI-020` | Invalid project name | `keld-cli/src/create.rs:28` |
| `KELD-CLI-021` | Target directory already exists | `create.rs:34` |
| `KELD-CLI-022` | Failed to write template | `create.rs:38` |
| `KELD-CLI-030` | Dev session I/O error | `keld-cli/src/dev.rs:30` |
| `KELD-CLI-031` | Dev session failed | `dev.rs:31` |
| `KELD-CLI-032` | Environment checks failed | `dev.rs:71` |
| `KELD-CLI-040` | Missing `--link` argument | `keld-cli/src/main.rs:111` |
| `KELD-CLI-044` | Unknown `create` / `dev` / `doctor` / `hello` flag (exit 2) | `keld-cli/src/flags.rs` |
| `KELD-CLI-045` | Reserved verb `build` / `migrate` / `gen` / `ext` (exit 2) | `keld-cli/src/verb.rs` |
| `KELD-CLI-046` | Unknown command (exit 2) | `keld-cli/src/verb.rs` |

### The "errors state the fix" rule, demonstrated

`keld-wv` is the reference implementation, and its fix text is *tested* — `error.rs:74-118` asserts
that each of the seven variants renders both its code and a fix hint, so a message that degrades to
a bare description fails CI:

```rust
// crates/keld-wv/src/error.rs:33-37
Self::UnsupportedPlatform { os } => write!(
    f,
    "KELD-WV-001: no webview backend for `{os}` yet. \
     Track KEL-27 (Windows) / KEL-28 (Linux) or run on macOS."
),
```

`keld-cli` follows the same shape (`KELD-CLI-021` names the colliding path *and* says "Choose
another name or remove the folder").

**The kipc codes do not meet the bar yet.** `KELD-IPC-001` through `005` render a code and a
description but no fix — compare "KELD-IPC-004: payload too large for kipc frame" with the `WV-001`
example above. Likewise `keld-guard`'s `DenyReason` renders the capability and the failing scope but
not the manifest edit that would grant it, which
[`07` §2](../architecture/07-agent-experience.md) names as the floor: `DenyReason` "is the floor —
it must also say what edit would grant it."

### Two conventions still unsettled

Worth knowing before you mint a new code, because both are cheap to fix now and expensive later:

- **Code format.** [`07` §2](../architecture/07-agent-experience.md) writes the pattern as
  `KELD-<area><nnn>` with `KELD-GUARD012` as its example — no separator before the digits. Every
  code actually in the codebase uses `KELD-<AREA>-<nnn>`. The implemented form is more readable and
  more greppable; the spec has not been updated to match.
- **Docs URLs and the registry.** The standard requires each code to resolve at
  `https://keld.dev/e/KELD-<area><nnn>`, and requires CI to fail when a code is added without a docs
  page. Error text does not yet carry that URL. The registry lives at
  [`docs/engineering/keld-error-codes.md`](../engineering/keld-error-codes.md) and is
  checked 1:1 against scanned sources by `crates/keld-cli/tests/error_registry.rs`.

The CLI half of the contract from [`07` §7](../architecture/07-agent-experience.md) is
partially implemented: `keld doctor --json` emits the findings array, and `keld mcp`
misuse exits `2`. Other verbs still exit `1` on failure and have no `--json` flag.

---

## 14. Change checklist

Before you touch anything in this document's scope:

| Change | Requires |
|---|---|
| Frame layout, `FrameKind`, flags, or handshake | Protocol version bump + spec [`02` §2](../architecture/02-ipc.md) edit + code, **one PR**, wire review gate |
| A new channel or a change to an existing payload struct | Public API review gate; contract belongs in a `.k.ts` schema once `@keld/schema` exists |
| Manifest schema (`keld.permissions.jsonc`) | Permission-model review gate **and** wire review gate |
| Update feed format | Wire review gate |
| A new config filename | Spec change — the four names in root `AGENTS.md` §Naming are exhaustive |
| A new error code | A docs page for it (the standard makes this a CI gate once the registry lands) |

And the standing rules that apply to all of it: test wire constants as protocol facts rather than as
struct layout (`HEADER_LEN == 16` is asserted independently for exactly this reason); fuzz the decode
paths, because malformed input from a webview is expected rather than exceptional; and keep the hot
path free of async and of steady-state allocation.
