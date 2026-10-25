# Spec: macOS draw.io contextBridge and ipcRenderer first-proof facade

Status: draft
**Approval/production qualification: BLOCKED** by unproved secure
first-inline callable publication and owner-approved dual-world binding.
Linear: GH-467 (#517 GitHub tracker of record; KEL-127 reference) · Owner: @0monish · Updated: 2026-10-25

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
proof is **one genuine pinned page-origin `isFullscreen` request** from
`EditorUi.init`, observed at the original page callback, live main
`rendererReq` handler, current native document and transport release.
A separately labelled **test-triggered** second `isFullscreen` request
checks distinct IDs and credit reuse; it is **not** a second natural page
call. Include the nested long-lived `watchFile` callback and copied/frozen
exposed data; protect KELD's trusted renderer bridge with Host grants.

This issue owns the **application-level facade and cross-world value behavior**,
not the WebKit application-preload loader. GH-484 owns the loader, world selection,
handler and source containment; GH-540 owns transport and credits. Landing a
successful JS unit test or a synthetic response is not sufficient.

**Selected first-proof product scope (Option A, security approval pending):**
use a **predeclared document-start page facade** with exactly four callable
methods `request`, `registerMsgListener`, `sendMessage`, `listenOnce`
bound, after private registration, to the **actual unmodified pinned draw.io
preload** in the separate app content world. Copy/freeze only the pinned
`process.type` and real session-derived `process.versions` values. A page
world message never authenticates a method or its caller and never grants
native or Bun authority. General arbitrary/dynamic
`contextBridge.exposeInMainWorld(key, api)` graph publication is **Option B,
not accepted by this first proof**. Every unimplemented, doc-cited
GH464 `contextBridge` cell remains **expected-fail** (or an expressly
owner-approved and recorded divergence), not UNKNOWN or PASS merely
because the fixed draw.io facade works. UNKNOWN is reserved for
genuinely unqualified/uncited engine observations.
This is an explicit, draw.io-specific Electron compatibility divergence,
not a claim of full contextBridge parity. The scoped product direction is
recorded on [GH467](https://github.com/gyldlab/keld/issues/467#issuecomment-6390880361);
it does **not** satisfy this specification's independent approval gate.

Non-goals for this first-proof slice:

- Arbitrary graph publication under the full dynamic Electron
  `contextBridge` API, generic third-party preload parity,
  `ipcRenderer.invoke`, `sendSync`, synchronous
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
| [KEL-142](kel142-macos-renderer-bridge-api.md) / architecture [05](../architecture/05-webview-and-native.md) §§2–3 | Existing **trusted** named world + frozen `window.keld.invoke` page surface. New app world needs a *separately approved* amendment under GH-484. |
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

**2026-10-25 pinned page-caller correction (evidence, not approval):**
The [original current webapp `ElectronApp.js`](https://github.com/jgraph/drawio/blob/24b76c2cbd55d88e354042e8d329a2e4708972bc/src/main/webapp/js/diagramly/ElectronApp.js#L694-L715)
calls `requestSync('isFullscreen')` in `EditorUi.init`; the helper
converts it to `{action:'isFullscreen'}`, the unchanged preload assigns a
dynamic `reqId` and sends `rendererReq`, and the desktop main
[`rendererReq` listener](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L4431-L4543)
replies on `mainResp` with a Boolean. The `isPluginsEnabled` main
switch case is a legacy fallback: none of the 406 checked-out pinned
current webapp JavaScript files invokes it. Prior two-request native
`isPluginsEnabled` fixtures called the preload explicitly and sent
synthetic responses; do not score them as genuine page-origin traffic.
A second *natural* page `isFullscreen` request follows the fullscreen
UI action's `sendMessage('toggleFullscreen')`, which needs a **separate
approved** `el:toggleFullscreen` grant; never infer or widen it from
`el:rendererReq`. A conditional `getLocalFonts` request may occur
earlier; do not assume `reqId=1`. See [source owner receipt](https://github.com/gyldlab/keld/issues/467#issuecomment-6392657686).

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

**Separate native credential feasibility, still not product proof:** the
[2026-10-24 physical-Mac two-world experiment](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584)
observed 33 native receipts per run under one test-owned checked-u32
document epoch and two distinct world-scoped 128-bit credentials (3/3
corrected baselines; credential-check and role-operation mutants RED).
It exposed a fixture bug where absent stored and supplied optional tokens
compared equal, then corrected the fixture to require an actual stored
binding and supplied string. This does **not** implement KELD's #542
document admission, #540 v2 binding or #552 guard, nor authenticate the
separate page-to-preload callable publication mechanism.

## 3. Acceptance criteria — binary observables

| ID | Given / when | Independently observed pass condition |
| --- | --- | --- |
| C01 | GH-484 installs the real app-world preload for a new main-frame document | Before the **first inline page script**, `window.electron` has exactly the **four predeclared callable methods** and `window.process` has only copied/frozen pinned facts **from the actual preload/session**. Independently prove the **unmodified real preload executed, registered each expected method and completed its conditional `.configuration` seed before first inline**; otherwise visible static wrappers alone must **not** pass. Each effect-bearing callable later resolves only to the matching **actual registered** current app-world method, never a page-chosen one. No *trusted or app-world* native handler, credentials, Node globals or bind authority are exposed to the page. **GH484 AC17 currently forbids even an unprivileged page-world native endpoint; Option A's cross-world ingress remains unapproved pending Q1 owner choice.** This is **not** a general dynamic `contextBridge` PASS; a late-filled method, synthetic seed or F0 DOM handshake is insufficient. |
| C02 | The proposed Option A facade exposes copied process data and allowed local argument snapshots | Page `process` data are independently copied, recursively frozen and non-writable/non-configurable at the top-level property; mutating either side does not affect the other. Supported argument snapshots preserve the separate **mutable copy** requirement in C03. Arbitrary exposed application graphs remain outside the Option A parity claim. |
| C03 | Page invokes a proxied preload function with an ordinary object | The preload receives a **mutable copied object**, can assign `msg.reqId` and delete `msg.listener`; page's original object remains unchanged. Freeze-on-all-crossings fails. |
| C04 | A callback is nested in an object argument (`watchFile`) | Preload retains the same bounded callback slot across N distinct values in issue order; each event releases its own native/ring reservation without freeing the still-live multi-fire callback. Unsubscribe/role-bound retirement/document replacement frees the slot once. No callback appears in IPC codec bytes. |
| C05 | The unmodified pinned preload registers `request`, `registerMsgListener`, `sendMessage`, `listenOnce` | All four predeclared page methods are callable with **void returns**, but effect-bearing calls dispatch only to their current verified private app-world function slots. Missing/mismatched registrations fail closed; no synthetic returned cross-world Promise or manufactured `mainResp` qualifies. |
| C06 | Full Electron dynamic `contextBridge` exposes an arbitrary admitted nested function graph | **Deferred / not a first-proof PASS.** The fixed four-method draw.io facade and its explicitly supported nested *argument callback* in C04 do not establish arbitrary dynamic graph publication, generic nested function identity or upstream parity. Keep every unimplemented **doc-cited GH464** contextBridge row `expected_verdict:fail` (or an independently approved recorded divergence) until an owner-approved Option B and real engine proof; do not mislabel pinned failures UNKNOWN. |
| C07 | Page accesses error-copy row | Supported local Error crosses as owned name/message and registered code, with unapproved stack omitted; a native/Host/process path or raw cross-world Error reference is never exposed. Pinned Electron stack differences remain UNKNOWN/divergence until observed. |
| C08 | `ipcRenderer.send` gets admitted channel and supported values | Local return is `undefined`, argument array snapshots before async admission, actual named topic reaches one real `ipcMain.on` listener through Host; no numeric invoke or second transport. |
| C09 | `ipcRenderer.on/once/off/addListener/removeListener/removeAllListeners` manage listeners | Registration order, duplicate handling, reentrant once-before-callback removal, original-listener removal, zero-late-delivery and exception cleanup match pinned cases for supported domain. |
| C10 | An inbound main event is delivered to preload | `IpcRendererEvent.sender` is the preload `ipcRenderer` facade; `ports` is empty for no-transfer first proof; it is not a raw native event, WebContents or transport authority. |
| C11 | Exact literal window grant absent | Final Host guard records an **internal** typed `KELD-GUARD003` refusal before any Bun frame/listener effect; the public `ipcRenderer.send` call still returns `undefined` and never gains a retroactive exception, event or Promise. Healthy explicitly granted neighbor still succeeds. No wildcard, case-fold, URL decoding or alternate grant. |
| C12 | Renderer, frame, document or Bun generation is stale, foreign or destroyed | Original-target callbacks/events and reservations settle once without delivery to successor. Same-document/hash and cancelled renderer provisional navigation preserve still-live document callbacks. |
| C13 | Preload has begun before first native application-document qualification | Page can observe only the exposed API. Native forwarding remains closed until GH-542 qualifies the document and one matching app-world bind plus live Bun generation. Early void send returns undefined with bounded private typed refusal, **zero retained native work**, no callback and no replay to a successor; a valid post-bind control succeeds. |
| C14 | The real unmodified pinned `EditorUi.init` page calls `requestSync('isFullscreen')` after actual Electron-mode bootstrap and current `file://` admission | Independently trace **one genuine page-origin** `rendererReq` action `isFullscreen`, its dynamically assigned `reqId`, the original desktop main handler and addressed Boolean `mainResp`, actual page callback and physical native/transport release. No fixed first ID, test-injected first caller or canned Bool qualifies. A **separate explicitly controlled second** `request({action:'isFullscreen'})` after callback **and native/ring release** proves a different ID and quota reuse, not a second natural page call. A later page UI `toggleFullscreen` requires its own reviewed `el:` grant, never silently inherited. |
| C15 | Actual file URL page participates in send/reply | Native committed `file://` document URL, approved package-resource origin and `senderFrame.url` match; a custom-scheme rewrite or guessed `file:` string cannot satisfy eligibility. |
| C16 | Application registers a listener after an earlier unsolicited event | Earlier event is not replayed. A later matching event is delivered once; no limitless hidden event queue. |
| C17 | Preload page navigates, reloads, closes or Bun role recovers | Document-owned proxy/listener retention terminates for a retired document, surviving doc API remains only if host lifecycle admits it, old queued calls do not replay or retarget, other WebViews stay isolated. |
| C18 | Attempts to pass `Function`, `Promise`, `Symbol` or other disallowed values through **IPC** | GH-544's supported IPC domain is enforced synchronously with no Bun write; a *local contextBridge function proxy* must not be misclassified as an IPC payload. |
| C19 | Unsupported/unknown Electron API or graph exceeding §4.3 depth, entries, strings, proxies, callbacks or retained bytes is requested | An allocated distinct typed KELD-COMPAT local bridge refusal occurs before partial exposure/retention; no silent truncation, unauthorized native effect, leaked handle or fabricated parity PASS. 0/max/max+1 independently exercise each bound. |
| C20 | Compatibility is scored/published | Every mapped GH-464 row has pinned source, expected verdict, implementing-ticket key and recorded device. The fixed draw.io facade is explicitly labelled a narrower, app-specific divergence; **all unmet doc-cited contextBridge rows** (including general dynamic functions, copy, Error and Symbol cells, not only C06) stay expected-fail unless separately approved as a documented divergence. Truly uncited engine observations stay UNKNOWN until qualified. No row flips to PASS from wrapper visibility alone. |
| C21 | A hostile same-origin page probes preload or KELD internal handlers | Directly forged shared-DOM events, guessed proxy/callback IDs or descriptor changes cannot invoke an unexposed preload function, bind a native role or produce a forbidden Bun effect. The page cannot read/call the trusted KELD handler, role endpoint, native stamp, Host grant or raw ipcRenderer. Allowed exposed wrappers still operate within exact Host grants. |
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
   JS; the **current unamended GH484 contract** admits only the explicitly
   published `window.electron` and copied `window.process` objects, no
   native page-world handler, Node, app/trusted native handler, host bind or
   grant. The F0 page-native carrier would violate that contract and is
   **not** silently authorized by choosing Option A. A separate page-visible
   data-only carrier could be proposed only through GH484/KEL-142 owner's
   explicit reviewed AC17/architecture amendment; even then it is merely
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
used a raw page-world native message handler that GH484 AC17 currently
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

**Selected fail-closed early-call policy for this draft:** when a
page-visible method is invoked before an actual #542 qualified app-world
bind **or** without a captured live Bun generation, allocate **zero**
pending native/transport records, return `undefined` for its declared
void interface and emit one bounded Host-private typed admission
refusal/diagnostic. Do not buffer, defer, replay, infer authorization
from API visibility, attach work to a later role, throw retroactively
or fabricate a `mainResp`. The application-level request callback
does not fire. A paired granted call after bind must still work.
If the pinned draw.io startup trace sends a required request during
that interval, **C14 remains BLOCKED** until #484/#540/#542 prove
a correct earlier admission/order or approve a revised finite policy;
the rejection is not counted as application compatibility.

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
      predeclared four-slot page facade, finite copied argument/proxy
      domain and readonly process facts; dynamic graph acceptance remains
      deferred. Check actual app-world method registration, argument
      mutability and callback registries with tests that fail on
      broken freeze, nested argument callbacks, missing actual preload
      method binding or premature callback retirement.
      Depends on GH-484's accepted document-start static-publication seam.
- [ ] **T4 — listener/composition:** implement the approved
      renderer `ipcRenderer` façade against the sole GH-540 transport,
      accepted GH-544 codec and GH-465/#543/466 events.
      Include once/remove and no-listener/exception negative controls;
      do not change channel grants.
- [ ] **T5 — real macOS application proof:** after accepted GH-484,
      GH-473, GH-542/552 and owner codec/transport artifacts,
      run the genuine pinned-page `EditorUi.init` `isFullscreen`
      request, a separately labelled test-controlled second request
      and watchFile fixtures on WKWebView. Run anti-stub and KEL-142
      hostile-world controls; never count a controlled request as
      a second application-origin call or mark PASS without proof.
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
| `BRG001_first_inline` (real WK) | C01,C13 | Delay predeclared four-method page wrapper until after first page inline script → RED; removing only wrapper leaves all four absent. The **unmodified preload's own conditional startup seed** must also be visible before inline (GH484 AC02), not a fabricated wrapper effect. Early API still has zero pre-bind Bun effects. **Do not count the existing F0 raw page handler as an authorized route:** GH484 owner must first select and approve Q1 A1 or A2. Under approved A2, direct page-world calls remain untrusted and use exactly the same private registry/final Host grants as ordinary wrappers, while unexposed/ungranted operations have zero forbidden effects. |
| `BRG001b_real_registration` (real WK/Host) | C01,C05,C21 | **Leave static page wrapper intact** but suppress only actual app-world `exposeInMainWorld` registration, or bind one name to a different real app function. First-inline wrapper appearance alone cannot PASS C01/C05; missing/mismatched binding must produce zero wrong-function/Host/Bun effects and an independent private failure. Restore correct registered methods and one granted call succeeds; a fake return/canned `mainResp` does not qualify. GH484 separately delays the actual preload's adaptive-colors side effect while keeping page wrapper visible; first-inline conditional seed must go RED. |
| `BRG002_exposed_freeze` | C02 | Remove recursive freeze / top-level read-only property → page mutation observable. |
| `BRG003_mutable_arg` | C03 | Freeze receiving argument object → draw.io `reqId` assignment / listener deletion fails. |
| `BRG004_nested_watch` | C04 | Proxy only top-level functions in the Option A argument path → nested `watchFile` callback absent; sequence test RED at second differing payload. **Does not qualify deferred generic C06.** |
| `BRG004b_graph_capacity` | C02–C05,C07,C19 (not deferred C06) | For each 16-depth/256-entry/65,536-byte/64-proxy/64-callback/524,288-retained ceiling actually exercised by Option A data/argument/callback crossings, prove 0/max pass and max+1 typed refusal before another allocation; bypass any one reservation → bounded-count observer RED. This does not qualify arbitrary general dynamic graph capacity. |
| `BRG004c_multifire_lifetime` | C04,C12,C17 | Two distinct `watchFile` deliveries reuse the same live slot, with per-event native/ring release; unlink or retire document then replay stale callback → zero delivery. Free slot after first event or keep it after retirement → RED. |
| `BRG005_void_return` | C05 | Add fabricated cross-world Promise or omit exposed method → exact shape/return RED. |
| `BRG006_error_copy` | C07 | Return raw Error/native path or silently null → name/message/security-cell RED. |
| `BRG007_send_snapshot` | C08,C18 | Mutate page object after send before Host handoff → transmitted bytes mismatch; Function/Symbol send must throw before Bun. |
| `BRG008_emitter_once` | C09 | Remove once listener *after* callback and recursively emit → duplicate event RED. |
| `BRG009_emitter_remove` | C09,C16 | Ignore original listener identity or buffer no-listener events → unmatched replay RED. |
| `BRG010_event_shape` | C10 | Send raw WebContents/Host object as event.sender → facade equality/authority RED. |
| `BRG011_final_guard` | C11,C21 | Replace byte-equal literal match with prefix/wildcard → unauthorized Bun write observed. |
| `BRG012_stale_document` | C12,C17 | Drop addressed-document stamp and target newest page → successor sentinel RED. |
| `BRG013_precommit_gate` | C13 | Treat early app-world publication or `file:` string as committed eligibility → forbidden handler count RED. |
| `BRG013b_world_bind` | C13,C21 | Native app/trusted credentials are distinct under one current checked generation. Refuse missing/stale/swapped/duplicate credentials and old-document delayed bind with independent Host/Bun zero-effect counters; remove present-binding check → RED. |
| `BRG013c_early_void` | C08,C13,C14 | Invoke exposed void method before app bind or accepted Bun link, observe undefined + one internal refusal, zero retained work and no queued successor replay, then allowed call after bind; buffer old work or invent reply → RED. |
| `BRG014_real_page_is_fullscreen` | C14 | Replace actual pinned `EditorUi.init` `isFullscreen` with a harness-injected first call, bypass original main listener, hardcode Boolean or duplicate assigned `reqId` → original-page/handler/response witness RED. |
| `BRG015_release_barrier` | C14 | Initiate the **controlled** second preload request at callback while first native reservation is still held → independent release/credit RED; after release a distinct request ID and healthy callback succeed. Controlled traffic is not a second natural page call. |
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
GH-467 additionally proposes the **finite document-scoped
JS callback/proxy graph** in §4.3: depth 16, 256 members,
65,536 UTF-8 data bytes per crossing, 64 proxy endpoints,
64 callback registrations and 524,288 accounted retained bytes
per document. These limits need independent implementation
review and actual retained-representation measurement; they are
not WebKit heap measurements or approved runtime constants.

Capture a source-backed retained-reference census at: API exposure,
an ordinary call, pending watchFile callback, dropped listener,
same-document navigation, cross-document retirement, Bun restart
and WebView destroy. Test max+1 refusal and a healthy next-document
call. Do not relax Host20/Guardian/native timeouts, add sleeps to
hide ordering, or infer speed from a small 4 KiB fixture.

## 10. Proposed decisions and BLOCKED acceptance gates

1. **Q1 — Option A product first-proof scope selected; security BLOCKED:**
   For the pinned draw.io only, use the document-start **predeclared
   four-function** page facade, with source-backed actual app-preload
   registry and copied frozen `process` facts (GH467 owner packet
   [selection](https://github.com/gyldlab/keld/issues/467#issuecomment-6390880361)).
   This deliberately does **not** implement full general dynamic
   `contextBridge`/C06 and is not a native Electron-parity PASS; GH-464
   must preserve the unproved generic cells. GH-484 must prove real
   document-start injection, exact authentic unmodified preload source,
   app-world handler and lifetime. **Separately, GH484/KEL-142 must
   resolve [Q1 A1 vs A2](https://github.com/gyldlab/keld/issues/484#issuecomment-6390973710):
   retain its existing no-page-world-native-endpoint rule and prove
   another WebKit transport, or explicitly amend that rule for a
   data-only untrusted page carrier with independent native/security
   review. Neither is currently approved.** GH-467 must prove that all four
   exposed methods invoke the current original app-registered methods
   and never derive authority from forgeable page messages. Independent
   WKWebView/Host/Bun positive, hostile-page, no-wrapper, no-bind,
   stale-document, absent-grant and one-fault allowlist controls remain
   mandatory. Existing fixed-wrapper 3/3 and 2/2 omission, F0 16/16
   and 6/6 nested-callback probes are **test-only** and do not approve
   shipping. **Alternative Option B:** full dynamic graph publication
   stays P0 blocked pending separately approved secure platform primitive
   and complete C06/score qualification. GH441's separate-world
   placement is CLOSED and unchanged. Named security/native/compat
   reviewers must still approve this narrowed first-proof contract
   before any production bridge code or conformance PASS.
2. **Q2 — Symbol oracle, UNKNOWN:** pinned Electron v44.4.5 documentation
   and native source disagree on Symbols. The executable pinned oracle
   or an explicitly approved compatibility divergence is required
   before a Symbol PASS. GH-544's IPC Symbol rejection is unchanged.
3. **Q3 — finite graph and local errors, proposed policy (§4.3):**
   depth 16, 256 keys/elements, 65,536 UTF-8 data bytes, 64 proxy
   endpoints, 64 callback slots and 524,288 charged retained bytes per
   document, with descriptor-only/no-accessor traversal and copied
   Error name/message. GH-467 and the error-registry owner must approve
   a distinct `KELD-COMPAT` over-limit code. Independent 0/max/max+1,
   alias/cycle, Proxy-trap, error-copy and measured-retention tests
   must fail the individual mutations before implementation qualifies.
4. **Q4 — pre-bind work, selected fail-closed:** zero queued early work;
   void caller sees `undefined`, the Host logs a bounded private typed
   refusal, and no later document/Bun role can replay it. Required
   pre-bind draw.io startup traffic would leave the first-proof C14
   **BLOCKED** until real startup order changes safely or #540/#542
   separately approve a revised policy. GH-540/#542 must approve two
   distinct native App/Trusted credentials under one checked-u32
   navigation owner. The [33-receipt physical native fixture](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584)
   supports that proposal, but does not prove KELD binding or grants.
5. **Q5 — multi-fire callback lifetime, proposed policy (§4.3):**
   retain `watchFile` slot across N events; charge/release independent
   GH-540 event/native reservations for every delivery; free the slot
   exactly once on unsubscribe, authenticated role-bound retirement
   or document retirement. Confirm exception copying and synchronous
   once-removal with original-function owner, repeated event controls,
   stale-role failure and counted slot teardown on the real Mac.
6. **Q6 — one bundled source and authority, BLOCKED:**
   GH-484 must identify the admitted immutable preload image, one
   renderer facade bundle built from `packages/@keld/electron` sources
   and a proven at-document-start import/export path that does not
   expose page-world Node or add a second transport. The admitted
   package source must satisfy GH-484's separately BLOCKED trusted
   manifest/digest and absolute path gates. Source presence alone is
   not proof of runtime execution.

**Disposition:** changes above are reviewable draft contracts. Full
security-qualified publication, #540/#542 world binding, registered
errors, source-image authenticity, real Host/Bun effects and independent
security/API review remain **BLOCKED**. Keep this PR OPEN/DRAFT and
do not enable compatibility or promote conformance cells until all
named gates pass.
