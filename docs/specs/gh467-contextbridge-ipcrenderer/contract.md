# Acceptance contract

Part of: [index](index.md)
Owns: section 3

## 3. Acceptance criteria

### AC-1: Authenticated first-proof facade and exact action admission

- **Given:** GH-484 approves a publication seam; GH-542 authenticates the committed document; GH-552 approves the final Host grant.
- **When:** The pinned preload exposes the facade and the current page invokes an allowed method and action.
- **Then:** Only the matching registered app-world method and exact action reach Host. Internal events stay private. Denials produce zero Bun frames and terminal callback cleanup.
- **Negative control:**
  - **Single fault:** Trust a page-forged event or skip exact action matching.
  - **At-risk precondition:** Hostile same-origin script can call the facade and shared-DOM seam.
  - **Expected RED:** The independent Host/Bun observer records a forbidden frame or a stale callback remains.

The C01–C23 rows below define the detailed binary observables for AC-1.
#### C01

**Given / when:**
GH-484 installs the real app-world preload for a new main-frame document

**Pass condition:**
Before the **first inline page script**, `window.electron` has exactly the **four predeclared callable methods** and
`window.process` has only copied/frozen pinned facts **from the actual preload/session**. Independently prove the **unmodified
real preload executed, registered each expected method and completed its conditional `.configuration` seed before first inline**;
otherwise visible static wrappers alone must **not** pass. Each effect-bearing callable later resolves only to the matching
**actual registered** current app-world method, never a page-chosen one. No *trusted or app-world* native handler, credentials,
Node globals or bind authority are exposed to the page. **#484 AC-17 currently forbids even an unprivileged page-world native
endpoint; Option A's cross-world ingress remains unapproved pending Q1 owner choice.** This is **not** a general dynamic
`contextBridge` PASS; a late-filled method, synthetic seed or F0 DOM handshake is insufficient.

#### C02

**Given / when:**
The proposed Option A facade exposes copied process data and allowed local argument snapshots

**Pass condition:**
Page `process` data are independently copied, recursively frozen and non-writable/non-configurable at the top-level property;
mutating either side does not affect the other. Supported argument snapshots preserve the separate **mutable copy** requirement in
C03. Arbitrary exposed application graphs remain outside the Option A parity claim.

#### C03

**Given / when:**
Page invokes a proxied preload function with an ordinary object

**Pass condition:**
The preload receives a **mutable copied object**, can assign `msg.reqId` and delete `msg.listener`; page's original object remains
unchanged. Freeze-on-all-crossings fails.

#### C04

**Given / when:**
A callback is nested in an object argument (`watchFile`)

**Pass condition:**
Preload retains the same bounded callback slot across N distinct values in issue order; each event releases its own native/ring
reservation without freeing the still-live multi-fire callback. Unsubscribe/role-bound retirement/document replacement frees the
slot once. No callback appears in IPC codec bytes.

#### C05

**Given / when:**
The unmodified pinned preload registers `request`, `registerMsgListener`, `sendMessage`, `listenOnce`

**Pass condition:**
All four predeclared page methods are callable with **void returns**, but effect-bearing calls dispatch only to their current
verified private app-world function slots. Missing/mismatched registrations fail closed; no synthetic returned cross-world Promise
or manufactured `mainResp` qualifies.

#### C06

**Given / when:**
Full Electron dynamic `contextBridge` exposes an arbitrary admitted nested function graph

**Pass condition:**
**Deferred / not a first-proof PASS.** The fixed four-method draw.io facade and its explicitly supported nested *argument
callback* in C04 do not establish arbitrary dynamic graph publication, generic nested function identity or upstream parity. Keep
every unimplemented **doc-cited GH464** contextBridge row `expected_verdict:fail` (or an independently approved recorded
divergence) until an owner-approved Option B and real engine proof; do not mislabel pinned failures UNKNOWN.

#### C07

**Given / when:**
Page accesses error-copy row

**Pass condition:**
Supported local Error crosses as owned name/message and registered code, with unapproved stack omitted; a native/Host/process path
or raw cross-world Error reference is never exposed. Pinned Electron stack differences remain UNKNOWN/divergence until observed.

#### C08

**Given / when:**
`ipcRenderer.send` gets admitted channel and supported values

**Pass condition:**
Local return is `undefined`, argument array snapshots before async admission, actual named topic reaches one real `ipcMain.on`
listener through Host; no numeric invoke or second transport. The Host validates exact `msg.action` for `rendererReq`; first proof
permits only `isFullscreen`. Other actions require separate owner-approved grants. The first-proof fixture disables local fonts or
records a separate approved `getLocalFonts` grant.

#### C09

**Given / when:**
`ipcRenderer.on/once/off/addListener/removeListener/removeAllListeners` manage listeners

**Pass condition:**
Registration order, duplicate handling, reentrant once-before-callback removal, original-listener removal, zero-late-delivery and
exception cleanup match pinned cases for supported domain. The exact page-visible `registerMsgListener` allowlist is `isModified`,
`removeDraft`, `saveAndClose`, `args-obj`. The exact `listenOnce` allowlist is `export-success`, `export-error`. No prefix or
suffix expansion is allowed.

#### C10

**Given / when:**
An inbound main event is delivered to preload

**Pass condition:**
`IpcRendererEvent.sender` is the preload `ipcRenderer` facade; `ports` is empty for no-transfer first proof; it is not a raw
native event, WebContents or transport authority. `mainResp` and `fileChanged` are internal preload channels. They are never
page-visible subscriptions. Only the original app-world preload listeners consume them.

#### C11

**Given / when:**
Exact literal window grant absent

**Pass condition:**
Final Host guard records an **internal** typed `KELD-GUARD003` refusal before any Bun frame/listener effect; the public
`ipcRenderer.send` call still returns `undefined` and never gains a retroactive exception, event or Promise. Healthy explicitly
granted neighbor still succeeds. No wildcard, case-fold, URL decoding or alternate grant. A grant for `el:rendererReq` does not
authorize arbitrary `msg.action`. The final Host guard checks the exact action allowlist and the channel grant.

#### C12

**Given / when:**
Renderer, frame, document or Bun generation is stale, foreign or destroyed

**Pass condition:**
Original-target callbacks/events and reservations settle once without delivery to successor. Same-document/hash and cancelled
renderer provisional navigation preserve still-live document callbacks.

#### C13

**Given / when:**
Preload has begun before first native application-document qualification

**Pass condition:**
Page can observe only the exposed API. Native forwarding remains closed until GH-542 qualifies the document and one matching
app-world bind plus live Bun generation. Early void send returns undefined with bounded private typed refusal, **zero retained
native work**, no callback and no replay to a successor; a valid post-bind control succeeds. Before bind, `registerMsgListener`
and `listenOnce` may store bounded document-local callbacks only. They cannot forward to Host, buffer events or replay pre-bind
events. A startup control registers during `EditorUi.init` and proves a later allowed event reaches the same document once.

#### C14

**Given / when:**
The real unmodified pinned `EditorUi.init` page calls `requestSync('isFullscreen')` after actual Electron-mode bootstrap and
current `file://` admission

**Pass condition:**
Independently trace **one genuine page-origin** `rendererReq` action `isFullscreen`, its dynamically assigned `reqId`, the
original desktop main handler and addressed Boolean `mainResp`, actual page callback and physical native/transport release. No
fixed first ID, test-injected first caller or canned Bool qualifies. A **separate explicitly controlled second**
`request({action:'isFullscreen'})` after callback **and native/ring release** proves a different ID and quota reuse, not a second
natural page call. A later page UI `toggleFullscreen` requires its own reviewed `el:` grant, never silently inherited. The first
proof permits only `isFullscreen` on `rendererReq`. Local fonts are disabled or separately approved. `toggleFullscreen` uses a
distinct `el:toggleFullscreen` grant. Other actions remain blocked.

#### C15

**Given / when:**
Actual file URL page participates in send/reply

**Pass condition:**
Native committed `file://` document URL, approved package-resource origin and `senderFrame.url` match; a custom-scheme rewrite or
guessed `file:` string cannot satisfy eligibility.

#### C16

**Given / when:**
Application registers a listener after an earlier unsolicited event

**Pass condition:**
Earlier event is not replayed. A later matching event is delivered once; no limitless hidden event queue.

#### C17

**Given / when:**
Preload page navigates, reloads, closes or Bun role recovers

**Pass condition:**
Document-owned proxy/listener retention terminates for a retired document, surviving doc API remains only if host lifecycle admits
it, old queued calls do not replay or retarget, other WebViews stay isolated.

#### C18

**Given / when:**
Attempts to pass `Function`, `Promise`, `Symbol` or other disallowed values through **IPC**

**Pass condition:**
GH-544's supported IPC domain is enforced synchronously with no Bun write; a *local contextBridge function proxy* must not be
misclassified as an IPC payload.

#### C19

**Given / when:**
Unsupported/unknown Electron API or graph exceeding §4.3 depth, entries, strings, proxies, callbacks or retained bytes is
requested

**Pass condition:**
An allocated distinct typed KELD-COMPAT local bridge refusal occurs before partial exposure/retention; no silent truncation,
unauthorized native effect, leaked handle or fabricated parity PASS. 0/max/max+1 independently exercise each bound.

#### C20

**Given / when:**
Compatibility is scored/published

**Pass condition:**
Every mapped GH-464 row has pinned source, expected verdict, implementing-ticket key and recorded device. The fixed draw.io facade
is explicitly labelled a narrower, app-specific divergence; **all unmet doc-cited contextBridge rows** (including general dynamic
functions, copy, Error and Symbol cells, not only C06) stay expected-fail unless separately approved as a documented divergence.
Truly uncited engine observations stay UNKNOWN until qualified. No row flips to PASS from wrapper visibility alone.

#### C21

**Given / when:**
A hostile same-origin page probes preload or KELD internal handlers

**Pass condition:**
Directly forged shared-DOM events, guessed proxy/callback IDs or descriptor changes cannot invoke an unexposed preload function,
bind a native role or produce a forbidden Bun effect. The page cannot read/call the trusted KELD handler, role endpoint, native
stamp, Host grant or raw ipcRenderer. Allowed exposed wrappers still operate within exact Host grants. A hostile page cannot
subscribe to reserved `mainResp` or `fileChanged`, forge their internal delivery, or use the `rendererReq` grant to select another
action.

#### C22

**Given / when:**
Electron v44.4.5 target is used

**Pass condition:**
The doc-versus-native Symbol discrepancy is resolved by **pinned executable** oracle or an explicitly approved documented
divergence before claiming the Symbol cell PASS (GH-464 has same gate).

#### C23 — Request callback terminal cleanup

**Request state:** The page calls `electron.request` with an allowed action and registered callbacks.

**Failure path:** A post-bind local refusal precedes the original preload call. A final Host denial follows `reqInfo[reqId]`. Bun can also retire before `mainResp` while the native document survives.

**Required cleanup:** A post-bind local refusal calls the error callback once before `reqInfo` exists.
Before bind, §4.7 wins: `electron.request` returns `undefined`, invokes neither callback and creates no `reqInfo`.
Its `requestSync` Promise remains pending until document retirement; bind never replays it.
A final Host denial or Bun-loss event sends one private `mainResp` to the same document and exact `reqId`.
Set `error: true` and use sanitized `msg` and `e` values.
The original preload calls its error callback, deletes `reqInfo`, and rejects `requestSync`.
GH-484/542 register the post-assigned `reqId` at the trusted app-world send seam.
GH-543 owns the terminal event on GH-540 role retirement; GH-1095 owns typed error mapping.
Use `KELD-WV-011` only for proved no-dispatch and `KELD-WV-013` for attempted or ambiguous dispatch.
GH-540 charges remain until native/ring quiescence; the event is not a release receipt.
If the document retires first, destroy its realm. Never deliver to a successor.
Direct `ipcRenderer.send` keeps its `undefined` return and no-retroactive-error contract.

**Cleanup oracle:**
- **Fault:** Suppress preflight, request-ID registration or terminal `mainResp`; duplicate it; or bypass cleanup on callback throw.
- **At-risk:** The pinned preload stores `reqInfo[reqId]` before sending `rendererReq` and deletes it only after `mainResp`.
- **Expected failure:** Bun loss leaves `requestSync` pending, repeats callbacks, retains `reqInfo` or delivers to a successor.

The real-KELD pass for C01/C12/C13/C14/C15/C21 is on **macOS
WKWebView** only, with exact build, source hash and native outcome. A mock,
source inspection or green baseline test cannot substitute for it.
