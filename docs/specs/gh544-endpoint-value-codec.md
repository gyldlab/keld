# Spec: host-opaque Electron endpoint value codec v0 (GH544)

Status: draft

Tracker: [GH544](https://github.com/gyldlab/keld/issues/544) · Owner: GYLDLAB / `@keld/electron` · Updated: 2026-10-25

## 1. Goal & non-goals

Provide **one** bounded TypeScript codec shared unchanged by the Bun
Electron facade, isolated renderer compatibility runtime and future
`executeJavaScript` result-value consumer. An endpoint value survives
encode → Host-opaque forwarding → decode with the selected first-proof
domain: JSON values, `undefined`, `Date`, ordinary `Error`
(name/message/optional stack), and class instances flattened to their
own enumerable data properties. Independent golden bytes and a
renderer-build identity check must refute a JSON-only replacement.

**Non-goals:** `ipcMain`/`ipcRenderer` implementation, native routing,
contextBridge callback proxies, envelope/result ownership, binary and
MessagePort transfer, general structured clone, new KIPC channel,
second transport or conformance score changes. Actual
`rendererReq → mainResp` belongs to GH465/GH543/GH466/GH467/GH484.
**Spec-only draft; no implementation authorized.**

## 2. Spec refs

- [GH544](https://github.com/gyldlab/keld/issues/544), F03-D4:
  **sole** Electron IPC value-codec owner. The page↔preload
  function proxy is a different GH467 boundary: draw.io `watchFile`
  callbacks remain local and are removed before sending data.
- [GH540 approved-but-unmerged PR #1122](https://github.com/gyldlab/keld/pull/1122),
  exact `3d72e492916259d89b1bf4723329e602ea6e1691`,
  `docs/specs/gh540-compat-ipc.md` §4.3 (maximum **4,096 bytes**
  encoded value; bounded generic envelope), §4.10 (value subset,
  unsupported values and synchronous typed errors), §6 T3c (codec
  independent of native-identity and transport lanes), §7 E3/W12
  (hostile-value and opaque-Host controls). Its architecture 02 §2/§5
  amendment is **not merged**; GH540 alone owns that amendment.
- [GH541 approved-but-unmerged PR #1123](https://github.com/gyldlab/keld/pull/1123),
  current `1596db78639adeda8d99d74245fd697bb4f5090e`,
  `gh541-channel-grants-navigation.md`: grants/navigation identity only.
- Main `de768dee8c3a9ba512ee862fe1c409a41fac5a1f` has
  `@keld/electron/src/index.ts` exporting app/error helpers,
  `@keld/api/src/channels.ts` handling Echo alone, and
  `@keld/kipc/src/transport.ts` handling frame/postcard/WorkerLink;
  **none** implements this value codec. Governed by architecture
  02 §§2,5; 03 (Host default deny); 04 (compatibility evidence).
- GH464/#1130 owns **expected-fail conformance** and any eventual
  score flip; this spec changes no GH464 or GH467 files.

## 3. Acceptance criteria (binary)

1. **V1, one source:** Bun and renderer consume the exact same TS
   codec source/build, reproduce §4.3 golden bytes independently, and
   fail deterministic cold build drift after a renderer-only edit.
2. **V2, fidelity:** `undefined !== null`, signed Date milliseconds,
   Error name/message/optional stack (no custom properties), and a
   class instance's own fields round-trip with no original prototype.
3. **V3, forbidden input:** Root and nested Function, Promise, Symbol,
   WeakMap, WeakSet, Map, Set, RegExp, ArrayBuffer, DataView, Buffer
   and typed arrays synchronously throw a typed error **before any
   send**; no application listener fires.
4. **V4, no lossy coerces:** Cycles, shared object aliases, BigInt,
   NaN/infinity, invalid Date, malformed UTF-16, sparse arrays,
   accessor properties and symbol-keyed data refuse rather than
   serialize an invented value or invoke a property getter.
5. **V5, hostile decoder:** Unknown tag, truncation, trailing bytes,
   duplicate/unordered map keys, invalid UTF-8, reserved prototype key
   and malformed nested value refuse without writing to a prototype.
6. **V6, bounds:** Exact encoded byte boundary 4096 accepts and 4097
   refuses (otherwise valid value); nesting 32/33 and total value
   nodes 256/257 accept/refuse. All checks precede unbounded
   traversal, allocation or dispatch.
7. **V7, copies and layers:** Mutating the caller's value after encode
   or the input `Uint8Array` after decode leaves the returned owned
   result unchanged. Host forwards the opaque byte sequence without
   knowing value tags; no Rust crate decodes them.
8. **V8, consumer:** Independently round-trip small draw.io
   `rendererReq` args and Error-shaped `mainResp` data with unchanged
   `reqId`, while GH464's expected-fail product cells remain **fail**
   until genuine native and listener acceptance.

## 4. Design

### 4.1 Source, trust, lifecycle and reuse

The only owner is
`packages/@keld/electron/src/endpoint-value-codec.ts`, exposing
package-internal `encodeEndpointValue(value: unknown): Uint8Array`,
`decodeEndpointValue(bytes: Uint8Array): unknown` and typed
`CompatCodecError`. A cold maintainer build generates the browser
copy **from that source**, with pinned build settings and a checked
artifact manifest; Bun imports the canonical module. The build
identity checker rebuilds in fresh scratch and compares full bytes
and source/build-config digest (not a self-attested comment hash).
The same independently authored goldens run in both environments.

The codec never transports a callback, principal, claimed URL,
navigation identity or Host authority. GH540's generic envelope and
GH541's native-stamped route determine destination/permission; the
Host forwards value bytes unchanged. Existing KIPC framing,
postcard, per-link Worker and canonical guard remain sole owners.
A local malformed-value failure is **not** a Host
`NotDispatched` proof and never authorizes a retry. No second codec,
socket, Worker, channel or JSON-only fallback. App-protocol
`mainResp` Error **value data** is not a thrown KIPC `CallError`.

### 4.2 Exact private binary grammar (proposed, not approved)

One root tagged value consumes the **entire** payload. All string
and key lengths and array/object counts are unsigned 16-bit
little-endian; strings are strict UTF-8 of Unicode scalar values
(never repair lone UTF-16 surrogates). f64 is IEEE-754 little-endian;
signed Date milliseconds use little-endian i64 in the JS valid
Date range. No extra version header: the owning GH540 generic
message envelope carries its version.

| Tag | Followed by | Value |
|---|---|---|
| `00` | nothing | `undefined` |
| `01` | nothing | `null` |
| `02` / `03` | nothing | Boolean false / true |
| `04` | 8-byte finite f64 | Number, preserves negative zero |
| `05` | u16 byte length + UTF-8 | String |
| `06` | u16 count + that many tagged values | Dense Array |
| `07` | u16 count + (u16 key byte length, UTF-8 key, tagged value) × count | Own-data Object |
| `08` | signed i64 epoch milliseconds | Valid Date |
| `09` | name string, message string (each u16+UTF-8), stack-present byte `00`/`01`, optional stack string | Ordinary Error |

For tag 07, keys are strictly ascending by **unsigned UTF-8
bytes**; no duplicates. Both encoder and decoder reject the own
keys `__proto__`, `constructor` and `prototype`. The decoder
constructs ordinary objects with validated own data descriptors
(`Object.defineProperty`), **never** `obj[key]`. Class instances
and plain objects encode only own enumerable string-keyed **data**
properties, excluding prototypes; enumerable accessors and symbol
keys reject before reading getters. Dense arrays have every
index `0..length−1` and no extra enumerable fields. Date/ordinary
Error custom properties are dropped; `stack` is optional, distinct
from the empty string. Error subclass identity is not preserved.

Reject unsupported/exotic values and forbidden input in V3/V4,
without `toJSON` or application getter execution on ordinary
data objects. `seen` identity is session-wide during encode,
rejecting cycles **and repeated references**, including reused
Date/Error objects. Check encoded length **<=4096** incrementally
and *before* any transport admission; decoder checks input bytes
<=4096 before parse, depth (root=0) <=32, total decoded value
nodes (including root and containers) <=256, declared count
<=remaining node budget **before allocation**, exact input
consumption and strict canonical forms. No unbounded recursion,
arrays or `TextDecoder` replacement. Root tags cannot be
concatenated.

Failures use three typed semantic classes:
`UnsupportedValue(type)`, `MalformedValue(reason)`,
`CodecLimit(bound)`. Existing canonical error-registry owner
must allocate noncolliding `KELD-COMPAT-*` codes with fix text
**before implementation**; this draft does **not** reserve a
number or reuse COMPAT-001..009. Each local error occurs
synchronously, not through an invented Electron error listener.

### 4.3 Independent literal golden bytes

These are complete payloads in hex, **not output copied from
the implementation**. `Error("x")` has its `stack` property
deleted when testing the absent-stack fixture; `class C { a=1 }`
has no other own enumerable properties.

| Input | Complete bytes (hex) | Decode oracle |
|---|---|---|
| `undefined` | `00` | undefined |
| `null` | `01` | null |
| `false` | `02` | false |
| `1` | `04 00 00 00 00 00 00 f0 3f` | Number 1 |
| `"a"` | `05 01 00 61` | `"a"` |
| `[null,undefined]` | `06 02 00 01 00` | dense length 2 |
| `{a:1}` | `07 01 00 01 00 61 04 00 00 00 00 00 00 f0 3f` | own `a=1` |
| `new C()` | `07 01 00 01 00 61 04 00 00 00 00 00 00 f0 3f` | plain object, **not** `C` |
| `new Date(0)` | `08 00 00 00 00 00 00 00 00` | epoch 0 |
| `new Date(-1)` | `08 ff ff ff ff ff ff ff ff` | epoch -1 |
| `Error("x")`, no stack | `09 05 00 45 72 72 6f 72 01 00 78 00` | Error, name/message, no stack |
| Error with stack `"z"` | `09 05 00 45 72 72 6f 72 01 00 78 01 01 00 7a` | stack `"z"` |

Hostile byte goldens: `ff` unknown tag; `04 00` truncated
number; `00 00` trailing byte; `06 ff ff` impossible count;
`07 01 00 09 00 5f 5f 70 72 6f 74 6f 5f 5f 01`
(one `__proto__` key). Each must yield typed rejection,
no completed value, and untouched `Object.prototype`.
Independently construct duplicate/out-of-order keys, invalid
UTF-8, malformed nested tag, depth=33, node=257 and exact
4096/4097 bytes; the intentional mutant **untagged JSON**
must fail undefined, Date and Error values.

### 4.4 Delivery, error and recovery boundaries

The encoder snapshots arguments into owned bytes *before*
a void send is admitted, and does not reuse input buffers.
The decoder returns owned JS values with no alias to the
transport byte buffer. The generated renderer codec is
browser-safe and does not expose raw `ipcRenderer` or
internal message fields to a page. A local unsupported
value rejects before any native forwarding. Host/role
retirement and `DocumentRetired`, grant evaluation, stale
result disposition, IPC errors and callback ordering are
**owned by GH540/GH541/GH465/GH543/GH466**.
GH467/GH484 continue to own page-world callback proxies
and the app preload lifetime. No persisted-state migration.

## 5. Boundaries

**Implement after independent approval only:** the single
`@keld/electron/src/endpoint-value-codec.ts`, its independent
byte-golden/hostile tests, and minimal cold Bun/browser
build and generated source-identity checker.

**This PR edits one new document only:**
`docs/specs/gh544-endpoint-value-codec.md`. No Rust,
KIPC table/generator, guard, native renderer bridge,
GH540/GH541 architecture amendment, GH464 conformance
registry, GH467/GH484 world, GH465/GH543 routing,
workspace Cargo/Bun locks or other owner's PR/worktree.
No new dependency, unsafe operation, permission, channel,
link, resource handle or application-facing API export.

## 6. Tasks (ordered, each independently reviewable)

- [ ] **T1: this draft specification.** Independent exact-head
  public-API and wire/value security review; resolve §10 with
  GH540/error-registry owners, and record an explicit
  `Status: approved` receipt **before any production changes**.
- [ ] **T2: one TS encoder/decoder.** After a fresh implementation
  issue claim and dependency disposition, failure-first
  hand-authored goldens, canonical error-code registration,
  forbidden-type/prototype/alias/depth/budget tests.
- [ ] **T3: identical endpoint builds.** Generate renderer from
  the same source; run full golden corpus separately in
  Bun and browser-like environment; mutation kills a
  hand-edited renderer output.
- [ ] **T4: consumer handoff, not implementation here.**
  GH465/GH543/GH466 consume tested codec; GH467/GH484
  prove genuine `rendererReq → mainResp`; GH464 alone
  updates scored cells on actual passing evidence.

## 7. Test plan

| Criteria | Test | Mutation that must fail |
|---|---|---|
| V1 | Two fresh endpoint builds match all independent goldens and checked source identity | Hand-edit renderer artifact without source change |
| V2 | Every §4.3 vector matches full bytes and typed shape in both realms | Replace tagged encoding with `JSON.stringify`, or keep class prototype |
| V3 | Root+nested forbidden type table, zero sends | Accept Function/Symbol/Map/Buffer or defer failure to Host |
| V4 | Cycle, `[x,x]`, invalid Date, sparse, accessor getter count zero | Remove seen set or invoke a getter |
| V5 | Hostile keys/tags/UTF-8/duplicate/trailing fixtures, Object.prototype unchanged | Assign decoded `__proto__` through setter |
| V6 | Exact length 4096/4097, depth 32/33, nodes 256/257 with bounded allocation | Omit preallocation count check or change `>` to `>=` |
| V7 | Encode then mutate caller object and decoder byte source; output unchanged; Host opaque W12 | Retain mutable caller/transport memory or parse value tag in Rust |
| V8 | Small `reqId`+Error data endpoint fixture, conformance score unchanged | Drop `reqId`, silently map Error to `{}` |

Pure codec tests use no sleeps, ports or native GUI.
Run Bun tests/typecheck and hosted relevant CI.
Real native/senderFrame/credit-release/draw.io two-request
acceptance belongs to GH465/GH543/GH466/GH467/GH484,
never inferred from a codec-only test.

## 8. Review gates triggered

- **Public API:** yes, Electron-visible value domain and
  synchronous typed refusal contract.
- **Wire protocol:** yes, endpoint-private **value** byte
  format under GH540's unmerged opaque Host envelope;
  no KIPC frame, channel or version change is claimed.
- **Permission model:** no change; GH541/GH552 own Host grants.
- **Unsafe:** none. **Dependency addition:** none.
- Independent Amisha/nonauthor review required on exact
  draft; CI-green documentation is **not** spec approval.
  No self-approval, merge or conformance-cell flip.

## 9. Perf impact

No performance gain claimed. An admitted value uses <=4096
encoded bytes, <=32 depth and <=256 nodes; later code must
measure transient scratch/renderer bundle size and mutation
cost. GH540 owns whole-hop quota, envelope and backpressure;
GH544 cannot raise them or add another Worker/pump/queue.

## 10. Open questions (block implementation)

1. Does the GH540/IPC and canonical error-registry owner
   approve these **proposed** closed tags `00..09`, signed
   Date, UTF-8 key sort, reserved keys, Error-stack optional
   flag, caps and noncolliding typed error codes/fix messages?
   No tags or numeric errors are currently deployed.
2. GH544 remains blocked by open GH464 and approved-but-
   **unmerged** GH540. The owner has explicitly authorized
   **this spec first**, not production T2. Confirm the exact
   implementation-edge disposition before T2.
3. Approve the cold renderer source-identity checker and
   generated artifact path without editing GH467/GH484
   preload/world or creating a second hand-written codec.
4. Keep unsupported value types an explicit v0 divergence
   from full Electron structured clone; only GH464 owns
   the downstream score and pinned product evidence.

**DRAFT / DO NOT MERGE.** Stop before implementation
until exact-content owner approval and `Status: approved`.
