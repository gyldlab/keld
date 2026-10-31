# Zettlr second-app milestone — macOS, explicit legacy profile

Planning record for the Electron-compat map [#391](https://github.com/gyldlab/keld/issues/391),
published 2026-10-31 by an AI agent (Claude Code, Fable 5.1) on behalf of @0monish under the user's
standing delegation. **Planning and research only**: no implementation, no worktree, no PR merge.
Statements are FACT (with a receipt), INFERENCE, UNKNOWN or BLOCKER. Nothing here is normative;
Keld code, approved specs and live GitHub/Linear state outrank it. Companion files:
[`zettlr-corpus-facts.md`](zettlr-corpus-facts.md) (verified source facts) and
[`zettlr-cross-os-lanes.md`](zettlr-cross-os-lanes.md) (per-engine primitives for macOS, Linux and
Windows with primary-source receipts). Revision v2 (2026-10-31): decisions taken under the user's
delegation after two independent refutations; see §8. Tracker for this milestone:
[#1358](https://github.com/gyldlab/keld/issues/1358) `program(zettlr)` (sub-issue of #391).

Pins: Keld `origin/main` `231a212` (2026-10-31) · research branch `research/electron-compat-map`
`46e59078` · Zettlr demand pin **`v4.9.0` = `04128af4`** (Electron 44.9.1 resolved; D-Z1 v2), with
`e6c7fd8` (43.6.0) as the superseded pin and `origin/develop` `3a8e4b99` (44.9.1) · Electron oracle v44.4.5 (`694f4585`) · WebKit `main` retrieved 2026-10-31 ·
mattpocock skills `4ae2317d` (wayfinder / to-tickets / triage formats).

## 1. Destination and claim limits

Destination of this milestone (v2, owner ruling D-Z14): Zettlr (pinned source `v4.9.0`, unmodified
or with a **scored** edit count) launches, opens a Markdown document, edits it, saves it and handles
an unsaved close under `keld dev` on **macOS first, then Linux, then Windows**, with the **explicit
`legacy` profile** (`LegacySandboxOff` in every record),
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

## 8. Decision register — v2, decided under delegation after two independent refutations

Every decision below was attacked by two isolated refuters (lens A: Keld invariants, ownership,
workflow; lens B: engine/Electron semantics, primary sources, corpus facts) whose default position
was "wrong". What survived is recorded as **DECIDED**; corrections the refuters forced are shown.
The user (@0monish, repository owner) delegated these technical decisions on 2026-10-31; items marked
*owner ruling* are taken under that delegation and can be reopened by comment with contrary evidence.
Receipts: `zettlr-cross-os-lanes.md` and the refutation reports (research branch handoff).

| Id | Decision (DECIDED) | Refutation outcome and correction applied | Gates / owner |
|---|---|---|---|
| D-Z1 | **Re-pin the Zettlr demand corpus to tag `v4.9.0`** (`04128af4`, 2026-10-25, lock-resolved Electron 44.9.1). Oracle stays Electron **44.4.5** docs/source. *Owner ruling* (consumes the pending #1183 W0 oracle question): the corpus `upstream` pin is the oracle docs commit; the app's own lock-resolved Electron version is a separately declared field; no oracle re-pin. | Facts survived (e6c7fd8 uses the Electron-43 `clipboard.writeImage`/sync `readText`, removed in 44.0; v4.9.0 uses the W3C API; all other 43→44/43.7 backport changes: 0 Zettlr hits). Lens A refuted the mechanism: `UpstreamV1` is `deny_unknown_fields` (`crates/keld-compat/tests/support/corpus_manifest.rs:421-426`) and gh532 AC1 rejects a second pin → the app-lock field needs a **gh532 + gh566 amendment** before the manifest can carry it; this also covers draw.io's 44.5.1 under one rule. Lens B corrected my falsifier: `windows/index.ts` is byte-identical e6c7fd8→v4.9.0; only the two clipboard files differ on the workflow paths (plus type-only `pinia/window-state-store.ts` and renderer files with no IPC lines). | Spec amendment PR on gh532/gh566 (docs/test schema; review gate none) before #1354's manifest; #1354/#1358 bodies edited to v4.9.0 |
| D-Z2 | **sendSync: one engine-neutral contract; live state = typed throw (#548) on every engine.** macOS/Linux candidate lane A (sync XHR to the host scheme handler, same wv-link envelope, nonce-bound, host-enforced deadline, rate-limited, dev-warned) has four ordered gates: (1) the app document is served from the host resource origin (GH473 scheme, #1195) and the sync endpoint lives on that same origin; (2) #401 arms 1–8 pass on a real device (Linux arm under #1127); (3) KEL-142 revision on #484 + #540 amendment; (4) a new lane ticket with wire-protocol, public-API and permission-model gates. **Windows: typed throw only.** | Lens B found the load-bearing precondition I missed: XHR is CORS-mode and WebKit's `checkURLSchemeAsCORSEnabled` refuses non-HTTP cross-origin schemes; a handler-backed scheme has a tuple origin, so a `file:` page is never same-origin with it and WKWebView has no public CORS-enable API (WebKitGTK has `register_uri_scheme_as_cors_enabled`). Lens A refuted Windows: #1307 names `sendSync` a non-goal and admits only top-level `WebMessageReceived`; `hostObjects.sync` makes the Host UI thread wait on Bun (#425) and hands the page a host object (#1184 AC ii) → withdrawn. | #401 (prototype), #484 (KEL-142 rev), #540, new lane ticket; Windows: #548 until a #1307 revision |
| D-Z3 | **contextBridge sync/function returns on WebKit engines via the shared-DOM relay** (dispatch synchronous; `detail` structured-cloned; `JSEventListener`/`EventTarget` have no cross-world filter). Windows: native JS in the shared main world (route D), divergence = isolation only (#1307 AC23). | Survived both lenses. Linux note: wry 0.56.1 injects a page-world `window.ipc` script unconditionally; #1127 must disable/replace it before a Linux world lane exists. | #399 (prototype) → #549; #1127 for Linux |
| D-Z4 | **`sandbox:false` windows: one profile-independent rule — accept at construction with a declared ▲ (inert: Keld never grants Node to a renderer) on every profile.** Owner: #526 row + a spec under window epic #447. | Lens A refuted my first shape (accept under legacy, refuse under strict): the owner is #526 (F02-T10), not #455, and KEL-78 forbids reading the Electron `sandbox` option as profile state; #455's negative control "no authority profile changes the outcome" is the rule. Lens B confirmed the preloads are inert: all 14 renderer entries share one bundled preload importing only `electron`. | Spec under #447/#526; review gates: permission model, public API |
| D-Z5 | **Grant list = main-side registered literals: 24 `ipcMain.handle` + 1 lazy `handleOnce` (`paste-image-retrieve-data`) + 6 `ipcMain.on` + 1 lazy `once` (`paste-image-ready`) = 32; `application` is one channel; no `el:*`.** The renderer-only `message` channel (sent at `dir-item-context.ts:128-137`, never registered by main) is **not** granted: Electron drops it silently, Keld refuses it with a typed error → recorded ▲ divergence cell under #465 and reported by `keld migrate` as an orphan channel. **Windows:** route D flags Zettlr's preload (reads `process.argv`, copies `process.env`, constant-argument wrappers) → attestation required, and `invoke` is not a route-D sink until #1154/#547 (#1307 AC13) → Windows grants are ∅ until then. | Lens B corrected the counts (25 was a doc-comment hit) and found the orphan channel; lens A refuted "no attestation needed". | #541/#552; migrate #592; Windows #1307/#1154 |
| D-Z6 | **Invoke-result bound measured on the bundled tutorial workspace in #546/#547's spec; bulk lane above it; never truncate.** | Undecidable on the number until measured; no counter-evidence. | #546/#547 spec |
| D-Z7 | **`setDocumentEdited` added to #534 (macOS, demanded: Zettlr calls it only under darwin); `setRepresentedFilename` is a #534 row only (zero Zettlr demand).** Windows/Linux rows cite the `_macOS_` tag and stay `unknown` (no doc sentence says "no-op"). | Both lenses: label correction (tag ≠ behaviour sentence). | #534; public API gate |
| D-Z8 | **`clearStorageData` on close: host-side clear of `cookies` + `localstorage`; `shadercache` recorded ▲ (#474 AC); never a throw in the close listener; Keld's close adapter runs both of Zettlr's per-window `close` handlers.** | Lens A: shadercache is ▲ in #474, and the engine primitives are receipts for future KEL-135 amendments, not #474 decisions. Lens B verified the storage names against `session.md:739-742`. | #474; Windows/Linux via KEL-135 T2/T4 |
| D-Z9 | **Launch cell on a fresh profile; the onboarding window is counted** (admissible under D-Z4 v2). | Depended on D-Z4; resolved by its v2 shape. | #1354 spec |
| D-Z10 | **Boot `child_process` probes run ambient under `legacy`, disclosed in every record; strict parked (#413).** | Survived (KEL-78 "forfeit is printed"). | #1354 / KEL-78 |
| D-Z11 | **Observed-only cells whose oracle is Electron source (not `docs/`) are `expected_verdict: unknown`**, never red-until-implemented; flipping them needs a gh532 amendment admitting recorder goldens as citations, which the #494 spec carries. | Lens A refuted "red cells": gh532 AC3/AC6 and #494/#495 require `docs/` citations. | gh532 amendment in #494 |
| D-Z12 | **Unsaved-close cell is OS-specific.** macOS: veto → async dialog → close. Windows/Linux (`nWindows === 1 && !leaveAppRunning`): Zettlr calls `app.quit()` without `preventDefault`; Electron source shows the native close proceeds and `window-all-closed` fires while `before-quit` is vetoed; a second dialog is the **predicted** (INFERENCE) observable. Cells stay `unknown` until the Electron arm records, per OS: `closed` vs first-dialog order, `before-quit` count, message-box count and parent, `will-quit` count. | Survived; lens B added the source chain and observables; lens A added the `nWindows === 1` qualifier. | lane tickets; X01-T5 recorder |
| D-Z13 | **`skipTaskbar` on Linux: row cites the `_macOS_ _Windows_` tag, status `unknown` until recorded. Tray parked (default `leaveAppRunning: false`).** | Label correction (tag ≠ "ignored"). | #456/#557 rows |
| D-Z14 | ***Owner ruling:* #1358's destination is Zettlr on all three OSes, ordered macOS → Linux → Windows.** Two lane tickets are minted under #1358 (Zettlr-Linux, Zettlr-Windows) that consume the draw.io programs' children (#1125–#1129, #1183–#1189) as blockers; #1124/#1182 keep their draw.io non-goals; #1354 stays macOS-only, per-OS cells live on the lane tickets. | Lens A refuted ownership (both OS programs name Zettlr a non-goal; #391 non-goals exclude Windows/Linux proofs) → resolved by the owner ruling and lane tickets. Lens B corrected the engine basis: Keld's Linux seam is wry (page-world bridge injected), so "primitives exist" is true of WebKitGTK, not of the seam. Order falsifier unchanged (#1307 approved and W1–W6 landed before #1125–#1129). | #1358 + two lane tickets |
| D-Z15 | **Plan files stay on `claude/zealous-volta-7qx541`; the orchestrator cherry-picks the commits onto `research/electron-compat-map`; #1358's file links are provisional until then.** | Survived; corrected commit count (the lanes file is a second commit). | orchestrator |
| D-Z16 | **`safe-file` on WebView2: environment-time `CoreWebView2CustomSchemeRegistration` in the static scheme table, `HasAuthorityComponent=false`, `AllowedOrigins` empty** (all 11 consumers are no-origin subresources). Sync carriers withdrawn (see D-Z2). | Scheme registration survived both lenses. | #471 + #1185 |
| D-Z17 | **No macOS observation stands in for another OS (KEL-79, gh532 AC9).** The Windows/Linux Electron-arm recorder is parked in X01-T5; Linux/Windows observed-only cells stay `unknown` until the lane tickets unpark X01-T5. | Survived; lens A named the parked recorder. | lane tickets ← X01-T5 |
| D-Z18 | **Engine receipt pins:** before any engine receipt cited at branch tip backs a cell it is re-fetched at WebKitGTK 2.52.6 / wry 0.56.1 (arch 05 §148), WebView2 runtime 156.0.4314.50 / SDK 1.0.4314.35 (#1307), and the recorded macOS build. Lanes vocabulary is `present/emulated/absent/unknown`, never scoreboard ✔/▲/✘. | New, from lens A's missed-decisions list. | lane tickets |
| D-Z19 | **Single-writer files:** #1354's claim line edits `DOCUMENTED_COMMITTED_PRODUCT_CORPORA` and `docs/engineering/compat-scoreboard.md`; a human-designated writer is required (`docs/agents/workflow.md` § Parallelism); #1354 lists this as a review condition instead of "none". | New, from lens A. | #1354 |

Closed by existing decisions (no new ruling): `process.env` stays `{}` (#407, F07-E1 A); `el:*`
never written (#398/#433); legacy never auto-selected (#586, KEL-78); no second kipc link (#424);
no host UI thread blocks on Bun (#425).

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
