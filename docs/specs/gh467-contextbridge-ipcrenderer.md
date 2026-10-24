# Spec: macOS draw.io contextBridge and ipcRenderer first-proof facade

Status: draft
Linear: GH-467 (#517 GitHub tracker of record; KEL-127 reference) · Owner: @0monish · Updated: 2026-10-24

> **Decision gate:** This is a proposed specification, not approved implementation.
> No app-facing API, third-world injection, security grant, native qualification,
> or compatibility PASS is claimed by the existence of this text. The exact reviewed
> spec must receive the issue's independent approval before implementation.
> Merge intent: **do-not-merge** until that gate and all integration owners pass.

## 1. Goal & non-goals

Make the pinned **unmodified** draw.io desktop preload expose its narrow page
API through Electron-like `contextBridge.exposeInMainWorld` and exchange
`rendererReq` / `mainResp` messages with the actual main-role listener using
`ipcRenderer.send/on/once`, on physical macOS WKWebView. A successful first
proof is **two sequential unique `isPluginsEnabled` requests**, each observed
at the page callback, actual main handler, original document and independently
released transport reservation. Include the nested long-lived `watchFile`
callback and frozen exposed data behavior; protect KELD's separate trusted
renderer bridge with default-deny Host grants.

This issue owns the **application-level facade and cross-world value behavior**,
not the WebKit application-preload loader. GH-484 owns the loader, world selection,
handler and source containment; GH-540 owns transport and credits. Landing a
successful JS unit test or a synthetic response is not sufficient.

Non-goals for this first-proof slice:

- General Electron API support, `ipcRenderer.invoke`, `sendSync`, synchronous
  or function-valued **cross-world returns**, `contextBridge.executeInMainWorld`
  or `exposeInIsolatedWorld`, arbitrary Promise crossing, Node integration or
  renderer native OS grants.
- Expanding the approved GH-544 IPC value codec, cloning arbitrary
  Map/Set/TypedArray/BigInt/cyclic object graphs, or transferring MessagePorts.
- Native window registry, Ready lifecycle, all browserWindow methods, broad
  plugin/open/save/close UI journeys, unconstrained IPC burst traffic, larger
  than the GH-540 admitted 4 KiB first-proof subset, or speed claims.
- A second KIPC link, ad-hoc renderer protocol, JS-side authority checks as
  substitutes for Rust Host enforcement, or an automatic legacy profile.
- Windows/Linux parity. Their observed cells remain UNKNOWN.

## 2. Spec refs and provenance

| Normative owner | Consumed contract, no duplicate owner |
| --- | --- |
| [KEL-142](kel142-macos-renderer-bridge-api.md) / architecture [05](../architecture/05-webview-and-native.md) ``2–3 | Existing **trusted** named world + frozen `window.keld.invoke` page surface. New app world needs a *separately approved* amendment under GH-484. |
| [GH-441 option A](https://github.com/gyldlab/keld/issues/441#issuecomment-6022098697) and [GH-484 handoff](https://github.com/gyldlab/keld/issues/484#issuecomment-6381705299) | Page / application preload / KELD trusted worlds; App preload only, main frame, at document start. |
| [GH-540 spec PR #1122](https://github.com/gyldlab/keld/pull/1122), `docs/specs/gh540-compat-ipc.md` at `3d72e492916259d89b1bf4723329e602ea6e1691` | Approved-but-unmerged **bounded** one-`compat-control` message contract, host-opaque values, original/current document destinations, terminal consumer + release receipts, single application link. |
| [GH-541 spec PR #1123](https://github.com/gyldlab/keld/pull/1123), `docs/specs/gh541-channel-grants-navigation.md` at `72266ab2383403db3ec201d8388df66a2945862d` | Approved-but-unmerged grant/navigation **spec only**: one checked u32 host navigation source, native application-document eligibility, byte-exact `windows.<w>.channels`, final `dispatch_privileged`. Actual GH-542/GH-552 implementations remain prerequisites. |
| [Architecture 02](../architecture/02-ipc.md), [03](../architecture/03-security.md), [04](../architecture/04-electron-compat.md) | Canonical link/guard boundary, explicit compatibility subset/divergence score, default deny. GH-540/GH-541 own proposed amendments. |
| [GH-464](https://github.com/gyldlab/keld/issues/464), [GH-567](https://github.com/gyldlab/keld/issues/567), [GH-532](gh532-first-proof-evidence-rules.md), [GH-566](gh566-corpus-manifest-owner.md) | Pinned conformance cells, expected-fail baseline before implementing, implementation-ticket keys, digest/validator/negative-control admission. |
| [GH-465](https://github.com/gyldlab/keld/issues/465), [GH-543](https://github.com/gyldlab/keld/issues/543), [GH-544](https://github.com/gyldlab/keld/issues/544), [GH-466](https://github.com/gyldlab/keld/issues/466) | Native one-way send, addressed events, sole endpoint IPC codec and ipcMain/webContents owner. |
| [GH-473](https://github.com/gyldlab/keld/issues/473), [GH-449](https://github.com/gyldlab/keld/issues/449), GH-452/GH-570 | Authentic committed `file://` and CSP, existing keyed window/facade, canonical process facts. |
| [GH-1095](gh1095-request-error-codes.md), [KEL-74](kel74-compat-evidence-schema.md) | Typed failure/settlement owner, no result or compatibility-score laundering. |

Pinned upstream oracle: Electron **v44.4.5**
`694f45852a0f1726cd23bfd379854de489cccb65`, notably
[contextBridge documentation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/context-bridge.md),
[native converter](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/renderer/api/electron_api_context_bridge.cc),
[upstream cases](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/spec/api-context-bridge-spec.ts),
and IPC documentation. Source blob hashes observed:
context-bridge.md `d5bddc8526c0cf7d60f5efdb24547e9af4646a62`;
converter `ade25537bc54a242f772461d946524ad0f396fef`;
spec `c549e304c7f1fc0f945317157321bded28748b2e`.

Pinned draw.io desktop
`2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`,
[unmodified preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js)
(blob `44f2f22f159a8a845ff7e573dd10b816383703ec`)
and webapp submodule `24b76c2cbd55d88e354042e8d329a2e4708972bc`.
Preload installs `ipcRenderer.on('mainResp')` and
`ipcRenderer.on('fileChanged')` before exposing `electron` and `process`;
`request` *mutates* `msg.reqId`; `watchFile` retains and deletes
`msg.listener` before `send('rendererReq', msg)`.

Working baseline: KELD origin/main
`b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50`, 2026-10-24.
`packages/@keld/electron/src/index.ts` currently exports the
main-role app/link, not a renderer-side contextBridge or ipcRenderer.
Existing native `keld-wv` bridge is KEL-142 v1; its page facade is not
an Electron preload runtime.

**Physical Mac F0 evidence update (2026-10-24; not product acceptance):**
Real WKWebView on macOS 26.5.1 executed the **unmodified pinned draw.io
preload** in a named app world under a deliberately **test-only**
contextBridge/ipcRenderer shim. Eager page-world API publication was
unreliable (each of two injection-order variants passed only 4/8 runs).
An order-independent, synchronous, two-way document-start publication
prototype exposed the four app-registered methods and copied, frozen
process metadata at the first inline script **16/16** times; suppressing
the `electron` registration removed them **3/3** times. A bounded
local page-to-preload proxy then invoked the actual unmodified
`request` method, observed preload-mutated `reqId=1,2,3` in a
**simulated** send log, and delivered two nested `watchFile` callbacks
in **6/6** runs. Omitting only nested callback revival removed those
two change events, leaving ordinary callbacks intact (**6/6** negative
controls). The independent native findings and source hashes are
recorded on [#484](https://github.com/gyldlab/keld/issues/484#issuecomment-6382401714)
and [#467](https://github.com/gyldlab/keld/issues/467#issuecomment-6382468009).
These fixtures use page-visible, forgeable DOM metadata/events and
synthetic `mainResp` values; **they do not prove an approved bridge,
native Host identity/grants, Bun handlers, transport release, actual
application round-trip or Electron parity**.

## 3. Acceptance criteria — binary observables

| ID | Given / when | Independently observed pass condition |
| --- | --- | --- |
| C01 | GH-484 installs the real app-world preload for a new main-frame document | Before the **first inline page script**, both `window.electron` and `window.process` expose the accepted members; the page has no app-world globals or KELD native handler. Merely seeing a preload marker is insufficient. |
| C02 | Preload exposes a string-keyed function/data graph | Exposed primitive/array/ordinary-object data are copied; page graph is recursively frozen and non-writable/non-configurable at the top-level key. Mutating page data does not affect preload data and vice versa. |
| C03 | Page invokes a proxied preload function with an ordinary object | The preload receives a **mutable copied object**, can assign `msg.reqId` and delete `msg.listener`; page's original object remains unchanged. Freeze-on-all-crossings fails. |
| C04 | A callback is nested in an object argument (`watchFile`) | Preload retains the nested callback, deletes it from outgoing IPC data and invokes it N times with N exact distinct values in issue order. No callback appears in IPC codec bytes. |
| C05 | Page registers functions via `request`, `registerMsgListener`, `sendMessage`, `listenOnce` | All four exist as callable **void-return** preload proxy methods. Callback/exception observation belongs to the page; no synthetic returned cross-world Promise qualifies. |
| C06 | Exposed API contains nested functions | Every admitted function is callable through a document-bound proxy in either direction; repeat calls and same-reference argument behavior are tested against upstream fixtures. |
| C07 | Page accesses error-copy row | A supported Error crosses with correct name/message class and sanctioned stack behavior; no internal app-link/native secrets or process paths leak. The pinned row and unknown divergence are recorded accurately. |
| C08 | `ipcRenderer.send` gets admitted channel and supported values | Local return is `undefined`, argument array snapshots before async admission, actual named topic reaches one real `ipcMain.on` listener through Host; no numeric invoke or second transport. |
| C09 | `ipcRenderer.on/once/off/addListener/removeListener/removeAllListeners` manage listeners | Registration order, duplicate handling, reentrant once-before-callback removal, original-listener removal, zero-late-delivery and exception cleanup match pinned cases for supported domain. |
| C10 | An inbound main event is delivered to preload | `IpcRendererEvent.sender` is the preload `ipcRenderer` facade; `ports` is empty for no-transfer first proof; it is not a raw native event, WebContents or transport authority. |
| C11 | Exact literal window grant absent | Final Host guard records an **internal** typed `KELD-GUARD003` refusal before any Bun frame/listener effect; the public `ipcRenderer.send` call still returns `undefined` and never gains a retroactive exception, event or Promise. Healthy explicitly granted neighbor still succeeds. No wildcard, case-fold, URL decoding or alternate grant. |
| C12 | Renderer, frame, document or Bun generation is stale, foreign or destroyed | Original-target callbacks/events and reservations settle once without delivery to successor. Same-document/hash and cancelled renderer provisional navigation preserve still-live document callbacks. |
| C13 | Preload has begun before first native application-document qualification | Page can observe only the exposed API. Native forwarding remains closed until GH-542 qualifies the document and Host link; no preload-supplied URL, nonce, window index or fresh binding bypasses eligibility. |
| C14 | Page calls unmodified draw.io `window.electron.request` twice for `isPluginsEnabled` | Two **distinct sequential** `reqId` values, original page, actual live `rendererReq`/`mainResp` handlers, main outcome and page callback match the pinned application path; second starts only after callback **and independent native/transport release** of first. Canned `false`/direct callbacks fail. |
| C15 | Actual file URL page participates in send/reply | Native committed `file://` document URL, approved package-resource origin and `senderFrame.url` match; a custom-scheme rewrite or guessed `file:` string cannot satisfy eligibility. |
| C16 | Application registers a listener after an earlier unsolicited event | Earlier event is not replayed. A later matching event is delivered once; no limitless hidden event queue. |
| C17 | Preload page navigates, reloads, closes or Bun role recovers | Document-owned proxy/listener retention terminates for a retired document, surviving doc API remains only if host lifecycle admits it, old queued calls do not replay or retarget, other WebViews stay isolated. |
| C18 | Attempts to pass `Function`, `Promise`, `Symbol` or other disallowed values through **IPC** | GH-544's supported IPC domain is enforced synchronously with no Bun write; a *local contextBridge function proxy* must not be misclassified as an IPC payload. |
| C19 | Unsupported/unknown Electron API or a graph beyond the accepted subset is requested | Explicit typed KELD-COMPAT unsupported/refusal rather than a silent no-op, arbitrary Node module, infinite graph, leaked native handle or fabricated parity PASS. |
| C20 | Compatibility is scored/published | Every mapped GH-464 row has pinned source, expected verdict, implementing-ticket key and recorded device. Missing/untested API stays expected-fail; observed-only cross-navigation or Symbols remain UNKNOWN until proper oracle. |
| C21 | A hostile same-origin page probes preload or KELD internal handlers | It cannot read/call privileged KELD handler, role endpoint, native stamp, host grant, raw ipcRenderer or Bun socket; allowed page wrappers remain usable and cannot increase authority. |
| C22 | Electron v44.4.5 target is used | The doc-versus-native Symbol discrepancy is resolved by **pinned executable** oracle or an explicitly approved documented divergence before claiming the Symbol cell PASS (GH-464 has same gate). |

The real-KELD pass for C01/C12/C13/C14/C15/C21 is on **macOS
WKWebView** only, with exact build, source hash and native outcome. A mock,
source inspection or green baseline test cannot substitute for it.

## 4. Design

### 4.1 First-principles atoms; existing owners win

| Atom | Owner; input → output | Failure mode; independent falsifier |
| --- | --- | --- |
| App-world boot | GH-484: admitted package-root preload bytes + host WebView → named isolated app-world script | Late API exposure; early inline page probe RED. |
| Value transfer | **GH-467**: page ↔ preload in *same renderer*, finite copied data / proxied callable slots | Freeze incoming arguments or proxy only top-level callable → unmodified draw.io mutation/callback RED. |
| Listener state | **GH-467**: preload registration / native event → ordered callback dispatch | `once` remains live across recursive emit, duplicate removal mismatch → exact listener transcript RED. |
| IPC encode | GH-544: endpoint arguments → immutable host-opaque bytes | JSON approximation or serialized nested function → golden-vector / zero-effect RED. |
| Send/receive | GH-540/465/543/466: validated bytes + native Host stamps → Bun listener and addressed renderer event | Missing credit, forged route, wrong direction, wrong original document → ingress, release and sentinel RED. |
| Authentication & identity | GH-542/KEL-142: actual native controller/world/frame/committed doc → host eligibility; GH-449 registry → window | App may self-select a new doc/window/nonce → zero Bun effects control RED. |
| Authorization | GH-541/552: immutable host-grant snapshot → final `dispatch_privileged` Allow closure | Prefix/wildcard or pre-queue-only check → guarded effect control RED. |
| Lifecycle/retention | GH-467 document-scoped proxy slots, GH-540 canonical queues, GH-542 native retirement | Old callback crosses into successor or reservation frees prematurely → exact old/new document marker RED. |
| Evidence | GH-464/567/532/566: oracle + fixture + mutant → fail/pass/unknown | Missing API marked PASS or test replaced by canned result → manifest/oracle admission RED. |

Edges: GH-484 world + C01 ready publication feeds GH-467 but cannot
authenticate Host ingress. GH-542 native admitted-document eligibility
precedes all granted Host forwarding. GH-540 single transport and GH-544
codec are not world-object copy mechanisms. Physical resource release is
separate from application callback observation; both precede the second
controlled request.

Reuse before rewrite: `packages/@keld/electron` is the facade package;
`@keld/kipc` is the one authenticated role link;
`keld-wv` is the single native WebKit registration owner; `keld-guard` is
the permission source; GH-544 supplies one TS value codec to both endpoints.
No generic bridge rewriting these primitives is justified.

### 4.2 JavaScript worlds and authority

There are exactly three logical JavaScript world roles:

1. **Page world**: untrusted draw.io HTML and potentially hostile same-origin
   JS; may access only the explicitly published `window.electron` and copied
   `window.process` objects. No Node, raw message handler, host bind or grant.
2. **App preload world**: app-supplied sandboxed preload via GH-484
   `WKContentWorld`, with a **separate** world-scoped handler. It can call
   only the GH-540 compat route admitted by Host and GH-541/542/552. It cannot
   call the KEL-142 trusted renderer handler.
3. **KELD trusted bridge world**: existing KEL-142 handler/private nonce
   and `window.keld.invoke` support. Not visible to app preload or page.

WebKit content-world separation isolates globals, **not shared DOM or page
storage**. DOM state cannot authenticate the message sender, an authorized
navigation, an app principal or a listener. App-world code must not gain
privileged authority merely by executing earlier than the page.

GH-484 owns document-start insertion and the fundamental cross-world
publication primitive. A late `evaluateJavaScript` Promise is **not** a
valid proof of first-inline availability; `window.postMessage` is not a
permitted substitute for native authenticated transport. GH-467 consumes
only a qualified no-cross-world-authority API publication primitive
and must not install another native handler/controller.

### 4.3 Proposed bounded contextBridge graph

This is an explicitly **narrow first-proof** subset, not a promise to
implement all v44.4.5 types. Supported exposed graph leaves:
`null`, `undefined`, booleans, strings, finite numbers, ordinary arrays
and plain own-enumerable string-key objects with supported leaves, and
functions as proxied callable leaves. Errors require the separately
mapped C07 copy rules; unsupported platform/exotic objects fail
explicitly rather than silently stringify or read OS state.

- `exposeInMainWorld(apiKey, api)` accepts an approved string key; exact
  `electron` and `process` from unmodified draw.io are required. Rebinding a
  non-configurable preexisting page-owned key fails; do not overwrite a
  hostile page key, use a hidden symbol as authority, or leak the preload's
  original mutable object.
- For the **exposed graph**, copy primitives/data and recursively freeze
  arrays/objects **in the page**; top-level published `window[apiKey]`
  is read-only/non-configurable. Mutating either original after exposure
  cannot mutate the other. Function leaf becomes a document-scoped
  callable proxy, without leaking the source function or JS realm.
- For **arguments** to a proxied method (including the `msg` object),
  copy data in the **receiving preload world** but **do not freeze this
  ordinary argument copy**. Retain a nested callback as a bounded proxy
  slot. Existing draw.io `request` writes `msg.reqId`, removes
  `msg.listener` in the watch-file path and sends the resulting data only.
  A blanket frozen-copy utility would violate C03/C04.
- Proxies never transport executable JS or callback identity across the
  Bun/Host value codec. No original Function constructor, prototype
  handle, accessors, private native globals, app-role linkage or
  live reusable function address is exposed to page script.
- Reentrant callback dispatch is serialized on the owning document's
  JavaScript executor. Exact same-document function identity, repeated
  arguments and return-error copying are compared against pinned
  upstream tests; any supported equality behavior has a documented
  per-document proxy cache/ownership.
- Close/destroy/reload retires every proxy slot of its document and
  settles outstanding registered callbacks at most once where applicable.
  A `watchFile` callback intentionally remains registered for multiple
  events *while the document is live*. Do not free it after its first
  callback or let the framework retain it after document retirement.
  Application-owned `reqInfo` and `fileChangedListeners` remain in the
  unmodified preload; framework cap/retirement is not a claim to rewrite
  that application code.
- The exact depth/member/byte/proxy quotas, rejection error and
  accessor/exotic-object policies are **OPEN for spec approval** (§10).
  They must be finite and tested by 0/max/max+1; neither an undocumented
  hard-coded limit nor a permissive unlimited graph is acceptable.

### 4.4 Symbol conflict — two distinct boundaries

Electron v44.4.5 [documentation table](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/context-bridge.md)
says Symbols are dropped. The pinned [native converter](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/renderer/api/electron_api_context_bridge.cc)
handles primitive `value->IsSymbol()` directly, and the pinned
[upstream spec](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/spec/api-context-bridge-spec.ts)
tests equality and symbol-key lookup (examples near tests "should proxy
symbols"). **Both primary sources are real and contradictory.**
No binary run has been performed by this task; the cell is **UNKNOWN**.

Do **not** replace GH-467/GH-464's existing "Symbol dropped"
conformance wording with a fabricated native parity claim. Qualify
the executable v44.4.5 oracle using the exact source/test fixture; then
amend #467/#464 to the observed semantics, or explicitly approve
a KELD first-proof deviation and record it in the compatibility score.
This does **not** change GH-544's **IPC** value-domain rejection of Symbol.

### 4.5 API / event and endpoint sketch

The implementation language remains TypeScript, but the cross-world
bridge must be qualified by actual WKWebView/JavaScriptCore semantics,
not assumed to behave like Electron V8. These are shape sketches, not
an extra checked-in type registry:

~~~typescript
type FirstProofValue =
  | null | undefined | boolean | number | string
  | readonly FirstProofValue[]
  | { readonly [name: string]: FirstProofValue };

type RendererListener =
  (event: IpcRendererEvent, ...args: FirstProofValue[]) => void;

interface IpcRendererEvent {
  readonly sender: IpcRendererFacade;
  readonly ports: readonly []; // no transferred ports in this slice
}
interface IpcRendererFacade {
  send(channel: string, ...args: FirstProofValue[]): void;
  on(channel: string, listener: RendererListener): this;
  once(channel: string, listener: RendererListener): this;
  off(channel: string, listener: RendererListener): this;
  addListener(channel: string, listener: RendererListener): this;
  removeListener(channel: string, listener: RendererListener): this;
  removeAllListeners(channel?: string): this;
}
interface ContextBridgeFacade {
  exposeInMainWorld(key: string, api: unknown): void;
}
~~~

A TypeScript `FirstProofValue` cannot encode callback leaves itself:
function slots are a **local cross-world** graph extension and are
excluded from the GH-544 wire domain. Actual code must use bounded,
well-typed internal graph tags and refuse unsupported values; it may
not use an unconstrained public `any` or a generic JSON stringify.

The page-visible draw.io API is exactly the four function names
listed in `3; a copied `process` object contains **only** copied
`type` and `versions` published by the original preload. KELD must
not inject a raw `ipcRenderer`, Bun path, app-link socket or grant
into the page. The app preload's `require('electron')` resolves to
GH-484's sandbox registry, not the desktop main-role package import.
Unknown modules and members use the approved GH-484 typed failure
policy, never a fake success.

### 4.6 Transport integration and real application witness

Outbound app-world `ipcRenderer.send(channel,...args)`:
1. Before async yielding, GH-544 snapshots supported argument values
   and applies local rejection as specified by the approved codec.
2. GH-540's single canonical link/native v2 envelope accounts for the
   framework-owned reservation; exact `el:` channel name construction
   happens once, and only native Host captures window/document/Bun stamps.
3. GH-552 Host `dispatch_privileged` checks the literal grant at final
   handoff; denial has zero forwarded frames, even if syntactic parse
   and a queue admission succeeded.
4. GH-466 `ipcMain.on` receives the decoded endpoint values with
   authentic Host-stamped `senderFrame`; handler uses original draw.io
   `rendererReq` multiplexer, not a hand-authored special-case stub.

Incoming main event:
1. GH-543 `DeliverToOriginalDocument` for reply or
   `DeliverToCurrentMain` for `webContents.send` captures an explicit
   target; neither permits successor retargeting.
2. GH-540/543 hold the physical reservation until consumer disposition
   **and** native/ring release, with separate bounded CONTROL capacity.
3. The current preload's `ipcRenderer.on('mainResp')` listener receives
   `(event, resp)`, finds `reqInfo[resp.reqId]`, calls real stored page
   callback, deletes the entry and completes the independent release
   barrier. The page callback is not a transport receipt itself.

Run request 2 only after both callback and release receipt from
request 1. Record the unmodified page caller, preload, main handler,
native committed file URL, original navigation stamp, Bun generation,
actual bytes/response and two distinct request IDs. An expected
`false` result alone says nothing about transport. The controlled
≤4 KiB case does not prove burst/backpressure or the larger draw.io
open/save corpus (GH-545).

Listener ownership is per document. `once` removes before invocation,
`removeListener` supports original references, registration order is
preserved for one endpoint, and a listener added after the event cannot
receive past events. Exception handling must release framework-owned
reservations even if an app listener throws, without claiming to catch
or swallow the application's exception as an API semantic. Handler
dispatch and callback execution must not block the Host UI thread.

### 4.7 Navigation, loss and failures

The current KEL-142 v1 bridge has a separate wrapping u64 counter;
it must **not** become a second Electron authority source. Consume
GH-542's **one checked u32 host-owned generation** and qualified
native document bind (GH-541 `4.3/N11). Same-document hash changes,
cancelled provisional renderer navigation and surviving document
across Bun reconnect keep document identity but do not automatically
replay any pending request. Trusted Host full-document loads revoke
early, then matching native commits advance as specified by GH-541.
Actual committed replacement, destroy and generation exhaustion are
terminal for the old document.

Typed values/errors are handled by their existing owners: GH-1095
for settlement/result shape, GH-540 for terminal disposition /
physical release, GH-544 for codec-local throw. The accepted
`ChannelForbidden` guard denial follows GH-541, not a JS-coded
secondary evaluator. No raw path, policy digest, socket, KIPC
correlation, native nonce or scoped grant appears in page diagnostics.

The **first-inline/first-qualified-bind race** is a blocking
integration experiment shared with #484/#542: exposing a callable
page-world API before the native committed-app document is qualified
does not permit forwarding. If the real app sends during that interval,
freeze an explicit selected bounded ready/failure contract with #540/
#542 before enabling channels. Never invent unbounded pre-bind
buffering, silent loss, or an app-selected nonce.

### 4.8 Runtime seam, configuration, migration

- Before runtime: GH-484 reads the admitted preload from an actual
  retained package-root handle; GH-449 creates a keyed Host WebView.
  GH-473 loads the original `file://` webapp/asset URL, preserving
  the committed address used for senderFrame.
- Config capture: one immutable Host window grant set and one
  `additionalArguments` vector for that window. Runtime policy is
  a snapshot, not mutable ambient page JS.
- During execution: all app callback references belong to one
  document's JS lifecycle; native KELD bridge never hands app
  preload direct OS authority. Bun can die independently; link
  lifetime/failure and final channel gate remain Host-owned.
- After execution: retirement releases document callback table and
  all corresponding native reservations, without consuming another
  document's active credits or host/window lifecycle events.
- No new OS grants, manifest keys, raw wire-frame format or
  transport/socket is introduced *by GH-467*. GH-540/541's reviewed
  contracts must land first at their own gates. Any proposed change
  to shared wire/grants requires those exact owner approvals.
- Implement in `packages/@keld/electron` renderer compatibility
  modules built from one source used by the GH-484 preload injector;
  do not hand-code a second runtime bundle. The end-to-end test
  exercises original draw.io preload/webapp/main, not a rewrite
  of either. Removed/temporary adapters are forbidden unless
  named with a single removal criterion.
- Windows WebView2 / Linux WebKitGTK require later engine-specific
  measurements and product tests; absence is explicitly UNKNOWN.

## 5. Boundaries and path ownership

- **This spec PR changes only**
  `docs/specs/gh467-contextbridge-ipcrenderer.md` and generated
  documentation *if* the generator already includes this source.
- Future GH-467 implementation may add scoped renderer runtime files
  under `packages/@keld/electron/src/renderer/` and focused
  `packages/@keld/electron` integration tests, after the approved
  spec and one producer/consuming interface review.
- GH-484 alone owns `crates/keld-wv/src/wkwebview/macos_bridge.rs`,
  native injection, content-world naming, sandbox require map,
  package-root preload bytes and KEL-142 isolation re-run.
- GH-540/465/543/544/466 own their protocol/codec/main-listener
  changes; GH-542/552 native navigation/guard. Do not write
  `crates/keld-ipc`, `keld-guard`, shared schema, KEL-142,
  architecture 02/03, current window registry, or Electron upstream
  files from this task.
- If matching C01/C13 semantics require architecture 05/KEL-142
  amendment, GH-484 owns it; GH-467 provides a consuming
  acceptance assertion, not a second amendment.

## 6. Ordered tasks — separate narrow PRs

- [ ] **T1 — current draft:** publish this GH-467 specification with
      existing contracts and exact approval questions. Independently
      review and STOP for the required approval. No product source.
- [ ] **T2 — corpus prerequisite:** GH-464/GH-567 land the pinned,
      mapped **expected-fail** contextBridge/Event/API and preload
      rows, including a baseline test that **passes by observing
      missing APIs**, with one fault mutation per decision atom.
      No fake missing-API PASS.
- [ ] **T3 — local facade behavior:** GH-467 implements the approved
      finite graph copy/proxy, readonly API publication/argument
      mutability and callback registries with tests that fail on
      broken freeze, nested proxy or premature callback retirement.
      Depends on GH-484's accepted no-late-publication primitive.
- [ ] **T4 — listener/composition:** implement the approved
      renderer `ipcRenderer` façade against the sole GH-540 transport,
      accepted GH-544 codec and GH-465/#543/466 events.
      Include once/remove and no-listener/exception negative controls;
      do not change channel grants.
- [ ] **T5 — real macOS application proof:** after accepted GH-484,
      GH-473, GH-542/552 and owner codec/transport artifacts,
      run the unmodified draw.io two-request and watchFile fixtures
      on WKWebView, anti-stub mutations and KEL-142 hostile-world
      requalification. Update corpus PASS only on independent evidence.
- [ ] **T6 — bounded handoff:** remaining Electron behavior
      outside first-proof stays UNKNOWN/explicit divergence and
      receives the existing owner, not a speculative extra shim.

No GH-484 ↔ GH-467 cycle: GH-484 may deliver its world/loader
substrate with its own first-inline native fixture **without requiring
the final draw.io round-trip**; GH-467 consumes it and owns the final
actual application witness. Do not close GH-484 by falsely claiming
GH-467's post-integration criterion.

## 7. Test plan and negative-control matrix

All tests are mapped to named ACs. A control qualifies only when
changing **one named faulty behavior** makes the test fail on the
mutant, and a clean implementation then passes. No sleep-based
readiness, unchecked green CI or mock Host counts as native proof.

| Test / control | AC | Fault inserted; independent expected failure |
| --- | --- | --- |
| `BRG001_first_inline` (real WK) | C01,C13 | Delay page API installation until after first page inline script → RED; early API still has zero pre-bind Bun effects. |
| `BRG002_exposed_freeze` | C02 | Remove recursive freeze / top-level read-only property → page mutation observable. |
| `BRG003_mutable_arg` | C03 | Freeze receiving argument object → draw.io `reqId` assignment / listener deletion fails. |
| `BRG004_nested_watch` | C04,C06 | Proxy only top-level functions → nested callback absent; sequence test RED at second differing payload. |
| `BRG005_void_return` | C05 | Add fabricated cross-world Promise or omit exposed method → exact shape/return RED. |
| `BRG006_error_copy` | C07 | Return raw Error/native path or silently null → name/message/security-cell RED. |
| `BRG007_send_snapshot` | C08,C18 | Mutate page object after send before Host handoff → transmitted bytes mismatch; Function/Symbol send must throw before Bun. |
| `BRG008_emitter_once` | C09 | Remove once listener *after* callback and recursively emit → duplicate event RED. |
| `BRG009_emitter_remove` | C09,C16 | Ignore original listener identity or buffer no-listener events → unmatched replay RED. |
| `BRG010_event_shape` | C10 | Send raw WebContents/Host object as event.sender → facade equality/authority RED. |
| `BRG011_final_guard` | C11,C21 | Replace byte-equal literal match with prefix/wildcard → unauthorized Bun write observed. |
| `BRG012_stale_document` | C12,C17 | Drop addressed-document stamp and target newest page → successor sentinel RED. |
| `BRG013_precommit_gate` | C13 | Treat early app-world publication or `file:` string as committed eligibility → forbidden handler count RED. |
| `BRG014_twice_real_app` | C14 | Stub `false`, bypass main handler or duplicate reqId → actual receipts/handler count RED. |
| `BRG015_release_barrier` | C14 | Start request 2 at callback while first native reservation held → exact release witness RED. |
| `BRG016_file_url` | C15 | Rewrite committed URL to custom scheme or substitute guessed senderFrame.url → native URL equality RED. |
| `BRG017_recovery` | C12,C17 | Replay old queued event after Bun restart or retain retired callback → stale-tag sentinel RED. |
| `BRG018_disallowed_wire` | C18 | Serialize nested page callback or Symbol into IPC data → codec/local error and zero-write checks RED. |
| `BRG019_manifest_admission` | C19,C20 | Change an expected-fail row to PASS/skip or drop implementing-ticket/source → GH-566 validation RED. |
| `BRG020_native_world_hostile` | C01,C21 | Register app handler in trusted world or omit controller/frame check → native hostile sender causes forbidden effect RED. |
| `BRG021_symbol_oracle` | C22 | Make observed-oracle comparison a fixed guess instead of pinned executable result → doc-vs-native discrepancy remains UNKNOWN and test cannot qualify PASS. |
| `BRG022_two_window` | C12,C17,C21 | Cross-bind old window/document callback to second window → other-window payload marker RED. |

Do not aggregate all 22 into a single giant mixed-owner
test module. GH-464 owns its corpus cells, GH-567 preload/process
corpus, GH-484 native realm smoke and GH-467 graph/listener/application
fixtures. Keep the provenance of each author, executable binary,
OS/WebKit version, exact repo head, test id, mutant id, raw failure,
clean pass and CI links. Record an unrun cell as unrun.

## 8. Review gates triggered

- **Public API / compatibility:** yes; cross-world graph, method
  return shape, EventEmitter and Electron source-vs-test contract.
- **Security / permission model:** yes; hostile preload/page,
  app-world handler separation, qualified document and exact grant;
  reviewers from GH-541/542/552 retain owner decision authority.
- **Native engine:** yes; independent real macOS WKWebView
  first-inline and message handler tests, not V8-only simulation.
- **IPC wire:** no change owned by this issue. Consume GH-540
  reviewed one-link message envelope; any requested wire mutation
  returns to GH-540.
- **Dependency addition / unsafe:** none proposed for this spec;
  a future implementation must request their exact gate if needed.
- **Review:** no self-approval; exact-head CodeRabbit or documented
  independent adversarial review/refutation; full applicable
  repository gates before qualification.

## 9. Performance and retained-memory impact

No performance comparison or claim in this specification. The
existing KEL-90/keld-benches authority owns metric registration;
GH-484 registers its one PANEL-D21 app-world diagnostic metric.
GH-540 owns per-direction queued DATA/CONTROL bytes, terminal
receipts, native/ring physical release and all-producer caps.
GH-467 additionally owns a **separately finite document-scoped
JS callback/proxy graph**, with explicit max graph traversal,
slot count and lifetime bounds to be frozen at approval.

Capture a source-backed retained-reference census at: API exposure,
an ordinary call, pending watchFile callback, dropped listener,
same-document navigation, cross-document retirement, Bun restart
and WebView destroy. Test max+1 refusal and a healthy next-document
call. Do not relax Host20/Guardian/native timeouts, add sleeps to
hide ordering, or infer speed from a small 4 KiB fixture.

## 10. Open decisions blocking Status: approved

1. **Q1: synchronous cross-world publication:** GH-484 must
   demonstrate how full callable page API is exposed before first
   inline script on real WKWebView, without a late asynchronous
   evaluator or page-origin-unsafe relaying. The 2026-10-24 real-Mac F0
   tests demonstrate **mechanical** first-inline publication, actual
   app-created function invocation and nested callback transfer using
   an **unapproved shared-DOM shim**; the observed source/event is
   forgeable and the main IPC response was simulated. Before a
   contract decision, independently prove a native-security-qualified
   proxy, or return to architecture review. No F0 mock becomes PASS
   on the final unmodified app/Host path.
2. **Q2: Symbol semantics:** pinned v44.4.5 docs say drop, native
   code/upstream specs preserve. Run the pinned executable oracle
   and fix GH-464's cell/this spec or explicitly approve a
   KELD divergence. Until then do not mark Symbol parity PASS.
3. **Q3: finite graph policy:** approve exact maximum depth, entry
   count, snapshot bytes, retained callback slots and clear typed
   over-limit error, as well as accessor/Proxy/cycle/shared-reference
   behavior and Error field/stack policy. The first-proof app uses
   ordinary objects but approval cannot leave hidden unlimited
   framework retention.
4. **Q4: original-document initial send:** resolve GH-542's
   app-preload-API-before-qualified-native-commit window with the
   host admission owner; choose bounded pending/failure behavior
   after a real startup trace. A successful early exposure is not
   native authorization. The current KEL-142 `BridgeState` admits
   one document nonce and refuses a second bind; GH-484's distinct
   app/trusted worlds and GH-540's v2 document field require
   a separately approved, world-scoped bind/credential model under
   **one** checked-u32 #542 navigation generation (or an explicitly
   reapproved #540 envelope). Never share the trusted nonce or add
   a second generation counter.
5. **Q5: exception and listener lifecycle:** approve how local
   cross-world thrown errors are copied and surfaced; define what
   is released for document teardown versus a live multi-fire
   `watchFile` listener; verify original callback ownership. The
   disposable native nested-callback probe proves **only local
   feasibility**, not finite retained-reference teardown,
   unforgeable callback origin or Host terminal accounting.
6. **Q6: single source/bundling seam:** GH-484/GH-467 must identify
   the approved bundle/source location and at-document-start
   import/export mechanism without duplicating the TS runtime or
   permitting Node in the page.

**Decision recommendation:** approve the narrow copy/freeze versus
mutable-arguments separation, nested callbacks, void page methods
and reuse of existing #540/#541 authority now; **hold the exact
spec approval and production implementation** until Q1–Q6 have
named falsifiable dispositions and an independent review. Only
the scoped spec can proceed without those runtime proofs.
