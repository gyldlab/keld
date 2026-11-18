# Endpoint codec qualification

Part of: [index](index.md)
Owns: sections 5, 6, 7, 9 and 10

## 5. Boundaries

**Implement after independent approval only:** the single
`@keld/electron/src/endpoint-value-codec.ts`, its independent
byte-golden/hostile tests, and minimal cold Bun/browser
build and generated source-identity checker.

**This PR edits one specification only:**
`docs/specs/gh544-endpoint-value-codec/`. No Rust,
KIPC table/generator, guard, native renderer bridge,
GH540/GH541 architecture amendment, GH464 conformance
registry, GH467/GH484 world, GH465/GH543 routing,
workspace Cargo/Bun locks or other owner's PR/worktree.
No new dependency, unsafe operation, permission, channel,
link, resource handle or application-facing API export.

## 6. Tasks

### T-1: this draft specification.

- **Blocked by:** none

Independent exact-head
public-API and wire/value security review; resolve §10 with
GH540/error-registry owners, and record an explicit
`Status: approved` receipt **before any production changes**.

### T-2: one TS encoder/decoder.

- **Blocked by:** T-1, B-1, B-2

After a fresh implementation
issue claim and dependency disposition, failure-first
hand-authored goldens, canonical error-code registration,
forbidden-type/prototype/alias/depth/budget tests.

### T-3: identical endpoint builds.

- **Blocked by:** T-2

Generate renderer from
the same source; run full golden corpus separately in
Bun and browser-like environment; mutation kills a
hand-edited renderer output.

### T-4: consumer handoff, not implementation here.

- **Blocked by:** T-3


GH465/GH543/GH466 consume tested codec; GH467/GH484
prove genuine `rendererReq → mainResp`; GH464 alone
updates scored cells on actual passing evidence.

## 7. Test plan & evidence

| Criteria | Test | Mutation that must fail |
|---|---|---|
| V1 | Two fresh endpoint builds match all independent goldens and checked source identity | Hand-edit renderer artifact without source change |
| V2 | Every positive §4.3 vector matches full bytes and typed shape in both realms, including `Object.is(decoded,-0)` and `Date(1)` little-endian | Replace tagged encoding with `JSON.stringify`, collapse `-0` to `+0`, flip Date i64 endianness or keep class prototype |
| V3 | Root+nested forbidden type table, zero sends | Accept Function/Symbol/Map/Buffer or defer failure to Host |
| V4 | Cycle, `[x,x]`, invalid Date, sparse; each Error own `name`/`message`/`stack` accessor must fail typed with getter counter **0** | Replace descriptor reads with `error.stack`/`error.name`, invoking an accessor |
| V5 | Independent 16-byte U+E000/U+10000 and 12-byte `aa`/`z` raw UTF-8 positives in both builds; corresponding wrong UTF-16 and wrong length-prefix order `MalformedValue` negatives; other hostile tags/keys and unchanged prototype | Mutate to JS default `.sort()` or length-prefixed-key sort (respective golden fails), or accept either reversed key order (typed negative fails) |
| V6 | See V6 boundary tests below | See V6 boundary mutations below |
| V7 | Encode then mutate caller object and decoder byte source; output unchanged; Host opaque W12 | Retain mutable caller/transport memory or parse value tag in Rust |
| V8 | Small `reqId`+Error data endpoint fixture, conformance score unchanged | Drop `reqId`, silently map Error to `{}` |

### V6 boundary tests

Independent 4096/4097 bytes, depth 16/17 (root included), value nodes 256/257, aggregate
value/property visits 255/256/257 (127-key nulls, exact-256-visit positive, 128-key
nulls), extra 254-key/509-visit negative, and 16/17 IPC arguments at the selected
arguments-root boundary; allow a generic 17-item result array. Encoder/decoder reject
over-budget work before dispatch; Proxy traps can run before returning but do not grant
Host authority.

### V6 boundary mutations

Mutations: reject exactly 256 visits instead of only over-limit (the exact positive MUST
fail), omit the IPC arguments cap (17 null args must fail), omit property-key charging
(128 and 254 null-key objects must fail), retain a property descriptor before budget
admission, or treat a Proxy trap as trusted Host admission.

Pure codec tests use no sleeps, ports or native GUI.
Run Bun tests/typecheck and hosted relevant CI.
Real native/senderFrame/credit-release/draw.io two-request
acceptance belongs to GH465/GH543/GH466/GH467/GH484,
never inferred from a codec-only test.

## 9. Perf impact

No performance gain claimed. An admitted value uses <=4096
encoded bytes, <=16 depth (root level 1), <=256 value nodes and <=256
aggregate value/property visits, with <=16 arguments for IPC callers;
later code must
measure transient scratch/renderer bundle size and mutation
cost. GH540 owns whole-hop quota, envelope and backpressure;
GH544 cannot raise them or add another Worker/pump/queue.

## 10. Open questions, blockers & unknowns

### Q-1: Owner question 1

Does the GH540/IPC and canonical error-registry owner
approve these **proposed** closed tags `00..09`, signed
Date, UTF-8 key sort, reserved keys, Error field own-data-only
extraction/defaults, Proxy trap limitations, stack optional
flag, caps and noncolliding typed error codes/fix messages?
No tags or numeric errors are currently deployed.

### Q-2: Owner question 2

GH544 remains blocked by open GH464 and approved-but-
**unmerged** GH540. The owner has explicitly authorized
**this spec first**, not production T2. Confirm the exact
implementation-edge disposition before T2.

### Q-3: Owner question 3

Approve the cold renderer source-identity checker and
generated artifact path without editing GH467/GH484
preload/world or creating a second hand-written codec.

### Q-4: Owner question 4

Keep unsupported value types an explicit v0 divergence
from full Electron structured clone; only GH464 owns
the downstream score and pinned product evidence.

### B-1: Exact-content approval

- **Status:** open
- **Owner:** GH544 and GH540 public-API/wire owners and canonical error-registry owner
- **Impact:** Production T2 and approval remain blocked.
- **Check:** Record exact-content approval, error allocation and canonical source-location reconciliation.

### B-2: Pre-enumeration resource proof

- **Status:** open
- **Owner:** GH540/GH544 resource owners
- **Impact:** V6 and public codec implementation remain BLOCKED.
- **Check:** Approve finite enumeration admission or tighter input policy and verify runtime memory/CPU high-water controls.

### B-3: Endpoint source identity

- **Status:** open
- **Owner:** GH544 endpoint build owner
- **Impact:** Endpoint qualification remains blocked.
- **Check:** Run both endpoint goldens and cold build-identity checks, including the renderer-edit negative.

### B-4: Native consumer acceptance

- **Status:** open
- **Owner:** GH465/GH543/GH466/GH467/GH484 and GH464 evidence owners
- **Impact:** Product acceptance and conformance-cell flips remain blocked.
- **Check:** Record genuine native/listener evidence and independently accepted consumer results.

**DRAFT / DO NOT MERGE.** Stop before implementation
until exact-content owner approval and `Status: approved`.
