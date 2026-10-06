import json
exec(open('drafts_part1.py').read().split('# ---------------- F01')[0])  # helpers only
DRAFTS={}
# ---------------- F04 ipc bridge ----------------
unit('F04','compat(ipc): ipcMain/ipcRenderer/contextBridge over host-routed el:<channel> frames, enumerated guard grants and a lifted renderer floor','Tier 1',
"""## Scope
15 entities / 77 members: `ipcMain` (9), `ipcRenderer` (11), `contextBridge` (3), `MessageChannelMain`/`MessagePortMain`, `utilityProcess`/`UtilityProcess`/`parentPort` (→ KEL-75 roles), `IpcMainServiceWorker` (never), `webUtils.getPathForFile`, event structures. Live today: one macOS page-world `window.keld.invoke` (channel 1 only, 4096-byte payload, one pending invoke per WebView, host-minted correlation, isolated-world relay) → `@keld/api` echo handler; no ipcMain, no preload injection, no renderer shim; the Unix VirtualPortRegistry (64-message queue, 4 KiB inline, one-shot transfer) has no renderer route.

## Corpus demand (resolved call sites)
draw.io: `ipcRenderer.send`/`on`/`once` + `ipcMain.on` + `event.reply` (15 replies) via one `rendererReq` channel multiplexed by `reqId`; `contextBridge.exposeInMainWorld` (2 keys); `event.senderFrame.url` read by `validateSender`; `webContents.send` 9. Zettlr: `ipcRenderer.invoke` 186 / `ipcMain.handle` 25, `send` 35, `sendSync` 6 (config store construction), `webContents.send` 49, `event.sender` as a WebContents handle, `webUtils.getPathForFile`, `setMaxListeners(100)`. Zero demand for MessageChannelMain/MessagePortMain/utilityProcess/parentPort/IpcMainServiceWorker.

## Maturity ladder
- L0: all 15 entities exist; never/deferred rows published with denominator honesty.
- L1: draw.io's send/on/once/reply pattern and Zettlr's invoke/handle pattern run on macOS over the lifted floor.
- L2: ordering per channel, error envelope (message-only to the renderer, typed code on the wire), `senderFrame` null-after-navigation, stale-generation reply drop, bounded credit/backpressure pass pinned conformance entries.
- L3: corpus apps' IPC workflows pass under `keld dev` with facade-overhead measured (X05).

## Linear owners consumed
KEL-80 (bounded virtual channels: N-pending credit window, host→renderer events — the floor v2), KEL-142 (bridge seam), KEL-136 (transport), KEL-75 (virtual ports, roles), KEL-102 (guard dispatch), SCV codec owner (F03-D4, open), `windows.<w>.channels` grant vocabulary spec (open, no owner today).

## Design facts fixed by research (two refuters + panel)
- Carriage: one fixed, generated kipc control channel per direction for the compat namespace; the Electron channel string, a kind discriminator (event | invoke | sync | ports) and the SCV bytes are payload fields; the host matches the channel string against the per-window enumerated `el:<channel>` grant (`KELD-GUARD003` on miss) and forwards opaque bytes. No runtime-minted u16 ids.
- The renderer→main authorization boundary is the enumerated per-window channel grant, not the preload (both corpus preloads expose arbitrary-channel send/invoke to the page); `keld migrate` emits enumerated grants from static analysis and refuses `el:*`.
- The live KEL-142 floor cannot carry either corpus app; floor v2 (KEL-80 slice) = (a) N pending invokes per WebView under a host-enforced credit window, (b) `window.keld.send` + `window.keld.on` host→renderer events, (c) payload bound raised by measurement (draw.io ships base64 PNG exports through IPC).
- `event.sender` = facade WebContents keyed by host webview id; `event.senderFrame.url` host-minted and null after navigation; `event.reply` binds to the sender document generation (survives same-document hash changes, as Electron keys on the frame host); stale replies are dropped silently like Electron.
- `ipcMain.handle` errors reach the renderer as the message only; the typed `KELD-*` code stays on the wire and in diagnostics.
- `sendSync` must remain a blocking CALL with deadline, rate limit and dev warning (arch 02 §5, KEL-127); it is SCAFFOLDED (typed throw + fix-it to invoke) until the WKWebView synchronous-XHR experiment passes (Zettlr-gated, 0 draw.io sites).
- MessageChannelMain/MessagePortMain/utilityProcess/parentPort are deferred behind zero demand and KEL-75 AC7 (pinned-oracle fixture first); IpcMainServiceWorker/exposeInIsolatedWorld/executeInMainWorld are documented ✘.
""",
[
 T('F04-T1','conformance(ipc): pinned entries for invoke/handle, send/on ordering, reply/senderFrame semantics, error envelope and sendSync contract','conformance','Tier 1','L2',
   'Land red-until-implemented entries citing `ipc-main.md`, `ipc-renderer.md`, `ipc-main-event.md`, `context-bridge.md`: invoke round trip; handler throw → message-only error; per-channel ordering of send/on; `event.reply` bound to the sender frame; `senderFrame` null after navigation; `sendSync` blocks and `returnValue` assignment replies; structured-clone throws on Function/Promise/Symbol/WeakMap/WeakSet; `contextBridge` value table (functions proxied, prototypes dropped, Symbol unsupported).',
   ['Each entry cites the doc sentence and has a negative control','Entries are engine-tagged; observed-only behaviour stays BEHAVIOR_MATCH-capped','No timing comparisons'],
   ['Deleting the ordering guarantee makes the send/on sequence entry fail'],
   ext=['KEL-237 manifest reuse'],owner='KEL-237',gates=['none'],members=['ipcMain.on','ipcMain.once','ipcMain.handle','ipcMain.handleOnce','ipcMain.removeHandler','ipcRenderer.send','ipcRenderer.on','ipcRenderer.once','ipcRenderer.invoke','ipcRenderer.sendSync','IpcMainEvent.reply','IpcMainEvent.sender','IpcMainEvent.senderFrame','IpcMainEvent.returnValue','contextBridge.exposeInMainWorld'],size='M',ready='ready-for-agent',
   current='No IPC conformance entries exist.',interfaces='KEL-237 manifest cells; fixtures main.js + preload.cjs + index.html.',oos='Implementation.'),
 T('F04-T2','feat(bridge): renderer floor v2 — N pending invokes under a host credit window, window.keld.send/on events, measured payload bound (KEL-80 product-spine slice)','feature','Tier 1','L1',
   'Lift the KEL-142 floor as the first KEL-80 slice: host-enforced credit window for N pending renderer calls per WebView (slow consumer gets a typed error, never an unbounded queue), host→renderer event fan-out (`window.keld.on`), fire-and-forget `window.keld.send`, and a payload bound chosen by measurement of draw.io export payloads; keep one HELLO, one principal per link, host-minted correlation.',
   ['Two overlapping invokes from one page both complete (today the second fails immediately)','A page exceeding the credit window receives a typed `KELD-WV`/`KELD-IPC` error and the link stays healthy','Host→renderer event delivery preserves per-channel order under 10k events','Stale-generation (post-navigation) replies are rejected before dispatch'],
   ['Removing the credit window turns the overflow test into an unbounded queue (test fails on memory/ordering assertion)'],
   blocked=['F04-T1'],ext=['KEL-80 (owner; product-spine subset per GitHub #323)','KEL-142 bridge'],owner='KEL-80 / KEL-142',gates=['wire protocol','public API'],members=['window.keld.invoke','window.keld.send','window.keld.on'],size='L',ready='needs-spec',plat='macOS first (WKContentWorld); WebView2/WebKitGTK follow',
   current='invoke only, channel 1 only, 4096 bytes, one pending call per WebView.',interfaces='keld-wv bridge envelope (wv-link v1 → v2); host credit accounting; `@keld/api` renderer client.',oos='Electron facade on top (F04-T3/T4).'),
 T('F04-T3','feat(ipc): ipcMain + webContents.send + event.sender/senderFrame/reply over the el:<channel> control channel with enumerated per-window grants','feature','Tier 1','L2',
   'Implement the main-process side: `ipcMain.on/once/handle/handleOnce/removeHandler`, `webContents.send` routed to one webview principal, `event.sender` (facade WebContents), `event.senderFrame` (host-minted url/origin, null after navigation), `event.reply` bound to the sender document generation, message-only error envelope; the host evaluates `windows.<w>.channels` (enumerated `el:<channel>`) before forwarding.',
   ['draw.io `rendererReq`/`mainResp` multiplexing works unmodified; `validateSender` sees a real `senderFrame.url`','Zettlr `invoke`/`handle` with 186 sites completes mount without `KELD-WV` floor errors','An ungranted channel is refused with `KELD-GUARD003` naming the manifest patch; `el:*` is refused by migrate','Handler throw reaches the renderer as message-only; the wire carries the typed code'],
   ['Removing the grant check lets an ungranted channel through — the negative test fails','Keying reply to the webview id instead of the document generation makes the stale-reply entry fail'],
   blocked=['F04-T1','F04-T2','F03-D4','F04-A1'],ext=['windows.<w>.channels grant spec (open, no owner)','KEL-102 guard dispatch'],owner='KEL-80 / KEL-102',gates=['permission model','wire protocol','public API'],members=['ipcMain.on','ipcMain.once','ipcMain.handle','ipcMain.handleOnce','ipcMain.removeHandler','ipcMain.removeListener','webContents.send','IpcMainEvent.sender','IpcMainEvent.senderFrame','IpcMainEvent.reply','IpcMainInvokeEvent'],size='L',ready='needs-spec',
   current='No ipcMain; `@keld/api` has one echo channel handler.',interfaces='Generated `el` control channel structs (contract owner to be named); keld-guard channel grants; facade EventEmitter semantics.',oos='Renderer shim (F07); sendSync (F04-A5); ports (F04-T5).'),
 T('F04-T4','feat(ipc): contextBridge.exposeInMainWorld and ipcRenderer facade in the app preload world (value table, proxied functions, per-engine isolation claim)','feature','Tier 1','L2',
   'Implement `contextBridge.exposeInMainWorld` once in the renderer runtime: values copied and frozen, functions proxied by id, prototypes dropped, Symbols unsupported, Promise returns supported; `ipcRenderer.send/on/once/invoke/removeListener/removeAllListeners/setMaxListeners`; real isolation on WKWebView/WebKitGTK worlds, labelled page-world emulation ▲ on WebView2 if no isolated world exists; the preload realm is decided by PANEL-D21.',
   ['draw.io preload runs unmodified and `window.electron.request/registerMsgListener/sendMessage/listenOnce` behave as under Electron','Exposed function returning a Promise resolves in the page; a Symbol argument throws like Electron','KEL-142 isolation negative tests still pass with the app preload installed (raw handler/nonce absent from the page)','Per-engine isolation claim recorded as ✔ (WK/WebKitGTK) or ▲ (WebView2)'],
   ['Running the preload in the page world passes functionality but fails the isolation negative test — must fail'],
   blocked=['F04-T2','PANEL-D21','F04-A3'],ext=['KEL-142 §AC isolation tests','KEL-79 for WebView2 isolated worlds'],owner='KEL-142',gates=['public API'],members=['contextBridge.exposeInMainWorld','contextBridge.exposeInIsolatedWorld','ipcRenderer.send','ipcRenderer.on','ipcRenderer.once','ipcRenderer.invoke','ipcRenderer.removeListener','ipcRenderer.removeAllListeners','ipcRenderer.postMessage','webUtils.getPathForFile'],size='L',ready='needs-spec',
   current='No renderer shim exists.',interfaces='Renderer compat user-script (F07-T1 runtime); cross-world relay; engine world APIs.',oos='sendSync (open); preload bundling (F07-T2).'),
 T('F04-T5','task(ipc): publish deferred/never rows for MessageChannelMain, MessagePortMain, utilityProcess, parentPort, IpcMainServiceWorker and pin one Electron oracle for all F04 cells','task','Tier 2','L0',
   'Publish ✘-tracked cells with denominator honesty for MessageChannelMain/MessagePortMain (deferred behind KEL-75 AC7: pinned-oracle fixture first), utilityProcess/UtilityProcess/parentPort (host-declared roles only; PID diagnostic), and documented-never for IpcMainServiceWorker/exposeInIsolatedWorld/executeInMainWorld; resolve the Electron oracle pin drift (lifecycle corpus 44.3.0 vs KEL-75 snapshot vs this map v44.4.5) by recording one pin for all F04 cells.',
   ['Scoreboard rows exist for every entity with status and tracking issue','One oracle pin (v44.4.5 tag commit) is recorded for the family and the drift is documented','`utilityProcess.fork` throws a typed error naming the role declaration path'],
   ['Returning a fake UtilityProcess object fails the honesty entry'],
   blocked=['F04-T1'],ext=['KEL-75 T5/T6'],owner='KEL-75',gates=['none'],members=['MessageChannelMain','MessagePortMain','utilityProcess.fork','UtilityProcess','parentPort','IpcMainServiceWorker'],size='S',ready='ready-for-agent',
   current='None exist.',interfaces='Scoreboard rows; facade throws.',oos='Port facade implementation (KEL-75 T5).'),
],
[
 D('F04-A1','Who owns the `windows.<w>.channels` grant vocabulary and its evaluator (no live evaluator exists; KEL-102 and KEL-80 do not own it)?','grilling',
   'The per-window enumerated `el:<channel>` grant is the renderer→main authorization boundary (panel consensus), but `evaluate` reads only `manifest.app` today and the MCP explain tool refuses `channel`. An approved arch 03 §2 amendment with a named owner is required; the `el:*` example in arch 03 §3 must be revised in the same PR (code/spec mismatch rule).'),
 D('F04-A3','Synchronous cross-world function calls: can a contextBridge-exposed function return a value synchronously on WKWebView/WebKitGTK worlds?','prototype',
   'Zettlr exposes functions whose page-visible return is produced synchronously (config.get via sendSync wrapper). Both worlds share the DOM, so a shared-DOM relay is a lead; unverified. Measure a cross-world relay (per-call cost, sync return feasibility) in the KEL-142 fixture; fallback is async-only exposure (▲) which keeps draw.io config-only.'),
 D('F04-A4','One navigation-generation owner: unify the bridge counter and the guard webview generation','grilling',
   'Two generation counters exist today (bridge navigation generation rotated on navigation start; guard Principal::Webview.generation). senderFrame/reply/stale-reply semantics need exactly one owner minted by the host window registry. Decide the owner and the rotation point (navigation commit vs start).'),
 D('F04-A5','Renderer sendSync mechanism on WKWebView (synchronous XHR to a host scheme handler) — measured feasibility','prototype',
   'Arch 02 §5 and KEL-127 require sendSync to exist as a blocking CALL with deadline, rate limit and dev warning; no engine-portable blocking primitive is known on WKWebView. Experiment: page issues a synchronous XMLHttpRequest to a WKURLSchemeHandler-backed scheme that forwards to a stub reply; record thread behaviour, stall, RTT and deadline; derive the rate limit. Zettlr-gated (6 sites); draw.io has 0.'),
],
['IpcMainServiceWorker / exposeInIsolatedWorld / executeInMainWorld','utilityProcess as raw spawn; PID as identity; inherited env/cwd','runtime-minted kipc channel ids for Electron string channels','wildcard `el:*` channel grants'],
['MessagePortMain facade implementation (KEL-75 T5, after a pinned-oracle fixture)'],
['SCV codec v0 domain and bulk-lane refs for large export payloads (draw.io ≥ 30 Mpx PNG base64)','WebView2 isolated-world availability for the preload runtime'])

# ---------------- F05 session network protocol ----------------
unit('F05','compat(session): custom schemes, permission handlers, net stack, webRequest, session/profile and downloads over host-owned transport and KEL-135 profiles','Tier 2',
"""## Scope
43 entities / 368 members: `Session` (88), `protocol` (18), `net`/`ClientRequest`/`IncomingMessage`/`WebSocket` (51), `DownloadItem` (28), `WebRequest`, `Cookies`, `ServiceWorkers`/`ServiceWorkerMain`, `Extensions`, `netLog`, 29 structures. Live today: no scheme registration (engine custom protocols are builder-time), no session/cookie/download/net facade, keld-guard evaluates `app.net.connect` URL scopes but nothing is wired to it, `web.camera`/`web.microphone` default-deny with a webview principal is live, KEL-135 gives one host-owned persistent profile per signed app (ephemeral in dev), KEL-79 (origin/resource transport) is Backlog with no spec file.

## Corpus demand
Zettlr `protocol.handle('safe-file')` (unprivileged scheme serving readable absolute paths; 23 consumers), `ses.clearStorageData` in 12 window factories, `net.online`; draw.io `session.defaultSession.webRequest.onHeadersReceived` (CSP for all contents) and `onBeforeRequest({urls:['file://*']})`, indirect `net.request` via electron-updater (`session.fromPartition('electron-updater')`); neither app installs a permission handler (Electron default-allow applies to them today); Extensions only in Zettlr's dev path.

## Maturity ladder
- L0: every entity exists with typed errors; never list published.
- L1: one live `Session` (= KEL-135 profile), `clearStorageData` subset, `net.fetch` over the Bun stack (legacy) / host net broker (strict), webRequest listeners as recorded no-ops behind host containment.
- L2: scheme table + `protocol.handle` producer routing, permission inversion, DownloadItem lifecycle pass pinned entries per engine.
- L3: Zettlr `safe-file://` workflow passes under `keld dev`.

## Linear owners consumed
KEL-79 (resource transport, parked), KEL-135 (profile identity; needs a `clear(types)` operation amendment), KEL-78 (strict net containment — decided: strict roles never connect directly), KEL-102 (broker admission), KEL-74/KEL-237 (evidence), arch 05 §2 resource adapter.

## Design facts fixed by research (two refuters)
- Scheme declaration is static host configuration (`keld.config.ts` scheme table lifted by migrate from `registerSchemesAsPrivileged`), consumed by keld-wv at engine construction; `protocol.handle` registers a Bun producer behind the host resource adapter; bytes flow guard → broker; privilege bits are declared for migrate/scoreboard purposes but enforcement is per engine (WKWebView does not implement Electron's non-standard-scheme restrictions; custom schemes have no Service Workers on WKWebView).
- No permission handler ⇒ guard default-deny for the requesting webview principal (▲ against Electron auto-approve); an installed handler is consulted only after the guard allows and can only narrow; permission strings are engine-hook-dependent (a third column: guard-evaluable vs engine-decided).
- `net.*` wraps the Bun HTTP stack (never the engine session) under legacy; under strict it is a facade over a host net broker (new keld-native `net` row + spec; KEL-78 decides strict roles never connect directly). `useSessionCookies:true` is a typed ▲.
- `session.webRequest` is not emulated generically (only WebView2 could); listeners exist at L0 with typed errors, become recorded no-ops ▲ only after host file:// containment + always-on CSP injection land with a negative control (draw.io's two uses migrate via generated policy).
- v1 has exactly one `Session` = the KEL-135 profile; `fromPartition('persist:x')`, in-memory partitions, `fromPath` are typed ▲ until KEL-135 grows; `clearStorageData/clearCache` map to a KEL-135 host-side profile clear operation (not a new keld-native module).
- Never: Extensions/loadExtension, netLog, ServiceWorkerMain, network emulation, certificate verify overrides (legacy-profile question is owner-decided), proxy/connection control assigned explicitly.
""",
[
 T('F05-T1','conformance(session): pinned entries for permission inversion, scheme handling, webRequest honesty, session identity and DownloadItem lifecycle','conformance','Tier 2','L2',
   'Land red-until-implemented entries: missing permission handler ⇒ deny (▲ vs Electron auto-approve); handler can only narrow; `protocol.handle` request/response shape and before-ready registration rules; `registerSchemesAsPrivileged` once-before-ready; `session.defaultSession === webContents.session`; `fromPartition("")` returns default; `clearStorageData` storage names; DownloadItem `updated`/`done` states.',
   ['Each entry cites the pinned session.md/protocol.md/download-item.md sentence with a negative control','Engine-decided permissions are tagged per engine'],
   ['Making a missing handler allow fails the inversion entry'],
   ext=['KEL-237 manifest reuse'],owner='KEL-237',gates=['none'],members=['setPermissionRequestHandler','setPermissionCheckHandler','protocol.handle','protocol.registerSchemesAsPrivileged','session.defaultSession','session.fromPartition','clearStorageData','clearCache','DownloadItem'],size='M',ready='ready-for-agent',
   current='None exist.',interfaces='KEL-237 cells; fixtures.',oos='Implementation.'),
 T('F05-T2','feat(protocol): static host-owned scheme table + protocol.handle Bun producer behind the host resource adapter (Zettlr safe-file://)','feature','Tier 2','L2',
   'Add a `keld.config.ts` scheme table (name + Electron privilege bits for migrate/scoreboard), registered by keld-wv at engine construction on all three engines; `protocol.handle(scheme, handler)` registers the Bun producer that the host resource adapter invokes with a bounded request and streams a bounded response through guard → broker; `registerSchemesAsPrivileged` in app code becomes a no-op-with-check (must match the declaration).',
   ['Zettlr `safe-file://` loads its 23 consumers\' resources through the Bun handler with host-enforced read scopes','A scheme declared privileged in app code but absent from the table fails before ready with a typed error naming the config patch','Per-engine privilege-bit enforcement differences are recorded ▲ (e.g. no Service Workers on WKWebView custom schemes)','Responses above the broker ceiling stream in bounded chunks or fail typed'],
   ['Letting app code mint a scheme at runtime fails the never-list negative test'],
   blocked=['F05-T1'],ext=['KEL-79 resource adapter','KEL-130 FsBroker ceilings','KEL-15 config schema'],owner='KEL-79',gates=['permission model','wire protocol'],members=['protocol.handle','protocol.unhandle','protocol.isProtocolHandled','protocol.registerSchemesAsPrivileged','CustomScheme','ProtocolRequest','ProtocolResponse'],size='L',ready='needs-spec',
   current='No scheme registration exists.',interfaces='keld-wv scheme registration per engine; host resource adapter; Bun producer channel.',oos='file:// containment (F05-T4); Service Worker support on custom schemes (engine limit).'),
 T('F05-T3','feat(session): permission request/check facade as one guard decision (missing handler = deny; handlers only narrow) with per-engine hook coverage table','feature','Tier 2','L2',
   'Evaluate `windows.<w>.web.<permission>` as the webview principal before any app handler; invoke request and check handlers after an Allow; a handler return of true after a guard deny is ignored with a diagnostic; publish the permission-string table with guard-evaluable vs engine-decided columns per engine (WKUIDelegate: geolocation/media/orientation; WebView2: 14 kinds; WebKitGTK: 10).',
   ['Camera request without a handler is denied with `KELD-GUARD` text naming the manifest patch','With a grant and a narrowing handler, the handler\'s false denies','Engine-decided permissions are recorded ▲ per engine, never silently allowed'],
   ['Letting the handler widen a guard deny fails the negative test'],
   blocked=['F05-T1','X04-D1'],ext=['KEL-102 window-level grants','KEL-132 media','KEL-79 requestingUrl ownership'],owner='KEL-102 / KEL-132',gates=['permission model'],members=['setPermissionRequestHandler','setPermissionCheckHandler','setDevicePermissionHandler','setDisplayMediaRequestHandler'],size='M',ready='needs-spec',
   current='Media requests deny for webview principals; nothing else exists.',interfaces='keld-guard windows.<w>.web grants; keld-wv permission hooks; facade handlers.',oos='Device permissions (HID/USB/serial/Bluetooth): documented-unsupported.'),
 T('F05-T4','feat(session): host file:// containment + always-on CSP injection so draw.io webRequest listeners become recorded no-ops with a negative control','feature','Tier 1','L2',
   'Load `file://` apps with a host-scoped read root (macOS `loadFileURL:allowingReadAccessToURL:` scoped to the app code dir; WebView2 virtual host mapping; WebKitGTK equivalent) and inject the generated CSP (admitting `wasm-unsafe-eval` etc. as declared) so that `webRequest.onBeforeRequest/onHeadersReceived` listeners are accepted as recorded no-ops ▲ only once the host policy is proven at least as strict as draw.io\'s own filter.',
   ['`fetch("file:///etc/passwd")` and `<img src="file:///etc/hosts">` from the page are refused before bytes load','draw.io boots over file:// with its CSP equivalent applied and its listeners recorded as ▲ no-ops','Removing the containment makes the negative control (outside-codeDir read) succeed — test must fail'],
   ['Disabling CSP injection fails the policy-strength entry'],
   blocked=['F05-T1'],ext=['KEL-79','arch 03 §2 windows.<w>.web.csp'],owner='KEL-79',gates=['permission model'],members=['webRequest.onBeforeRequest','webRequest.onHeadersReceived','webRequest.onBeforeSendHeaders','webRequest.onCompleted','webRequest.onErrorOccurred'],size='L',ready='needs-spec',
   current='No file:// concept beyond a URL navigation target; no CSP injection.',interfaces='keld-wv file load with read root per engine; CSP injection; migrate policy emission.',oos='Generic webRequest emulation (refused: single-platform partial).'),
 T('F05-T5','feat(session): one live Session = KEL-135 profile; clearStorageData/clearCache via a host profile clear operation; partitions typed ▲','feature','Tier 2','L1',
   '`session.defaultSession` and `webContents.session` resolve to the KEL-135 profile (ephemeral under dev; `isPersistent()` truthful); `clearStorageData/clearData/clearCache` map Electron storage names onto a KEL-135 amendment `profile.clear(types)` (WKWebsiteDataStore / WebView2 profile / WebKitGTK data manager); `fromPartition("persist:x")`, in-memory partitions, `fromPath`, `webPreferences.partition/session` are typed ▲; cookies read/write exposure decided per engine.',
   ['Zettlr window factories calling `clearStorageData({storages:[...]})` succeed and the data is gone on relaunch','`fromPartition("")` returns the default session; `persist:x` throws the typed ▲','Dev profile reports `isPersistent() === false`'],
   ['Minting a second profile identity for a partition fails the one-profile negative test'],
   blocked=['F05-T1'],ext=['KEL-135 amendment (profile clear op)'],owner='KEL-135',gates=['public API'],members=['session.defaultSession','session.fromPartition','session.fromPath','clearStorageData','clearData','clearCache','isPersistent','getStoragePath','cookies.get','cookies.set','cookies.remove'],size='M',ready='needs-spec',
   current='One host-owned profile per signed app; no facade.',interfaces='KEL-135 profile operations; facade Session class.',oos='Multi-profile partitions.'),
 T('F05-T6','task(session): publish the documented-never and ▲ rows (Extensions, netLog, ServiceWorkerMain, network emulation, certificate overrides, proxy/connection control) and the net.* strict/legacy routing decision','task','Tier 3','L0',
   'Publish rows: Extensions/loadExtension ✘ (no engine extension runtime on two of three engines), netLog ✘, ServiceWorkerMain ✘, network emulation ✘, certificate verify overrides (owner-decided legacy question), `setProxy/resolveProxy/closeAllConnections/setUserAgent` assignment; record that `net.request/fetch/WebSocket` wrap the Bun stack under legacy and a host net broker under strict (new keld-native `net` row + spec), with `useSessionCookies:true` ▲.',
   ['Every F05 entity has a scoreboard row','`net.fetch` under legacy replays Electron event ordering (response/finish/abort/error/close) in a conformance entry','Strict-profile net calls fail typed until the broker spec lands'],
   ['Routing net.* through the engine session fails the "Bun stack, not engine" entry'],
   blocked=['F05-T1'],ext=['KEL-78 strict net decision','arch 05 §3 net row (new)'],owner='KEL-78',gates=['none'],members=['net.request','net.fetch','net.WebSocket','net.online','net.isOnline','ClientRequest','IncomingMessage','netLog','Extensions','ServiceWorkers','ServiceWorkerMain','setProxy','resolveProxy','setUserAgent'],size='S',ready='ready-for-agent',
   current='None exist.',interfaces='Scoreboard; facade over Bun fetch/WebSocket.',oos='Host net broker implementation (needs spec).'),
],
[
 D('F05-D3','Strict-profile network: host net broker shape and enforcement location (Linux netns + host proxy; Windows LPAC; macOS unknown)','research',
   'KEL-78 decides strict roles never connect directly; `net.*` under strict needs a host net broker (new arch 05 §3 row + spec). Windows has a live all-or-nothing LPAC egress primitive; macOS is unknown. Produce a three-legged prototype plan and the broker contract before any strict net cell is claimed.'),
],
['Extensions / loadExtension / extension-* events','netLog','ServiceWorkerMain (experimental, Electron-internal)','network emulation / enableNetworkEmulation','certificate-error callback(true) / setCertificateVerifyProc overrides in strict','app-minted scheme privileges at runtime (bypassCSP)'],
['Device choosers (HID/USB/serial/Bluetooth) — documented-unsupported, no broker, no principal'],
['Resource size ceilings for scheme producers (8 MiB broker content, 4096-byte inline ports) and the bulk lane','DownloadItem lifecycle over host downloads (zero corpus demand beyond Zettlr updater)'])

# ---------------- F06 native desktop ----------------
unit('F06','compat(native): dialog, shell, Menu/Tray, clipboard, screen, nativeTheme, nativeImage, Notification, safeStorage and power/shortcut brokers behind generated grants','Tier 1',
"""## Scope
38 entities / 350 members, all main-process: Menu (13), MenuItem (21), Tray (36), dialog (8), Notification (30), clipboard/ClipboardItem (11), shell (7), screen/Display (31), globalShortcut (7), powerMonitor (16), powerSaveBlocker (3), nativeTheme (9), nativeImage/NativeImage (24), systemPreferences (28), safeStorage (8), pushNotifications/inAppPurchase (mac), desktopCapturer (Tier 3), ShareMenu. Live today: keld-native ships only the retained FsBroker (fs.read/fs.write) and a MODULES name list (`capture` is missing from it: code/spec gap); no F06 channel is reachable until KEL-102/T3 admission; `@keld/electron` exports only `app`.

## Corpus demand
dialog (draw.io showMessageBox ×7 incl. one `showMessageBoxSync` on the close path, open/save ×4 each; Zettlr showMessageBox 24, dialog in 21 files) > shell (openExternal, openPath, showItemInFolder, trashItem) > Menu (both: buildFromTemplate + setApplicationMenu; 13 roles in draw.io; 45 accelerators per Zettlr platform menu; popup) > screen > clipboard (draw.io uses the W3C promise shape with ClipboardItem; Zettlr sync readText) > nativeTheme (themeSource, `updated`) > nativeImage (createFromDataURL().toPNG(), createFromPath) > Tray/Notification (Zettlr) > systemPreferences (accent colour, theme-changed notification). Zero use of globalShortcut, powerMonitor, powerSaveBlocker, safeStorage, pushNotifications, inAppPurchase, ShareMenu, desktopCapturer.

## Maturity ladder
- L0: every module exists; unsupported members throw typed errors; `capture` added to MODULES or named as intentionally absent.
- L1: dialog/shell/Menu/screen/clipboard/nativeTheme/nativeImage work for draw.io and Zettlr on macOS through guarded brokers.
- L2: dialog-minted scopes, menu role execution host-side, accelerator semantics, theme `updated` event, clipboard shapes pass pinned entries on each OS.
- L3: corpus workflows pass under `keld dev` with KEL-74 records and `app.system` grants recorded.

## Linear owners consumed
KEL-130 (FsBroker retained scopes — dialog grants extend the same set), KEL-102 (broker admission; the guard's accepted operation set is hard-coded to fs.read/fs.write today), KEL-89 (secrets/auth), KEL-78 (strict: no keychain reach from the child), KEL-132, KEL-208/KEL-209 (URL-scope authority rule), KEL-19/KEL-103 (Windows AppUserModelId, bundle metadata).

## Design facts from research (NOT yet independently refuted — batch 2 refutes; decisions below stay open)
- Host-owned UI handles: every Menu/MenuItem/Tray/Notification/popup is a host main-thread resource with a host-assigned `(generation, id)`; facade proxies are write-through; clicks arrive as host→Bun messages; `role` items execute host-side without a round trip (Electron ignores `click` when `role` is set).
- Resource-less capabilities keep the arch 03 §2 literal shape `app.system: ["clipboard.read", …]` evaluated with the existing matcher (operation=system, path=literal) — no guard schema change.
- NativeImage is a Bun-side immutable value object (bytes + sniffed format + size + representations); decode/encode happens host-side only at consumption; clipboard baseline is the v44.4.5 W3C shape.
- safeStorage is a thin facade over a host `secrets` module; Linux `basic_text` plaintext degradation is reported, never emulated.
- Blocking dialogs (`showMessageBoxSync`, draw.io close path) depend on the panel's S0 link-drain gate; until then they are SCAFFOLDED (typed throw + fix-it) or served through the worker-owned transport once approved.
""",
[
 T('F06-T1','conformance(native): pinned entries for dialog results, menu role/accelerator semantics, nativeTheme updated, clipboard shapes, screen displays and nativeImage conversions','conformance','Tier 1','L2',
   'Land red-until-implemented entries citing dialog.md/menu.md/menu-item.md/native-theme.md/clipboard.md/screen.md/native-image.md: dialog cancel vs selection results; `role` ignores `click`; accelerator normalisation per platform; `nativeTheme.themeSource` writes and `updated`; clipboard write/read text + W3C `ClipboardItem`; `screen.getPrimaryDisplay().workArea`; `createFromDataURL().toPNG()` byte equality for PNG input.',
   ['Each entry cites the doc sentence with a negative control','Platform-tagged members carry per-OS entries or explicit gaps'],
   ['Executing a role item through the facade click path fails the role entry'],
   ext=['KEL-237 manifest reuse'],owner='KEL-237',gates=['none'],members=['dialog.showOpenDialog','dialog.showSaveDialog','dialog.showMessageBox','dialog.showErrorBox','Menu.buildFromTemplate','Menu.setApplicationMenu','Menu.popup','MenuItem','nativeTheme.themeSource','nativeTheme.updated','clipboard.readText','clipboard.writeText','clipboard.write','ClipboardItem','screen.getPrimaryDisplay','screen.getAllDisplays','nativeImage.createFromDataURL','NativeImage.toPNG','NativeImage.resize'],size='M',ready='ready-for-agent',
   current='None exist.',interfaces='KEL-237 cells; fixtures.',oos='Implementation.'),
 T('F06-T2','feat(native): guarded dialog broker (open/save/message/error) parented to host windows, with dialog-minted generation-bound fs scopes','feature','Tier 1','L2',
   'Implement a keld-native `dialog` broker: NSOpen/NSSave/NSAlert on macOS (Win32/GTK/portal later), parented to the host window the facade names; chosen paths are returned AND registered as generation-bound retained scopes in the same FsBroker set KEL-130 built (open → read leaf, save → write leaf plus a declared sibling pattern for draw.io\'s `.bkp`), revoked on generation death, never persisted; `showMessageBox` async first; `showMessageBoxSync` SCAFFOLDED until PANEL-P1 decides the blocking transport.',
   ['draw.io open → edit → save completes through the broker with no manifest widening','A path chosen in the dialog is readable by the app; a sibling outside the declared pattern is denied with `KELD-GUARD` text','After a Bun generation restart the previous dialog grants are gone (typed deny)','`showMessageBoxSync` throws the typed SCAFFOLDED error naming the invoke-style fix-it until the transport decision lands'],
   ['Persisting dialog grants across generations fails the revocation negative test'],
   blocked=['F06-T1','F06-D1','F02-T2'],ext=['KEL-130 FsBroker','KEL-102/T3 admission','KEL-140 first guarded fs op'],owner='KEL-130 / KEL-102',gates=['permission model','wire protocol'],members=['dialog.showOpenDialog','dialog.showOpenDialogSync','dialog.showSaveDialog','dialog.showSaveDialogSync','dialog.showMessageBox','dialog.showMessageBoxSync','dialog.showErrorBox','dialog.showCertificateTrustDialog','FileFilter'],size='L',ready='needs-spec',
   current='Only fs.read/fs.write brokers exist; no dialog.',interfaces='keld-native dialog broker; FsBroker retained-scope mint; facade dialog module.',oos='Blocking transport (PANEL-P1); certificate trust dialog (▲).'),
 T('F06-T3','feat(native): shell broker (openExternal with a confirm-per-scheme grant, openPath, showItemInFolder, trashItem, beep) under app.shell grants','feature','Tier 1','L2',
   'Implement `shell.openExternal` as a guarded operation whose broker canonicalises the URL and evaluates exact literals plus a confirm-class grant (host-rendered confirmation showing the full URL for `https:`/`mailto:`/`tel:` when no literal scope matches; `file:`/`javascript:`/executable launchers never); `openPath`/`showItemInFolder`/`trashItem` require fs scopes for the target; Windows 2081-char limit enforced; renderer-triggered calls carry the webview principal.',
   ['draw.io/Zettlr help links open after confirmation; an `https://docs.example/**` literal opens silently','`openExternal("file:///…")` is refused with a typed error','`trashItem` outside any fs scope is denied with the manifest patch text'],
   ['Removing the scheme canonicalisation lets `HTTPS://evil` bypass the literal match — negative test fails'],
   blocked=['F06-T1','X04-D4'],ext=['KEL-208/KEL-209 authority rule'],owner='KEL-102',gates=['permission model'],members=['shell.openExternal','shell.openPath','shell.showItemInFolder','shell.trashItem','shell.beep','shell.readShortcutLink','shell.writeShortcutLink'],size='M',ready='needs-spec',
   current='No shell broker.',interfaces='keld-native shell broker; keld-guard `app.shell.open` grant evaluation (spelling decided by X04-D4).',oos='Windows shortcut link APIs beyond ▲.'),
 T('F06-T4','feat(native): Menu/MenuItem/Tray as host-owned handles — template submission, host-side role execution, accelerators, popup, Tray click events','feature','Tier 1','L2',
   'Submit one bounded menu tree per `setApplicationMenu`/`popup`/`Tray.setContextMenu`; the host assigns command ids, executes `role` items host-side, normalises accelerators per platform, emits click events with `(commandId, focusedWindowId, KeyboardEvent)`; Tray icon/tooltip/menu with click/right-click/double-click events; `app.system: ["tray"]` grant.',
   ['draw.io application menu (13 roles) and Zettlr platform menus (45 accelerators) build and respond on macOS','`Menu.popup({window})` shows at the cursor and resolves `callback` after close','Tray click delivers the Electron event shape; a Tray without the `tray` grant is denied typed'],
   ['Executing `role` items via Bun fails the host-side role entry'],
   blocked=['F06-T1','F02-T2'],ext=['KEL-102 admission'],owner='KEL-102',gates=['permission model','wire protocol'],members=['Menu','MenuItem','Menu.buildFromTemplate','Menu.setApplicationMenu','Menu.getApplicationMenu','Menu.popup','Menu.closePopup','Tray','Tray.setContextMenu','Tray.setToolTip','Tray.setImage','MenuItemBadge'],size='L',ready='needs-spec',
   current='No menu or tray broker.',interfaces='keld-native menu/tray brokers; facade proxies with host ids.',oos='ShareMenu, TouchBar (Tier 2).'),
 T('F06-T5','feat(native): screen, nativeTheme, clipboard and nativeImage facades (host-pushed mirrors for displays/theme; W3C clipboard shape; Bun-side NativeImage value object)','feature','Tier 1','L1',
   '`screen.getPrimaryDisplay/getAllDisplays/getDisplayMatching/getCursorScreenPoint` from a host-pushed display mirror with `display-added/removed/metrics-changed`; `nativeTheme.shouldUseDarkColors/themeSource` mirror with `updated`; clipboard read/write text/html/image via a broker under `app.system` clipboard grants (W3C promise shape baseline; sync `readText` served from a changeCount-refreshed mirror ▲); NativeImage as an immutable Bun value object with host-side decode only at consumption.',
   ['draw.io reads `nativeTheme.shouldUseDarkColors` synchronously at window construction and receives `updated` on OS theme change','`clipboard.writeText` then `readText` round-trips under the grant; without the grant denied typed','`nativeImage.createFromDataURL(png).toPNG()` returns the input bytes; `createFromPath` uses one fs.read'],
   ['Serving displays by a per-read host CALL fails the zero-traffic mirror entry'],
   blocked=['F06-T1'],ext=['KEL-102 admission'],owner='KEL-102',gates=['permission model'],members=['screen.getPrimaryDisplay','screen.getAllDisplays','screen.getDisplayMatching','screen.getCursorScreenPoint','Display','nativeTheme.shouldUseDarkColors','nativeTheme.themeSource','nativeTheme.updated','clipboard.readText','clipboard.writeText','clipboard.readHTML','clipboard.writeHTML','clipboard.readImage','clipboard.writeImage','clipboard.clear','clipboard.write','clipboard.read','ClipboardItem','nativeImage.createFromDataURL','nativeImage.createFromPath','nativeImage.createEmpty','NativeImage.toPNG','NativeImage.toJPEG','NativeImage.toDataURL','NativeImage.getSize','NativeImage.resize','NativeImage.isEmpty'],size='L',ready='needs-spec',
   current='None exist.',interfaces='keld-native screen/clipboard brokers; host-pushed mirrors; facade value objects.',oos='Clipboard change polling beyond ▲; bookmark APIs.'),
 T('F06-T6','task(native): Notification, safeStorage (secrets broker), globalShortcut, powerMonitor/powerSaveBlocker, systemPreferences, pushNotifications/inAppPurchase/ShareMenu/desktopCapturer rows and tiering','task','Tier 2','L0',
   'Publish rows and typed facades: Notification via a host notify broker (macOS UNUserNotification; Windows toast; libnotify) under the `notifications` grant; safeStorage over a host `secrets` module (Linux `basic_text` reported, never emulated); globalShortcut with conflict detection; powerMonitor/powerSaveBlocker; systemPreferences accent colour + theme notification subset; pushNotifications/inAppPurchase (MAS) ✘-tracked; ShareMenu ▲; desktopCapturer Tier 3; add `capture` to the native module list or document its absence.',
   ['Every F06 entity has a scoreboard row with status and tracking','Zettlr Notification click round-trips on macOS once the broker lands; until then the typed error names this ticket','`capture` code/spec mismatch is closed in one PR'],
   ['Emulating safeStorage with plaintext fails the never-list negative test'],
   blocked=['F06-T1'],ext=['KEL-89 secrets','KEL-78'],owner='KEL-89 / KEL-78',gates=['permission model'],members=['Notification','NotificationAction','safeStorage','globalShortcut','powerMonitor','powerSaveBlocker','systemPreferences','pushNotifications','inAppPurchase','ShareMenu','desktopCapturer'],size='M',ready='ready-for-agent',
   current='None exist.',interfaces='Scoreboard; facade throws; native module list.',oos='Implementations beyond Notification and safeStorage facades.'),
],
[
 D('F06-D1','Dialog-minted filesystem scopes: generation-bound session grants in the FsBroker retained set (sibling pattern for draw.io .bkp) — approve the permission-model shape','grilling',
   'Proposed (unrefuted): the host dialog returns paths and registers each as a generation-bound fs scope into KEL-130\'s retained set (open → read leaf, save → write leaf + declared sibling pattern), revoked on generation death, never persisted. Needs an independent refutation (batch 2) and a permission-model review; the user-granted-roots-survive-restart question is a separate KEL-79/KEL-130 spec gap.'),
 D('F06-D3','Resource-less capability grants (clipboard, notifications, tray, shortcuts, power): keep `app.system` literals evaluated with the existing matcher?','grilling',
   'Proposed (unrefuted): evaluate `app.system` literals as operation=system, path=literal with the existing matcher — zero guard schema change. The guard\'s accepted operation set is hard-coded to fs.read/fs.write today, so admission for new operations is a KEL-102 change; confirm with the owner.'),
 D('F06-D4','shell.openExternal confirm-per-scheme grant (`confirm:https`) vs exact literals only — permission-model decision (joint with X04-D4)','grilling',
   'Median apps open arbitrary https/mailto/tel links; scheme-wide globs are inert by the KEL-208 authority rule. Proposed: a confirm-class grant (host-rendered confirmation showing the full URL) alongside literal scopes; `file:`/`javascript:` never. Needs KEL-208 owner ruling on spelling; the broker must canonicalise before matching.'),
],
['renderer-process direct clipboard/shell access (webview principal)','scheme-wide `shell.open` globs','safeStorage plaintext emulation','systemPreferences post*Notification','Notification.toastXml raw XML','auto-prompting macOS accessibility trust','app-chosen Tray GUID / AppUserModelID strings'],
['TouchBar (macOS) and JumpList/Thumbar (Windows) until demand appears','inAppPurchase (MAS) and pushNotifications beyond ✘ rows'],
['Windows and Linux dialog backends (Win32 common dialogs; XDG portals) and portal-first behaviour for sandboxed formats','Menu accelerator conflict handling with globalShortcut'])

# ---------------- F07 renderer / webview tag ----------------
unit('F07','compat(renderer): preload runtime, sandboxed-preload require shim, process polyfill, disabled-<webview> contract, browser-quirk ledger','Tier 1',
"""## Scope
`<webview>` tag (113 members, Tier 3) plus the renderer-side compat user-script: `ipcRenderer`/`contextBridge`/`webFrame` subset, `process` polyfill (`type='renderer'`, versions, argv from `additionalArguments`, platform/arch/env policy), `webUtils.getPathForFile`, preload execution order. Live today: macOS KEL-142 isolated `WKContentWorld` script + frozen page-world facade (`window.keld.invoke` only), main-frame-only, document start; no app-preload injection; no `@keld/web`.

## Corpus demand (corrected: draw.io DISABLES `<webview>` everywhere — all 7 hits are `webviewTag:false` or a `will-attach-webview` preventDefault)
draw.io preload: `contextBridge.exposeInMainWorld('electron', {request, registerMsgListener, sendMessage, listenOnce})` over `ipcRenderer.send/on/once`, `process.type/versions`, `--initial-adaptive-colors=` from `process.argv`, localStorage seeding, a Navigation-API guard. Zettlr preload: ipc {send, sendSync, invoke, on → off closure}, config get/set via sendSync, `process` {platform, version, versions, arch, uptime, getSystemVersion, env copy, argv}, `webUtils.getPathForFile`; `sandbox:false` on two windows that still only use the sandboxed allow-list.

## Maturity ladder
- L0: `webviewTag` accepted and defaulted false; `<webview>` is an unknown element; `will-attach-webview` registrable (never fires).
- L1: draw.io's preload runs byte-for-byte unmodified in a Keld content world and its page round trips work on macOS.
- L2: preload order (before page scripts), sandboxed `require` allow-list, process polyfill fields, isolation negative tests, Error/Symbol value-table semantics pass pinned entries per engine (✔ WK/WebKitGTK; ▲ WebView2 where no isolated world exists).
- L3: Zettlr preload (sync-returning wrappers) handled per the F04-A3/A5 decisions.

## Linear owners consumed
KEL-142 (world/injection seam; spec revision for a third world and the relay), KEL-79 (WebView2 worlds, origin), KEL-80 (floor v2), KEL-132.

## Design facts fixed by research (invariants refuter; semantics refuter pending batch 2)
- Ship the renderer user-script + disabled-webview contract first; `<webview>` element stays Tier 3 as a host-owned child webview mapping (never a DOM-embedded engine), with zero corpus pressure.
- The preload runs in a dedicated app content world (third world) registered at document start on the same per-view user-content controller as the KEL-142 scripts, with a relay no broader than `window.keld.*`; the KEL-142 isolation negative tests must pass with the preload present (panel contested decision PANEL-D21 decides same-world vs third-world on measured results).
- `require('electron')` resolves exactly the sandboxed-preload module map (contextBridge, crashReporter, ipcRenderer, nativeImage, webFrame, webUtils) plus events/timers/url and the globals Buffer/process/setImmediate/clearImmediate; preload bytes are host-loaded from the bundle-relative path inside the content-authenticated package (never an absolute path read).
- `process.env` re-exposure to the page (Zettlr) is an information-flow decision with no owner (open).
- Migrated apps change origin when `file://` or a custom scheme is replaced; web-storage continuity is a product decision (open F07-A6).
""",
[
 T('F07-T1','feat(renderer): preload runtime — app content world, sandboxed require shim, process polyfill, document-start ordering, isolation negative tests re-run with the preload present','feature','Tier 1','L2',
   'Inject the bundled app preload at document start into the app content world (per PANEL-D21), provide `require` for exactly the sandboxed-preload module map plus Node globals, a `process` polyfill (type=renderer, versions, argv from `additionalArguments`, platform/arch, `getSystemVersion`, `uptime`; `env` per the open decision), and prove the KEL-142 isolation negative tests still pass; main-frame-only by default.',
   ['draw.io `electron-preload.js` runs unmodified; `window.electron.request` round-trips through F04','`require("fs")` inside the preload throws the Electron sandbox error; `require("electron").contextBridge` exists','Page world cannot reach the native handler, nonce, endpoint or token with the preload installed (KEL-142 negative tests green)','Preload referenced by an absolute path outside the package is refused typed'],
   ['Injecting after page scripts fails the ordering entry','Exposing the raw bridge handler through the preload world fails the isolation test'],
   blocked=['F04-T2','PANEL-D21'],ext=['KEL-142 spec revision (third world/relay)','KEL-79 for WebView2 worlds'],owner='KEL-142 / KEL-79',gates=['public API','permission model'],members=['preload (webPreferences)','additionalArguments','process.type','process.versions','process.argv','process.platform','process.arch','process.sandboxed','process.contextIsolated','webUtils.getPathForFile','webFrame.setZoomFactor','webFrame.getZoomFactor'],size='L',ready='needs-spec',plat='macOS first (WKContentWorld); WebKitGTK named worlds; WebView2 ▲ until an isolated world exists',
   current='No app preload injection; only the KEL-142 bridge scripts.',interfaces='keld-wv user-script registration per view/world; preload bundling by migrate; renderer runtime script.',oos='sendSync (F04-A5); `<webview>` (F07-T3).'),
 T('F07-T2','task(renderer): disabled-<webview> contract — webviewTag defaults false, <webview> is an unknown element, will-attach-webview registrable; publish the browser-quirk ledger','task','Tier 1','L0',
   'Accept `webPreferences.webviewTag` (default false) and record it; `<webview>` renders as an unknown element; `will-attach-webview` can be registered and never fires; publish the per-engine browser-quirk ledger (Service Workers on custom schemes, iframe origins, clipboard permissions, drag/drop, IME/focus, WebGL/WebGPU, codecs, media capture, storage, WebRTC) as scoreboard ▲/✘ rows with receipts.',
   ['draw.io with `webviewTag:false` and its `will-attach-webview` handler boots without errors','The quirk ledger lists every quirk with an engine column and a receipt','`<webview>` usage in a migrated app is reported ✘ by migrate with the Tier-3 tracking issue'],
   ['Rendering `<webview>` as an iframe fails the honesty entry'],
   blocked=[],owner='KEL-79',gates=['none'],members=['webviewTag','will-attach-webview','webviewTag element (113 members, Tier 3)'],size='S',ready='ready-for-agent',
   current='Nothing exists.',interfaces='BrowserWindow option handling; scoreboard.',oos='A real `<webview>` mapping (F07-T3).'),
 T('F07-T3','feat(renderer): <webview> tag as a host-owned child webview routed through the same bridge (Tier 3; no corpus demand)','feature','Tier 3','L1',
   'Map `<webview>` to one host-owned child WebviewId per element (bounds synced from the element rect), routed through the same `window.keld` bridge with its own principal and `channels: []`; attributes `src`, `preload`, `partition`, `allowpopups`, `webpreferences` mapped or ▲; methods/events subset by demand.',
   ['A `<webview src>` element shows a child webview with its own principal; `will-attach-webview` can veto','Guest IPC is isolated from the host page (negative test)'],
   ['Sharing the host page principal with the guest fails the isolation test'],
   blocked=['F07-T2','F02-T2','F04-T2'],ext=['KEL-79','KEL-75 window-bound roles'],owner='KEL-79',gates=['permission model','wire protocol'],members=['webviewTag (113 members)'],size='L',ready='needs-spec',
   current='Nothing exists; Tier 3 per arch 04 §4.',interfaces='Host child webview minting; bridge principal routing.',oos='Guest process model emulation.'),
],
[
 D('F07-A6','Web-storage continuity when `keld migrate` replaces `file://`/custom-scheme origins: empty first launch (▲) or a one-shot host-side storage import?','grilling',
   'A migrated app changes origin, so localStorage/IndexedDB keyed to the old origin become invisible. Decide: declare "first launch under Keld starts with empty web storage" as a visibly labelled ▲ in the migrate report, or specify an engine-specific one-shot import (WK/WebView2/WebKitGTK data stores). draw.io stores `.configuration` and recents in localStorage.'),
 D('F07-E1','process.env re-exposure to the page (Zettlr copies process.env into the renderer) — information-flow policy','grilling',
   'On Keld the renderer has no process; the shim must synthesise `env`. Copying the host environment into the page leaks secrets; omitting it may break apps that read `process.env.NODE_ENV`. Decide the allow-list (e.g. NODE_ENV only) and the migrate report wording.'),
],
['`<webview>` as a DOM-embedded engine or iframe emulation presented as a webview','preload executed in the page world to obtain synchronous contextBridge semantics','absolute-path preload reads outside the package'],
['`webFrame` full surface beyond the zoom subset (zero demand)'],
['WebView2 isolated-world availability (▲ emulation vs real)','Per-engine `beforeunload` primitive (0 demand)'])
json.dump(DRAFTS,open('drafts_part2.json','w'),indent=1); print('part2 units:',list(DRAFTS),'tickets:',sum(len(u['tickets']) for u in DRAFTS.values()))
