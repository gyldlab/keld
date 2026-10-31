# Zettlr second-app milestone — macOS, explicit legacy profile

Planning record for the Electron-compat map [#391](https://github.com/gyldlab/keld/issues/391),
published 2026-10-31 by an AI agent (Claude Code, Fable 5.1) on behalf of @0monish under the user's
standing delegation. **Planning and research only**: no implementation, no worktree, no PR merge.
Statements are FACT (with a receipt), INFERENCE, UNKNOWN or BLOCKER. Nothing here is normative;
Keld code, approved specs and live GitHub/Linear state outrank it. Companion file:
[`zettlr-corpus-facts.md`](zettlr-corpus-facts.md) (verified source facts). Tracker for this milestone:
[#1358](https://github.com/gyldlab/keld/issues/1358) `program(zettlr)` (sub-issue of #391).

Pins: Keld `origin/main` `231a212` (2026-10-31) · research branch `research/electron-compat-map`
`46e59078` · Zettlr `e6c7fd8` (Electron 43.6.0 resolved) and `origin/develop` `3a8e4b99`
(44.9.1 resolved) · Electron oracle v44.4.5 (`694f4585`) · WebKit `main` retrieved 2026-10-31 ·
mattpocock skills `4ae2317d` (wayfinder / to-tickets / triage formats).

## 1. Destination and claim limits

Destination of this milestone: Zettlr (pinned source, unmodified or with a **scored** edit count)
launches, opens a Markdown document, edits it, saves it and handles an unsaved close under
`keld dev` on macOS with the **explicit `legacy` profile** (`LegacySandboxOff` in every record),
with every cell red-until-implemented in a gh566-shaped `zettlr` v1 manifest and flipped only
on a real Mac ([#1354](https://github.com/gyldlab/keld/issues/1354)).

Claim limits (unchanged from the map): no percentage, no "runs unmodified" until the scored cells
say so, no "default-deny demonstrated" under legacy, no performance sentence without a registered
metric id, every Electron behaviour sentence traceable to a v44.4.5 doc/source receipt.

FACT (#391 comments C4–C8): the draw.io first proof itself has **not** landed; real macOS
draw.io launch/open/edit/save/dirty-close execution is UNKNOWN/BLOCKED as of 2026-10-24. The
Zettlr milestone therefore stays `milestone:next`; its startable work is the part that does not
need the draw.io proof (prototypes, specs, conformance entries, the two confirmed-gap tickets).

## 2. What Zettlr actually needs (verified)

Full receipts in `zettlr-corpus-facts.md`. The decision-bearing differences from draw.io:

| Dimension | draw.io (first proof) | Zettlr (this milestone) |
|---|---|---|
| Renderer→main shape | `send`/`on`/`once`/`event.reply` only; 0 invoke | **186 `invoke` + 25 `handle`**, 35 `send`, 6 `on`, 3 `reply` |
| Blocking calls | 1 main-side `showMessageBoxSync` (close path) | **0 main-side blocking; 6 renderer `sendSync`** (config/citeproc) |
| contextBridge | 2 keys, all functions return `undefined` | 5 keys; `ipc.on` returns a function, `config.get` returns a sync value, `getCitationCallback` returns a function that returns a sync value |
| Custom scheme | none | `protocol.handle('safe-file')`, unprivileged, 11 consumer files |
| Unsaved close | `showMessageBoxSync` inside an async listener | `close` → `preventDefault` → **async** `showMessageBox` → `window.close()`; `before-quit`/`will-quit` `preventDefault` + `app.quit()` re-entry |
| Boot-path natives | menu, dialog, screen, nativeTheme | + `systemPreferences.subscribeNotification` (unconditional), `protocol.handle`, `child_process.spawn` probes, chokidar watcher, splash/onboarding windows with `sandbox:false` |
| Window chrome | defaults | `titleBarStyle 'hiddenInset'`, vibrancy, splash `frame:false`/`alwaysOnTop`/`skipTaskbar` |
| Session | 2 `webRequest` no-ops | `webContents.session.clearStorageData` in every window's `close` listener |
| Env | none in preload | whole `process.env` copied into the page (decided ▲, env stays `{}`) |

Consequence for the kill-condition ranking on the map: the main-side link-stall kill condition
(#418, now landed as GH-527/#528 T1–T3) is **not** on Zettlr's path; the renderer `sendSync`
kill condition (#401) **is**, but it gates only activation of six read sites, not launch.

## 3. Atomic decomposition (root `AGENTS.md` § Atomic problem-solving protocol)

Each atom: owner · boundary (inputs → outputs) · failure mode · observable contract · first falsifier.
Independence edges are the dependency graph in §5; no atom's pass is proof of another.

| # | Atom | Owner (ticket) | Boundary | Failure mode | Observable / falsifier |
|---|---|---|---|---|---|
| A1 | Blocking host CALL from a Bun role | #528 (GH-527, T1–T3 landed) | Bun main parked → Worker drains link → REPLY by correlation | link stall, fabricated reply | 8 KiB+ events during park replay in order; drop one → order check fails |
| A2 | Renderer floor v2: one-way send + host→renderer events + credit | #465 / #543 / #540 | page ↔ app world ↔ wv-link v2 ↔ el control channel | unbounded renderer queue; stale-nonce delivery | N sends in one task arrive in order; N+1 refused; stale nonce refused |
| A3 | N pending renderer invokes | #546 | credit window per WebView | second invoke KELD-WV-011 | two overlapping invokes settle; remove credit check → N+1 accepted |
| A4 | invoke/handle facade + Electron error text | #547 | separate `_invokeHandlers` registry; `{error: String(err)}` reply | handler errors collapsed to KELD-API-001 | pinned strings: `Error invoking remote method '<ch>': <err>`, `No handler registered for '<ch>'`, `Attempted to register a second handler for '<ch>'` |
| A5 | Renderer `sendSync` blocking lane | #401 (decision) → #548 / lane ticket | page sync XHR → WKURLSchemeHandler → host → Bun → value in same task | page never unblocks; second routing protocol | WebKit `LoadSynchronousURLSchemeTask` (source FACT); host-enforced deadline; arms 1–8 |
| A6 | contextBridge sync + function-valued returns | #399 (decision) → #549 | page `dispatchEvent` → app-world listener → shared-DOM result | value only as Promise; object identity leak | `cloneAcrossWorlds` serialises (source FACT); same-task readback measured |
| A7 | App preload runtime in a separate content world | #484 → #467 → #571 | document-start injection; sandboxed require map; `process.getSystemVersion()` at load | preload throws (`getSystemVersion` missing) | Zettlr preload evaluates with no TypeError |
| A8 | Custom scheme producer behind the host resource adapter | #471 ← #554 | static scheme table; request → adapter → guard → Bun handler → bounded Response | scheme mints fs authority; traversal | encoded dot-dot refused before handler; >ceiling streams or typed ▲ |
| A9 | Window registry, two-phase close, chrome | #449 → #450 → #456 / #534 | host-minted (id, generation); `close` preventDefault; `hiddenInset` | throwing close listener counts as veto (today) → window never closes | Zettlr `close` listener runs `session.clearStorageData` (A10) |
| A10 | Session = KEL-135 profile; `clearStorageData` subset | #474 | host-side profile clear on close | TypeError in close listener → veto forever | window closes after `clearStorageData` |
| A11 | Cancelable two-phase quit with re-entry | #451 ← #450 | `before-quit`/`will-quit` preventDefault; `app.quit()` called again from the async continuation | second `app.quit()` refused (KELD-COMPAT-012 today) | Zettlr quits after Save/Discard; stays on Cancel |
| A12 | Async dialog broker + showErrorBox | #478 (in flight #1306/#1308) → #561 | parented sheet, paths only under legacy | paths minted into grants | Yes/No/Cancel index returned; Cancel keeps window |
| A13 | Application menu + non-role click dispatch + accelerators | #480 → #556 | host menu → Bun click `(menuItem, focusedWindow)` | Cmd+S never reaches `webContents.send('shortcut')` | save cell |
| A14 | Host-pushed display/theme facts + `systemPreferences` | #481 → #558 → #1356 | mirror, zero kipc on read | boot throws at `subscribeNotification` | AppearanceProvider constructs; one callback per theme change |
| A15 | Boot-static facts + single `electron` module + process facts | #452 / #453 / #502 / #570 / #593 | spawn env → sync getters | `app.getVersion()` at module top level undefined | config/index.ts:35 evaluates |
| A16 | chokidar 5 on Bun (legacy) | #1355 (decision) | `fs.watch` → chokidar → FSAL events | construction throws at boot; no `change` | arms 1–6 |
| A17 | Legacy ambient authority (fs, spawn, network) | KEL-78 / #586 / #588 | declared profile key; printed forfeit | silent auto-legacy | `authority_profile: LegacySandboxOff` in every record; strict cells `unknown` |
| A18 | Exact-literal `el:` grants for Zettlr's channels | #541 → #552; artefact in #1354's spec | 25 `handle` + 6 `on` literals, no wildcard | `el:*` written by migrate | ungranted channel → KELD-GUARD003; migrate emitting `el:*` refused |
| A19 | Product corpus manifest + scoring run | #1354 (consumes gh532/gh566/#587) | red cells → real-Mac flips | claim without committed id | claim line only with the committed-list edit |

## 4. Current state the plan builds on (FACT, origin/main 231a212)

- `@keld/electron` exports `app` (lifecycle subset) and a macOS-only `BrowserWindow` subset; nothing
  else, not even typed throws. `BrowserWindow.close` is two-phase with a real `preventDefault`; a
  throwing `close` listener counts as a veto (`browser-window.ts:627-636`). `app.quit()` rejects
  with KELD-COMPAT-012 while native windows are live.
- Renderer bridge: one frozen `window.keld.invoke`, echo channel only, 4096-byte payload, one
  pending invoke per WebView (KELD-WV-011 enforced in five places), `window.postMessage` relay
  between page world and `defaultClientWorld`. No `send`, no `on`, no custom scheme handler.
- GH-527 worker-owned blocking transport: T1–T3 landed (`WorkerLink.callBlocking`), T5 qualified
  on three OSes, T4 (GRANT credit) not landed. `FrameKind::Grant` has no live sender/receiver.
- Conformance corpora: `electron-app-v1`, `electron-window-v1`, `electron-native-v1` (all macOS,
  red cells for the draw.io tickets), `electron-lifecycle-v0` (44.3.0, stale pin). No ipc-main,
  context-bridge, protocol, system-preferences or web-utils doc snapshot in the shared store.
- In-flight PRs that this milestone consumes rather than duplicates: #1122 (#540 IPC contract),
  #1123 (#541 grants spec), #1191/#1190/#1192 (grant enforcement, codec, dual-world identity),
  #1154 (#467 contextBridge spec), #1306/#1308 (#478 dialogs), #1163 (#564), #1166 (#480 menu),
  #1164 (#481 theme), #1195 (#473 resource origin), #1278 (entity status TSV), #1294/#1323 (#569
  conformance), #1172/#1170/#1159/#1157 (migrate/corpus specs).
- Linear (fetched 2026-10-31): KEL-80 Backlog, KEL-79 Backlog, KEL-127 Todo, KEL-78 In Progress,
  KEL-143 In Progress. Tracker of record for every ticket below is its GitHub issue (#517).

## 5. Dependency graph

Edges read "A → B" as "B needs A". Only edges that change pull order are drawn; native GitHub
blocked-by links remain the single source of truth and must be wired by the orchestrator for the
three new tickets.

```text
                         decisions / prototypes (scratch, real Mac)
   #401 sendSync lane ──┐          #399 cross-world returns ──┐        #1355 chokidar on Bun
         │              │                 │                   │                 │
         ▼              ▼                 ▼                   ▼                 ▼
 IPC v2 / contextBridge                                                   file handling
   #540 contract ─► #465 v2a send ─► #543 v2b events ─► #546 N invokes ─► #547 invoke/handle ─► #549 returns ─► #548 / sync lane
        │                                                   ▲                      │
        └──► #544 codec ─► #545 payload bound ──────────────┘                      ▼
   #484 preload world ─► #467 contextBridge/ipcRenderer ─► #571 Zettlr preload tranche ─► #550 webUtils
                              ▲
   #541 grants spec ─► #552 guard enforcement ─► (every el: channel)    #542 document identity ─► #543, #484

 custom protocols                       native desktop                        windows / app
   #554 cells ─► #471 safe-file         #477 cells ─► #478 async dialogs ─► #561 showErrorBox      #449 registry ─► #450 close ─► #451 quit re-entry
   #473 file:// containment ─► #471     #480 menu ─► #556 click dispatch + Cmd+S               #449 ─► #456 chrome, #534 members (+setDocumentEdited)
                                        #481 theme mirror ─► #558 ─► #1356 systemPreferences    #474 clearStorageData (close listener)
 runtime                                                                                        #524 open-file / second-instance queue
   #452 session facts, #453 lock, #502 one module, #570 process facts, #593 no auto-install, #586/#588 profile

 product proof
   #589 draw.io proof (first) ─► #1354 Zettlr proof ◄── all of the above + #555 net.online (non-blocking) + #592 analyzer (reporting only)
```

Edge table (Zettlr-critical edges not already encoded as native blocked-by links on 2026-10-31):

| Needs | Because | Action |
|---|---|---|
| #547 ← #545 (or its invoke-result successor) | `get-document` returns the whole document text; the live inline bound is 4096 B, the envelope cap 64 KiB | #546/#547 spec states the invoke-result bound and the bulk lane for larger documents (D-Z6) |
| #549 ← #401 | `config.get` / citation callback return `sendSync` values; the relay alone cannot make them synchronous | sequence #401 arm 1 before scoring #549 for those two shapes |
| #450 ← #474 | Zettlr's `close` listener calls `session.clearStorageData`; a throwing listener is a veto today | add edge; #474 moves from "next, low" to Zettlr-critical |
| #451 ← #450 and A11 re-entry | `app.quit()` is called again from inside the `before-quit` continuation and after `will-quit` | #451 spec covers re-entrant quit |
| #1354 ← #556 | Cmd+S is a non-role accelerator with a click handler | #556 is on the save cell |
| #1354 ← #1356, #1355 | boot throws without `subscribeNotification`; FSAL constructs chokidar at boot | new tickets |
| #471 ← #473 | GH473's native resource reservation owns the WKURLSchemeHandler seam; #471 must not create a second registry | recorded on #471 (comment 6396933045) |

## 6. Prioritized backlog

### Immediate (startable now; no dependency on the draw.io proof landing)

1. [#401](https://github.com/gyldlab/keld/issues/401) — run the eight arms on a real Mac (feasibility is now source-established; see the 2026-10-31 comment). Scratch only.
2. [#399](https://github.com/gyldlab/keld/issues/399) — run the five arms (same harness family as the #484 DOM-ingress fixture). Scratch only.
3. [#1355](https://github.com/gyldlab/keld/issues/1355) — chokidar 5 on Bun 1.4.2 under legacy. Scratch only.
4. [#554](https://github.com/gyldlab/keld/issues/554) — protocol/session/net conformance cells (red, Zettlr keys); add the `protocol.md`, `session.md`, `net.md` doc snapshots to the shared store (gh566 A5).
5. Doc snapshots + red cells for `ipc-main.md`, `ipc-renderer.md`, `context-bridge.md`, `system-preferences.md`, `web-utils.md` under the existing conformance owners (#464 for the IPC family holds the first-proof slice; the Zettlr-only rows land under #547/#548/#549/#550/#1356 as conformance-first entries). The Electron source strings quoted in §9 are the oracle for the observed-only rows until the #494/#495 recorder exists.
6. Specs that are unblocked or blocked only by in-flight work: [#456](https://github.com/gyldlab/keld/issues/456) (chrome; ← #449), [#1356](https://github.com/gyldlab/keld/issues/1356) (← #481/#558), [#474](https://github.com/gyldlab/keld/issues/474), [#546](https://github.com/gyldlab/keld/issues/546) (← #465), [#471](https://github.com/gyldlab/keld/issues/471) (← #554; consume #1195's seam).
7. Decision D-Z1 (Zettlr corpus pin) — owner judgment, no code.

### Next milestone (Zettlr critical path, in pull order)

IPC v2 and contextBridge: #540 → #465 → #543 → #546 → #547 → #549 → #548 (+ lane ticket if #401 passes) → #550.
Renderer: #484 → #467 → #571.
Windows/app: #449 → #450 → #474 → #451 → #456 → #534 (+ `setDocumentEdited`) → #524.
Native: #477 → #478 → #561; #480 → #556; #481 → #558 → #1356; #479 (shell, navigation handler).
Protocol/session: #554 → #471; #473 seam.
Runtime: #452, #453, #502, #570, #593, #586/#588 (all first-proof; consumed, not re-specified).
Proof: #589 (draw.io) → #1354 (Zettlr). Migrate: #592 reports the Zettlr blocker classes; the proof uses a hand-authored artefact (X02-T3 pattern) because `keld migrate --write` (#591) is not required for scoring.

### Parked (named re-entry condition)

- Strict-profile Zettlr: node:fs (28 `readFile` sites), `child_process` probes at boot (#413), `got`/`ky` network — own later spec consuming KEL-78/KEL-140/KEL-143. Re-entry: legacy proof scored.
- #557 Tray, #482 Notification rows (not on the default-config path), #559 clipboard (not on the five cells), #503 nodehun/fsevents (dictionary feature, not on the path), #581–#583 updater, #529 (close during a park: Zettlr has no blocking dialog), #468 rows, Windows/Linux lanes (#1182, #1124 own them).

## 7. Capability table

| Capability | Owner ticket(s) | Blockers | Electron oracle (v44.4.5) | Acceptance (first falsifiable check) | Negative control | Platform evidence required |
|---|---|---|---|---|---|---|
| N pending invokes | #546 | #465 (credit accounting) | `ipc-renderer.md` invoke; no documented bound → declared ▲ | two overlapping invokes settle; N+1 typed refusal | remove credit check → N+1 accepted | macOS WKWebView real run |
| invoke/handle + error text | #547 | #546, #466, #467 | `ipc-main.md` "only the `message` property…"; source strings in §9 | round trip; handler throw text; no-handler text; duplicate `handle` throws; non-cloneable arg rejects | collapse errors to generic → handler-throw entry fails | macOS |
| sendSync | #401 → #548 / lane | #467, #466; bridge-owner amendment (#484 KEL-142 revision, #540) | `ipc-renderer.md` "block the whole renderer"; `{error:"reply was never sent"}` on GC (source) | typed throw today; if lane: value in same task, host deadline | returning a Promise fails no-silent-async | macOS WKWebView; WebKitGTK separate |
| contextBridge returns | #399 → #549 | #467 | `context-bridge.md` table: Function/Promise ✅ as return values | `ipc.on` returns a callable; sync value where Electron returns a value | Promise where Electron returns a value → entry fails | macOS |
| webUtils.getPathForFile | #550 | #467 | `web-utils.md` empty string for JS-constructed File | dropped file → path; constructed File → `''` | derive from `File.name` → fails | macOS (OS drop probe UNKNOWN) |
| Preload tranche | #571 | #484, #567, #570 | sandboxed-renderer `process` subset | `getSystemVersion()` non-empty at load | remove it → preload throws | macOS |
| safe-file scheme | #471 | #554, #473 seam | `protocol.md` handle/registerSchemesAsPrivileged; CustomScheme privileges per engine | 11 consumer files load; traversal refused; ceiling never truncates | allow scheme after construction → runtime-minting test fails | macOS (WKWebView privilege cells measured) |
| Session clear | #474 | — | `session.md` clearStorageData storage names | window closes after clear; names outside the subset typed ▲ | throw in close listener → window never closes | macOS |
| Two-phase close + async dialog | #450, #478, #561 | #449, #477 | `browser-window.md` 'close' preventDefault; `dialog.md` return shapes, `cancelId` | Yes/No/Cancel branches | immediate close → Cancel branch fails | macOS real Mac |
| Quit re-entry | #451 | #450 | `app.md` before-quit/will-quit preventDefault | Cancel keeps running; Save quits | second `app.quit()` refused → fails | macOS |
| Menu + Cmd+S | #480, #556 | #477, #449, #450 | `menu-item.md` click `(menuItem, window, event)`; accelerators | `webContents.send('shortcut','save-file')` observed | role-quit native terminate → fails #563 | macOS |
| Theme + systemPreferences | #481, #558, #1356 | #449 mirror | `native-theme.md`, `system-preferences.md` | AppearanceProvider constructs; one callback per change | second native observer → two callbacks | macOS |
| Chrome options | #456 | #449 | `browser-window-options` per option | host introspection: fullSizeContentView, hidden title | ignore `closable:false` → fails | macOS |
| chokidar watcher | #1355 | — | n/a (Node semantics) | add/change/unlink events; `close()` returns | watch nonexistent path → harness reports absence | macOS; Bun backend recorded |
| Channel grants | #541, #552 | — | n/a (Keld invariant) | ungranted → KELD-GUARD003 | `el:*` emitted → migrate refuses | CI + macOS |
| Product proof | #1354 | all above | gh532 rules | five cells + activation on a real Mac | mocked host → provenance fails | macOS only |

## 8. Unresolved decision register

Each row carries the recommendation taken under delegation; reopen with contrary evidence.

| Id | Decision | Options | Recommendation | Falsifier / first check | Owner |
|---|---|---|---|---|---|
| D-Z1 | Zettlr corpus pin vs oracle version (gh532 parks it) | (a) keep `e6c7fd8` (Electron 43.6.0) as demand pin; (b) re-pin to `origin/develop` (44.9.1); (c) wait for a tag resolving 44.4.x | **(a)** — every cited file is byte-identical at HEAD; run the Electron-arm recorder with Electron forced to 44.4.5 and record the resolved-version delta exactly as #589 records draw.io's 44.5.1 | a cited Zettlr file differs at HEAD, or the recorder refuses an override | @0monish (gh532 owner) |
| D-Z2 | renderer `sendSync` mechanism | A: sync XHR lane (same envelope, nonce-bound, host deadline); B: typed throw + "config + 6 edits" (#548) | **A if #401 arms 1–4 and 6 pass**; else B with the six sites scored as edits | #401 transcript | #401 then #484/#540 owners |
| D-Z3 | contextBridge sync/function returns | A: shared-DOM relay (result written to shared DOM state); B: async-only ▲ | **A** (dispatch synchronous and `detail` cloned across worlds are source FACTs); cost diagnostic only | #399 arm 1 readback fails | #399 then #549 |
| D-Z4 | `sandbox:false` splash/onboarding windows | refuse at construction (#455 six-value rule) vs accept + ▲ under legacy | **accept + declared ▲ under legacy, refuse under strict** — Keld never grants Node to a renderer, both preloads use only contextBridge/ipcRenderer | a `sandbox:false` window gains any authority a `sandbox:true` window lacks | #455 (F02-T4) |
| D-Z5 | grant list source for Zettlr's unfiltered preload | main-side literal registrations (25 `handle` + 6 `on`) vs recorder | **literals**; the generic `application` channel multiplexes ~51 commands and is one grant (same class as draw.io's `rendererReq`); no `el:*` | analyzer finds a non-literal registration | #541 / #1354 spec |
| D-Z6 | invoke-result payload bound | 4096 B inline (today) / 64 KiB envelope / bulk lane | measure Zettlr document sizes on the tutorial workspace; **bound chosen by measurement in #546/#547's spec**, bulk lane for larger documents, never truncate | a tutorial document exceeds the chosen bound | #545 successor inside #546/#547 |
| D-Z7 | `BrowserWindow.setDocumentEdited` / `setRepresentedFilename` ownership | add to #534 vs new ticket | **add to #534** (two members, macOS registry mirror) | #534 owner refuses scope | #534 |
| D-Z8 | `session.clearStorageData` on close | #474 host-side clear vs no-op ▲ | **#474 host-side clear of the three named storages**; anything else typed ▲; never a throw in the close listener | window stays open after close | #474 |
| D-Z9 | Zettlr launch cell on a fresh profile | onboarding window first (sandbox:false) vs pre-seeded config | **fresh profile, onboarding window counted in the launch cell** (D-Z4 makes it admissible) | onboarding window refused | #1354 spec |
| D-Z10 | Boot-path `child_process.spawn` probes under legacy | ambient Bun spawn vs declared `app.shell.spawn` rows | **ambient under legacy, disclosed in the record**; strict parked (#413) | spawn fails under Bun legacy | #1354 / KEL-78 |
| D-Z11 | Electron-arm recorder for observed-only cells | wait for #494/#495 vs source-string oracle | **source strings now (§9) with `expected status unknown` where no doc sentence exists; recorder flips them** | recorder shows a different string | #551 / #494 |

Closed by existing decisions (no new ruling): `process.env` stays `{}` (#407, F07-E1 A); `el:*`
never written (#398/#433); legacy never auto-selected (#586, KEL-78); no second kipc link (#424).

## 9. Concrete test gates

Repository gates (unchanged): `just ci` (fmt, warning-denied clippy, workspace tests, generated-doc
freshness), `env -u CLAUDECODE cargo test -p keld-compat` (corpus admission; learning :197),
`bun test` in `packages/@keld/electron`, review gates named per ticket (wire protocol: #540, #546,
#471, #1356 none; public API: #547, #549, #548, #550, #1356; permission model: #471, #550, #1356).

Oracle strings (pinned v44.4.5 source, retrieved 2026-10-31) for the conformance-first entries:

| Entry | Expected observable | Source |
|---|---|---|
| invoke handler throw | renderer rejects with `Error invoking remote method '<ch>': <error.toString()>`; main logs `Error occurred in handler for '<ch>':` | `lib/renderer/api/ipc-renderer.ts`; `lib/browser/ipc-dispatch.ts` |
| invoke, no handler | reject text `No handler registered for '<ch>'` | `lib/browser/ipc-dispatch.ts` |
| duplicate `handle` | throws `Attempted to register a second handler for '<ch>'` | `lib/browser/ipc-main-impl.ts` |
| non-function handler | `TypeError: Expected handler to be a function, but found type '<typeof>'` | `lib/browser/ipc-main-impl.ts` |
| `sendSync` no listener | stderr `WebContents #<id> called ipcRenderer.sendSync() with '<ch>' channel without listeners.`; renderer stays blocked | `lib/browser/ipc-dispatch.ts` |
| `sendSync` returnValue never set | renderer receives the object `{ error: "reply was never sent" }` when the event is garbage-collected (timing not asserted) | `shell/common/gin_helper/reply_channel.cc` |
| `sendSync` returns a value, not a Promise | `SendSync` returns `DeserializeV8Value(result)` | `shell/renderer/api/electron_api_ipc_renderer.cc` |
| contextBridge value table | Function/Promise supported as parameters and return values; Symbols dropped; prototypes dropped | `docs/api/context-bridge.md` table |
| `registerSchemesAsPrivileged` after ready | error text UNKNOWN — capture from a pinned run, never invent (#554 note) | — |

Prototype gates (scratch, real Mac; close the decision ticket only on a transcript): #401 arms
1–8; #399 arms 1–5 (arm 2 `postMessage` must fail); #1355 arms 1–6.

End-to-end Zettlr product proof cells (#1354; each red until its ticket lands; each flipped only
in the change that makes it pass, on a real Mac, `LegacySandboxOff`):

| Cell | Observable | Flipping tickets |
|---|---|---|
| activation | `keld dev` starts the role, `app.whenReady` resolves, `bootApplication` completes | #452, #453, #502, #570, #586/#588, #1356, #471, #1355 |
| launch | main window `ready-to-show` → shown; first inline script sees the five globals; onboarding window (first start) created and closable | #449, #456, #484, #467, #571, #455 (D-Z4) |
| open | `showOpenDialog` → root added; `invoke('documents-provider','open-file')` → `get-document` text > 4096 B delivered | #478, #547, #546, D-Z6 bound, #552 grants |
| edit | ≥ 20 consecutive `push-updates` invokes without KELD-WV-011; `documents-update` broadcast to all windows; `setDocumentEdited(true)` | #546, #543, #534 (D-Z7) |
| save | Cmd+S → menu click → `webContents.send('shortcut','save-file')` → `invoke('save-file')` → bytes on disk equal editor text with BOM/linefeed preserved | #480, #556, #547 |
| unsaved-close | close → veto held across async `showMessageBox` → Yes saved+closed / No closed / Cancel stays; `clearStorageData` in the close listener does not veto; Cmd+Q → `before-quit` → dialog → `app.quit()` re-entry → `will-quit` → shutdown → quit | #450, #478, #474, #451 |
| install | `unknown` naming KEL-141 | — |

Negative controls at the proof level: launch pass with `subscribeNotification` absent → fails;
`el:` grant for `documents-provider` removed → open/save fail closed with KELD-GUARD003;
immediate close instead of veto → Cancel branch fails; non-macOS or mocked host → provenance fails;
`will-attach-webview` count ≠ 0 → not pass.

## 10. Security: Strict/Legacy and default-deny, preserved

- Zettlr's proof runs **only** under the explicit `legacy` profile key (KEL-78 §: never inferred
  from Electron options; #586/#588 own the declaration and forfeit reporting). Every record carries
  `authority_profile: LegacySandboxOff`. Strict cells stay `unknown`; the headline wording is
  "brokers correct, containment forfeited", never "default-deny demonstrated for Zettlr".
- Renderer→main authorization is the exact-literal `el:<channel>` grant evaluated by keld-guard
  in `dispatch_privileged` (#541/#552); Zettlr's unfiltered preload changes nothing about that
  boundary (the preload is not the boundary, #433). No wildcard, no facade-side allowlist.
- The `safe-file` producer mints no filesystem authority: bytes come through the role's own fs
  grants (legacy: ambient, disclosed) and the adapter refuses traversal before the handler runs (#471).
- Any `sendSync` lane or cross-world relay is untrusted transport inside one document; the native
  handler, document nonce and host stay unreachable from the page (KEL-142 AC2, #484 adversarial
  test, GH441 separate app world).
- `process.env` is never copied into the page (F07-E1 A); Zettlr's `env` copy becomes `{}` (▲).
- No fifth unique, no Electron-isms in keld-core/keld-ipc, no second kipc link, no second
  renderer-routing protocol.

## 11. Issue links

New (this plan): [#1354](https://github.com/gyldlab/keld/issues/1354) Zettlr product proof ·
[#1355](https://github.com/gyldlab/keld/issues/1355) chokidar-on-Bun prototype ·
[#1356](https://github.com/gyldlab/keld/issues/1356) `systemPreferences.subscribeNotification` ·
[#1358](https://github.com/gyldlab/keld/issues/1358) `program(zettlr)` tracker.

Decisions updated with primary-source receipts (not closed): [#399](https://github.com/gyldlab/keld/issues/399), [#401](https://github.com/gyldlab/keld/issues/401).

Existing tickets reused (no duplicates created): #540 #465 #543 #544 #545 #546 #547 #548 #549 #550
#551 #541 #552 #542 #484 #467 #571 #567 #570 #554 #471 #473 #474 #555 #449 #450 #451 #455 #456 #534
#524 #529 #477 #478 #561 #480 #556 #563 #481 #558 #479 #559 #557 #482 #452 #453 #502 #593 #586 #588
#592 #503 #413 #494 #495 #589 #500 #499 #587 #528 #418 #432 #313 #517 #1182 #1124.

## 12. What this plan does not claim

No Zettlr cell is passing. No percentage exists. The draw.io proof has not landed; this plan does
not reorder its frontier. The real-Mac prototypes (#399, #401, #1355) were not run in this session.
Windows and Linux Zettlr lanes are undefined. The `keld migrate` analyzer is not required for the
proof and is not promised by it.
