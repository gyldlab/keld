# Spec: deliver named Electron invokes to the registered main handler

- Status: draft
- Tracker: GH-547
- Owner: Monish Khandelwal (approval pending)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Let an admitted renderer call `ipcRenderer.invoke(topic, ...args)` and receive the result of the matching Bun main handler.
Preserve the original caller, exact channel, argument order and terminal disposition throughout the call.
Qualify Zettlr’s macOS development-stage mount under explicit Legacy. Qualify installed operation separately. Every AC below is a target, subject to section 10.

- Exclude `sendSync`, `returnValue`, ports, subframes, service workers and per-frame or per-WebContents handler registration.
- Exclude new transports, topic-to-numeric channel registries, manifest parsers, filesystem grants and app-specific handler substitutions.
- Leave draw.io, installed packaging, cross-platform qualification and complete Zettlr product scoring with their existing owners.
- This document neither approves changing the current bridge nor enables disclosure of exception text.

## 2. Spec refs

| Status | Source | Decision-bearing fact |
| --- | --- | --- |
| FACT | `docs/architecture/02-ipc.md:450` | The architecture maps named invoke/handle onto CALL/REPLY. |
| FACT | `docs/architecture/03-security.md:8`, `docs/architecture/04-electron-compat.md:63` | Principal isolation and the compatibility facade have separate owners. |
| FACT | `packages/@keld/electron/src/index.ts:29`, `packages/@keld/api/src/channels.ts:93` | The pinned facade exports app and BrowserWindow. The numeric application dispatcher accepts Echo and rejects other channels. |
| FACT | `docs/specs/kel142-macos-renderer-bridge-api.md:353`, `docs/specs/gh1095-request-error-codes.md:3` | The existing bridge documents ambiguous API failures. The error-split revision at this main pin is a draft. |
| FACT | #1365 `packages/@keld/electron/src/ipc-main.ts:99@6a30a4213775ad0c6f3b9d172075296ac78142c4` | The registration candidate owns a private handler map, distinct from EventEmitter listeners. |
| FACT | #1365 `packages/@keld/electron/src/ipc-main.ts:124@6a30a4213775ad0c6f3b9d172075296ac78142c4` | Its wrapper removes the entry before invoking the user callback. Registration alone delivers nothing. |
| FACT | #1122 `docs/specs/gh540-compat-ipc.md:367@16b00a2e388449c4525a30913937bc2f6e11c472` | The private numeric invoke adapter explicitly excludes Electron named invokes. |
| FACT | #1190 `docs/specs/gh544-endpoint-value-codec.md:115@0437c9104077e6a52c9128e4367e91c57937b22a` | Returned Error data and a thrown transport error are distinct. |
| FACT | #1191 `docs/specs/gh552-exact-channel-grants-guard.md:136@a73f301cfe7027ad3e1bd700800e9abe60c66bc7` | The trusted native window association selects grants. The page cannot select the grant bucket. |
| FACT | #1192 `docs/specs/gh542-native-document-dual-world-binding.md:36@c889a0f054b89de9b838a1a3451238cf638f86e2` | Cross-document commit revokes old realm bindings. |

FACT: [GH547](https://github.com/gyldlab/keld/issues/547) owns invoke delivery and asks for a KEL-142 error-disclosure amendment.
[GH546](https://github.com/gyldlab/keld/issues/546) owns concurrent renderer invokes and credits. [GH545](https://github.com/gyldlab/keld/issues/545) owns larger payload qualification.
[GH467](https://github.com/gyldlab/keld/issues/467) owns the preload-to-page boundary. [GH1354](https://github.com/gyldlab/keld/issues/1354) owns the Zettlr product verdict.
These references do not transfer approval, wire ownership, device claims or product scores.

## 3. Acceptance criteria

### AC-1: Preserve registration semantics at the shared owner

- **Given:** One handler and one listener share a literal topic. The handler has a distinct return marker.
- **When:** Attempt duplicate `handle`, then remove the listener. Invoke the topic. Remove the handler and invoke again.
- **Then:** Duplicate registration fails synchronously before replacement. Listener removal preserves the handler. Handler removal makes the second invoke reject.
- **Negative control:**
  - **Single fault:** Replace the first handler on duplicate registration.
  - **At-risk precondition:** Both candidates are callable. The first registration succeeds. Both results fit.
  - **Expected RED:** The registration oracle misses the duplicate throw or the first invocation returns the replacement marker.

### AC-2: Deliver values through the real application route

- **Given:** The installed main module registers synchronous and asynchronous fixtures. Two arguments differ by type and order.
- **When:** Invoke through the real isolated preload and native Host route. Return a primitive, `undefined`, a supported Error value and a delayed value separately.
- **Then:** The renderer receives a Promise. Each admitted value fulfills once with its declared codec semantics. No ordinary listener runs.
- **Negative control:**
  - **Single fault:** Dispatch invokes through the listener table.
  - **At-risk precondition:** The same topic has a listener and a different invoke handler. All identity, grant, codec and credit checks pass.
  - **Expected RED:** The listener counter increments or the required handler result is absent. A fabricated acknowledgment cannot match the handler marker.

### AC-3: Consume a one-shot handler before entering application code

- **Given:** `handleOnce` registers a callback that installs its replacement on the same topic before yielding.
- **When:** The first real invocation enters that callback. Issue another admitted invocation after replacement registration and before first settlement.
- **Then:** Replacement registration succeeds. The old callback runs once. The second invocation reaches the replacement. Late cleanup preserves the replacement.
- **Negative control:**
  - **Single fault:** Move one-shot removal until after the user callback returns.
  - **At-risk precondition:** The first callback attempts replacement synchronously. No other registration owns the name.
  - **Expected RED:** Replacement registration throws a duplicate-handler error inside the first callback. Observe this before any reply cleanup.

### AC-4: Reject unsupported arguments before main effects

- **Given:** An allowed topic has an installed handler and spare credit. One arm sends a function. The control sends a supported literal.
- **When:** Call the real renderer facade and separately capture a synchronous throw and a rejected Promise.
- **Then:** The unsupported argument rejects the Promise before transport admission. No main invocation occurs. The supported control fulfills.
- **Negative control:**
  - **Single fault:** Replace an unsupported function argument with `null` during argument encoding.
  - **At-risk precondition:** `null` is valid for this handler. Count admission and effects independently of the codec.
  - **Expected RED:** The invalid argument reaches admission or the handler observes `null`.

### AC-5: A denied request cannot consume a handler

- **Given:** A current bound window lacks the literal permission. Another current window has it. The same one-shot handler serves both.
- **When:** The denied window calls first. The permitted window calls after the denial receipt.
- **Then:** The denied caller receives the approved guard refusal. Only the permitted call consumes the handler and returns its marker.
- **Negative control:**
  - **Single fault:** Treat the missing literal as allowed at the final privileged dispatch decision.
  - **At-risk precondition:** Parsing, source, document, role, codec and credit admit the denied request. No earlier check filters this literal.
  - **Expected RED:** The guard's final disposition admits the first caller. The owner-local decision trace exposes the fault even if later delivery refuses.

### AC-6: Construct the invoke event from its authenticated sender

- **Given:** Two live windows have distinct URLs, frame identities and WebContents instances. B-3 has qualified the native event inputs.
- **When:** Invoke from A with argument data naming B. Hold the event through an observed retirement of A's source frame.
- **Then:** Event type is `frame`. IDs and sender match A. The initial frame matches A's URL/origin. Its retired frame getter returns `null`, never B.
- **Negative control:**
  - **Single fault:** Select `event.sender` from an application argument instead of the verified binding.
  - **At-risk precondition:** The payload field is valid application data. Both windows are live at handler entry. Their registered identities differ.
  - **Expected RED:** The handler observes B's WebContents object for A's admitted call.

### AC-7: Distinguish missing handlers, thrown errors and returned values

- **Given:** One admitted topic lacks a handler. Another throws ordinary `Error` with a safe marker. A third returns the same Error as value data.
- **When:** Send each call over the same healthy route. Compare error class, message, fulfillment state, entered-handler count and reply owner separately.
- **Then:** Missing and thrown cases reject once with their approved distinct texts. Returned Error data fulfills. No remote stack becomes the renderer's error stack.
- **Negative control:**
  - **Single fault:** Replace the supported handler-error marker with the generic numeric bridge failure.
  - **At-risk precondition:** The ordinary marker fits the approved byte and envelope bounds. The proper handler ran under the active disclosure amendment.
  - **Expected RED:** The caller's exact message differs from the independently recorded pinned oracle.

### AC-8: Confine failure disclosure to the approved field

- **Given:** A handler throws a supported ordinary error with a sentinel only in its remote stack and cause. Its allowed message is distinct.
- **When:** Capture the endpoint-produced failure payload and the caller's rejection after decode. Exercise hostile getter and oversized-message fallback cases separately.
- **Then:** The failure carries only the approved bounded text and classification. The sentinel never crosses the endpoint. Unsupported forms use the approved safe failure.
- **Negative control:**
  - **Single fault:** Serialize thrown errors with the returned-value Error codec, including the remote stack.
  - **At-risk precondition:** The stack is an ordinary supported string. The sentinel fits the budget. Inspect the endpoint output before Host filtering.
  - **Expected RED:** The emitted failure contains the private stack sentinel even if a later renderer refuses that payload.

### AC-9: Retire the original request without replay or rebinding

- **Given:** One invocation from A reaches an effectful handler and waits. A valid successor can issue a fresh call.
- **When:** Order completion before retirement and retirement before completion. Repeat for document replacement, view destruction and role loss.
- **Then:** The live winner settles once. Retirement revokes delivery to A. Late work cannot resolve a successor or replay the effect. Same-document navigation preserves eligible calls.
- **Negative control:**
  - **Single fault:** Accept a retired result at the final return-route currentness check.
  - **At-risk precondition:** Hold the real result at that owner through retirement. Observe its decision before settled-Promise filtering.
  - **Expected RED:** The owner admits the old result to a retired binding even if a later caller filter drops it.

### AC-10: Bound concurrent calls and physical retention

- **Given:** B-2 has approved finite record and byte ceilings, return reserves, lifetime bounds and deadlines. At least two concurrent requests fit.
- **When:** Hold two valid calls and finish them in reverse order. Then fill the exact record limit and submit one more fitting request.
- **Then:** Results correlate to their callers. Excess receives typed refusal before dispatch. A subsequent legal call succeeds after actual release.
- **Negative control:**
  - **Single fault:** Skip the record-limit check before native request retention.
  - **At-risk precondition:** All live slots are occupied. Enough byte capacity remains. Keep handlers pending and all non-record checks valid.
  - **Expected RED:** The native owner admits one excess record. Observe admission before a downstream refusal can mask it.

### AC-11: Qualify the Zettlr document workload in its recorded mode

- **Given:** B-5 freezes the development stage, app, oracle, explicit Legacy policy, inputs and observer. An installed arm additionally requires B-7. The fixture text alone exceeds 4,096 UTF-8 bytes. Freeze its independently measured length and digest.
- **When:** Launch the pinned app on macOS in the recorded mode. Verify AC-12’s initial store reads before the open and edit probes. Use the real preload.
- **Then:** The editor mounts. Required startup calls and at least twenty consecutive documents-authority/push-updates invokes from one window settle without unexpected bridge-floor errors.
  Verify opened text bytes/digest, final edit, and Host-observed setDocumentEdited(true) independently.
- **Negative control:**
  - **Single fault:** Remove only the shipping registration for `documents-authority`.
  - **At-risk precondition:** The unchanged real app reaches an admitted `get-document` request for that topic. Other startup and file-input prerequisites pass.
  - **Expected RED:** The requested document content does not appear. The missing-handler receipt identifies the failed request. A visible window alone cannot qualify mount.

### AC-12: Prove both overlapping mount reads reach the document store

- **Given:** A non-null window ID selects a seeded pane tree. The main application's modified-file list contains a distinct nondefault marker. Both requests fit the approved credits.
- **When:** Mount the real document-tree store. Hold the first `retrieve-tab-config` completion at its admission owner until `get-file-modification-status` arrives there.
- **Then:** Record two overlapping, separately correlated `documents-provider` calls through the real preload. Verify the restored pane tree and modified paths from their initial replies. A logged failure, later update event or visible editor cannot satisfy either assertion.
- **Negative control:**
  - **Single fault:** Restore a one-pending-call limit at the shared renderer admission owner instead of the approved limit of at least two.
  - **At-risk precondition:** Keep the first call’s slot occupied at that owner. The second store call reaches it with valid binding, grant, handler, byte capacity and all other credits.
  - **Expected RED:** The second initial call is refused. Its required reply and store update are absent. Observe this before any later refresh can supply the marker.

## 4. Design

### D-1: One pending-call path, separate decisions

All design paragraphs below propose targets. Do not read a target as a current implementation claim.

| Atom and existing owner | Input to output | Failure prevented and witness |
| --- | --- | --- |
| #467/#484 exposure | Approved app-world method to request | Page authority substitution. AC-5/AC-6. |
| #542 native identity | Actual view, document and realm to live binding | Successor receives predecessor's request. AC-6/AC-9. |
| #541/#552 guard | Binding and exact literal to privileged disposition | Live sender invokes forbidden method. AC-5. |
| #540/#546 transport | Admitted request to one correlated bounded call | Wrong reply, unbounded retention or duplicate dispatch. AC-9/AC-10. |
| #466 registration | Topic to existing handler or absence | Listener delivery or consumed replacement. AC-1/AC-2/AC-3. |
| #544 endpoints | Argument list or return value to bounded bytes | Silent unsupported-value conversion. AC-2/AC-4. |
| KEL-142/GH547 outcomes | Handler outcome to approved single settlement | Text leakage, hidden failure or retry. AC-7/AC-8/AC-9. |
| GH1354 evidence | Mode-specific launch to store, document and edit effects | Missing initial reply or substituted text. AC-11/AC-12. |

A named invocation does not grant filesystem or network authority. The application's own operations retain their normal owners and checks.
The scope of `documents-provider` includes multiple commands. An exact topic grant does not validate an argument or make its path an OS grant.

### D-2: Reuse the private handler table

FACT: #1365 `packages/@keld/electron/src/ipc-main.ts:109@6a30a4213775ad0c6f3b9d172075296ac78142c4` checks duplicate registration before handler type.
The candidate includes Keld diagnostics. Byte-for-byte Electron registration-error parity is not established.

Use one module-private lookup-and-enter operation at the registration owner. Do not expose the map or a second public dispatch API.
Look up the handler after validated routing and decoding. Snapshot the selected callable before invocation.
`removeHandler` affects later lookup. It cannot cancel an already entered callable.
Call the selected wrapper directly. Do not remove one-shot entries again on Promise settlement.
Keep `removeAllListeners` independent of `removeHandler`, including channel `error` and names resembling object properties.
On role replacement, release old registrations with the old role. A new main entry creates its own registrations.

Test duplicate callable, duplicate non-callable, fresh non-callable, absent removal, removal during a pending handler and replacement registration.
Match the frozen #466 diagnostic class, exact text, code and fix independently. Record differences from the Electron oracle as divergences.
A non-callable passed to `handleOnce` installs the pinned wrapper first. Test its first invocation failure and subsequent missing-handler behavior.
Do not claim full Electron error compatibility until the relevant rows agree or a named divergence is accepted.

### D-3: Amend the existing carrier before named dispatch

FACT: The current #540 proposal does not carry an Electron invoke result. Its private numeric `invoke` has different authority and result semantics.
Consuming the send/event proposal unchanged would lose the handler result or its provenance.

Require B-1 approval of a versioned named-request and named-terminal amendment inside the canonical carrier.
Use a Host-allocated transport correlation and the existing authenticated connection. Never treat page request IDs as role, frame or grant authority.
A request needs an exact literal and opaque argument bytes.
Target endpoint outcomes are `Value(bytes)`, `NoHandler` and `HandlerFailure(boundedText)`. Keep transport refusal and retirement outside that application union.
An endpoint report is not a Host proof of non-dispatch. The Host must not inspect or reinterpret success bytes.
B-1 freezes actual tags, generated schemas, byte bounds, mode negotiation, compatibility disposition and independent wire vectors before implementation.
Reject a mismatched kind or version. Never reinterpret named invokes as numeric Echo, send/reply, arbitrary `el:*`, or a page-chosen endpoint.
No new IPC channel number, public grant syntax, side socket, or parallel reply registry is allocated here.

Do not expand an existing send-only permission into invoke access by upgrading the runtime alone.
B-1 must settle activation and method eligibility with the exact-literal grant owner. Retain denied behavior for older policies until then.
The Host authenticates the native document and role, canonicalizes no topic aliases, and performs the final grant decision before forwarding.
An unauthenticated or denied route cannot probe handler existence or consume `handleOnce`.
The Bun endpoint selects the handler. Rust does not decode application values, dereference app paths or invoke app code.

### D-4: Deliver the attributed event and correlated result

Return a Promise from the renderer facade, including on validation failure. Use a single endpoint argument codec in the app world and Bun.
Preserve order, arity, `undefined` and approved returned Error data. Reject unsupported values, including a function, before the first write.
Check the full return payload before encoding and publication using the approved incremental codec budget. Do not retain an unbounded intermediate result copy.
Test oversized and unsupported returns after the handler's sentinel effect. Reject the call without claiming that its effect did not run.

Only accepted eligible main-frame operations can mint application events. Use the existing WebContents wrapper for the verified originating window.
Use opaque framework process/frame identifiers from the approved identity owner. Do not substitute OS process IDs, guessed zeros, URLs or payload fields.
Read `senderFrame` through the lifetime owner. After observed retirement, return `null`. Never point an old event at a new frame.
B-3 must settle native input provenance and observable retirement order before integration. A JavaScript type definition cannot supply those inputs.
Do not add `reply`, ports, `returnValue`, raw credentials or native handles to this invoke event.

Make one terminal decision per request. A handler can return immediately, fulfill later, throw synchronously, or reject later.
Distinguish returned Error values from exceptions before using the value codec. A remote exception does not transfer its object identity.
After forwarding, retirement may race a real effect. Reject or discard stale delivery according to the existing owner. Never infer nonexecution from timeout.
Preserve operation identity across same-document hash changes. Never replay on reconnect, document replacement or unknown outcomes.

### D-5: Limit error disclosure and retained work

Propose the Electron-style outer message only for an admitted named call with a supported outcome.
For a missing handler, use the frozen missing-handler text. For an ordinary thrown Error, carry only approved bounded application message text and the approved class label.
For topic `calc`, freeze these candidate `error.message` fixtures against the actual Electron arm:

- Missing: `Error invoking remote method 'calc': Error: No handler registered for 'calc'`.
- Thrown ordinary `Error("marker")`: `Error invoking remote method 'calc': Error: marker`.

Do not derive the expected fixtures with the Keld renderer formatter.
B-1 must freeze exact public text, registered diagnostics, safe fallback and disclosure authorization for this route.
A message can itself contain application-sensitive data. This proposal does not promise to detect secrets inside approved application-authored text.
Keep trusted framework refusals, transport failures and application exceptions separate. Application text cannot mint an authoritative framework status or allow replay.
Never expose remote stacks, causes, arbitrary properties, paths added by the framework, or diagnostics from another call.
Never invoke application-defined `toString`, getters or coercion to render a thrown value. Refuse unsupported, hostile or oversized forms through a bounded redacted failure.
Do not log payloads as evidence. Use test-only markers and content hashes for fixtures.

Before admission, reserve a request record, all owned request copies and the selected return capacity under the corresponding shared owners.
Account for simultaneous encoded copies, delayed replies, timers, retained callbacks and pending Promise records. Share aggregate limits across eligible views/roles as approved by the owners.
Bound the encoded diagnostic, including topic escaping, class prefix and complete outer envelope. Reject excess without truncating into a valid but different error.
Do not create an unbounded wait queue, extend deadlines, or transfer capacity twice. Caller settlement does not prove native or endpoint quiescence.
Hold an unresolved operation's charges until owner-observed release or role teardown. Stop new admission when available capacity is exhausted.
Framework accounting excludes application-owned heap outside the transport. It does not establish a process-memory ceiling.

### D-6: Freeze the reference and product evidence separately

FACT: Electron v44.4.5 sources below were retrieved 2026-11-06 at commit `694f45852a0f1726cd23bfd379854de489cccb65`.

- [Registration](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/ipc-main-impl.ts#L15-L33): `this.removeHandler(method);` precedes the one-shot callback. Duplicate registration precedes type validation.
- [Main dispatch](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/ipc-dispatch.ts#L95-L122): `error.toString()` produces the error field. The source distinguishes missing handlers and thrown handlers.
- [Renderer](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/renderer/api/ipc-renderer.ts#L21-L27): `Error invoking remote method` prefixes the renderer error. The runtime transcript must freeze punctuation and quoting.
- [Invoke event](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/ipc-main-invoke-event.md#L3-L7): `May be` and `null` qualify `senderFrame` after navigation or destruction.
- [Renderer API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/ipc-renderer.md#L157-L164): `Structured Clone Algorithm` describes Electron serialization. Keld's approved subset and limits require separate dispositions.

FACT: The [Zettlr proof owner](https://github.com/gyldlab/keld/issues/1354) distinguishes app release v4.9.0, its Electron 44.9.1 lock, and the Electron 44.4.5 reference.
FACT: The [program owner](https://github.com/gyldlab/keld/issues/1358) selects macOS `keld dev` with explicit Legacy. GH1354 keeps `install` unknown pending KEL-141 and rejects a dev-to-install promotion. Retrieved 2026-11-06.
The following app sources were retrieved 2026-11-06 at `04128af42b58f7fea3f39f7b1efd597f5021f48a`:

- [Preload](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/common/modules/preload/index.ts#L39): `ipcRenderer.invoke(channel, ...args)` forwards dynamic topics. A public wrapper is not a grant.
- [Mount store](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/pinia/document-tree-store.ts#L185-L195): `retrieve-tab-config` starts for a non-null window ID. `get-file-modification-status` starts without awaiting it. Both catches log errors.
- [Initial main results](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/documents/index.ts#L292-L320): `toJSON()` supplies that window’s pane tree. The modification branch returns actual modified paths.
- [Open action](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/win-main/file-manager/util/item-composable.ts#L157-L169): `command: 'open-file'` accompanies the `documents-provider` invoke.
- [Main handlers](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/documents/index.ts#L270-L301): `case 'get-document':` and `case 'push-updates':` select genuine application operations.

Source inspection does not prove mount, overlapping invokes or document size support. Preserve all failed and unrun attempts.
A qualified small-payload mechanism test must not qualify a product document above the existing codec ceiling.

## 5. Boundaries

- Future facade/handler work belongs to the existing `@keld/electron` and app-preload owners.
- Coordinate native admission, Core forwarding, generated codecs and retirement with their designated writers. No shared file changes in this PR.
- Keep the numeric bridge, send/event paths, strict defaults, product scoring and unrelated OS work intact.
- Migrate the real preload, bridge parser, Host dispatch, Bun main handler and caller-return path in one approved integration sequence.
- Preserve permanent Electron facades. Remove any transitional adapter only when its named consumers migrate and its owner verifies the removal.

## 6. Tasks

### T-1: Resolve the owner decisions and register evidence first

- **Blocked by:** none
- Freeze independent Electron fixtures and a Zettlr demand inventory. Obtain separate test-only scope before native oracle runs.
- Register failing Keld conformance cells before implementation. Preserve true absent/unsupported dispositions when the route does not exist.
- Close Q-1 and B-1 with the registration, security, bridge and wire owners. Freeze each arm's expected public outcome.
- Obtain independent upstream evidence under B-2/B-3. This task does not require the future GH547 integration to exist.

### T-2: Integrate a disabled named request/return route

- **Blocked by:** T-1, B-1, B-2, B-3
- Build an isolated acceptance configuration. Keep production delivery disabled.
- Reuse the installed module, admitted preload, one parser, guard, generated carrier and private handler table.
- Run AC-1 through AC-8 with independent error, value, caller, handler and native owner witnesses.
- Run the numeric invoke and ordinary send/listener regression suites. Do not change their published error behavior.

### T-3: Qualify teardown, concurrency and recovery

- **Blocked by:** T-2
- Run AC-9 and AC-10, plus every T-2 control at the final integrated revision.
- Prove zero, exact and excess boundaries separately for records, bytes, diagnostics and return capacity.
- Publish actual native, Bun, Host and fixture identities and results. This task produces B-4 evidence.

### T-4: Qualify the development-stage mount

- **Blocked by:** T-3, B-4, B-5
- Launch the actual staged app via `keld dev` on macOS under explicit Legacy. Record stage/source, runtime, engine, policy and fixture identities.
- Run AC-12 before AC-11. Seed nondefault state through legitimate app setup, not substituted replies or direct renderer-store writes. Label any second-window/remount fixture separately from a fresh-profile launch.
- Record each initial correlation, both returned values, both store effects and both fault-arm outcomes. Then exercise the independent actual open/edit workload.
- Record the encoded reply size separately. Wrapper metadata cannot meet the text-size requirement.
- Run the large-text and push-updates cases after capacity qualification. Reject truncated or substituted document text.
- T-4 produces B-6 evidence for the development mode only. No installer is required. Keep scoring with the corpus owner.
- A staging pass cannot authorize installed activation or change the installation cell. Record `install: unknown` until its own qualification.

### T-5: Qualify the installed mode separately

- **Blocked by:** T-4, B-6, B-7
- Launch the authenticated installed application through its installed entry. Record its installation identity, executable, resources, runtime, engine and observed launch path.
- Repeat AC-12 and AC-11 with the real installed preload and handlers. Reuse expected fixture values, never staged receipts. Reject fallback to `keld dev`.
- No package or no installed run means no installed result. Enable only the tested mode after the production owner approves its exact implementation and evidence.

## 7. Test plan & evidence

| Criteria | Independent observable |
| --- | --- |
| AC-1, AC-3 | Exact synchronous error, public registration result, handler identity/effect counters, missing-handler result and healthy replacement |
| AC-2, AC-4 | Real encoded/decoded fixture values, actual renderer Promise state, caller and listener/handler entry counts |
| AC-5, AC-6 | Final Host guard decision, per-window effects, registry-backed wrapper identities, independent native frame retirement |
| AC-7, AC-8 | Recorded reference text, own error properties, endpoint failure-byte sentinel check and public result discriminator |
| AC-9, AC-10 | Ordered Host/native/Bun barriers, first-terminal transcript, retained-object census, one healthy successor |
| AC-11 | Stage or installed provenance, actual document-text length/digest, twenty update replies, final edit and Host edited flag |
| AC-12 | Non-null window, overlapping initial correlations, both distinct handler replies and both store effects before refresh |

Observe each order with barriers, never sleeps. Run every single-fault variant in isolation and restore the positive fixture.
Test malformed, oversized, hostile-coercion and unexpected-terminal inputs before enabling a route. Retain unrelated working correlations.
Exact executable commands belong to the implementation receipts once the fixtures exist. A source table is not a substitute.

### E-1: Pinned source inspection

- **Run:** Inspected the cited immutable main, branch, Electron and Zettlr sources through GitHub on 2026-11-06.
- **n:** One main pin, five referenced candidate PRs, one Electron commit, one Zettlr commit. Zero executions.
- **Environment:** Cloud documentation workspace. No native operator, application, Host, Bun or Electron test process.
- **Limits:** Facts establish source shape and upstream contracts. No test fixture, native timing or interoperability result is claimed.

### E-2: External semantic discovery

- **Run:** Queried Context7 `/electron/electron/v44.4.5` for invoke, handleOnce, errors and sender-frame lifetime. Checked the official pinned primary sources in D-6.
- **n:** One discovery query and source inspection. Zero differential runs.
- **Environment:** Electron 44.4.5 documentation and release source, retrieved 2026-11-06.
- **Limits:** The supported Error subset, actual engine error renderings, native identity timing and Zettlr mount remain unqualified.

## 8. Review gates triggered

- Public API: invoke results, registration divergence, event identity and failure text require exact-content approval.
- Permission model: method activation, sender identity, literal authorization and bounded application error disclosure require security review.
- Wire protocol: named request/terminal amendments require review by the canonical bridge and carrier owners.
- Dependency addition: none proposed.
- Unsafe: none proposed. Any new native unsafe operation requires separate exact-operation and lifetime review.

## 9. Perf impact

This document has no runtime effect. Measure end-to-end latency, event-loop progress, retained bytes and active records separately in future work.
Use the same pinned workload, architecture budgets and delivery trace. Report compiler/runtime/OS versions and error cases, not just a successful median.

## 10. Open questions, blockers & unknowns

### Q-1: Scope of compatibility and disclosure

Approve or revise D-2 through D-5. Recommend bounded ordinary Error messages only, with explicit divergences for unsafe error forms and registration diagnostics.
Require the existing owners' exact activation contract before any named route can forward. Do not imply that send authority alone permits invoke.

### B-1: Exact specification and protocol approval

- **Status:** open
- **Owner:** Monish Khandelwal with KEL-142, GH466, GH540, GH541/GH552, GH544 and error-registry owners.
- **Impact:** Blocks T-2, privileged delivery, error-text disclosure and any new public or wire behavior.
- **Check:** Resolve Q-1. Approve exact source, trigger semantics, grants, field layouts, mode/version, error subset, bounded fallback, oracle fixtures and migration order.

### B-2: Producer contracts and capacity inputs

- **Status:** open
- **Owner:** GH466/GH467/GH540/GH544/GH546 and aggregate transport owners.
- **Impact:** Blocks T-2 until compatible producer interfaces and proven bounds exist, including concurrent request and return reservations.
- **Check:** Pin the approved upstream registry, exposure, codec, base carrier and capacity contracts. The named extension itself belongs to T-2. Verify their independent producer tests without requiring GH547 handler delivery.

### B-3: Native identity and event facts

- **Status:** open
- **Owner:** GH542 and WebContents/window registry owners with the macOS operator.
- **Impact:** Blocks T-2, authenticated event construction and stale delivery claims.
- **Check:** Provide independently qualified source/window/realm/frame/role associations and retirement evidence. Do not require a GH547 handler effect to establish these inputs.

### B-4: Integrated native mechanism acceptance

- **Status:** open
- **Owner:** GH547 implementation owner with native, transport and security reviewers.
- **Impact:** Blocks T-4 and mechanism compatibility claims. It does not block disabled T-2/T-3 construction.
- **Check:** T-3 records AC-1 through AC-10, all single-fault controls, both retirement orders, healthy recovery and independently measured retained-work bounds.

### B-5: Development-stage run preconditions

- **Status:** open
- **Owner:** GH1354 product owner with the macOS operator, preload, profile, document and capacity owners.
- **Impact:** Blocks T-4 and development-stage mount claims. It does not require packaging or block isolated mechanism qualification.
- **Check:** Pin the actual `keld dev` stage, explicit Legacy profile, loader, non-null window, initial pane/modification markers and observers. Qualify concurrent calls, the complete document result and update workload.

### B-6: Development-stage qualification

- **Status:** open
- **Owner:** Monish Khandelwal with GH1354, security and compatibility owners.
- **Impact:** Blocks any development-stage compatibility claim and T-5 entry. It does not block T-4 or imply installed success.
- **Check:** T-4 passes AC-12 and AC-11 on the exact approved development stage. Record divergences, all attempts and the scope. The package/install cell remains independent.

### B-7: Authenticated installed artifact and launch prerequisites

- **Status:** open
- **Owner:** KEL-141 packaging and GH1354 product owners with the macOS operator.
- **Impact:** Blocks only T-5 and installed claims. It does not block T-4, B-5 or B-6.
- **Check:** Supply a verifiable installable artifact, installation procedure and independent launch-provenance observer. This prerequisite does not require GH547’s installed mount to pass. T-5 produces that later evidence.

### U-1: Product pressure and real retirement orders

UNKNOWN: Actual mount concurrency, request/return sizes and native release timing at the target implementation are unmeasured.
Measure the pinned app with the admitted fixture. A codec microtest, hypothetical grant or another operating system cannot close this unknown.
