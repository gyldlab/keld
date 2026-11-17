# Design

Part of: [index](index.md)
Owns: section 4

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
   JS; the **current unamended GH484 contract** admits only the explicitly
   published `window.electron` and copied `window.process` objects, no
   native page-world handler, Node, app/trusted native handler, host bind or
   grant. The F0 page-native carrier would violate that contract and is
   **not** silently authorized by choosing Option A. A separate page-visible
   data-only carrier could be proposed only through GH484/KEL-142 owner's
   explicit reviewed #484 AC-17/architecture amendment; even then it is merely
   untrusted request input, never an app/trusted-world credential or principal.
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
valid proof of first-inline availability. **Production must not use the F0
shared-DOM event/descriptor as a function capability, callback identity,
readiness receipt, authentication token or native permission.** A page
can forge DOM events without invoking an exposed method. Changing an
event name, hiding a DOM node or comparing a page-provided nonce cannot
turn that carrier into an authenticated method call.

**Candidate boundary, not a proved WebKit primitive:** GH-484 owns
at-document-start installation and a single per-view native controller;
GH-467 owns only the application-defined copied graph and page callable
wrappers. GH-542 must supply one host-owned qualified document identity
and distinct app/trusted world-scoped bind credentials approved with
GH-540. Initial binding is native-admitted by controller/view/main
frame/world/qualified document and mints only that realm's credential;
each **effect-bearing** app message must then independently establish
those native facts, a **present** matching stored app credential, and
captured Bun-session identity. GH-552 must authorize
each exact `el:` literal at the final handoff. The page can supply
ordinary arguments to exposed methods, but cannot supply a credential,
proxy-table authority, native handler destination or admitted role.
No page-world carrier may confer a callable or privileged native handle.

**BLOCKED — production-qualified first-inline publication:** Option A
predeclares only the pinned four page functions and copied `process`
facts, as a fixed `WKUserScript` in the untrusted page world at document
start. The *actual* unmodified preload must separately execute and
register those exact functions in GH-484's app world. Later wrapper
calls must reach only the matching current-document private registered
method, not a page-selected function name or guessed proxy id.
No supported production seam has yet proved this source, binding,
timing and Host authorization composition. The independent real-Mac
[static-wrapper native fixture](https://github.com/gyldlab/keld/issues/467#issuecomment-6388049733)
observed four predeclared methods at first inline (3/3) and absence
when omitted (2/2), with synthetic IPC; this is *not* a product proof.
**Missing approved page→app ingress:** the [test-only F0 native path](https://github.com/gyldlab/keld/issues/467#issuecomment-6388049733)
used a raw page-world native message handler that #484 AC-17 currently
forbids. GH484/KEL-142 must expressly select and independently approve
**either A1**, a different secure supported cross-world transport retaining
the absolute no-page-handler rule, **or A2**, a narrowly amended
**data-only untrusted page ingress** with no authority, separate from app
and trusted world handlers. See the owning [Q1 packet](https://github.com/gyldlab/keld/issues/484#issuecomment-6390973710).
Under a future approved A2, direct page messages would be callable by
hostile page JS; a permitted method name must take only the same private
app-method allowlist and final Host grants as the visible wrapper, while
unexposed or ungranted requests have zero effects. A raw page message
can never authenticate an app function, mint a world credential or bypass
the Host's per-call `el:` grant. Neither A1 nor A2 is approved today.
GH-484 must own the native installation, loader and teardown; GH-467
must prove original-document bound method/callback routing and mutable
argument copies. A hostile first-inline page must cause zero effects
for unexposed operations, forged metadata, missing/stale world binds,
denied channels and replaced preload source. Independently record a
real granted Host/Bun call, a denied call and two serial original-page
main responses with actual native credit release.

Option B's **full dynamic** function-graph publication and general
cross-world identity remain P0 unproved and **out of scope for an Option A
first-proof compatibility PASS**. A later complete `contextBridge`
claim requires its own secure supported WebKit mechanism, independent
review and GH-464 corpus qualification; no shared-DOM workaround or
silent score flip is permitted. GH-441's separate-world placement was
already decided and must not be reopened just to weaken this gate.
Until Option A's production evidence and #540/#542/#552 owner approvals
exist, **no production callable bridge or fallback is authorized**.

### 4.3 Proposed bounded contextBridge graph

For Option A, expose only the **fixed** four pinned draw.io page methods
and owned frozen process facts, not an arbitrary runtime-exposed
function graph. The following bounded data/callback policy still
applies to its admitted page arguments and nested `watchFile`
callback, and describes a proposed future Option B general graph
domain—not a claim that it is already implemented or scored PASS.
Supported bounded graph leaves:
`null`, `undefined`, booleans, strings, finite numbers, ordinary arrays
and plain own-enumerable string-key objects with supported leaves, and
functions as proxied callable leaves. Errors require the separately
mapped C07 copy rules; unsupported platform/exotic objects fail
explicitly rather than silently stringify or read OS state.

- In Option A, the pinned preload may call
  `exposeInMainWorld('electron', actualRegistry)` and publish its copied
  `process` values, but the page-visible document-start wrapper has
  **exactly the four predetermined method slots**. Authentic app-world
  registration must match those slots before invocation. Unexpected
  keys/shapes or a second registration fail closed, not silently grant
  dynamic publication or overwrite the page facade. In Option B,
  `exposeInMainWorld(apiKey, api)` requires separately approved dynamic
  page publication. Neither option may overwrite a hostile page-owned
  property, use a hidden symbol as authority or leak a mutable preload
  object.
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
**Proposed exact per-document local graph limits for approval (separate
from GH-540's 4 KiB IPC value limit):**

| Resource | Maximum and accounting |
| --- | --- |
| Graph traversal depth | 16 levels including the root; fail before entering level 17. |
| Own keys / array elements examined | 256 total per crossing; each array length <=256. Count keys before reading values. |
| Snapshot string/key data | 65,536 UTF-8 bytes in one exposed or argument graph, with length checks before further copies. |
| Live callable proxy endpoints | 64 total per document, across both directions and nested locations; repeat references reuse one charged slot. |
| Live callback registrations | 64 distinct slots per document, including long-lived multi-fire `watchFile`; one synchronous delivery stack at a time on the owning document executor. |
| Framework-retained copied data/proxies | 524,288 accounted bytes per document across both JS worlds, counting UTF-16 copies at two bytes per code unit, keys and 1,024 bytes per live proxy/callback record. Independently census any extra engine representations before asserting an actual memory bound. |

Zero, exactly-max and max+1 for *each* limit must be independent
acceptance cases. Reserve slot/byte charges **before** registering,
copying or invoking a method; a partial failure rolls back only that
attempt's reservations. This is a proposed finite first-proof policy,
not a claim that the implementation or measured WebKit heap obeys it.
The existing `KELD-COMPAT` error registry owner must allocate a distinct
typed **local bridge-value/limit error** before implementation. Reject
the entire crossing synchronously with code, limit name and fix; never
silently truncate, partly expose an API or turn a local over-limit
error into `KELD-GUARD003`.

Enumerate own property descriptors without invoking accessors; reject
getters/setters and unsupported exotic prototypes. A transparent JS
`Proxy` cannot reliably be detected as a type in ordinary WebKit JS:
any traps executed while enumerating are treated as untrusted
application/page code, subject to the same **incremental** limits, and
cannot supply Host authority. A proxy that presents an ordinary valid
snapshot can only contribute copied data; no live source reference
crosses. Reject cyclic and aliased *data-object* graphs rather than
silently dropping reference identity; preserve same-function identity
within one document through the bounded proxy cache. For local copied
`Error`, expose an owned name/message and approved typed code, omit
unapproved stack or native/process paths, and reject unsupported
subclasses. Exceptions from the proxied function are converted to that
bounded local form; no raw cross-realm error object is retained. Pinned
Electron differences are explicit divergence/UNKNOWN cells until their
own oracle runs.

For a long-lived `watchFile` callback, **dispatching an event releases
only that event's GH-540 transport/native reservations**; it does not
release the registered callback slot. Each next event requires fresh
bounded delivery credit and validates the original document/role.
Release the slot on explicit unregister/cancellation, proven role-bound
subscription retirement, or document destruction/replacement; cleanup
is idempotent and cannot free another document's slot. Bun recovery
cannot replay an old subscription or revive old role callbacks, while
the surviving document's exposed ordinary methods remain usable only
after fresh Host admission. A once callback is removed before
reentrant invocation, unlike this multi-fire registration.

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

The Option A page-visible draw.io API is exactly the four function names
listed in §1; a copied `process` object contains **only** copied
`type` and `versions` published by the original preload. KELD must
not inject a raw `ipcRenderer`, Bun path, app-link socket or grant
into the page. The app preload's `require('electron')` resolves to
GH-484's sandbox registry, not the desktop main-role package import.
Unknown modules and members use the approved GH-484 typed failure
policy, never a fake success.

#### 4.5.1 Exact page-visible inbound subscription allowlist

The pinned page uses a finite listener set. `registerMsgListener` accepts only `isModified`, `removeDraft`, `saveAndClose` and
`args-obj`. `listenOnce` accepts only `export-success` and `export-error`. These names come from the pinned `ElectronApp.js`
source in section 2.

`mainResp` and `fileChanged` are reserved internal preload channels. They never enter the page-visible listener table. The
original app-world preload consumes them. Reject unknown, prefix-matched, suffix-matched or page-selected channels before
registering a callback. The inbound event allowlist does not grant `rendererReq` actions or `sendMessage` authority.

#### 4.5.2 Exact renderer action admission

`rendererReq` is a multiplexer, not a permission. The Host validates `msg.action` separately from the exact channel grant. The
first-proof action allowlist contains only `isFullscreen`. The first-proof fixture disables local fonts. If the real profile
enables `getLocalFonts`, it needs a separate reviewed action grant and an explicit trace; it never inherits `el:rendererReq`
authority.

`toggleFullscreen` uses the distinct `sendMessage` channel and requires its own `el:toggleFullscreen` grant. File, dialog,
clipboard, window, `exit` and all other multiplexer actions remain denied until separately reviewed. Validate the action before
invoking the original preload method and repeat the exact action check at final Host dispatch. A channel grant alone cannot select
an action.

#### 4.5.3 Request preflight and terminal callback cleanup

After bind, and before the original preload `request` runs, the facade checks the exact action and GH-544 value shape.
A post-bind local refusal calls the bounded error callback once. It does not call the original preload. It creates no
`reqInfo` entry and sends no `rendererReq` frame. The void `electron.request` call returns `undefined`.
The `requestSync` wrapper rejects with the bounded local refusal.

Before bind, §4.7 takes precedence. The void `electron.request` call returns `undefined`.
Neither callback runs, including the supplied error callback. No `reqInfo` entry or frame is created.
The `requestSync` Promise remains pending for that document; bind does not replay it.
Document retirement destroys the realm and discards that Promise. A required early startup request keeps C14 BLOCKED.

A final Host denial occurs after the pinned preload stores `reqInfo[reqId]`.
GH-543 delivers one Host-owned terminal error `mainResp` to the same current document and request ID.
Set `error: true` and use sanitized `msg` and `e` values.
This is not a main-handler success or canned Boolean. It creates zero Bun frames.
The original preload calls its error callback and deletes `reqInfo`.

**Bun loss before `mainResp`:** GH-542 preserves a surviving document across Bun role restart.
The GH-540 link retirement alone does not clear the original preload's `reqInfo`.
GH-484 must expose the post-assigned `reqId` from the original app-world send seam.
It registers `(document stamp, role generation, reqId)` with the GH-542 document owner before forwarding.
GH-542 serializes registration with the role-retirement sweep. An earlier registration enters that sweep. Never forward to a retired role.
If retirement wins, refuse forwarding and send the same private terminal `mainResp` to the original surviving document.
Settlement releases the pending-ID record exactly once on success or terminal error. Callback exceptions cannot prevent release.
The Host never reads page-supplied IDs or decodes opaque endpoint bytes.
This document-scoped record uses existing GH-467 callback/retained-byte limits. It adds no app-link field, channel, parser, queue or ledger.
On authenticated GH-540 role retirement, GH-543 sends one private terminal `mainResp` to that original document.
It uses the exact registered request ID and sanitized terminal error. The preload calls its error callback, deletes `reqInfo`, and rejects `requestSync`.
GH-1095 maps proved no-dispatch to `KELD-WV-011`; attempted or ambiguous dispatch uses `KELD-WV-013`.
Never replay or retarget the request. The terminal event is not physical release.
GH-540 retains charges until native/ring work is quiescent.
If GH-542 retires the document first, destroy its realm and pending-ID records; never send to a successor.
This is a proposed cross-owner contract, not shipped behavior.
GH-484, GH-542, GH-543, GH-540 and GH-1095 must approve it before implementation.
If they cannot provide the request-ID sidecar and terminal event, C23 stays BLOCKED.

Wrap supplied success/error callbacks before passing them to the original preload.
A callback exception must not skip the preload's following `delete reqInfo`.
Contain the exception at the wrapper and report it after cleanup.
For `watchFile`, keep a bounded proxy slot.
A terminal failure deactivates its captured page callback.
Only an inert bounded wrapper may remain until realm retirement.

Direct `ipcRenderer.send` is separate. It keeps its `undefined` return and receives no retroactive exception, Promise or
fabricated `mainResp` result.

#### 4.5.4 Pre-bind document-local listener registration

`registerMsgListener` and `listenOnce` may store bounded document-local callbacks before native bind. This operation has no
Host/Bun effect. It does not create a native subscription, queue an event, or replay an earlier event. The app-world listener
table remains bound to its document. After a valid bind, only later allowlisted events reach those callbacks. A retired document
loses the table.

The startup control registers the pinned `EditorUi.init` listeners before bind. It then binds the same document and delivers one
later allowlisted event. The callback must run once. No early native frame is allowed. If the pinned application requires replay
of an event that arrived before bind, the first-proof row stays BLOCKED until its owner approves a separate bounded rule.

#### 4.5.5 Selected A2 data-only page carrier and native authority boundary

A2 is the selected carrier direction recorded by the owner in [GH-484 decision](https://github.com/gyldlab/keld/issues/484#issuecomment-6560086477) and [A2 correction handoff](https://github.com/gyldlab/keld/issues/484#issuecomment-6560285327). A1 direct cross-world callable publication is not selected.

The page-world carrier transports untrusted data only. Its event source, origin, DOM token, payload or claimed method cannot authenticate a sender, create a principal, mint a realm credential, register a privileged method or widen a grant. Treat every carrier event as hostile input, including events produced by the real page.

The original app-world preload and native receiver share one per-view `WKUserContentController` and named `WKContentWorld`. The receiver accepts only the current GH-542-qualified committed main frame and trusts native world/frame/document facts, never page event fields.
It resolves methods through the original app-world registry, checks exact method/action, then enters the single GH-552 Host Guard.
Page events cannot create or mutate that registry; Host rechecks current document, role, channel, action and grant before effect.
A2 never uses tokenless Bind to mint a credential or emit `bound.document`; GH-540/GH-542 own the current-document stamp and correlated Bind.
The visible static facade has no authority. If native receiver cannot bind before first inline, A2 is NO-GO; do not fall back to F0, page-world authentication or A1.
This is the selected direction, not native proof. Hostile-page, original-method, zero-forged-effect and custody tests remain mandatory.

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

Record the genuine pinned-page `EditorUi.init → isFullscreen` call
and callback **without a harness-initiated first request**. Observe
the original `rendererReq` main handler, addressed `mainResp`,
Bun generation, native document stamp and physical release.
Only **after both the genuine page callback and native/ring release**,
initiate a separate explicitly test-controlled second call through
the real preload/main handler to prove a different `reqId` and
reusable quota. Record each actual initiator, committed `file://`
URL, request/reply bytes and both settlement receipts. The controlled
second is not another natural page-origin witness. A canned Boolean
or synthetic main response cannot qualify the real product result.
The ≤4 KiB case does not prove burst/backpressure or the larger
draw.io open/save corpus (GH-545).

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
native document bind (GH-541 `4.3/N11`). Same-document hash changes,
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

**Selected fail-closed early-call policy for this draft:** a
page-visible method invoked before a qualified #542 app-world bind
or without a live Bun generation creates zero pending records.
A void method returns `undefined`. It emits one bounded Host-private
admission refusal. Do not buffer, defer, replay or infer authority
from API visibility. Do not attach work to a later role or fabricate
a `mainResp`. A pre-bind `electron.request` invokes neither callback.
Its `requestSync` Promise remains pending until the realm is retired.
A paired granted call after bind must still work.
If draw.io requires this early request, C14 remains BLOCKED.
#484/#540/#542 must prove earlier admission or approve a finite policy.
The rejected request is not application compatibility.

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
