# Spec: macOS public filesystem adapter and explicit project policy
Status: approved
Linear: KEL-140 · Owner: GYLDLAB · Updated: 2026-10-10
Source base: `0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f`; implementation base: `4dba1144e1a7de22c739df080b3a58ca7db4ecce`

Human approval: the repository owner approved the explicit macOS project-policy recommendation in the Codex continuation chat on 2026-10-10, after reviewing its behavior. Exact approved draft SHA-256: `8716cca8139e6a0c55f310e1817ce40e4d3eabce5ce66012ddc7165565b2c339`; approved task graph: `c094153fe178e30ae21f2b3bc96b4318521cb21865a0f733cb1b511df7925105`. The approval source is retained in the continuation records and referenced by KEL-140's trusted Agent claim. Independent exact-content review found no unresolved contract finding. Approval authorizes this implementation and linked producer amendments; it does not establish implementation acceptance or authorize a merge.

This specification is the canonical implementation contract. The reviewed local draft, graph and original decision packet remain historical immutable inputs. Source-base to implementation-base changes are dependency action pins only; affected production source is unchanged.

## 1. Goal & non-goals

An ordinary prepared macOS app uses public `@keld/api.fs.read/write`, its existing singleton app-link, the landed KEL-102/T3 route and KEL-130 broker. The existing CLI dev producer snapshots an explicit fixed project policy, or retains historical all-denied bytes when the project policy is absent. One OS-visible renderer action reaches an ordinary Echo handler, then guarded allow/read and independently observed deny/no-effect. The app's public adapter and normal producer, rather than post-stage substitution or a private transport fixture, are the acceptance path.

Non-goals: generic schema/IDL, `keld gen`/new build command, npm publication/distribution, dependency installation during staging, general module copying, renderer FS admission, new channels/CallError fields/wire enums, a new principal/guard/broker/transport/restart owner, permission merging/expansion, release authenticity, strict-profile authority, other native modules, performance claims, retry/replay/rollback, native syscall preemption, or exactly-once effects after a lost reply. This child qualifies macOS only. Windows/Linux project-policy staging retains current exact all-denied behavior until a separately approved platform slice; no cross-OS product acceptance is inferred.

## 2. Spec refs

Governing: architecture01 host authority; architecture02 postcard/receiver/deadline/correlation; architecture03 default-deny; architecture05 renderer/native composition; architecture06 supervision/shutdown. Approved predecessors: KEL-139 AC4/T2, KEL-142 renderer/lifecycle/channel and its public-index bundle composition, KEL-102/T3 exact landed artifact, KEL-130/T1, KEL-133 receiver semantics, KEL-136 canonical TS transport, KEL-98 bounded cold codegen. This child explicitly proposes a macOS successor amendment to KEL-96 AC3.6/§4.3 and KEL-102 development-fixture wording. It does not reopen predecessor acceptance.

KEL-140 records exact T3 terminal comment `cde25f5e-db85-4d22-9349-553469fecea0`, landed head `66ccbbc68c3dd44be55878bb0c14e9788e25a4b1`, tree `9b23e5b456b2590cf58829eb23c8ad76ff4dc13d`, PR357, CI37313738834 success. Root refreshes exact artifacts, ownership/claim and pin delta before implementation. A Done label is insufficient.

## 3. Acceptance criteria

1. **Exact API / singleton:** public index exports the `fs` object below and retains existing exports. A real Bun app calls `app.whenReady`, registers the declared Echo handler and performs FS on the same single `WorkerLink` instance. A second connection attempt or copied transport cannot satisfy the test. Compatibility lifecycle consumers continue delegating to the same app owner.
2. **Payload binding:** generated FS binding comes from the authoritative Rust enums, not hand-written TS declarations/variant constants. Existing generator generate/check owns the output. Rust and TS agree on immutable semantic vectors, including both requests/responses, empty/multibyte content and UTF-8 paths; malformed/truncated/trailing/unknown-variant/wrong-response-variant controls reject. Source-schema or codec-order mutation must fail a named independent oracle.
3. **Producer selection / capture:** on macOS a present fixed project `keld.permissions.jsonc` is selected by retained no-follow root/leaf handles, regular type and invoking-owner validation. One bounded capture buffer is written and digested. Replacing a path after open does not replace consumed bytes. Missing leaf yields exact historical `{}\n`/digest. Symlink, non-regular, foreign-owned, unreadable or oversized present input never becomes fallback and no application resource is created. Original dev-stage cleanup survives every failure.
4. **One parser / immutable policy:** malformed/invalid staged policy fails through existing host verified-loader/broker before window/listener/Bun. The CLI adds no JSONC parser, matcher, `evaluate` call or permit. Changing project policy after source capture cannot change that running session. The host still reads/hash/parses from its already retained stage handle; missing stage or digest mismatch is startup failure.
5. **Normal public app:** the configured entry is prepared from the public API index with the established separate canonical transport file. Ordinary `keld dev`/stock `stage_dev_boot` handles this project without policy replacement, private API import or direct socket. OS-visible input through `window.keld.invoke(1, payload)` → declared Echo handler → public `fs.write/read` returns exact small bytes under explicit narrow grants. Independently verify staged policy/digest and executable/process/window/document identities.
6. **Denial/result preservation:** the outside target returns actual `KELD-GUARD002` with original message/fix through public FS and typed sample application data; an independent OS sentinel is unchanged. Explicit `{}` or absent policy produces existing all-denied `KELD-GUARD001`. Throwing an unrelated Echo handler failure remains existing `KELD-API-001`. No CallError field/forwarding rule changes.
7. **Limits/retirement:** public FS calls use existing finite async-call deadline and same canonical pending/abandoned-call behavior. Native request and effect semantics remain unchanged. Replay prerequisite symlink/race/oversize/deadline and retired/post-quiesce cases through application/public FS; no old handler entry, duplicate terminal outcome, fabricated native cancellation result or automatic replay. Observe canonical overflow at the actual ring boundary, without queue enlargement.
8. **Compatibility/qualification:** default hello's historical stage bytes/digest, Echo, real Ready and ordered Quit remain healthy; non-macOS staging output remains historical. Exact-final-diff review/gates and real-macOS product/negative-control evidence pass. Approval itself establishes no passed implementation criterion.

## 4. Design

### 4.1 Ownership, trust, lifecycle and I/O

**No authority ownership boundary change.** Host still mints principal/generation, retains policy/broker and performs sole authorization; Bun owns app logic and payload data, not a permit or retained scope root. The existing Worker owns one socket, HELLO, correlation, reader/ring and serialized writes. UI stays outside blocking filesystem execution. Existing coordinator owns admission/cancel/quiescence/destruction; local caller timeout does not prove native effect/return or free old authority.

CLI source capture is cold tooling, separate from the host's immutable policy read. A source pathname selects a root/leaf object once; an owned byte buffer stabilizes the captured input. It is not an atomic filesystem-version snapshot: a same-user concurrent in-place writer may change bytes while they are read. The guarantee is that **the buffer actually captured**, staged and hashed is the same, and path replacement cannot reopen a different object. Validity and authority are assessed by the existing host owners before resources. No release-authentication or zero-ambient-Bun claim follows.

### 4.2 Exact public adapter

```ts
export const fs: {
  read(path: string): Promise<Uint8Array>;
  write(path: string, bytes: Uint8Array): Promise<void>;
};
```

Only `fs` is added to the package public index; no public generic outbound call, FS channel descriptor, codec, endpoint/token or request-authority object. Existing `app`, `channels`, Echo and error exports are unchanged.

Implementation is private `src/fs.ts` plus an internal invocation seam shared with app.ts. Keep `ensureLink` and the sticky connection promise in app.ts; it may export a **package-private** `invokeFs(payload)` function to sibling modules, absent from public index. `invokeFs` awaits existing `app.whenReady`, obtains that same promise, and delegates to a private FS-call method on `LifecycleLink` using existing `FS_CHANNEL`, `WorkerLink.call` and `APP_LINK_IO_DEADLINE_MS`. It does not reset a dead promise or mint a successor. No new Worker options, receiver table, socket, retry queue or import-time readiness.

Both public methods are implemented as `async` methods and **always return a Promise**, including invalid local arguments or encoding failures. Perform argument validation and request encoding/copy synchronously inside the async body before its first await: those failures become Promise rejections, never synchronous throws from the exported method. `write` encodes into a fresh owned payload at that call-entry boundary. Subsequent same-thread changes to the caller's view/backing buffer cannot change admitted bytes; no coherent concurrent SharedArrayBuffer writer guarantee is made. `read` decodes into a fresh independent `Uint8Array` using copy/slice, never a view into ring/transport memory. No caller buffer is transferred/detached. No native content partials are exposed as successful reads.

Ready alone is not admission: app.ts retains `hostReady` after Ready, so its ready promise can resolve after local Quit. Immediately before `WorkerLink.call`, the private `LifecycleLink` FS method rejects if `#closed` is true **or a local `#quitPromise` exists**, including while Quit's reply is pending. Use the existing closed-branch mapping from [LifecycleLink.quit](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/packages/@keld/api/src/link.ts#L136): reject with the recorded `#dead` error object unchanged when present; otherwise reject with the existing `kipcError("KELD-IPC-001", "session is closed")`, whose exact message is `KELD-IPC-001: session is closed`. This local closed error remains the existing ordinary Error shape; do not invent a code field, substitute IPC022/023, wait for Quit to admit FS, or replace an already recorded dead cause. Current Quit sets `#closed` before calling the transport; preserve that ordering, its sticky/shared Quit promise, and close/onEnd behavior. Already-admitted calls keep their existing canonical terminal outcomes. No lifecycle or transport implementation change is authorized by this check.

Native/transport ERR remains the existing `KeldCallError{code,message}` without alteration. Generated local type/Unicode/codec failures use existing codec-error semantics (`Error` message begins `KELD-IPC-003`, as current Echo codec does); they do not masquerade as host guard errors or invent codes. A structurally valid REPLY with malformed postcard, unknown enum, wrong response variant or trailing bytes rejects that call once with local codec failure. It does not fabricate a return value, retry, or change canonical link retirement. Header/session/correlation/ERR grammar failures remain WorkerLink-owned and link-terminal where its contract requires.

### 4.3 Transfer range resolved from actual transport

The adapter uses **async `WorkerLink.call`**, so the default blocking-reply slot is not its read-result limit. Outbound encoded payload must fit canonical `MAX_FRAME_LEN`; native `MAX_FS_PATH_BYTES`/`MAX_FS_CONTENT_BYTES` and other native bounds remain enforced by the native owner. Do not locally short-circuit admissible oversized-native requests into invented errors: a native-limit negative can reach native validation when its encoded frame still fits the canonical frame cap.

An async reply consumes `HEADER_LEN + encoded FsResponse bytes` in the existing `DEFAULT_RING_BYTES` ring and one existing record slot. For `Read`, encoded payload length is `enum-varint length + vector-length-varint length + content length`; the generated codec owns this calculation. The instantaneous supported delivery is the available canonical ring capacity, **not the whole native content ceiling**. An empty default ring can accept a Read exactly when that envelope fits; other retained frames reduce free capacity. Canonical excess closes with existing `KELD-IPC-026`, not native-limit/cancellation/no-effect error. No extra API cap or altered queue size is added. Tests derive boundary sizes from the existing constants/encoded length and include just-fit, one-byte-over and occupied-ring cases. This explicitly preserves bounded flow; bulk/streamed FS transfer is future work.

### 4.4 Bounded generated FS binding

Extend existing `packages/@keld/kipc/scripts/echo-codegen.ts` generate/check; retain its Echo/channel-table/stamp outputs. Add one target reading `crates/keld-native/src/fs.rs`, producing `packages/@keld/api/src/fs.generated.ts` (marked generated). Do not add a second generator owner or change channel-table/wire constants.

This target admits only the current rustfmt-normalized `FsRequest`/`FsResponse` enum forms: their named `Read`/`Write` variants, unit variant, named fields of `String`/`Vec<u8>`, derive/doc lines. It preserves declaration and field order and rejects explicit discriminants, serde layout/rename/skip attributes, tuple/private/generic/unknown fields/types, or unsupported syntax; no guessed layout. This grammar expansion is an explicit public payload-binding review item under KEL-98's future-extension rule.

Generated internal contract:

```ts
type FsRequest =
  | { variant: "Read"; path: string }
  | { variant: "Write"; path: string; bytes: Uint8Array };
type FsResponse =
  | { variant: "Read"; bytes: Uint8Array }
  | { variant: "Write" };
function encodeFsRequest(value: FsRequest): Uint8Array;
function decodeFsResponse(payload: Uint8Array): FsResponse;
```

Exports are internal to the generated module, not package public exports. Reuse canonical varint/string helpers and shared Unicode validation. The generator derives discriminants/field order from authoritative Rust; byte-vector length/content handling is generated once. Decoder checks lengths before copy, exact end position and recognized variants. Public read/write selects the expected variant.

Add a **semantic** FS byte fixture under existing native test ownership (e.g. `crates/keld-native/tests/fixtures/fs-payload-v0.tsv`), read by native `fs_session` codec tests and TS binding tests. Existing receiver corpus `fs-call-valid=deadbeef` proves opaque receiver admission and is not this semantic oracle. Initial statically derived candidates, to be verified by the live Rust codec before any pass claim: Read request `/p` → `00 02 2f 70`; Write `/p`, bytes00/ff/41 → `01 02 2f 70 03 00 ff 41`; Read response → `00 03 00 ff 41`; Write response → `01`. Include empty vector, multi-byte length and Unicode cases. Independently verify Rust encode equals fixture and strict decode equals semantic value; TS encode/decode equals the **same fixture**, not a second algorithm. Mutating Rust variant order or TS generated field order must break the independent oracle. Fixtures are wire facts, not a new enum/format on the app-link.

### 4.5 Exact macOS policy-source amendment / reuse

Replace KEL-96 AC3.6 and §4.3's unconditional fixed-policy producer requirement **only for the macOS KEL-140 development path**:

> `boot.rs` selects the fixed root leaf `keld.permissions.jsonc` from the already canonical project. A no-follow retained root and leaf are validated as directory/regular file owned by the invoking principal. A genuinely absent leaf produces the historical exact `{}\n` and fixed digest. A present leaf is captured once with the guard-owned bounded read helper; those owned bytes are written as the fixed stage sibling and hashed into the existing descriptor. No source filename/path selector, permission merge, variable expansion, scope rewrite or auto-grant is added. Unsafe/present failures are never converted to fallback. Host retained-file selection and one read/hash/parse, explicit valid all-denied `{}`, missing/invalid staged-policy failure, immutable session policy and development-only trust classification remain unchanged.

Update matching KEL-102 dev-fixture wording to allow explicit macOS project policy while retaining absent-project fallback and requiring an explicit staged file. Historical predecessor artifacts and other-platform fixed bytes are unchanged.

Resolve the static helper gaps as follows:

- `contained_source` and `stage_project_file` currently canonicalize/check then reopen a pathname; they are **not reused for policy capture**, because they cannot prove the new selected-object/one-buffer contract. Do not copy core's general boot traversal helper or expose opaque host boot selection.
- In sole producer boot.rs, use existing workspace-pinned `rustix::fs` on macOS: open the canonical root as a directory/no-follow/close-on-exec handle, check metadata owner, then open only fixed leaf relative to it with read-only/no-follow/close-on-exec/**nonblocking open**, checking retained metadata for regular file and invoking owner before read. Nonblocking acquisition prevents a swapped FIFO/device from hanging before type rejection. Only leaf-open `ENOENT` yields fallback; symlink/no-follow and other errors fail. No path canonicalize/reopen of the policy. Keep root/leaf handles alive through capture and close via RAII.
- Reuse the CLI's existing current-principal ownership rule, factoring a private Unix metadata owner check so both path discovery and retained-file validation use one owner predicate. Preserve existing `ProjectOwnershipError`/`KELD-CLI-049`; source type/open failures use existing `BootCompileError::Staging`/`KELD-CLI-047` with fix. Do not create a new owner/ACL evaluator. Windows/Linux remain unchanged; their future handle checks require their own platform acceptance.
- **Explicit tiny guard public-API addition**, to avoid duplicating the manifest ceiling/read rule: document and expose existing `keld_guard::read_manifest_bytes<R: std::io::Read>(reader: R, path: &std::path::Path) -> Result<Vec<u8>, ManifestError>` (currently `pub(crate)`). `path` is diagnostics-only, never opened; helper owns existing bounded byte-read behavior and neither parses, authorizes nor validates source identity. CLI passes the already checked handle. No duplicate numeric limit in CLI. Read/TooLarge failures map through existing staging failure with original guard detail/fix retained; no new CLI error variant/code is necessary. This exact helper signature is included in mandatory public API review.
- `write_new_file` writes the same capture buffer with current mode/nonce cleanup. Existing `Sha256::digest` hashes that buffer; no later source read/digest. Do not add a CLI parser. Existing `load_verified_manifest` is the only runtime policy loading boundary and reuses the guard's single JSONC parser; `FsBroker::prepare` remains scope-serviceability owner. Thus malformed capture may produce a stage but cannot create app resources. Guard/Broker failures retain their existing registered errors/fixes.

`rustix` fs features and guard dependency are already pinned/enabled by the workspace/CLI; no dependency addition or new unsafe is proposed. The helper visibility and Unix owner factoring are reviewable changes, not existing approvals or tests.

### 4.6 Public package / ordinary staging composition

Static fact: boot.rs copies configured entry, renderer and optional `src/kipc-transport.ts`; it does **not** copy node_modules/package dependencies or compile/bundle entry. api package publicly exports only `.`; its sources import canonical transport by sibling relative path. A raw unbundled package-import app is therefore not implicitly deployable by this producer.

Reuse KEL-142's already implemented `bundle_api_entry`/`KIPC_SIDECAR_BUILD` composition: a cold app preparation uses **public `api/src/index.ts` only**, bundles app/API/generated bindings into configured `src/main.ts`, resolves every canonical transport import to the one external `./kipc-transport.ts`, and supplies the exact canonical file beside that entry. At runtime the bundle contains no repository source import and no inlined `class WorkerLink`; its Worker loads the independently stamped transport file. Stage those normal project inputs unchanged. No imports from private api/link.ts or raw `KELD_APP_LINK` parsing in product app.

Generalize the existing test composition function minimally to accept the prepared public app source and reuse its existing build script; do not copy the script into another harness. Provide a documented prepared sample project under the existing examples convention or an equivalent generated test project with its captured artifacts; no new CLI build verb/package install path is claimed. Package publishing and KEL-141 immutable distribution remain separate owners. “Normal app” in this slice means **a project containing its prepared entry and canonical sidecar**, using stock policy staging and public API composition; it does not mean the absent npm-distribution/build system exists. A future child requiring bare-package resolution must scope that contract separately.

### 4.7 Bounded sample application result

Reuse declared Echo `AppChannel` unchanged. Sample request's ordinary Echo `message` chooses a fixed application action; `count` is an application witness copied back. App-selected allow/outside test paths are ordinary request resources, not host-retained roots or authority fields. Renderer payload supplies no authority. Unknown action/authority-shaped request fails the sample handler, preserving existing API001 behavior.

Sample TS application-data type, serialized into existing `EchoResponse.message`:

```ts
type FsSampleOutcome =
  | { ok: true; bytes: number[] }
  | { ok: false; code: string; message: string };
```

Success carries small exact byte values; denial catches actual `isCallError` and copies original `.code`/`.message` verbatim, preserving fix inside message. Renderer decodes existing Echo response then this application's JSON result; no new framework ERR shape or enum. Prepare short unique paths/small content so the whole sample request and returned outcome are bounded by the existing renderer control budget; assert encoded sizes, never truncate error/fix or silently spill to a new route. Hostile native oversize/deadline cases are selected by small app controls and created inside the Bun handler/owning test seam; no large renderer request is needed. Deterministic app-data checks do not replace existing bridge fuzz coverage; a newly added untrusted Rust parser would require its owned fuzz extension.

### 4.8 Migration / compatibility / platform notes

Existing public exports and Echo-only channel registration remain. Electron lifecycle facade is permanent and still delegates. Default hello continues its current composed entry/transport/legacy Echo path; do not force a scaffold rewrite to prove public FS. Absent policy makes old projects behave identically. macOS policy selection is an explicit documented platform gap elsewhere, rather than silently changing Windows/Linux staging without proof. No temporary second adapter/transport/parser is retained. No persisted wire state migrates. No release or containment status changes.

## 5. Boundaries

After exact approval/claim only: api index/app/link plus private fs/generated output/tests; existing kipc cold generator/tests (not transport implementation); native semantic codec fixtures/tests (not broker); guard existing bounded read helper visibility/docs/tests; CLI sole boot producer/owner predicate and tests; existing macOS renderer product test composition/fixture and one FS test module, macOS discovery registration if needed; sample/docs and exact governing producer amendments/generated docs.

Forbidden without a separately recorded required contract/gate: native broker traversal/effect changes, core FS admission/quiescence or public boot APIs, supervisor/role grants/principal minting, renderer channel admission, canonical transport options/read/write/deadline implementation, manifest fields/capability IDs, CallError fields/resolveEchoCall forwarding, CI/Cargo/dependency/unsafe expansion, build/install/distribution architecture, platform producer widening. Root is tracker/claims/integration owner; native execution belongs to its qualified Mac owner.

## 6. Tasks

The approved atomic proof/task graph is recorded below; ROOT owns the aggregate issue claim and gives each delegated writer an exclusive file partition.

- [ ] T0: refresh predecessors/pin/claims, exact contract review and human approval with head/blob/digest metadata. Exact approval is recorded above; §10 has no remaining product choice. The source-open owner records the workflow's current-documentation receipt for the selected OS/API semantics before implementation and binds its real retained-object/FIFO tests; local source feasibility is not an exercised OS/API receipt.
- [ ] T1: bounded generated FS binding + semantic Rust/TS vector conformance, including malformed/mutation controls; no API link change.
- [ ] T2a: public/private adapter over same singleton, buffer/variant/deadline/error/overflow contracts; consumes T1.
- [ ] T2b: guard bounded-read helper exposure and sole macOS boot source capture/default compatibility; independent of T1/T2a.
- [ ] T3: shared existing app composition + sample application outcome + normal project producer/public route proof; consumes T2a/T2b.
- [ ] T4: real-macOS allow/deny/sentinel and hostile prerequisite/retirement replay; exact gates/current-head independent reviews, predecessor artifact refresh and passed landed artifact publication. Root coordinates integration and one builder/native operator; no task label substitutes for acceptance.

### Atomic model before implementation

| Atom / owner | Boundary, inputs → outputs | Failure mode / independent first falsifier | Present evidence / status |
|---|---|---|---|
| A0 Entry / root | Exact KEL-139/KEL-142/KEL-102T3/KEL-130T1 artifacts + exact approved child → eligible scope | Replace exact T3 with parent Done or T2 artifact; entry check must reject | Saved live snapshot identifies exact passed/landed T3; fresh live/artifact/claim reconciliation pending. Exact child approval recorded above; fresh claim/artifact reconciliation remains required. |
| A1 Native payload binding / generator worker | Native FsRequest/FsResponse source + owning Rust codec → generated TS byte binding | Swap variant/field order only in Rust or generated TS; shared semantic-vector assertion must fail | Native enum/codec and limited existing generator read directly. FS binding absent. Candidate byte predictions unrun. |
| A2 One application transport / API worker | Existing app singleton/LifecycleLink + closed/dead/pending-Quit state → admitted async FS correlated result or existing rejection | Attempt second WorkerLink; one-open assertion/real-link operation must fail. Hold Quit reply after Ready and invoke FS; zero outgoing FS plus exact `KELD-IPC-001: session is closed` must hold. Remove private closed/Quit check and that test fails. External death preserves recorded dead error unchanged | Existing singleton, sticky Ready, LifecycleLink closed/dead/Quit mapping and WorkerLink.call code available; new internal seam absent. |
| A3 Value/error ownership / API worker | Caller arguments/buffer + terminal host bytes/errors → always-Promise exact independent read bytes/void or unchanged failure | Invalid local encoding must return a rejected Promise without synchronous throw or outgoing call. Mutate buffer immediately after invocation; admitted bytes stay original. Wrong variant/trailing bytes reject once. Alter code/message; denial oracle fails | Canonical ring copy/ERR conversion available; proposed async method body performs validation/copy before first await. Adapter and ownership tests absent. |
| A4 Transport capacity / API worker | Existing frame/ring constants + encoded payload/current occupancy → normal delivery or canonical overflow retirement | Boundary+one frame exceeds free bytes and must terminate IPC026; raising ring size or using blocking slot to mask it fails contract/source check | Async append records HEADER_LEN+payload, defaults from transport inspected. Boundary/occupied-ring tests unrun. |
| A5 Source identity / producer worker | Canonical project + fixed no-follow retained root/leaf/current owner → selected input handle or genuine-absent fallback | Replace leaf after open and swap symlink/FIFO/foreign input; wrong object/no-follow bypass must fail before read/resources | Current path-reopen helpers insufficient. Existing pinned rustix/CLI ownership rule identified; proposed capture unimplemented. |
| A6 Snapshot bytes / producer worker | Guard-owned bounded read of retained source → one buffer → stage bytes + digest | Hash a different buffer or reopen replaced pathname; stage-byte/digest oracle must fail. Existing guard ceiling+one rejects without unbounded read | Guard read helper exists privately; exact public exposure proposed. No new manifest limit needed. |
| A7 Parser/authorization / existing host/guard/native owners | Fixed staged handle/digest → verified manifest + prepared broker → sole guard decision/effect | Malformed/digest-mismatched/invalid-scope policy starts no resources; removing sole guard yields denied outside effect and sentinel failure | Landed owner route available. No new implementation owner; product consumer proof pending. |
| A8 Public app composition / product worker | App imports public index; existing cold bundle composition + exact transport sidecar → ordinary configured stage | Runtime repo/private-import/inline WorkerLink or missing sidecar causes source/composition check/launch failure | KEL142 existing public bundle/external transport seam inspected. General staging never installs dependencies. New FS composition unrun. |
| A9 Application outcome / product worker | Existing bounded Echo request → handler public FS → existing Echo response with sample typed app data | Throw guard error instead of catch/data encoding; existing resolveEchoCall maps to API001, so exact guard-code/fix comparison fails | Existing error rule inspected. Bounded sample types/encoding proposed, unimplemented. |
| A10 Real allow/deny / qualified Mac owner | Stock CLI stage + explicit narrow policy + OS pointer → exact bytes + outside sentinel unchanged | Change policy to {} or remove guard; independent file/code/sentinel checks must fail | No public product/native run in this draft. |
| A11 Retirement/quiescence / qualified Mac owner consuming existing owners | Retired/post-quiesce requests + pending calls/native drain → no old handler effect and one caller outcome | Admit retired generation, replay, publish successor before drain, or omit terminal caller rejection; exact current regression fails | Existing owner contracts consumed, not reimplemented. Public replay/native run pending. |
| A12 Compatibility/qualification / root | Absent/explicit {} + old default hello + exact final diff → historical behavior and complete evidence | Changed fallback bytes/digest or default hello no longer Ready/Echo/Quit; compatibility assertion fails | Historical bytes/digest and scaffold/staging source inspected; checks unrun. |

Independence edges are explicit: A1 binding cannot establish A7 authorization; A2 link success cannot establish A5 source identity; A6 digest consistency cannot establish release authenticity or A7 validity; A8 bundle reachability cannot establish A10 real effect; A9 error data cannot establish outside no-effect; A11 local terminal rejection cannot establish native syscall return/rollback. A4 overflow may retire A2/A3, but its code/ownership outcome remains canonical rather than being averaged into native semantics. All unknowns remain unknown until their own oracle passes.

### Execution DAG after exact approval

```text
T0 exact approval + predecessor/claim refresh
  ├─ T1 generated FS binding / native+TS semantic vectors
  │    └─ T2a singleton public adapter + value/error/capacity tests
  └─ T2b sole producer + guard bounded-read API + capture/default tests
       T2a + T2b → T3 public app/sample + existing composition integration
                    → T4 real Mac allow/deny + hostile/retirement replay
                    → T5 exact-final-diff gates/reviews + landed artifact
```

The text graph describes dependencies only; it is not an implementation artifact or claim. T1 and T2b are disjoint, safe parallel development partitions. T2a begins after T1's generated artifact/interface lands or is integrated by root from the sole writer; it can overlap remaining T2b work. T3 source preparation can proceed on its separate fixture while T1/T2 work after interfaces freeze, but product pass waits for both. Root independently schedules native runs and the one builder; no shared native/GUI race or unsupported parallel compile is implied.

#### Writer partitions

| Partition | Sole writable paths | Consumes / must not edit |
|---|---|---|
| T1 binding worker | `packages/@keld/kipc/scripts/echo-codegen.ts`, focused generator tests; generated `packages/@keld/api/src/fs.generated.ts`; native semantic vector fixture and focused codec assertions in `crates/keld-native/tests/fs_session.rs` | Existing channel table/transport/native enums are read-only. Do not change broker, regenerate transport from stale inputs, or write public api/app/link. Root verifies generation/check covers all current outputs. |
| T2a API worker | `packages/@keld/api/src/{index,app,link,fs}.ts`, focused sibling API tests, including always-Promise/call-entry snapshot and pending-Quit/no-outgoing-FS controls | T1 generated output read-only; canonical transport code/options/queues and Echo error rule unchanged. Reuse existing closed/dead mapping; no lifecycle-state rewrite, private package export or second open. |
| T2b producer worker | `crates/keld-cli/src/boot.rs` and its tests; exact shared bounded-read helper visibility/docs/tests in `crates/keld-guard/src/lib.rs` | Root explicitly designates this cross-crate concern under one winning claim. No parser/guard matcher/core boot edit or Windows/Linux policy-source change; existing dependent files read-only. Shared Cargo/CI untouched. |
| T3 product worker | Existing `crates/keld-host/tests/no_flag/macos/renderer_bridge.rs` bundle composition owner, dedicated FS app/page fixture and FS product test module; bounded prepared-project sample documentation if required | Public api/index and sole producer are consumers only. No copying KIPC_SIDECAR_BUILD/private transport harness, no after-stage policy mutation, no native broker/hooks. Root owns `no_flag_macos.rs` discovery registration if needed. |
| T0/T5 root integrator | Exact proposed KEL140 child, KEL96/KEL102 amendment text, generated docs, tracking/artifact records and any shared test discovery key | Approval metadata/status changes only after actual approval. Root reconciles claims/new main changes and reviews exact integrated diff. Subtasks do not acquire tracker/merge/native authority from this graph. |
| T4 qualified Mac executor | Evidence output under owned managed receipt paths; test fixture corrections only within its approved claim/owner | Consumes exact integrated source SHA. No simulator/mock/JS element.click substitute; no uncontrolled process signals, authority changes or concurrent GUI ownership. |

Review and builder tasks use exact integrated heads, not uncommitted sibling work as acceptance. Root refreshes file overlaps against unrelated claims, including KEL130 fixture fixes; `fs_session.rs` differs from retained_fs setup but semantic overlap still requires check. One claim per issue/tree/writer concern; proposed partitions are not permission to compete for KEL140's same root issue.

### Completion artifact checklist

Each task reports exact source SHA, owned diff, first failing baseline/negative control, actual tests, unverified cells, and predecessor outputs. Root retains all AC rows individually. Real Mac rows require source/artifact digests, Mac/Bun/WebKit facts, one public link/handler/correlation attribution, exact allow/read byte oracle, native outside sentinel, original registered failure/fix, process/window/document identities, ordered Quit/cleanup and healthy next launch. Hostile/retirement or generator/source-capture gaps keep their own row incomplete.

Format, warning-denied workspace Clippy, full workspace tests, strict TS/Bun, generated freshness/docs and exact-final-diff public/permission review remain mandatory; conditional wire/dependency/unsafe gates apply when introduced. Preparing this graph established no production, build or native pass; approval is recorded above.


## 7. Test plan

AC1/2: strict public-index compile, generator freshness and source ownership assertions, same semantic native/TS vectors, unsupported Rust schema forms, Unicode/type/trailing/variant tests, schema/order negative mutations. A real canonical WorkerLink fixture separately proves one connection, lifecycle+FS reuse and host CallError preservation; no mock proves this boundary.

AC1/7 adapter controls additionally verify that invalid argument/encoding calls return rejected Promises without synchronous throw or link dispatch, and mutation immediately after `write` cannot alter its call-entry snapshot. Hold a real Quit reply pending after Ready, invoke read/write, and independently observe **zero outgoing FS CALLs** plus the exact existing local closed message. Repeat after `close` and after an externally dead link, preserving the original recorded dead object/code/message. A temporary removal of the private closed/Quit admission check must make the pending-Quit/no-FS test fail; a fulfilled Ready promise cannot substitute for that predicate. These tests exercise the public methods and real canonical link, not a second socket or altered lifecycle.

AC3/4: real retained temp root/leaf fixture; absent/dangling-symlink/directory/FIFO/foreign/unreadable/guard-ceiling+one, path-replacement-after-open, captured-buffer-vs-replaced-file, digest/stage bytes and malformed/empty/invalid-scope stage through real no-flag pre-resource fail. Reuse existing injected owner-check regression seam only for CI state; qualified OS foreign-owner/reparse claims need real OS identity/object evidence. Same-user mutation is not a release adversary. Temporary remove-no-follow/remove-owner/hash-different-buffer/oversize-read mutations must each fail their named test. Default hello and non-macOS compatibility remain required.

AC5/6: stock CLI project launch/prepared public index app, actual host Ready, one OS pointer/native invoke observation, owning Rust FS traces, exact file bytes, original registered denial+fix and independently unchanged outside sentinel. A post-stage policy substitution or private-link app fails the source ownership/provenance check. Logs are diagnostics, not effect or lifecycle proof.

AC7: replay named KEL-130/KEL-102 hostile cases and current generation/quiesce seams through public API. Derive ring boundary from constants and encoded vector size, observe overflow code and healthy fresh generation after owner teardown. Outer expiry returns once; old native authority drains according to its owner before successor readiness. No sleep-sync, unbounded polling, deterministic-failure retries, or inferred effect cancellation. Hazardous crash/block/teardown runs are isolated children with independent identities/census and real gates; root coordinates those runs after implementation.

No build/test/OS pass is established by this specification. Static candidate byte predictions, helper feasibility and bundle reachability require the tests above before acceptance. Full format/clippy/workspace/TS/Bun/generated-doc/real-Mac evidence remains UNKNOWN.

## 8. Review gates triggered

- Public API: **yes**, exact `fs` export/semantics, generated native payload binding, guard helper visibility/signature and explicit KEL-96 producer input contract.
- Permission model/security: **yes**, selected source identity/bytes/fallback and immutable host policy, no authority widening/default allow. Independent reviewer binds exact final diff and removes-no-follow/owner/guard negative controls.
- Wire: **none proposed for frame/HELLO/FS/CallError**. Existing application Echo bytes carry sample JSON data; no new admitted framework shape. Any discovered payload/accepted-envelope/wire change stops for its own exact gate and applicable decoder/fuzz coverage.
- Dependency: **none proposed**; pinned existing rustix/guard/serde/sha2/Bun tooling reused. New package or feature widening requires named gate.
- Unsafe: **none proposed** in production. Existing real-OS census test allowances remain path-local; introducing new unsafe requires exact owner update/review.

Routine gates are actual format, warning-denied workspace Clippy, full workspace tests, strict TS/Bun, generated freshness/docs/LLMS where included, and conditional routing. Public/permission/wire review does not substitute for real-OS effect/ownership acceptance. No authority boundary exception is requested.

## 9. Perf impact

No performance claim. Cold bounded manifest capture and app preparation reuse current owners. Native ceiling, ring/frame/record bounds and transport deadlines remain unchanged. Track buffer copies without claiming improvement; new performance-motivated change requires its own attributed evidence.

## 10. Open questions

None. The human owner approved the recommended explicit macOS project-policy amendment and selected public semantics, including the existing shared read-helper exposure and bounded native binding extension. Fixture-only staging is not the selected completion path. Routine implementation and test placement remain agent-owned. A discovered inability to satisfy a named invariant must be reported with evidence; it does not authorize unreviewed authority, wire, queue or platform redesign.

### Exact source anchors and instruction basis

Primary source SHA above; [KEL-139 AC4](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/docs/specs/kel139-macos-product-spine.md#L135), [KEL-96 AC3.6/§4.3](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/docs/specs/kel96-no-flag-host-boot.md#L124), [KEL-102 policy load](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/docs/specs/kel102-host-guard-enforcement.md#L256), [KEL-142 API](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/docs/specs/kel142-macos-renderer-bridge-api.md#L139), [native enums](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/crates/keld-native/src/fs.rs#L56), [CallError](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/crates/keld-ipc/src/call_error.rs#L32), [generator boundary](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/docs/specs/kel98-echo-codegen.md#L126).

Local-code decisions: [CLI capture gaps/producer](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/crates/keld-cli/src/boot.rs#L252), [existing owner check](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/crates/keld-cli/src/boot.rs#L109), [guard bounded helper](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/crates/keld-guard/src/lib.rs#L655), [host loader](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/crates/keld-guard/src/verified_manifest.rs#L43), [KEL-142 bundle composition](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/crates/keld-host/tests/no_flag/macos/renderer_bridge.rs#L139), [async call](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/packages/@keld/kipc/src/transport.ts#L1996), [ring envelope](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/packages/@keld/kipc/src/transport.ts#L2752), [Echo error rule](https://github.com/gyldlab/keld/blob/0eb680dcd8b9f09654bc4841d0eb8b2965c46f2f/packages/@keld/api/src/echo-call.ts#L22).

Loaded root/router/workflow/spec-template and nearest cli/ipc/core/guard/native/host instructions; testing playbook was sliced for future test-oracle design. Bounded relevant learnings: trailing-byte postcard rejection, one HELLO per link, Unix timeout classification, unique hostile path names, code registry ownership, short socket fixture paths. These support proof shape, not new approval. Approval and claim records do not substitute for implementation/OS evidence.
