# Spec: bounded compatibility IPC for the first draw.io round-trip

Status: approved

Linear: GH-540 (#517 tracker); KEL-80 carries the reference link · Owner: Monish Khandelwal · Updated: 2026-10-23

Tracker and claim: [GH-540](https://github.com/gyldlab/keld/issues/540),
[publication claim](https://github.com/gyldlab/keld/issues/540#issuecomment-6370449449).
This document owns the F04 IPC contract and architecture 02 §2/§5 amendment.
It consumes existing protocol owners. It does not take their implementation claims.
The repository owner delegated the decisions and publication to this agent on
2026-10-23. That delegation is recorded separately from independent review of the
exact diff; it is not a claim of personal human review of a commit. Merge remains
prohibited by the task instruction. Approval here is the agent's bounded decision
under that explicit owner delegation; the publication receipt identifies the exact
reviewed commit. It does not approve another owner's unread artifact or waive CI.

## 1. Goal & non-goals

Complete the pinned draw.io preload/main `rendererReq → mainResp` exchange on macOS
through the existing host, native bridge, authenticated Bun link and guard. The first
witness uses two sequential, uniquely identified `isPluginsEnabled` requests, authentic
`file://` sender metadata and the original target document. The second request begins
after both the first application callback and an independent transport-release barrier.
The profile is a controlled small-message proof, not qualification of all startup,
burst, open/save or close-path traffic.

Non-goals: Electron invoke/handle, sendSync, transferred MessagePorts, subframes,
utility processes, unrestricted structured clone, new bulk/shared-memory lanes, a new
IDL, transport, supervisor or window registry. Handler-exception disclosure belongs to
F04-T14; an ordinary Error value in `mainResp` remains endpoint codec data. Windows and
Linux native acceptance remain explicit gaps. No runtime performance gain is claimed.

## 2. Spec refs

- [Architecture 02](../architecture/02-ipc.md) §§1–7: host mediation, authenticated
  link identity, canonical receive validation, WorkerLink, framing and lifecycle.
- [Architecture 03](../architecture/03-security.md): host-enforced default deny;
  #541/#552 own the exact window-channel grant amendment and its approval.
- [Architecture 04](../architecture/04-electron-compat.md): compatibility evidence
  and explicit subset/divergence reporting.
- [GH-508](gh508-kipc-channel-table.md) §4.5, approved in
  [#613](https://github.com/gyldlab/keld/pull/613#issuecomment-6043289015): **one**
  family-neutral `compat-control` entry, not the old two-ID #540 planning brief.
- [GH-527](gh527-worker-owned-blocking-call-transport.md): sole Worker socket owner,
  single outbound order, inbound ring and future GRANT version gate.
- [GH-532](gh532-first-proof-evidence-rules.md) and
  [GH-566](gh566-corpus-manifest-owner.md): existing conformance evidence owner.
- [GH-446 draft in #657](https://github.com/gyldlab/keld/pull/657), [GH-531](gh531-window-registry-close-state-machine.md)
  and #449: facade registry, shared lifecycle Subscribe/readiness and window ownership.
  Unmerged #657 changes to those contracts remain with its owner.
- #541/#542/#552: one navigation mint, native lifetime and guard; #544: one endpoint
  codec; #545: larger-payload measurement; #473/#484: real file URL and preload/world
  composition; #1095/#1096/#1105: existing native settlement and error provenance.

This PR changes architecture 02 §2's prohibition on all per-call string names into a
narrow compat topic exception, and replaces §5's proposed host-visible postcard SCV
with an opaque endpoint codec. It leaves architecture 02's primary Ready paragraph
and the #657 ownership boundary unchanged. The generic postcard control envelope is
still decoded by the host; **its application-value byte field is never decoded there**.

Source baseline: main `3503a8aa383427dcf50ccbf6653c4298e7fb8aad`. The channel table,
canonical receiver, generator and WorkerLink exist. The v1 native bridge supports
bind/invoke and a one-pending floor, not this v2 send/event contract. Main has neither
the ipcMain facade nor generic compat dispatch. The approved #1095 settlement contract
is not described as landed. Current native navigation is wrapping u64, while Guard's
Webview generation is u32; implementing the checked-u32 prerequisite is mandatory,
not an unchecked conversion in this spec's consumer.

## 3. Acceptance criteria (binary, each becomes a test)

1. **AC1, application witness:** pinned draw.io's unmodified main multiplexer and
   preload callback complete two sequential unique requests with byte/value-equal
   arguments/results, authentic file URL, exact reqId and original document. A fixed
   response or bypassed route cannot satisfy the witness.
2. **AC2, send:** `ipcRenderer.send` returns undefined, clones the full argument array
   before admission, sends no invoke call and waits for no application reply. Known
   local type/codec/capacity errors are synchronous; later refusal is internal.
3. **AC3, listeners:** on/once/off/addListener/removeListener/removeAllListeners obey
   pinned EventEmitter registration order, duplicates, original-function removal and
   once-before-callback reentrancy. Application callbacks run outside host locks.
4. **AC4, permission:** an absent literal window-topic grant produces zero forward and
   listener effects, a typed denial and a healthy allowed follow-up. Transport
   admission, subscriptions and principal authentication do not substitute for it.
5. **AC5, sender:** sender is the existing host-keyed WebContents facade. senderFrame
   exposes only host-stamped url/origin for that admitted navigation, becomes null
   after ordered invalidation, and is never reconstructed from renderer metadata.
6. **AC6, destination:** reply retains the original document. webContents.send captures
   the current main document at host admission. A chosen target that retires cannot be
   replaced by its successor. Neither method returns the internal delivery Promise.
7. **AC7, identity:** committed cross-document navigation invalidates the old route;
   cancelled renderer navigation and same-document changes preserve generation.
   Exhaustion fails closed. Rebind and role restart do not reset document sequences.
8. **AC8, order:** one document/direction preserves admission order across topics;
   reentrant sends append. No global or asynchronous-handler-completion order is promised.
9. **AC9, finite retention:** every framework-owned hop has checked byte and record
   reservations; max+1 refuses before new retention. Separate direction guarantees and
   reserved control capacity permit a handler's reply with incoming DATA full.
10. **AC10, completion:** consumer disposition plus independent framework release
    retires exactly the named reservation. A receipt alone cannot release still-active
    native work. Timeout is not cancellation; ambiguous work stays charged until quiescence.
11. **AC11, values:** identical endpoint source/vectors implement the declared supported
    domain. Host tracing shows only opaque bytes; no Electron value tags enter Rust.
12. **AC12, receiver:** wrong version/kind/flags/channel/correlation/direction/length
    fails canonical admission. New EVENTs require the shared lifecycle subscription.
13. **AC13, loss:** stale, duplicate, early, foreign-role or wrong-direction receipts
    cannot free another record; no-listener discards this dispatch; listener exceptions
    preserve app semantics while releasing framework ownership; no automatic replay.
14. **AC14, evidence:** documentary cells bind exact Electron v44.4.5 source and real
    mapped cases. Native observations have their own receipt. Missing APIs and selected
    divergences never become conformance passes because a local harness is green.
15. **AC15, retirement:** close old producers before drain; successor readiness requires
    old queued work, active native work and framework continuations all quiescent.
    Late controls cannot reset or disable a successor and retirement needs no spare queue slot.

## 4. Design

### 4.1 First principles, ownership and reuse

The host owns windows and routing authority; Bun owns application listeners; endpoints
own JS values. keld-wv alone owns the native bridge/private document nonce. Core composes
host-stamped facts and the existing guard. keld-ipc owns generic frames, table, control
DTOs and receive admission. @keld/kipc owns the role's sole WorkerLink. @keld/electron
owns endpoint values and facade semantics. Native handles stay on the existing UI
owner; I/O workers carry owned bytes and typed context. There is no process-boundary,
OS-grant or crash-domain change. A Worker death remains role link loss.

Existing framing, decoder, table/codegen, dispatch_privileged, window registry, Worker
ring and EventEmitter are reused. Their missing requirements are closed new operations,
bounded aggregate retention, authentic navigation context and compatibility listeners.
A parallel parser/socket, registry, policy evaluator, general schema language or
Electron value decoder in Rust has no justified ownership or performance requirement.

| Atom | Owner and observable boundary | Failure / independent falsifier | Status |
| --- | --- | --- | --- |
| Identity | Native/registry mint → admitted document and link context | Replay old document/role → zero successor effects | Existing owners; native implementation prerequisite |
| Authentication | Runtime accepted link and native controller/world/frame/nonce → trusted source | Foreign endpoint or spoofed JSON metadata must fail before guard | Reuse existing boundaries |
| Authorization | Existing guard + literal `el:<topic>` → allow/deny | Allowed neighboring topic must not grant this one | #541/#552 prerequisite |
| Carriage | Canonical receiver/codegen → exact typed operations | Wrong direction or oversized header cannot enter body allocation | Existing owner extension |
| Memory/liveness | Queue/native owners → terminal bounded retention | Pause each consumer; early receipt and max+1 must not free/grow | Implementation proof required |
| Values/facades | #544/#466/#467 → values, callbacks and void APIs | JSON substitute / once-after-callback must fail | Separate endpoint owners |
| Evidence | #464/#532/#566 → cited pass/fail/unknown cells | Absent API or uncited observed claim cannot pass | Existing harness |

Edges are explicit: authentication → identity/guard → routing; header bound → bounded
decode → operation classification → reservation; consumption and native release are
independent completion requirements. OS containment is unchanged and is not proved by
a grant test. Source/schema checks do not prove native execution or queue bounds.

### 4.2 Identities and authority

| Identity | Owner and lifetime | Constraint |
| --- | --- | --- |
| WebviewId | Existing host window registry; host lifetime | Nonzero checked u32, never reused; exhaustion refuses creation |
| NavigationGeneration | #541/#542 native document owner | One checked nonzero u32 mint; no cast from current wrapping u64 |
| Document nonce | Existing keld-wv document binding | 32 lowercase hex characters; stays private to native/isolated world |
| RoleGeneration | Runtime authenticated accepted connection | Opaque private u64 metadata, never peer-selected or serialized as JS number |
| Message / delivery sequence | Per document and direction | Nonzero checked u32; no reset on bind or role restart; exhaustion closes route until a real new document |
| KIPC correlation | Canonical link owner | One pending CALL's transport identity; never draw.io reqId |
| draw.io reqId | Application preload | Opaque application data; framework must not invent or interpret it |

Ledger key: `(webview, navigation, authenticated role generation, direction, sequence)`.
The renderer may supply only its private document nonce, topic, sequence and value
bytes. Source/target ids, role, generation, URL, origin and principal fields in renderer
JSON are forbidden. Native controller/world/main-frame/WebView/nonce checks precede
host metadata stamping. The admitted principal is the existing Webview principal,
never AppProcess merely because the next hop is Bun. Guard matches the exact literal
`el:<topic>` under the window's grants; no wildcard/prefix/case/Unicode normalization.

App-supplied destination handles remain selectors: validate ownership against the
authenticated role and existing registry. processId/frameId are opaque registry
projections, not OS-PID-derived authority. Numeric inequality with every OS PID is not
the contract and is not a valid security test. No separate facade counter is minted here.

### 4.3 Exact value domains

The following are validation rules for generic envelope fields, not #544 value-codec rules:

| Field | Exact admitted domain |
|---|---|
| `v` in the new private envelope | JSON integer `2`, selected by trusted native construction and injected script; no peer-negotiated downgrade |
| `document` | Exactly 32 lowercase ASCII hexadecimal characters, native-minted and kept inside keld-wv |
| `message`, `delivery`, existing `request` | Integer 1..=4,294,967,295; checked, non-reusing within the document; never bitwise-wrap |
| `webview` on app-link | u32, nonzero; only an existing admitted host registry identity is usable |
| `navigation` on app-link | u32, nonzero, **after** #541/#542's one checked-u32 mint is implemented. No cast from the current independent native u64 counter. |
| `channel` on private Send/Event, `topic` on app-link | A Unicode-scalar string whose UTF-8 encoding is at most 256 bytes; exact literal matching; empty string is structurally permitted and still requires an exact grant. No case folding, normalization, trimming, or lone-surrogate replacement. |
| Existing private Invoke `channel` | Nonzero u16, still checked against the table-derived admitted numeric id; never a topic string |
| `payload` | Private JSON: an array of integral bytes 0..255, length <=4,096. App-link: postcard Vec<u8> / generated Uint8Array, length <=4,096. Contents are opaque to Rust. |
| `url` / `origin` | Host-stamped Unicode-scalar strings, <=4,096 / <=1,024 UTF-8 bytes. The navigation owner provides their exact committed meaning. Opaque origin is the native owner's serialized value (e.g. `"null"`), never reconstructed from a renderer string. |
| Diagnostic `code` / `message` | Existing registered KELD code, ASCII <=64 bytes; sanitized message with code and fix <=512 UTF-8 bytes. No truncation. Existing error owner must produce a bounded valid rendering; an endpoint report cannot impersonate a host certainty proof. |
| Private DATA envelope | <=65,536 encoded UTF-8 bytes, checked before JSON decoding |
| Private CONTROL envelope | <=4,096 encoded UTF-8 bytes, enforced in addition to field bounds |
| Generic compat CALL/EVENT payload | <=16,384 encoded bytes, checked by the canonical selected receiver before body allocation |
| Generic compat REPLY/ERR/control-operation payload | <=4,096 encoded bytes; same canonical bound plus closed operation validation |

The 512-byte diagnostic limit makes the new private controls fit their 4,096-byte limit even under worst-case JSON escaping: 512 ASCII control characters expand to 3,072 bytes, code contributes at most 64 safe ASCII bytes, and the fixed fields are small. Full envelope length remains authoritative. A 4 KiB application payload consisting entirely of 255 encodes to fewer than 16.5 KiB of decimal-array JSON, below the existing 65,536-byte envelope ceiling.

The u32 choice consumes the #541/#542 contract and stays exact in JS `number`. If those owners instead retain a u64 navigation identity, amend this contract through its approval gate to Rust u64 / TS bigint and add an encoder through the canonical transport primitive; never use Number or `as u32`. Existing `decodeU64Varint` and generated Auth bigint are direct reuse examples. RoleGeneration does not need either encoding because it never leaves the authenticated connection metadata.

### 4.4 Closed generic app-link types

These are canonical Rust DTOs to implement within the existing keld-ipc payload owner. Derives, documentation, checked constructors, and bounded deserializers are implementation work. The declaration and field order below are normative postcard order. All enums are closed; there is no catch-all/Other/serde-other variant. Primitive integers in these DTOs are selectors to validate, not permission-bearing host types.

```rust
pub enum CompatControlRequest {
    DeliverToCurrentMain {
        webview: u32,
        topic: String,
        payload: Vec<u8>,
    },
    DeliverToOriginalDocument {
        webview: u32,
        navigation: u32,
        topic: String,
        payload: Vec<u8>,
    },
    ReleaseConsumed {
        webview: u32,
        navigation: u32,
        message: u32,
        disposition: ConsumerDisposition,
    },
}

pub enum CompatControlEvent {
    RendererMessage {
        webview: u32,
        navigation: u32,
        message: u32,
        url: String,
        origin: String,
        topic: String,
        payload: Vec<u8>,
    },
    DocumentRetired {
        webview: u32,
        navigation: u32,
    },
}

pub enum CompatControlResponse {
    Deliver { disposition: TerminalDisposition },
    Released,
}

pub enum ConsumerDisposition {
    Consumed,
    NoListener,
    Refused { error: CallError },
}

pub enum TerminalDisposition {
    Consumed,
    NoListener,
    Stale,
    Refused { error: CallError },
    RouteRetired { certainty: DispatchCertainty },
}

pub enum DispatchCertainty {
    NotDispatched,
    OutcomeUnknown,
}
```

`CallError` is the existing `keld_ipc::CallError { code, message }`, not a second error struct or codec. Rust never decodes the contents of `payload` into Electron values. No new role generation, frame identity, app listener identity, document nonce, application return value, or draw.io reqId is encoded here.

The exact generated TypeScript projection follows the existing generator's `variant` convention:

```ts
type ErrorFields = ReturnType<typeof decodeCallError>;

export type ConsumerDisposition =
  | { variant: "Consumed" }
  | { variant: "NoListener" }
  | { variant: "Refused"; error: ErrorFields };

export type DispatchCertainty =
  | { variant: "NotDispatched" }
  | { variant: "OutcomeUnknown" };

export type TerminalDisposition =
  | { variant: "Consumed" }
  | { variant: "NoListener" }
  | { variant: "Stale" }
  | { variant: "Refused"; error: ErrorFields }
  | { variant: "RouteRetired"; certainty: DispatchCertainty };

export type CompatControlRequest =
  | { variant: "DeliverToCurrentMain"; webview: number; topic: string; payload: Uint8Array }
  | { variant: "DeliverToOriginalDocument"; webview: number; navigation: number;
      topic: string; payload: Uint8Array }
  | { variant: "ReleaseConsumed"; webview: number; navigation: number; message: number;
      disposition: ConsumerDisposition };

export type CompatControlEvent =
  | { variant: "RendererMessage"; webview: number; navigation: number; message: number;
      url: string; origin: string; topic: string; payload: Uint8Array }
  | { variant: "DocumentRetired"; webview: number; navigation: number };

export type CompatControlResponse =
  | { variant: "Deliver"; disposition: TerminalDisposition }
  | { variant: "Released" };
```

The existing `echo-codegen.ts` already has closed per-target Rust enum grammars, `variant` unions, Vec<u8> ↔ Uint8Array, u64 ↔ bigint, and exact trailing-byte checks. It does not currently admit these new target/type names. Add one bounded compat target to that owner and admit only the listed u32 and exact named nested-enum/CallError references. Do not add a generic schema language, independently authored TS payload mirror, Rust Electron-value codec, or handwritten app-side control codec. Reuse existing postcard primitive/error helpers. The independent vectors below remain the drift oracle, not encoder-then-decoder round trips alone.

#### Semantics and source provenance

- `Consumed` means the consumer completed synchronous dispatch and released its required framework ownership. It does not mean every listener succeeded or an async listener Promise resolved. Resource release happens in finally even when an application listener throws; that exception retains application exception semantics.
- `NoListener` means the endpoint found no listener during this dispatch and discarded the message. This is a selected Keld transport disposition, not a claim of pinned native Electron observation. No future-listener queue is created.
- `Refused` from the consumer means the endpoint could not decode/admit the supported value envelope. Its bounded error is an endpoint diagnostic. The host may forward it as such but cannot infer host NotDispatched from its code. #544 owns concrete codec error codes and value tags.
- A consumer cannot supply `Stale`, `RouteRetired`, or `DispatchCertainty`. Those are host-owned proofs based on admitted target lifetime, attempted write/delivery facts, and owner-observed quiescence. Retirement is not application cancellation.
- `ReleaseConsumed` is a control operation for the renderer→Bun ledger, keyed by authenticated connection context plus webview/navigation/message. It carries no quantity of credit. The host looks up its recorded charges; duplicate/stale/unknown receipts create no capacity and no unbounded completion history. `Released` acknowledges bounded handling of the release request and does not assert that a nonmatching request freed anything. No acknowledgement of this REPLY is sent.
- Deliver CALLs use canonical first-terminal-wins correlation. On a live admitted link, normal Deliver REPLY waits for both the required consumer release and native completion; a matching consumer report alone cannot release an active native evaluation. Canonical retirement may answer an ERR-capable call with a best-effort ERR before Native Drained; close, EOF or the existing timeout may reject a pending call without an on-wire terminal reply. These logical outcomes do not release physical ledger/native charges, establish NotDispatched, or permit replay. Keep those charges until owner-observed quiescence, send no second terminal answer, and keep the reader able to process release control. This preserves GH-527 retirement rather than introducing a compat-only close protocol.
- Deterministic pre-admission failure is the existing `CallError` ERR via `write_call_error`; no special per-channel ERR envelope or `CompatControlResponse::Error` is introduced. A post-admission endpoint `Refused` can be represented by terminal Deliver REPLY because the requested delivery has a terminal disposition, not a privileged application return.
- Keep original-document routing in the closure's original link context. The host validates membership and lifetime under that context; it never trusts numeric coincidence as identity or resolves a stale target to the current main document. The facade process/frame metadata is projected through the existing registry owner, not independently minted in this payload.

### 4.5 Closed private v2 JSON envelopes

These TS definitions describe exact wire objects; they are not a second deployed parser. keld-wv's existing `decode_wv_link` remains the one Rust parser and native endpoint owner. Unknown and duplicate object keys, invalid Unicode, wrong JSON types, null-instead-of-absent, and fields belonging to another kind fail before app effects. Object member order is not significant; no canonical-JSON codec is introduced.

```ts
type JsonBytes = readonly number[]; // Each element integral 0..255; bounded above.
type JsonConsumerDisposition =
  | { status: "consumed" }
  | { status: "no-listener" }
  | { status: "refused"; code: string; message: string };
type JsonTerminalDisposition =
  | JsonConsumerDisposition
  | { status: "stale" }
  | { status: "route-retired"; certainty: "not-dispatched" | "outcome-unknown" };

type RendererToNativeV2 =
  | { v: 2; kind: "bind" }
  | { v: 2; kind: "invoke"; document: string; request: number;
      channel: number; payload: JsonBytes }
  | { v: 2; kind: "send"; document: string; message: number;
      channel: string; payload: JsonBytes }
  | { v: 2; kind: "consumed"; document: string; delivery: number;
      disposition: JsonConsumerDisposition };

type NativeToRendererV2 =
  | { v: 2; kind: "bound"; document: string;
      limits: { payload_bytes: 4096; channel_utf8_bytes: 256;
        send_records: 1; event_records: 1 };
      ready: boolean }
  | { v: 2; kind: "invoke-result"; document: string; request: number;
      result: { status: "reply"; payload: JsonBytes }
            | { status: "error"; code: string; detail: string } }
  | { v: 2; kind: "send-result"; document: string; message: number;
      disposition: JsonTerminalDisposition }
  | { v: 2; kind: "event"; document: string; delivery: number;
      channel: string; payload: JsonBytes }
  | { v: 2; kind: "route-state"; document: string; state: "ready" | "retired" };
```

The Rust decoded representation is a closed `WvLinkEnvelope` extension with corresponding Bind, Invoke, Send and Consumed variants; send/consume sequence fields become `NonZeroU32`, Invoke's channel stays `NonZeroU16`, payload uses the existing bounded byte visitor, and disposition is a closed enum. Native output uses the corresponding closed structs/enums and owning serializer. It does not expose the nonce to Core. Rust wire DTO and TS projection must be kept under the current bridge script/decoder owner, not a second handwritten general parser.

The v2 Invoke form is a versioned adapter for existing byte-level `window.keld.invoke`, not Electron invoke/handle. It preserves the existing one-pending Invoke floor and shares the native admission budget with other bridge work; it must not mint a separate uncounted native allowance. Its numeric channel and Reply/Error payload semantics remain KEL-142/#1095-owned. The `detail` name deliberately follows the existing native bridge result, while generic app-link `CallError` retains `message`. Do not silently rename v1 results or move #1095 settlement/error provenance into a new owner. Consume the landed #1095 exact result changes before implementing the adapter.

Private `invoke`, `send`, `event`, and successful `invoke-result` payloads are DATA under the shared directional/native budget. `bind`, `bound`, `consumed`, `send-result`, `route-state`, and error `invoke-result` are CONTROL under the 4,096-byte encoded-envelope cap, including the existing bounded `detail` rendering. A successful invoke-result is the completion representation of its already-reserved Invoke operation, not a second outstanding DATA admission or an uncounted exception; reserve any simultaneous request/result copies in that operation's retained/scratch budget. A 4,096-byte numeric reply can expand beyond 4,096 JSON bytes and therefore must not be forced through the CONTROL ceiling. The adapter consumes the existing Invoke settlement and native-completion owner; it adds no second pending floor.

`bound.limits` advertises fixed ceilings, not fresh spendable credit. Rebind does not reset a pending record, sequences, or already-spent capacity. `bound.ready` and route-state are notifications under the native owner's ordered FIFO/quiescence contract. Neither alone settles a request or proves old work never ran. Native retirement removes authority immediately; its later notification is not a prerequisite for that authority change.

Every v2 output repeats `v:2` and `kind`. Both `consumed` and `ReleaseConsumed` carry an exact consumer disposition. Source/target IDs, generation, role, URL, origin and principal remain forbidden renderer input fields.

#### Version selection

Use one native bridge owner and parser with an explicit trusted endpoint/document protocol mode. A LegacyV1 mode admits exactly the existing v1 schema; CompatV2 admits exactly these v2 forms, including the numeric Invoke adapter. A mismatched `v` refuses, never switches the selected mode. The injected handler/functions and native selection must agree. Keep the old public invoke contract through that existing adapter until bundled callers migrate; v1 acceptance in a deliberately LegacyV1 endpoint is not silent downgrade. Do not expose two competing active parsers for one document.

This changes the private `keld.wv-link` version namespace. It does not itself change KIPC `PROTOCOL_VERSION` or `HELLO`.

### 4.6 Exact receive matrix and caps

Let `C` be the eventually allocated `compat-control` ChannelEntry id. No numeric id is allocated or reserved here. It is appended at then-current maximum+1 by the table owner and generated into TS; native construction gets its trusted numeric id from that entry.

| Receiver / trusted state | Admitted kind / flags / channel / correlation | Body and bound |
|---|---|---|
| Host, authenticated admitted app role | CALL / 0 / C / nonzero unused correlation | Exact CompatControlRequest, <=16,384 bytes; after bounded decode classify Deliver as DATA or ReleaseConsumed as CONTROL; ReleaseConsumed encoded body <=4,096 |
| Bun, authenticated link and subscribed event receiver | EVENT / 0 / C / 0 | Exact CompatControlEvent, <=16,384 bytes; RendererMessage is DATA, DocumentRetired is CONTROL and has encoded length <=11 bytes |
| Bun, matching pending Deliver CALL | REPLY / 0 / C / exact outstanding correlation | Exact CompatControlResponse::Deliver only, <=4,096 |
| Bun, matching pending ReleaseConsumed CALL | REPLY / 0 / C / exact outstanding correlation | Exact CompatControlResponse::Released only, <=4,096 |
| Bun, matching pending CALL of either operation | ERR / 0 / C / exact outstanding correlation | Existing CallError only, <=4,096 |

All other new-channel directions/kinds fail the existing semantic owner: host does not receive compat EVENT/REPLY/ERR; Bun does not receive compat CALL. Wrong reply variant for the outstanding operation is a payload/protocol failure and never settles another operation. Legacy Echo/lifecycle/PING behavior remains under its original policy and is not silently removed; PING is not a compat payload operation.

The primary receiver must gain a trusted compat-CALL selection, not `privileged_call_receiver` widened to all HostInternal entries. Bun's event policy is still chosen from generated `HOST_EVENT_CHANNELS`. Add the <=16,384 / <=4,096 bounds to the existing receive-policy owner and its TS mirror (for example a bounded codec payload mode), before allocating bodies. Existing raw `PayloadMode::Codec` is insufficient because its only ceiling is `MAX_FRAME_LEN = 16 MiB`.

One channel means header fields cannot distinguish Deliver DATA from ReleaseConsumed CONTROL. Do not claim they do. The canonical bounded body/operation decoder must classify the closed variant before dispatch, with its decode scratch independently bounded, then charge the correct existing reservation owner. Do not charge every compat CALL as DATA (release deadlock), or as CONTROL (quota bypass). No new receiver/IDL or parallel queue manager is needed.

The same bounded variant classification applies to incoming compat EVENTs now that DocumentRetired is CONTROL. A Worker configuration that treats every compat EVENT as DATA would strand invalidation at a full data window. Preserve the single FIFO and its bounded control share; do not add a second event channel or bypass its receiver policy.

Lifecycle Subscribe and the current role admission are mandatory before EVENT. Register JS dispatch before subscription. Do not add a compat-specific Subscribe operation or send an EVENT to advertise the channel to an unsubscribed peer. The `HostInternal` table description needs the approved narrow clarification that it admits this session control carriage while the renderer-originated topic still always passes the existing Webview principal guard. That is not an AppProcess permission grant.

### 4.7 Independently calculated serialization vectors

These literal vectors were calculated from declaration order, postcard enum ordinal tags, unsigned LEB128, length-prefixed UTF-8, and byte-vector lengths using a small independent Python arithmetic script, not the runtime generator or codec. This is specification evidence, not a runtime acceptance result. The opaque bytes `[0,1,255]` are deliberately not #544 value-codec bytes.

`ConsumerDisposition` tags: Consumed=0, NoListener=1, Refused=2. `TerminalDisposition`: Consumed=0, NoListener=1, Stale=2, Refused=3, RouteRetired=4. DispatchCertainty: NotDispatched=0, OutcomeUnknown=1. Request tags and response tags follow their declarations above. A nested CallError is exactly its existing concatenated code/message fields, without an added tag or vector length.

| Value | Exact payload hex | Length |
|---|---|---:|
| DeliverToCurrentMain(webview=1, topic="x", payload=[0,1,255]) | `00 01 01 78 03 00 01 ff` | 8 |
| DeliverToOriginalDocument(webview=1, navigation=2, topic="x", payload=[0,1,255]) | `01 01 02 01 78 03 00 01 ff` | 9 |
| ReleaseConsumed(webview=1, navigation=2, message=3, Consumed) | `02 01 02 03 00` | 5 |
| ReleaseConsumed(webview=1, navigation=2, message=3, NoListener) | `02 01 02 03 01` | 5 |
| RendererMessage(webview=1,navigation=2,message=3,url="file:///a",origin="null",topic="x",payload=[0,1,255]) | `00 01 02 03 09 66 69 6c 65 3a 2f 2f 2f 61 04 6e 75 6c 6c 01 78 03 00 01 ff` | 25 |
| DocumentRetired(webview=1,navigation=2) | `01 01 02` | 3 |
| DocumentRetired(webview=u32::MAX,navigation=u32::MAX) | `01 ff ff ff ff 0f ff ff ff ff 0f` | 11 |
| Deliver(Consumed) | `00 00` | 2 |
| Deliver(NoListener) | `00 01` | 2 |
| Deliver(Stale) | `00 02` | 2 |
| Deliver(RouteRetired(NotDispatched)) | `00 04 00` | 3 |
| Deliver(RouteRetired(OutcomeUnknown)) | `00 04 01` | 3 |
| Released | `01` | 1 |
| DeliverToCurrentMain(webview=u32::MAX, topic="x", payload=[]) | `00 ff ff ff ff 0f 01 78 00` | 9 |
| Existing CallError(code="KELD-GUARD003",message="x"), syntax-only | `0d 4b 45 4c 44 2d 47 55 41 52 44 30 30 33 01 78` | 16 |

The last row is a codec-shape vector, not permission for an endpoint to emit a guard error or production approval of the message `"x"`. Deliver(Refused(error)) would prefix its 16 bytes with `00 03`; ReleaseConsumed(...,Refused(error)) would prefix with `02 01 02 03 02`. Provenance and bounded diagnostic-rendering tests are separate.

Exact UTF-8 JSON byte strings (no terminating newline/BOM):

```json
{"v":2,"kind":"bind"}
```

21 bytes.

```json
{"v":2,"kind":"send","document":"0123456789abcdef0123456789abcdef","message":1,"channel":"rendererReq","payload":[0,1,255]}
```

123 bytes.

```json
{"v":2,"kind":"consumed","document":"0123456789abcdef0123456789abcdef","delivery":1,"disposition":{"status":"no-listener"}}
```

123 bytes.

Do not invent a positive full-frame vector with a future guessed channel id. Once C is allocated, the existing owner combines each payload literal with the 16-byte header: `4b 49 02 kind 00 00 C_lo C_hi corr_le32 len_le32`. CALL=1, REPLY=2, ERR=3, EVENT=4; EVENT correlation is zero. The header remains independently tested through the shared hostile corpus.

Negative vectors must include unknown request tag 3, unknown nested disposition tags, zero id/sequence, u32 overflow `ff ff ff ff 10`, truncation, trailing `00` appended to a valid payload, 16,385-byte CALL/EVENT, 4,097-byte control/REPLY/ERR, unexpected reply variant for a known operation, malformed UTF-8 and UTF-16 surrogate, duplicate JSON `kind`/`v`/document keys, v1-shaped numeric channel on v2 Send, v2-shaped string channel on Invoke, payload null or bytes outside 0..255, and forbidden role/generation/URL/target keys. Existing canonical varint acceptance rules remain in force; do not silently demand shortest-form encodings if the current primitive admits overlong nonoverflow varints.

### 4.8 Completion, direction quotas and retention

Renderer→Bun: reserve → native admission/stamp → guard → subscribed RendererMessage
EVENT → synchronous endpoint dispatch → canonical ring release → bounded release drain
sends ReleaseConsumed → host releases that ledger entry → private send-result. The
ReleaseConsumed CALL's REPLY ends that control operation; no acknowledgement follows it.

Bun→renderer: reserve Deliver CALL → validate/capture target → native delivery with
final document check → synchronous dispatch/discard sends consumed → independent native
evaluation completion → normal Deliver REPLY after both required releases while the
link remains admitted. If native proves the queued operation never ran, a stale/refused
terminal disposition may replace consumption. On retirement, preserve the canonical
best-effort ERR/close path even before Native Drained; correlation settlement and retained
capacity are separate. A late native completion only releases its retained charges,
never answers again or replays the call. The host reader continues processing controls
while Deliver is pending and never blocks on the renderer. The facade observes internal
Promise rejection but returns undefined; user listener Promises are not awaited.

WorkerLink currently calls EVENT listeners before releasing the inbound ring record.
Sending ReleaseConsumed from the listener's finally is too early. Extend the canonical
owner with a post-release hook or one bounded/coalesced drain after the synchronous
dispatch stack returns. No per-message unbounded microtask queue. Compat receipts retire
application retention; they do not replace the ring's own accounting or implement GRANT.

Each queue owner reserves before enqueue, postMessage, allocation, conversion or native
scheduling, maintains checked `records <= record_limit` and `bytes <= byte_limit`, and
releases only its own departed representations. At handoff, acquire downstream capacity
before releasing upstream ownership; count overlap in both owners or in fixed scratch.
Zero payload bytes still consume a record. DATA in each direction has a guaranteed
separate share. An aggregate pool may not let incoming DATA consume outgoing/control shares.

| Resource | Hard sequential-proof profile | Accounting and release |
| --- | --- | --- |
| Compat DATA | 1 outstanding record per direction across the role; at most 1 per document | Include retired work and all role generations; no new allowance on restart |
| Endpoint payload / private DATA envelope | 4,096 / 65,536 encoded bytes | Reject before a larger framework allocation |
| Topic / URL / origin | 256 / 4,096 / 1,024 UTF-8 bytes | Exact Unicode scalars; no truncation or normalization |
| Compat app-link body | CALL/EVENT <=16,384; control operation/REPLY/ERR <=4,096 bytes | Canonical preallocation policy plus bounded operation decoding |
| DATA retained per owner/hop/direction | 1 record and 262,144 accounted bytes | Include every owned representation; terminal release only |
| Conversion scratch per owner/hop/direction | 1 active operation and 262,144 accounted bytes | Acquire before copy/encode; never use scratch as an extra queue |
| CONTROL per role at each asynchronous hop | 4 records, each encoded <=4,096; total <=65,536 accounted bytes | Independent of DATA; coalesce idempotent state; exhaust → close admission/retire |
| Native queue/evaluations | 1 DATA + at most 4 CONTROL across the role, including retired work; at most 1 active evaluation per document | One FIFO; charge until actual completion or owner-proved quiescence |
| Codec work | <=16 arguments; depth <=16 including root args array; array length <=256; <=256 total value/property visits; aggregate UTF-8 strings <=4,096 | Incremental checks before traversal/allocation; keys count |
| Diagnostics | 8 fixed records, each <=1,024 encoded bytes | Coalesce/drop excess; no raw payload or unauthenticated stack |
| Missing terminal receipt | 5,000 ms from original admission, never renewed | Close route admission and invoke existing retirement; retain ambiguous charges |

Account backing-buffer capacity, strings at `2 * UTF-16 code units`, and 1,024 fixed
accounting bytes per retained record. One largest private UTF-8 envelope (65,536), its
UTF-16 representation (131,072), generic form (16,384), value bytes (4,096) and record
charge (1,024) total 218,112, below 262,144. This is an accounting envelope, not a measured
VM/WebKit heap or process RSS bound. Extra retained copies must be charged; hidden
representations fail qualification. The actual census and queue high-water tests are
required before any bounded-memory claim.

This deliberately retains the existing 4 KiB value ceiling; #545 must measure and
approve a larger finite profile before larger open/save acceptance. The first request's
callback is not permission to enqueue request two: the proof also waits for release.
Arbitrary callback chaining, burst concurrency and all draw.io startup are unqualified.

The canonical transport owner must bound its **total** queues across every producer,
protecting these compat/control shares. This spec does not assign other APIs' quotas.
Those finite totals and their owner-approved admission amendment are a named prerequisite
for enabling compat; a facade-local limiter does not meet it. Reuse the current
postMessage/WriteQueue/pending-map/ring owners; no parallel socket or queue policy.

Required census: endpoint serialization; isolated-world pending records; native JSON,
strings and decoded buffers; bridge request/outcome queues; core ledger; Worker
postMessage; WriteQueue; pending CALL/timer maps; inbound ring and dispatch copies;
UI proxy; evaluation closures; release/terminal records; diagnostics. Include metadata,
fixed bookkeeping and retired generations. App retention after callback and browser
allocations before native admission are outside the framework-owned queue guarantee.

CONTROL reservation applies at every hop, including reply bookkeeping and native
evaluation. Preserve one outbound FIFO; reservation is not priority reordering or a
second writer. Release/retirement never waits on DATA. The reader either admits under
a bound or refuses, then continues. No queue lock crosses user callbacks, socket waits
or native evaluation. One stream can still suffer head-of-line delay; its existing
finite write deadline supplies fail-closed termination, not a zero-delay guarantee.

Consumer receipts name recorded charges, never supply credit quantities. Duplicate,
future, wrong-direction or stale receipts release nothing and create no growing history.
Use bounded outstanding records/watermarks, not a completed-ID set. A forged early
consumed cannot release active native retention. Call timeout abandons local correlation,
not remote execution. Hold charges after ambiguous loss until authority is retired and
the owning work quiesces. No replay, optimistic refund or unhandled internal rejection.
The 5-second completion policy is new; it does not change any socket deadline or claim
the current timeout cancels remote work. If quiescence is unprovable, stay closed/charged.

### 4.9 Navigation, ordered invalidation and role retirement

#541/#542 own the one native checked-u32 generation: rotate at cross-document commit,
and invalidate earlier on host-initiated load where that owner defines it. Do not rotate
for same-document changes or a cancelled renderer navigation. Demonstrate actual
WKWebView callback/commit semantics with native barriers, not callback-name inference.

Admission is serialized per document/direction, including cross-topic order. Reentrant
sends append. Listener promises may finish/reply in a different order; application reqId
handles that. No global order across views, directions or app roles is promised.

The existing native owner serializes DATA and state-changing bound/readiness/retirement
controls in one per-document FIFO. Each queued command retains host-authenticated role
context outside JS. Close old-role producer admission before drain. Revalidate role and
document under the same queue owner on **every** enqueue, including delayed core results.
Discard unscheduled retired commands; wait for scheduled evaluations and framework
contextBridge/dispatch continuations to quiesce. Retirement revokes host authority
immediately; it cannot depend on enqueueing a fifth control.

Successor readiness is one atomic transition requiring old queued=0, old active=0,
old framework continuations=0, and old producers permanently unable to enqueue. A
queue-empty snapshot or native callback alone is insufficient. Trusted control handlers
mutate state synchronously during evaluation, with no detached future state mutation.
Completion callbacks revalidate identity before mutation. A nonce match alone does not
permit an old role's delayed readiness control to affect its successor.

No second private role epoch is introduced. If quiescence cannot be proved, the document
route remains unavailable and retained charges remain. Already-admitted work may have
executed before retirement; no retrospective cancellation is claimed. New subscriptions
are role-bound; app restart preserves document identity and sequence counters.

DocumentRetired is a CONTROL fact, never an application event or an acknowledged
message. #542 supplies it at authority invalidation; Core closes old-document source
admission and serializes it before any successor-document RendererMessage. Retain a
monotonic retired-navigation watermark in the existing view association, not an
unbounded tombstone set. Older facts do not invalidate a successor; an older queued
message cannot reactivate a retired source. State application during a blocking CALL
may precede dispatch of an earlier message, whose senderFrame must then be null. Existing
WindowEvent::Destroyed invalidates all associated documents through the same registry
without changing its separately specified public destruction-event timing. If a required
invalidation cannot enter the bounded CONTROL path, retire the link; never drop it and
continue serving a live mirror.

senderFrame is a getter against the existing facade mirror's admitted document, not a
permanently non-null object captured on arrival. Host-stamped url/origin are snapshots
for that admitted navigation. Ordered DocumentRetired invalidation is applied through
the canonical WorkerLink state-applier/mirror path before user delivery, including while
the main thread is parked in a host call. The field becomes null after that observable
invalidation barrier. Canonical WorkerLink onEnd invalidates all source mirrors bound to that link before
forwarding its terminal notification to app-facing observers; old closures never attach
to a successor. onEnd follows retained-record delivery and pending-CALL rejection
handlers in a later task. No earlier null-in-rejection guarantee is made. Register
framework cleanup on the canonical end path, not solely LifecycleLink.onLinkDead, which
is suppressed after explicit Quit/close. No instantaneous cross-process freshness is
claimed; this timing limit is an explicit Keld subset pending stronger facade evidence.
Host authorization and final native delivery always revalidate independently of the mirror.

### 4.10 Errors, value domain and facade behavior

| Failure | Contract | Owner |
| --- | --- | --- |
| Malformed, foreign-world, wrong-version or authority-bearing envelope | No app dispatch; no private result to an unauthenticated document | keld-wv |
| Known local type/codec/record/byte excess | Synchronous local error before admission; typed reason and bound | Endpoint / #544 |
| Host pre-admission invalid/busy/stale | Bounded typed refusal; WV-011 only with proved zero attempted write | #1095 consumed by #465/#543 |
| No admitted app role | WV-012 only with proved zero attempted write | Existing native/session owner |
| Attempted/partial write or ambiguous link loss | WV-013 outcome unknown; retire poisoned stream; no replay | Existing writer / #1095 |
| Missing literal channel grant | Existing KELD-GUARD003 and fix; zero listener effect | Existing guard / #552 |
| Invalid KIPC frame policy | Existing KELD-IPC-005 before body allocation; canonical teardown policy | keld-ipc |
| Endpoint value unsupported/malformed | Registered #544 CompatCodecError; no host-value parse | #544 |
| Stale chosen output target | Terminal no-delivery; never retarget | #543/#466 |
| No listener | Discard this dispatch and release; no future-listener queue | #466/#467 |
| Listener throws | Preserve application exception behavior; finally releases framework ownership | #466/#467 |

Use existing CallError/write_call_error. Full WV names are KELD-WV-011/012/013 and are
the approved #1095 meanings, not a claim that main already implements them. API-002 is
the missing @keld/api handler error, never the ordinary no-listener Electron case.
Only host-owned attempted-delivery facts establish host certainty. Consumer Refused
may carry only the endpoint owner's registered codec codes, not forged WV/GUARD proof.
Code is bounded ASCII <=64 bytes; sanitized code-and-fix message <=512 UTF-8 bytes.
Do not truncate identifiers/messages or expose raw value bytes to diagnostics.

Public send/reply/webContents.send stay void. Known local errors may throw; host denial
after a completed void call becomes a bounded internal diagnostic/result, never a
retroactive throw, application `error` event, synthetic reply or Promise API. The
framework handles every internal rejection. Finite-capacity refusal and the selected
clone-error policy are explicit Keld behavior, not unqualified Electron parity.

#544 owns a single TypeScript codec compiled into both endpoints and reused by the
executeJavaScript-result consumer. Minimum domain: JSON values, undefined, Date,
ordinary Error name/message/stack, and class instances flattened to own enumerable
properties. Function/Promise/Symbol/WeakMap/WeakSet and Map/Set/RegExp/Buffer/typed arrays
are explicit unsupported values in this slice. Cycles, shared-reference identity,
BigInt, non-finite numbers and invalid Date must be explicitly refused in v0; no silent
JSON coercion or alias loss. #544 owns exact tags, bounded traversal, malformed-value
handling and newly allocated codec error codes; existing COMPAT evidence codes are not
repurposed. Its implementation/spec must consume these limits before integration.

Use runtime EventEmitter after differential verification; do not reuse @keld/api's
single-handler map. Keep on-listeners separate from invoke handlers. Duplicate listeners
remain duplicates, removeListener removes one occurrence, once removes its wrapper
before callback and removal by original function works. Preserve Electron ipcMain's
internal no-op `error` listener. Listener returns/Promises never become IPC replies.
Renderer events expose sender===ipcRenderer and empty ports in this no-transfer subset.
The page wrapper exposes arguments/callbacks, not raw ipcRenderer, native event handles,
nonces or trusted route fields; #467/#484 retain those contextBridge/world owners.

**Exact Electron v44.4.5 boundary:** the pinned WebFrameMain.send wrapper validates
channel outside its try/catch and catches native _send serialization errors; public
docs say unsupported values throw. event.reply and webContents.send traverse this
wrapper, and missing/foreign frames can exit before serialization. Source inference
alone does not qualify the native live/stale/destroyed matrix. This contract selects a
deliberate Keld first-proof rule: validate channel and bounded endpoint codec locally
before transport, throwing synchronously on invalid/unserializable input; valid payload
to an already-retired original route is an inert void call. Native Electron observed-only
cells remain unknown until the existing oracle owner records them. Record any observed
difference as divergence rather than changing a source citation into runtime evidence.

### 4.11 Platform and migration unit

macOS WKWebView is the first native target. Windows/WebView2 and Linux/WebKitGTK keep
explicit unknown cells until their owners run the same contracts on those engines.
The source/control DTOs can be platform independent; that does not prove native delivery.

Migration includes native decoder/scripts and bridge DTOs, existing primary router,
table/codegen/receiver corpus, canonical transport admission, endpoint codec builds,
main/renderer facades and registered fixtures. keld-wv owns a fixed trusted LegacyV1
or CompatV2 mode and the v2 numeric Invoke adapter. Preserve #1095's settlement/error
contract and existing invoke concurrency; #546 owns multi-invoke. Remove a temporary
adapter only after all bundled callers migrate. Persisted application state is unchanged.

## 5. Boundaries

| Interface / inputs → outputs | Existing implementation owner | Required rule |
| --- | --- | --- |
| Native frame facts → admitted document context | #542 / keld-wv, existing registry | One checked generation; nonce never enters Core |
| Renderer topic + admitted source → guard result | #541/#552 / keld-guard | Literal window grant, one existing dispatcher |
| Generic control DTO ↔ generated TS/bytes | keld-ipc table/codec/receive + existing generator owner | One compat entry, one closed generator target, independent vectors |
| Worker post/ring/write → bounded transport retention | Existing @keld/kipc / GH-527 owner | All producers bounded; control share; post-ring-release seam |
| Send → admitted Bun EVENT | #465 | Identity, guard, subscription and inseparable capacity/release tests |
| Deliver CALL → native event + terminal reply | #543 | Current/original target distinction; normal live-link completion consumes both releases; canonical retirement settles logically without refunding native retention |
| Host retirement → facade mirror | #542 fact, #449 registry, #466 getter | Existing ordered state-applier; no new registry or async user callback |
| JS values ↔ opaque bytes | #544 | Same codec source and golden vectors; no value parsing in Rust |
| on/once/reply/send → listener/route behavior | #466, existing WebContents owner | EventEmitter registry; void API; original-document reply |
| Preload/page callback → first real app witness | #467/#484/#473 | Isolation true, node integration false, real file identity |
| Evidence cells → scored result | #464 consuming #532/#566/#551 | Pinned citations, real cases, unknown/divergence preserved |
| Small proof → larger payload profile | #545 | Measured maximum and renewed max/max+1/equality tests |

#540 writes only this spec, architecture 02 §2/§5 and generated documentation. Do not
edit another owner's guard evaluator, navigation counter, window registry, bridge
settlement, codec, source transport or preload implementation in this PR. No dependency,
workspace manifest, CI policy, root instruction, architecture 03 or #657 Ready change.
The generic schema extension belongs to the existing KEL-98/X05 generator owner; its
current closed Echo/FS/Auth support is extended, not replaced with a general IDL.

## 6. Tasks (each approximately one PR; ordered)

| Task / tracker | Prerequisites | Independently reviewable outcome |
| --- | --- | --- |
| T0 / #540 | Current state and exact-diff review | Adopt this contract and architecture amendments; record delegated decisions; no merge by this session |
| T1 / #464 | Landed #532/#566 | Real mapped absence-baseline cases, citations/denominator and expected fail cells; no runtime stubs |
| T2 / #1095 owner | Existing approved #1096 | Consume their landed settlement fix; do not reimplement it |
| T3a / #541/#552/#542 | Their exact approved grant/navigation amendment | Real native identity, literal default-deny and checked generation with stale tests |
| T3b / canonical transport + #449 | Approved total-queue quotas and #657/readiness decisions | Bounded all-producer admission/control reserve, post-release hook and shared Subscribe |
| T3c / #544 | Its approved codec specification consuming this subset | One endpoint source, exact tags/errors, independent goldens and traversal limits |
| T4 / #465 with canonical table/receive owner | T2, T3a, T3b, generic codegen/receive review | Lower send vertical slice with guard, exact v2, canonical bounds and release path |
| T5 / #543 | T4 and native completion contract | Current/original native event delivery, terminal Deliver reply, stale and full-quota tests |
| T6 / #466 | T1, T3c, T4, T5; existing facade registry | Listener facade, sender metadata/getter, reply and webContents.send; mapped cells only flip on proof |
| T7 / #467 with #484/#473 | T6, preload/world/URL owners | Actual two-request draw.io witness; then controlled close-path qualification |
| T8 / #545 | Independent raw census may start now; implementation follows approved measured profile | Larger open/save acceptance with byte-equal max and max+1 refusal |

This table is the dependency graph: T3a/T3b/T3c can progress independently; T4 joins
identity/transport, T6 joins codec and facades, T7 joins actual preload/URL composition.
T1 can land before transport implementation. #545 raw measurement does not wait on
new IPC. The first-round-trip cut is T7; full clone, invoke, sync IPC and ports are not
dependencies. Existing #1095 and #657 PRs keep their own writers and gates.

## 7. Test plan

The matrix below is a design, not a list of executed passes. For each implementation:
retain a real failing assertion before its fix, then require the fixed result, then
apply the named independent mutation, require failure, restore and rerun. Expected
bytes and no-effect witnesses must not be calculated by the implementation under test.
Use per-window/process counters, barriers, independent byte literals and native facts.
No sleep synchronization, timeout-as-success, stub facade, skip/todo, test.failing
inversion or fake output. Timeouts terminate hangs only.

GH-532's **baseline evidence** rule is different from a runtime red/green test: ordinary
mapped tests assert the current missing behavior and pass while the evidence cell is
expected_verdict=fail. When implementation arrives, replace that absence assertion with
the documented behavior and change the cell only after proof. This keeps CI honest
without presenting missing APIs as compatible. Reuse #566's manifest/runner/parser.

### 7.1 Wire, identity and authority

| ID / owner | Test and independent observation | Deliberate negative control |
|---|---|---|
| W1 · #465 | v2 bind/send/event/consumed fixture bytes: accept exact valid forms; reject unknown/duplicate fields, version downgrade, zero/overflow IDs, invalid Unicode and authority fields. Native forward counter remains zero for rejected input. | Remove one strict-field/version check; the relevant invalid golden vector must be accepted and fail the test. |
| W2 · #465/#543 | Payload 4096 succeeds, 4097 refuses; encoded envelope 65536 boundary checked before parse/allocation. Allocation/retention counters and allowed follow-up independently observed. | Check length after parse/copy or change `>` to `>=`; boundary/allocation witness fails. |
| W3 · keld-ipc / #597 | Wrong kind, flags, correlation, direction, channel and length rejected by the canonical selected receive policy before body allocation; same shared hostile corpus in Rust/TS. | Admit all HostInternal channels or allow an EVENT with nonzero correlation; hostile vector reaches body counter. |
| W4 · #449 | Old/unsubscribed app link receives no compat EVENT; listener is installed before Subscribe readiness; role restart requires new Subscribe. | Mark ready before handler registration, or preserve subscription across restart; a first-message/stale-peer witness fails. |
| W5 · #465/native | Same JSON through wrong content world, foreign WebView, subframe or wrong native handler receives zero forwarding; allowed native path still works. | Bypass each native provenance check one at a time; forbidden path reaches independent host counter. |
| W6 · #552 | Exact granted `el:rendererReq` forwards; prefix, suffix, Unicode variant, wildcard, ungranted import-handler topic and another window's grant do not. Observe zero listener/file effect plus valid follow-up. | Remove guard call, normalize channel or evaluate only AppProcess grants; denied witness fires. |
| W7 · #542/#466 | Forged source IDs/URL/origin/generation cannot alter senderFrame or routing; real URL equals committed native document URL. | Populate source metadata from renderer payload; the comparison to native-side fixture fails. |
| W8 · #543/#466 | Two windows, distinct sentinels: a role can send only to its owned window; reply goes only to original source. A foreign window/frame receives zero events. | Remove membership check or broadcast output; independent other-window counter increments. |
| W9 · #542 | Hold old output behind a barrier; commit new document; release output; successor receives none. New request with reused local sequence still works. | Key pending/delivery only by WebviewId or request number; successor receives stale bytes or loses its pending state. |
| W10 · #542 | Same-document change and cancelled renderer navigation retain usability; committed cross-document and host-initiated invalidation reject old work. Force u32 exhaustion in a seam. | Increment on every navigation attempt, omit host invalidation, truncate u64 or wrap u32; corresponding independent identity witness fails. |
| W11 · #1095/#543 | After old result arrives, live pending result remains settleable; after role restart/rebind in the same document, sequence never resets and old-role receipts cannot settle/refund new work. | Clear current pending before document comparison, reset sequence on rebind, or ignore RoleGeneration; new operation is lost or falsely credited. |
| W12 · #544/host router | Send a manually chosen byte vector between raw endpoint fixtures; host egress digest equals ingress digest, including byte sequences a JS codec rejects. Host rejects invalid routing but is indifferent to value tags. | Insert JSON/codec decoding or byte normalization in Rust; opaque-vector or layer dependency check fails. |
| W13 · #465/#1095 | A successful 4,096-byte numeric Invoke reply is DATA and remains admitted under the private DATA envelope bound; error-only results and receipts obey CONTROL bounds. Concurrent Invoke/Send and completion-copy barriers never exceed the shared native budget. | Classify all results as CONTROL, mint a second result allowance, or retain an uncharged request/result copy; boundary and independent retention witnesses fail. |

### 7.2 Capacity, concurrency and liveness

| ID / owner | Test and independent observation | Deliberate negative control |
|---|---|---|
| Q1 · #465/#543 | With injected small record/byte limits, fill exact limit, refuse next before allocation; zero-byte data consumes a record; healthy follow-up after terminal release succeeds. | Remove record limit, undercharge metadata or reserve after enqueue; independent high-water counter exceeds declared bound. |
| Q2 · transport/native owners | Stall each queue boundary separately: postMessage, WriteQueue, pending CALLs, core outcomes, UI proxy, native eval and receiver. Burst above bound across several views. Record all live queue bytes/records, not just facade counters. | Unbound one stage or release upstream before uncharged copy; that stage's independent peak exceeds bound. |
| Q3 · #465/#543 | Fill incoming guaranteed quota; every synchronous main listener sends a reply. Outgoing guaranteed quota and control remain available, with finite explicit refusal only at their own boundary. | Merge direction shares into one exhaustible pool; replies cannot be admitted or the test deadlocks. |
| Q4 · ledger owners | Replay consumed/receipt; wrong direction, role, document, future sequence, altered bytes and random IDs. Exactly one original reservation releases; invalid receipt floods stay bounded. | Trust caller's byte amount or release without key match; aggregate counters underflow/available capacity grows. |
| Q5 · @keld/kipc | Hold canonical ring record until after listener returns; prove ReleaseConsumed is issued only after release hook/coalesced drain. | Send release in listener `finally`; barrier observes an acknowledgement while the ring record remains retained. |
| Q6 · #543/native | Send a valid-looking renderer consumed early while native completion is paused. Native capacity stays charged and additional eval is bounded. | Treat consumed as proof of native release; scheduling exceeds native high-water bound. |
| Q7 · transport/native owners | Fill data at every hop and then issue release, refusal and teardown. No ACK-for-ACK; no reader wait on data credit; one FIFO writer stays intact. Exhaust all four controls: host retirement still revokes authority without enqueueing a fifth. | Put controls in the exhausted data pool, make retirement depend on another control enqueue, require receipts for receipts, or add priority writer; authority/progress/order witness fails. |
| Q8 · #543/transport | On an admitted live link, normal Deliver REPLY waits until synchronous consumed/discard and native release; user async Promise does not block completion. Missing listener yields a terminal discard. | Reply on mere admission or await listener Promise; terminal-state barrier observes premature release or hung completion. |
| Q9 · liveness owner | Expire local CALL deadline while remote delivery is paused, then release remote work. No refund/replay until proven quiescence; no unhandled rejection; route cannot accept an unbounded second batch. | Refund on local timeout or retry automatically; duplicate receiver count or retained-byte limit fails. |
| Q10 · #1095/canonical writer | Inject no app, zero-byte write failure, partial write and post-write link loss. Distinguish WV-012 only for no attempted write; ambiguous outcomes WV-013; stream cannot continue after partial frame. | Classify any failed write as unavailable or replay on new generation; native write-count/effect witness fails. |
| Q11 · #466/router | Reentrant sends, listener mutation, thrown listener and close during callback do not hold a host lock, leak reservations or invoke once twice. Verify later allowed work. | Hold registry/router lock across callback or omit release on throw; deterministic reentrancy/count test fails. |
| Q12 · all route owners | Pause an old native readiness/retirement control and restart the app in the same document: successor readiness must remain withheld until old DATA/controls quiesce; after release it is applied once and cannot be changed by an old callback. Concurrent close/navigation preserves bounded totals including retired work; generation/sequence/WebviewId exhaustion closes safely. | Publish successor readiness early, defer a control's state mutation past its evaluation completion, omit role check, mint fresh budget before quiescence, or reuse identity; wrong readiness, stale delivery or excess retention appears. |
| Q13 · #543/canonical retirement | Pause native evaluation, retire its app role and observe the existing first-terminal-wins ERR/close path. Logical correlation may settle before Native Drained; retained charges stay until quiescence, and late completion sends no second answer or replay. | Require the retiring call to remain pending until native release, refund on ERR/EOF, or reply again after completion; independent correlation/lease/write counters fail. |

### 7.3 Endpoint and actual-application behavior

| ID / owner | Test and independent observation | Deliberate negative control |
|---|---|---|
| E1 · #466/#467 | Two on listeners, one duplicate, once and removal by original function; exact callback IDs/order across two sends and reentrant emit. Compare pinned EventEmitter behavior. | Replace with @keld/api single-handler map, deduplicate or remove once after callback; independent sequence fails. |
| E2 · #544/#467 | Clone the whole argument vector: undefined/null/false/0/empty string, Date, Error, plain/flattened object; mutation after send does not affect receiver snapshot. | Substitute JSON, strip error fields or retain original object reference; receiver's separately constructed expected value fails. |
| E3 · #544/#464 | Function/Promise/Symbol/WeakMap/WeakSet root and nested rejection before dispatch. Map/Set/RegExp/typed arrays and omitted clone domains have explicit unsupported results. Golden malformed tag/depth/key/node/length corpus has no prototype effect. | Accept one forbidden value, silently degrade or allocate before complexity check; zero-dispatch/prototype/limit witness fails. |
| E4 · #466/#464 | Two asynchronously completed main handlers may reply in reverse completion order; send/reply/webContents return undefined. App reqIds match the correct callbacks. | Add request Promise return, serialize callback Promises globally or assign framework correlation as reqId; transcript changes. |
| E5 · #466/#542 | After independently observed host invalidation, retained event.senderFrame becomes null; authentic IDs/URL are host-derived, not required to be numerically unequal to OS PIDs. | Freeze senderFrame forever or accept payload metadata; native/JS lifetime comparison fails. |
| E6 · #466/#543 | Current-main send and original-document reply are distinguished with navigation between admission and delivery; retired output never retargets. Electron arm may add a subframe; Keld declares it unsupported. | Implement reply as current mainFrame.send or resolve a queued target again after navigation; successor sentinel fires. |
| E7 · #467 | Renderer event.sender equals its ipcRenderer and ports is empty; actual contextBridge wrapper exposes args without native event or routing internals. | Pass the raw transport event or return WebContents as sender; page-access/identity assertion fails. |
| E8 · #466/#467 | Dispatch an event without a listener, observe release barrier, register listener, then send another. Only the latter is observed. | Retain first event for future listeners; two observations occur. Keep Electron observed-only cell unknown. |
| E9 · #466/#551 | Pinned macOS native matrix: live frame / stale original frame / destroyed WebContents × valid data / Function / invalid channel. Capture synchronous throw/return, return tag, stderr and receiver count. | Change throw/return tag or remove receiver observation; comparator must reject. Until recorded, row13 unknown. |
| E10 · #466/#467 | Actual draw.io preload plus actual main `rendererReq` switch: two unique `isPluginsEnabled` requests yield false via mainResp; compare request and handler witnesses as well as callbacks. | Disable native event delivery, substitute constant ping, wrong channel or wrong reqId; actual app callback/handler witness fails. |
| E11 · #466/#467 | Actual close-path isModified, saveAndClose and removeDraft exchanges: exact target, once and removal counts. | Broadcast to another window, leave removed listener or duplicate draftRemoved; app-visible sequence fails. |
| E12 · #545 | Measured open/save corpus includes >4096 bytes; encoded/retained sizes recorded; after approved lift, byte-equal output at max and refusal at max+1. | Truncate, transcode, compress away the asserted bytes or keep old bound; independent digest/length fails. |
| E13 · #464/#566 | Wrong Electron commit, missing cited sentence, wrong platform, absent/unexecuted mapped test and mutation-that-still-passes cannot produce a scored pass. | Feed test.failing/skip/lookalike-only output or reuse fixture-generated expected output; shared admission gate must reject. |

These controls are deliberately non-overlapping: framing checks do not prove authorization; authorization logs do not prove no side effect; arrival counters do not prove value fidelity; memory counters in only one queue do not prove end-to-end retention; and a successful fresh-document send does not prove stale-document safety.


Add the following independent controls to W3/Q7/E5: compat body 16,384 versus 16,385
before allocation; control 4,096 versus 4,097 after bounded variant classification;
ReleaseConsumed admitted with DATA full; wrong response variant for a pending operation;
DocumentRetired applied during a blocking CALL; old RendererMessage after retirement
cannot reactivate a mirror; old retirement after new document cannot invalidate it;
explicit Quit/close still invokes terminal mirror cleanup even when onLinkDead is
suppressed. Retire the link if an authoritative invalidation cannot be admitted; never
silently lose it. Each control must fail when only its associated check is removed.

Documentation validation: `just llms`, `just llms-test`, `just llms-check`, repository
documentation checks and `just ci` under its canonical applicability router. The exact
diff receives named independent wire/authority and lifecycle/retention review, with
independent refutation of each finding. No native acceptance follows from a docs pass.

### Versioned upstream evidence

| Evidence | Exact primary source | Supported claim / limit |
| --- | --- | --- |
| Electron pin | [v44.4.5](https://github.com/electron/electron/releases/tag/v44.4.5), commit `694f45852a0f1726cd23bfd379854de489cccb65` | Fixed target, not a rolling latest branch |
| Listener API | [ipc-main.md](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/ipc-main.md), [implementation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/ipc-main-impl.ts) | EventEmitter and internal error listener; individual doc cells need exact snapshot anchors |
| Original reply | [ipc-dispatch.ts](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/ipc-dispatch.ts) | reply captures originating frame/process route |
| Current-main send | [web-contents.ts](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/web-contents.ts) | send uses mainFrame; sendToFrame resolves original frame membership |
| Error boundary | [web-frame-main.ts](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/web-frame-main.ts), [native](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_web_frame_main.cc) | Source catch/clone behavior; cannot alone supply observed native cells |
| senderFrame lifetime | [api-web-frame-main-spec.ts](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/spec/api-web-frame-main-spec.ts) | Native upstream tests distinguish navigation/disposal; Keld must run its own path |
| draw.io target | [main](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js), [preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js) | rendererReq/mainResp, reqId, isPluginsEnabled, validateSender and isolation settings |

Retrieved 2026-10-23. Exact native oracle artifacts, if produced, remain separate
version/platform-bound evidence. Do not quote repository source as an executed result.

## 8. Review gates triggered

Public API, permission model and wire protocol: required named independent exact-diff
evidence. In particular review HostInternal carriage plus per-renderer guard, private
version mode, CONTROL classification, post-release/native quiescence, error provenance
and explicit divergences. Unsafe: none in this docs PR; native consumer diffs trigger
their own FFI review. Dependency addition: none. Specification approval is recorded
with delegation provenance and exact reviewed content. Required checks and every
implementation owner's own spec gate remain intact. This session must not merge.

## 9. Perf impact

No measured latency, throughput, startup or heap improvement is claimed. Bookkeeping,
endpoint encoding, Worker hops and native evaluation can change costs. Before enabling
the profile, capture a reproducible retention/copy census and stall each queue against
its declared limits. Benchmark broader throughput only with the existing IPC metrics
owners and approved workload. The 4 KiB sequential proof does not justify shared memory,
a larger window, a raised payload bound or a new transport.

## 10. Open questions

No unresolved choice within the selected #540 contract. The delegated decisions are:

| Decision | Selected contract | Evidence/implementation still required |
| --- | --- | --- |
| D1, first milestone | Controlled 4 KiB actual round-trip; keep larger open/save with #545 | Raw payload distribution and broader app acceptance remain open |
| D2, carriage | One compat-control; HostCallWithEvents/HostInternal + mandatory renderer guard; exact schemas above | Canonical table/receiver/codegen owner review and implementation |
| D3, capacity | Explicit one-record direction shares, four controls, finite hop/scratch limits | Canonical all-producer total quotas, actual representation census and negative controls |
| D4, identity/grants | Consume one checked-u32 native mint and exact literal grants | #541's own exact-content amendment approval; #542/#552 implementation; no approval by proxy |
| D5, local error surface | Synchronous local channel/codec validation; void late failures; explicit divergence where needed | Exact native oracle matrix stays unknown until observed; no blanket parity claim |
| D6, sender lifetime | Host-origin metadata, ordered DocumentRetired and terminal cleanup; no PID inequality rule | Existing mirror/registry integration; native lifetime/invalidation proof |
| D7, common owners | Existing Subscribe, WorkerLink, guard, generator and registry only | #449/#657 and canonical transport prerequisites; no competing implementation |
| D8, values | Bounded endpoint subset with explicit refusal of omitted domains | #544 exact tags/errors/goldens and approved consumer spec |
| D9, completion | Terminal Deliver receipts, post-ring release, native completion, five-second nonrenewing retirement trigger | Missing-receipt/early-ACK/retired-generation quiescence tests |

These are decisions for this contract, not approvals of unread or unmerged #541, #657
or #544 content. Their exact owner artifacts remain dependencies with clear next
actions. The same applies to the canonical transport owner's total quotas: compat
cannot be enabled until the owning shared amendment and actual bounds pass. No agent
may fill those gaps with a facade-local queue, guessed quota for unrelated APIs, a new
navigation mint or a copied permission rule. #540 remains open until its approved
specification and architecture amendments are merged by an authorized later action.
