# Spec: first-proof window registry, window wire contract and two-phase close state machine (macOS)
Status: draft
Linear: GH-531 (#517) · Owner: @0monish · Updated: 2026-10-07

## 1. Goal & non-goals

draw.io's window path cannot run on Keld today. The host creates one primary window,
no window identity crosses kipc, and the macOS loop tears the view down on every
`CloseRequested` (`crates/keld-wv/src/wkwebview/mod.rs:385-403`), so a close cannot be
cancelled. This spec defines the parts F02-T2 (#449), F02-T3 (#450) and F02-T4 (#455)
need for draw.io on macOS:

- a host window registry that alone mints `(WindowId, WindowGeneration)`;
- an ordered window-state event stream on the app link, consumed by one `@keld/api`
  mirror primitive that serves synchronous reads with no kipc traffic;
- a two-phase close state machine with no host timer;
- constructor triage that refuses the six escalating `webPreferences` values with no
  opt-in.

The observable outcome: `new BrowserWindow(<draw.io options>)` creates one native
window whose `id` is the host's WindowId; a `close` vetoed with `preventDefault()`
leaves the window open for as long as the app wants; the window closes only on the
app's allow reply, `destroy()`, or the session's terminal end; and `isDestroyed()`
behaves as the Electron 44.4.5 transcripts record.

Sections 4.a to 4.h map to the ticket's sections (a) to (h).

Non-goals:

- Windows and Linux (F02-T12, parked). Window-channel calls on those hosts fail with a
  typed error.
- Multiple views per window, `BrowserView`, `WebContentsView` (Tier 3).
- Window-bound roles (KEL-75/T4). This spec only keeps the close transition at which
  KEL-75's `WindowClosing(w)` tombstone is linearized.
- Quit sequencing (PANEL-D23 #443, F01-T3 #451). §4.e states the registry-side half
  of the #419 quit rules, which F01-T3 consumes.
- Close while the role is parked (F02-T11 #529). Recoverable role loss with window
  adoption is **in scope** (D6, §4.i): this slice takes the ownership transfer, the
  replay and the recovery gap (§4.e) from F02-T11, which keeps only the parked-close
  part.
- The modified-document dialog (`showMessageBoxSync`, F06-T7) and renderer
  `beforeunload` (no corpus demand, no owner).
- `show:false`/`ready-to-show` (F02-T9), chrome (F02-T5), the full option table
  (F02-T10), `fromId`/`fromWebContents` (F02-T8), `loadURL` and the webContents event
  vocabulary (F03).
- Handshake-time channel resolution and general `keld gen` schema authoring
  (architecture 02 §4 destination).

### Identity vocabulary (binding for this document)

Four identities exist near a window. Each has one minter. This document uses no other
form of the word.

| Name | Minter | Rotates | Used here for |
|---|---|---|---|
| **WindowGeneration** | keld-core app-session window registry (this spec, KEL-75) | once per window incarnation; never reused within the app session | addressing one window incarnation |
| **RoleInstance generation** | keld-core app-session role registry (KEL-75) | on every Bun role spawn | the role that receives window events and whose loss ends a pending close |
| **navigation generation** | keld-wv per-WebView host navigation state, the only minter (F04-A4 #400, decided; implementation #542) | on cross-document commit | nothing; this spec neither mints nor reads it |
| **guard webview-principal generation** | the guard `Principal::Webview` field; under F04-A4 option A its value is stamped from the navigation generation (#542) | with the navigation generation | nothing; webview principals cannot reach the window channel (§4.g) |

## 2. Spec refs

- `docs/architecture/01-overview.md` crate table: keld-core owns "lifecycle, window
  registry, session orchestration"; keld-wv owns the engine layer; keld-native depends
  only on ipc and guard. This spec places the registry in keld-core.
- `docs/architecture/02-ipc.md` §2 (frame, `CallError`, KEL-133 receiver semantics,
  v0 lifecycle channel), §4 (contracts), §7 ("Window close: the destination host
  revokes that window" incarnation; per-wait deadlines). No frame or `HELLO` change.
- `docs/architecture/03-security.md` (lifecycle is session control on an
  already-minted app link and stays ungated; webview principals deny until window-level
  grants exist). The window channel follows the lifecycle rule for the app-process
  principal only (§4.g).
- `docs/architecture/05-webview-and-native.md` §1 (`WebEngine` trait; no trait change
  here), §3 (native module table). **Deviation, amended in this PR:** §3 lists a
  `window` module on the keld-native surface. keld-native cannot own native windows,
  since it does not depend on keld-wv. The amendment says the keld-core registry over
  keld-wv handles owns the `window` row (§5 lists the changed sentences).
- `docs/specs/kel75-principalized-bun-child-roles.md` §T4a: the app-session registry
  owns monotonic identity counters; `WindowClosing(w)` is a monotonic tombstone; a
  native window number is never an identity.
- `docs/specs/kel139-macos-product-spine.md` AC5 (recovery keeps the same window), AC6
  (Quit has one ordered owner), T1b (nothing may fabricate a native completion).
- `docs/specs/kel136-generated-ts-app-link-transport.md`: one shared TypeScript
  transport; codecs are thin adapters.
- `docs/specs/kel133-kipc-receiver-semantics.md`: one validator owns admission.
- Draft, not approved: `docs/specs/gh527-worker-owned-blocking-call-transport.md`
  (branch `agent/gh-527-worker-link-spec`, #527/#528). §4.6 wake-time rule: mirror facts
  are applied before a blocking call returns; listeners run after the caller resumes.
- Decisions consumed: PANEL-P2 #419 (resolution 2026-10-07, transcripts at
  `research/electron-compat-map@1c2e8945:wayfinder/electron-compat/probes/panel-p2/`),
  PANEL-D19 #439 (three exits, no timer), PANEL-D5 #425, F04-A4 #400, F04-A1 (webview
  window operations wait for KEL-102 per-window grants), X06-D7 (#517 tracker rule).
- Predecessor rules consumed: F01-T2 #446 (who creates window #1, what Ready means),
  X05-T2 #508 (channel-table rule; draft PR #613), X05-T4 #597.

## 3. Acceptance criteria (binary, each becomes a test)

Every criterion names its negative control (NC): the one mutation that must make it
fail. "Owner" is the implementing ticket.

### (a) Identity

1. **Host is the sole minter.** Given a facade boot after Ready, when
   `new BrowserWindow(opts)` returns, then `win.id` equals the `WindowId` in the host's
   `Created` event for that window, and the `Create` request schema has no identity
   field. *NC:* minting the id in the facade, so the id was never delivered by the host,
   fails the id-equality assertion. **Snapshot source:** `Created.state` is always the
   `WindowSnapshot` carried by keld-wv's `WindowBuilt`, read from the built native
   window (with any buffered facts overlaid, §4.e), including when `Create` has `x` and
   `y` as `None`. Given a fake UI port whose `WindowBuilt` snapshot is
   `(x 37, y 91, 800×600, focused, not maximized, not full-screen)` for a `Create` with
   no position, the `Created` event carries exactly that state. *NC:* a registry that
   fills `Created.state` from the `Create` request or from defaults (no snapshot
   source) writes `x 0, y 0` and fails. Owner: F02-T2.
2. **Never a native number.** Given a registry test harness in which keld-wv returns
   `WebviewId` values 7 and 8, when two windows are created, then their WindowIds are 1
   and 2. *NC:* returning the `WebviewId`, tao `WindowId` or `NSWindow` `windowNumber`
   as the WindowId fails. Owner: F02-T2.
3. **No reset on role rotation.** Given window 1 destroyed and the primary role
   restarted with a new RoleInstance generation, when the successor creates a window,
   then its pair is `(2, w)` with `w` greater than every WindowGeneration minted before.
   *NC:* a per-role-coordinator counter that resets at role restart re-mints a retired
   pair, and the test fails. Owner: F02-T2.
4. **Retired pair fails typed.** Given window A destroyed and window B created, when a
   window call carries A's `(WindowId, WindowGeneration)`, then the host answers `ERR`
   with the registered stale-window `KELD-CORE-*` code, B's native state is unchanged,
   and the message names the fix ("drop the reference in the 'closed' listener").
   *NC:* reusing a pair across incarnations fails, because the call then addresses B.
   Owner: F02-T2.
5. **Forged pair fails typed.** Given one live window `(1, w)`, when a call carries
   `(1, w+1)` or `(9, w)`, then the reply is the same stale-window `ERR` and no window
   changes. *NC:* matching on WindowId alone accepts `(1, w+1)`. Owner: F02-T2.
6. **Exhaustion fails closed.** Given the WindowId counter at `u32::MAX`, when one more
   window is requested, then `Create` fails with the registered exhaustion code and no
   window is created. *NC:* a wrapping increment yields id 0 or 1, and the test fails.
   Owner: F02-T2.
7. **Vocabulary.** A text check over this spec and the F02-T2/T3/T4 diffs finds every
   case-insensitive match of the regex `[Gg]eneration` inside one of the four §1 names.
   *NC:* inserting one bare use fails the check. Owner: F02-T2 adds the check for its
   own diff.

### (b) Wire

8. **Golden vectors.** Every `WindowEvent`, `WindowRequest` and `WindowResponse`
   variant in §4.b has a pinned postcard byte vector in one checked-in vector file. The
   Rust codec and the Bun adapter both replay that file. *NC:* changing one
   discriminant or field order in either language fails the replay. Owner: F02-T2.
9. **Channel id from the table.** The window channel id is read from the X05-T4 table
   entry by Rust and from the generated TypeScript constant by Bun. No numeric literal
   exists outside the table. *NC:* a hand-written TypeScript constant fails the X05-T4
   drift check. Owner: F02-T2.
10. **No protocol bump.** With the window channel live, `PROTOCOL_VERSION` is 2, the
    16-byte header is unchanged, and the existing kipc golden vectors and the KEL-133
    hostile corpus pass unmodified. Only rows for the new channel are added. *NC:*
    bumping the version or changing a header field fails the existing vectors. Owner:
    F02-T2.
11. **Undecodable fact is terminal.** This criterion covers decode failures only.
    Given a window `EVENT` whose discriminant is unknown to the adapter, or whose
    payload is not valid postcard for `WindowEvent`, when it arrives, then the link
    closes with the cause `KELD-IPC-005` (gh527 §4.4: a throwing applier records
    `STATE = 22`, and the cause is written to the role log) after earlier records are
    delivered, and no getter returns a value newer than the last applied fact. A
    parked `Create` or `Destroy`, and every pending call, then throws `KELD-IPC-022`
    whose detail names that `KELD-IPC-005` cause. The codec's internal `KELD-IPC-003`
    is never surfaced. The other gh527 transport wake codes (`KELD-IPC-022` to
    `KELD-IPC-027`, gh527 §4.4) stay distinct and pass through to a parked `Create` or
    `Destroy` unchanged; the facade never remaps or swallows them. *NC 1:* skipping the
    unknown record and continuing fails, because the mirror then serves state that the
    host has already changed. *NC 2:* a facade that rethrows the parked call's
    `KELD-IPC-022` as `KELD-IPC-005`, or as any `KELD-COMPAT-*` code, fails the
    pass-through assertion. Owner: F02-T2.
12. **One transport owner.** The window codec is one thin adapter module over the
    shared transport; it opens no socket and owns no reader. *NC:* a second `connect`
    or `FrameReader` in the window adapter fails the single-owner source check that
    F02-T2 adds beside the KEL-136 rule. Owner: F02-T2.

### (c) Window #1

13. **No host window before the app asks.** Given a facade boot as F01-T2 declares it,
    when Ready is observed, then the host's native window count is 0. After the first
    `new BrowserWindow()` it is 1, and that window's WindowId is 1. *NC:* keeping
    `CreateInitialWindow` for facade boots makes the count 1 at Ready and 2 after the
    constructor. Owner: F01-T2 (count at Ready); F02-T2 (after construction).
14. **Registry is live at Ready.** Given a facade boot, when `Create` is sent in the
    same task that `whenReady()` resolves in, then it succeeds. *NC:* starting the
    registry after Ready is written makes this first `Create` fail with the registry's
    not-ready error. Owner: F02-T2.
15. **Renderer boots unchanged.** Given a renderer-declared (non-facade) boot, the
    existing KEL-96 macOS close and `LastWindowClosed` tests pass unchanged, and no
    window-channel frame is written. *NC:* routing the host-created initial window
    through the veto path makes the window outlive its close, and the existing test
    fails at its kill-switch. Owner: F02-T3.

### (d) Mirror primitive

16. **Fact before listener.** Inside a `'maximize'` listener, `isMaximized()` returns
    true. Inside an `'unmaximize'` listener, it returns false. *NC:* applying the host
    fact after user listeners run fails. Owner: F02-T2.
17. **Zero traffic per read.** 1,000 successive `getFocusedWindow()`, `getSize()`,
    `getPosition()`, `isMaximized()` and `isFullScreen()` reads leave the app-link frame
    counter unchanged in both directions. *NC:* implementing any getter as a host round
    trip fails. Owner: F02-T2.
18. **Per-window causal order.** Given the host writes `Bounds(r)` and then
    `CloseRequested` for one window, inside the `'close'` listener `getSize()` and
    `getPosition()` equal `r`. *NC:* dispatching `'close'` before the preceding `Bounds`
    record is applied fails. Owner: F02-T3.
19. **Staleness under park.** Given the role is parked in a blocking call and the host
    writes `Bounds`, `Focus` and `Maximized` facts, when the call returns, then the
    getters already show those facts, no window listener has run, and the listeners
    then run in issue order after the synchronous continuation. *NC:* dispatching
    listeners inside the wake drain fails (gh527 criterion 4, #419 E3). Owner: F04-T18
    #528 lands the sequence test; F02-T2 registers the window applier.
20. **Admission and subscription.** The window channel is a host-internal event
    entry (`Authority::HostInternal`, #613 criterion 16). Window events for a window are
    written only to the app link of that window's `owner_role_generation`, which is
    always a `primary` role declared in KEL-75, and only after that link subscribed with
    the X05-T2 `lifecycle` `Subscribe` call. The facade subscribes before its first
    `Create`. `SubscribeRefused` applies, and changes no state, when the link's role is
    not the declared `primary` (`RoleNotAdmitted`), when the host has no window entry
    (`UnknownChannel`, a peer newer than its host), or for the reasons #613 defines.
    *NC 1:* broadcasting to every connected role fails, because a second authenticated
    `app-bound` role link that also sends `Subscribe` is refused `RoleNotAdmitted` and
    receives no window frame. *NC 2:* an unsubscribed primary link receives zero window
    frames. Owner: F02-T2. F06-T5 reuses this rule for display and theme facts.

### (e) Close state machine (the #419 rules plus PANEL-D19)

21. **Fail-closed veto, no timer.** Given a window whose `'close'` listener calls
    `preventDefault()`, when the fixture advances the registry's injected clock by any
    amount, then the window stays in `Open` and the native window stays on screen. It
    reaches `Destroyed` only after the app calls `destroy()`. No sleep is used. *NC 1:*
    any host-side completion of a pending close driven by a timer fails once the clock
    passes its bound. *NC 2:* removing the view on `CloseRequested` before the reply, as
    today, fails. Owner: F02-T3.
22. **Unmodified-document path.** The close button yields `'close'` with
    `preventDefault()`, then a renderer `isModified` round trip, then `destroy()`. The
    result is exactly one `'closed'`, and the host's native window count reaches 0.
    *NC:* emitting `'closed'` from the `Destroyed` event as well as from `destroy()`
    yields two. Owner: F02-T3.
23. **Tombstone at `closed` (#419 rule).** Inside every `'closed'` listener,
    `win.isDestroyed()` is true. Inside `'close'` it is false. A later `getSize()` throws
    the registered `KELD-COMPAT-*` error whose message contains "Object has been
    destroyed". *NC:* emitting `'closed'` before the window tombstone flips fails.
    Owner: F02-T3.
24. **webContents tombstone on the destroy path (#419 rule).** On the `destroy()`
    path: inside `'closed'`, `win.webContents.isDestroyed()` is false; `'closed'` is
    emitted before `destroy()` returns; the webContents `'destroyed'` event fires after
    `destroy()` returns; inside it, `isDestroyed()` is true. On the normal close path:
    `'destroyed'` fires before `'closed'`, and inside `'closed'` both tombstones are
    true. *NC:* flipping the webContents tombstone when the mirror applies the
    `Destroyed` fact, instead of when `'destroyed'` is emitted, makes it true inside
    `'closed'` on the destroy path. Owner: F02-T3.
25. **Repeat request merges (#419 rule).** Given a window in `ClosePending(s)`, when a
    second native close attempt or `win.close()` arrives, then the host writes no second
    `CloseRequested`, `'close'` is emitted once, and `win.close()`'s reply is
    `Merged(s)`. After the reply for `s`, the next attempt writes `CloseRequested(s+1)`.
    *NC:* the ungated registry writes a second `CloseRequested`, which stacks a second
    prompt (harness scenario `n3_second_close_during_modal_ungated`). Owner: F02-T3.
26. **Re-emission per attempt.** Given three close attempts, each made after the
    previous reply was a veto, the app observes three `'close'` events with strictly
    increasing `close_seq`. *NC:* caching the first verdict and auto-replying fails.
    **Exhaustion:** given a window whose last `close_seq` is `u64::MAX` (a registry
    test constructor), when `RequestClose` arrives, then it answers the registered
    registry-counter exhaustion `ERR`, no `CloseRequested` is written and the window
    stays `Open`; a native close attempt in that state writes nothing and records one
    diagnostic. `Destroy` (T8) and the terminal end (T10) still close it. *NC:* a
    wrapping increment writes `CloseRequested(0)` and fails. Owner: F02-T3.
27. **Stale reply ignored.** Given `ClosePending(s+1)`, when a reply carrying `s`
    arrives with `Allow`, then the window stays in `ClosePending(s+1)`, the call answers
    `Stale`, and the host records one diagnostic. *NC:* accepting a reply that does not
    carry the current `close_seq`, so a stale allow from an earlier attempt closes the
    window, fails. Owner: F02-T3.
28. **Replies delivered in common run-loop modes (#419 rule).** Given the macOS UI
    thread inside a nested run loop in `NSModalPanelRunLoopMode`, when an `Allow` reply
    for another window arrives, then the teardown command reaches the UI loop before the
    nested loop ends. *NC:* a keld-wv mutation that queues `AppWindowCommand`s in a
    keld-wv-owned buffer drained only from the outer loop's `MainEventsCleared`
    callback (so delivery waits for the nested loop to end) fails. That is the same
    default-mode-only failure harness scenario `s3d_terminate_later_default_mode_delivery`
    showed 5/5, produced by changing Keld code, not tao. Owner: F02-T3.
29. **Throwing listener.** A `'close'` listener that throws produces exactly one reply
    for its `close_seq` and exactly one diagnostic. The reply is `Veto` (fail-closed,
    decision D2 in §4.i; ▲ until the Electron 44.4.5 oracle cell records the real
    behaviour). *NC:* letting
    the exception escape the dispatcher sends no reply, and the window stays in
    `ClosePending` forever. Owner: F02-T3.
30. **Exit on RoleInstance generation loss and adoption (PANEL-D19; D6).** Given
    `ClosePending(s)` on window A, when the primary role is killed, then `s` gets
    exactly one terminal outcome (`Abandoned`), `CloseRequested(s)` is never written to
    any successor role, and A stays `Open` with its pair unchanged (KEL-139 AC5).
    - **Transfer and replay.** When the recovered successor of the same declared role
      subscribes, the host transfers ownership explicitly (`owner_role_generation` :=
      the successor's generation) and replays one `Created { window, state,
      pending_close }` per window in `Open` or `ClosePending`, in creation order, with
      the registry's current state, before any other window frame on that link. Calls
      from any other link with A's pair, including a late frame attributed to the
      retired generation, get `KELD-CORE-043`.
    - **Recovery gap.** The gap runs from `Role loss` until the successor's admitted
      `Subscribe`. (i) Given the gap, when a native close or a `Bounds`, `Focus`,
      `Maximized` or `FullScreen` fact occurs, then zero frames are written to the
      retired link, and the registry's host snapshot holds the latest value of each
      fact (coalesced, not a stream). (ii) Given a native close during the gap, then A
      enters `ClosePending(n)` with no `CloseRequested` written and the veto held;
      after the successor subscribes, the replayed `Created` carries
      `pending_close: Some(n)`, the successor's facade emits `'close'` once, and its
      `CloseReply(n, _)` takes Q5 or Q6. (iii) Given the gap, when the role's recovery
      fails (KEL-75 provisions no successor) or the session ends, then A takes the
      `Session end` row (Q12: `n` is `Abandoned`, A closes on the quit path), with no
      timer. (iv) A window already in `Closing` at the successor's `Subscribe` finishes
      its teardown and is not replayed.
    *NC 1:* replaying the pending `CloseRequested(s)` to a successor fails. *NC 2:*
    omitting the replay leaves the successor's mirror empty, so `getAllWindows()`
    returns `[]` and fails. *NC 3:* transferring ownership to a role of a different
    declaration (an `app-bound` role) fails, because that role's call must get
    `KELD-CORE-043`. *NC 4:* a host that writes `CloseRequested(n)` or any fact to the
    retired link fails (i). *NC 5:* a replay without `pending_close` leaves `n`
    unanswered and A stays `ClosePending(n)` forever, which fails (ii). *NC 6:*
    replaying the gap's facts as separate `EVENT`s instead of one coalesced `Created`
    per window fails (i)'s count. *NC 7:* resolving the gap close by an injected clock
    advance fails (iii). *NC 8:* a `Closing` window appearing in the replay fails (iv).
    Owner: F02-T3.
31. **Exit on session terminal end.** Given `ClosePending(s)`, when the host accepts
    `Quit`, then the window reaches `Destroyed` with no `CloseRequested` written and no
    reply awaited. *NC:* making `Quit` wait for the pending reply leaves the host alive,
    and the existing quit test fails at its kill-switch. Owner: F02-T3.
32. **`closed` precedes `window-all-closed`.** Given one window, when it is destroyed
    outside a quit, then the host writes `Destroyed` before `LastWindowClosed`, and the
    facade emits `'closed'` before `'window-all-closed'`. The existing KEL-237
    `window-all-closed` cell passes. *NC:* writing `LastWindowClosed` first fails.
    **Destroy path:** given one window and no `'window-all-closed'` listener, when
    `destroy()` is called, then the step log is `REPLY Destroyed` read, `'closed'`,
    default `Quit` written, `destroy()` returns, and the host reads no `Quit` before it
    has written the `Destroy` reply. *NC:* letting the lifecycle applier run the
    last-window policy in the wake drain writes `Quit` before `'closed'` and fails.
    Owner: F02-T3.
33. **Quit keeps asking every window (#419 rule; registry half).** Given windows A and
    B and the facade's quit loop, when A vetoes, then a close request for B is still
    accepted and `CloseRequested` is written for B. The registry holds no cross-window
    latch. The facade's quit loop asks windows in reverse creation order (newest
    first), as PANEL-P2 E5 recorded `w2` then `w1`. *NC:* a stop-at-first-veto latch in the registry writes nothing for B.
    Owner: F02-T3 (registry). F01-T3 owns re-emitting `before-quit` on every
    `app.quit()`.
34. **`window-all-closed` suppressed during quit (#419 rule; facade half).** Given the
    facade quit state is active, when the last window's `Destroyed` and
    `LastWindowClosed` arrive, then no `'window-all-closed'` is emitted. *NC:* emitting
    it during a quit-initiated close fails F01-T3's two-window sequence cell. Owner:
    F01-T3 (the registry only guarantees criterion 32's order).
35. **`app.quit()` returns at once (#419 rule).** `app.quit()` returns to its caller
    before any window's `'closed'` is emitted. No window call made during quit is
    blocking. *NC:* making `app.quit()` wait (a blocking call or an awaited host `Quit`)
    fails the returns-first assertion. Owner: F01-T3. This spec only forbids a blocking
    `RequestClose`.

### (f) Constructor triage

36. **draw.io's set constructs.** A window created with draw.io's exact option set
    (top level: `backgroundColor`, `width`, `height`, `x`, `y`, `icon`; `webPreferences`:
    `preload`, `additionalArguments`, `webviewTag:false`, `contextIsolation:true`,
    `nodeIntegration:false`, `webSecurity:true`, `disableBlinkFeatures`, `spellcheck`)
    exists. `backgroundColor`, `icon`, `disableBlinkFeatures` and `spellcheck` each
    produce exactly one diagnostic per window. *NC:* refusing `disableBlinkFeatures`
    fails, because draw.io's main window is not created. Owner: F02-T4.
37. **Six escalating values refused.** For each of `nodeIntegration:true`,
    `nodeIntegrationInWorker:true`, `nodeIntegrationInSubFrames:true`,
    `contextIsolation:false`, `webSecurity:false` and `allowRunningInsecureContent:true`:
    construction throws the registered `KELD-COMPAT-*` triage error, no `Create` frame
    is written, the host window count is unchanged, and the message names the safe
    value. *NC:* honouring or silently ignoring `nodeIntegration:true` fails. Owner:
    F02-T4.
38. **Values, not keys.** Each of the six fields set to its safe value creates the
    window. *NC:* refusing a field because the key is present fails. Owner: F02-T4.
39. **No opt-in.** No `keld.compat.ts` entry, flag or authority profile changes the
    outcome for an escalating value. *NC:* a quirk entry that makes
    `contextIsolation:false` construct fails. Owner: F02-T4.
40. **Nothing to widen on the wire.** The `Create` request carries only the fields in
    §4.b. Its golden vector has no `webPreferences` field. *NC:* adding any
    `webPreferences` field to `Create` without a public-API review fails the
    golden-vector criterion (8). Owner: F02-T4.

### (g) Guard

41. **App-process principal only.** A window-channel `CALL` is admitted only on the
    primary app link of the current primary RoleInstance generation. The same frame on
    any other authenticated role link fails `KELD-IPC-005` before payload decode.
    *NC:* adding the window channel to a receive policy shared by every role link
    fails. Owner: F02-T2.
42. **Webview principals denied.** The macOS renderer bridge refuses a request naming
    the window channel (it admits only echo, `macos_bridge.rs:383`). No webview-origin
    path creates, closes or mutates a registry window. *NC:* adding the window channel
    to the bridge's admitted set fails. Owner: F02-T2. The amendment that lifts this is
    KEL-102's per-window grants (F04-A1).
43. **Host-internal table entry.** The X05-T4 table entry for the window channel has
    `authority() == Authority::HostInternal` and class
    `ReceiveClass::HostCallWithEvents`; a unit test asserts both. *NC:* changing the
    entry to `Authority::Guarded(&[keld_guard::capability::FS_READ])` (a real
    two-variant `Authority` value) fails that assertion, and the facade's `Subscribe`
    is then refused `GuardedNotSupported` (#613 criterion 16), so criterion 20 fails
    too. `Authority::Guarded(&[])` fails X05-T4's `EmptyCapabilityList`. Owner: F02-T2.

### (h) Drift

44. **Core ownership claim corrected.** After F02-T2,
    `rg -n "Owns the platform event loop" crates/keld-core/src/lib.rs` returns no
    match. The crate doc names the window registry, and the `crate.keld-core`
    product-status row describes the landed registry. *NC:* leaving the line as it is
    returns a match. Owner: F02-T2.
45. **Architecture sentence matches.** `docs/architecture/05-webview-and-native.md` §3
    states the keld-core registry over keld-wv handles as the `window` owner (this PR).
    *NC:* reverting the paragraph leaves §3 attributing windows to the keld-native
    surface, and spec review fails. Owner: this spec.

## 4. Design

### 4.0 First-principles and reuse decision

Atomic decomposition. Each atom has one owner and its own falsifier. Status is
*passed* (direct evidence), *unknown*, or *blocker*.

| # | Atom | Owner and boundary | Failure mode | Observable | Status and evidence |
|---|---|---|---|---|---|
| A1 | Identity minting | keld-core window registry; in: admitted `Create`; out: pair | facade-minted, reused or reset pair | criteria 1–6 | passed as a design input: KEL-75 §T4a puts the counter in the app-session registry; learnings 2026-08-29 (coordinator reset collides) |
| A2 | Native handle ownership | keld-wv UI thread; `views: BTreeMap<u32, View>` (`wkwebview/mod.rs:74`) | handle touched off the UI thread | keld-wv invariant; criterion 28 | passed: FACT, current code; no change |
| A3 | Teardown order | keld-wv `View` field order (`view_drop_order.rs`) | window released before webview | existing drop-order test; criterion 24 | passed: FACT, the `#[repr(C)]` field-order test exists |
| A4 | Event order | host writer of the primary link (`PrimaryRouterHandle`) | reorder across one window's facts | criteria 18, 32 | passed as a design input: one writer per link, one stream (02 §2) |
| A5 | Apply before listen | `@keld/api` mirror primitive | listener reads a stale value | criterion 16 | passed as a design input: gh527 §4.6, which is a draft |
| A6 | Zero traffic per read | facade getters | getter round trip | criterion 17 | passed: design; the frame counter is the oracle |
| A7 | Staleness under park | gh527 wake rule (consumed) | stale read after wake | criterion 19 | unknown until #528 lands; prototype FACT in #418 |
| A8 | Close state and `close_seq` | keld-core registry | stale accept, stacked prompts | criteria 25–27 | passed: FACT, harness `p3`/`n3` 5/5 |
| A9 | No timer | keld-core registry | timed auto-close | criterion 21 | passed: FACT, harness `n2` fails as required 5/5; PANEL-D19 |
| A10 | Exits on loss and terminal end | registry plus KEL-75 revoke; `owner_role_generation` | replay to a successor; a successor addressing a predecessor's window | criteria 30, 31 | passed as a design input: PANEL-D19; decided: D6 option B by the owner 2026-10-07 (transfer and replay, KEL-139 AC5) |
| A11 | Facade tombstones | `@keld/electron` | wrong `isDestroyed` at `closed` | criteria 23, 24 | passed: FACT, Electron E1 5/5 |
| A12 | Run-loop mode delivery | keld-wv wake bridge over tao's proxy | reply never delivered inside a nested loop | criterion 28 | source registered in common modes: FACT (tao 0.35.3 `event_loop.rs:336`). Whether tao dispatches its user callback inside a nested modal loop: **unknown** |
| A13 | Synchronous `id` and `closed` | `Create`/`Destroy` as blocking calls | `win.id` undefined after the constructor; a fabricated `closed` | criteria 1, 24 | decided (D1, §4.i): blocking calls over F04-T18 #528; edges #449 ← #528 and #450 ← #528 |
| A14 | Admission | keld-ipc receive policy plus table marker | other roles or webviews reach the channel | criteria 41–43 | passed: FACT, bridge admits only echo |
| A15 | Triage | `@keld/electron` (F02-T4) | escalating value accepted | criteria 36–40 | passed as a design input: #455, KEL-78 |
| A16 | Codec ownership | keld-ipc Rust types, one vector file, a thin TS adapter | two parsers drift | criteria 8, 12 | passed as a design input: KEL-136 and KEL-98 patterns |
| A17 | Window #1 | F01-T2 (consumed) | adoption shim or double window | criteria 13–15 | unknown until F01-T2 decides the boot declaration |
| A18 | Throwing-listener verdict | `@keld/electron` | no reply | criterion 29 | decided ▲ (D2, §4.i): `Veto`; Electron 44.4.5 oracle cell pending |
| A19 | Page `unload` on WKWebView teardown | engine | — | none; the machine never waits on the page | **unknown**; not decision-bearing, because no step waits for it |

Edges promoted from hidden coupling:

- A13 → A1 and A11. A non-blocking constructor cannot return a host-minted `id`, and a
  non-blocking `destroy()` cannot emit `'closed'` after host confirmation. So A1 and
  A11 depend on #528.
- A7 → A5. The parked path uses gh527's applier cursor. The unparked path uses the
  same per-channel applier on the dispatch cursor. There is one applier per channel.
- A10 ↔ KEL-75 T4a. Only the `Allow`, `Destroy` and terminal-end transitions linearize
  `WindowClosing(w)`. `CloseRequested` does not, because a veto must leave the window
  fully live.
- A12 ↔ F01-T3. A terminate-later wait runs in `NSModalPanelRunLoopMode`, so the same
  common-modes rule covers quit.

**Ownership, process, memory, I/O, lifecycle, trust and failure facts.**

- Handles. Native `NSWindow`/`WKWebView` stay owned by keld-wv on the UI thread (A2).
  The registry holds only `WindowId → (WindowGeneration, WebviewId,
  owner_role_generation, state)`. `owner_role_generation` is the RoleInstance
  generation of the link whose `Create` minted the window; it never changes in this
  slice. It never
  holds a native handle, and no handle crosses kipc (epic #447 never-list).
- Identity. Only the registry mints `WindowId`, `WindowGeneration` and `close_seq`. The
  facade receives them and never invents them (A1).
- Process and crash. The registry lives in the host process and survives Bun role
  crashes. A role crash ends pending closes through PANEL-D19's exits (A10). A host
  crash ends everything (KEL-75 host-death row). No new crash owner is added.
- I/O and queues. Host → app: window `EVENT`s and `REPLY`s go through the one primary
  link writer. App → host: `CALL`s through the existing primary reader. Core → UI:
  the existing `AppWindowCommand` channel bridged by tao's `EventLoopProxy`. UI → core:
  the existing `AppWindowEvent` channel. No new thread or socket is added.
- Lifecycle. A window is `Opening`, `Open`, `ClosePending`, `Closing` or `Destroyed`. Entries for
  destroyed windows stay as tombstones for the session, so a pair is never reused.
- Trust. The app process is semi-trusted (02 §4). Every app-supplied pair and
  `close_seq` is validated against registry state, never trusted.
- Failure. Every error is a registered `KELD-*` code in a `CallError`; deterministic
  failures are never retried.

**Reuse evaluated.**

| Option | Verdict |
|---|---|
| Lifecycle channel 3: add window variants to `LifecycleEvent` | rejected. Window facts are per-window and high-rate (`move`/`resize`). Mixing them into the session-control enum couples two contracts and widens a channel whose receive policy and replay rule (Ready replay to a recovered role) differ. |
| keld-native broker (`window` row in 05 §3) | rejected. keld-native depends only on ipc and guard (01 crate table) and cannot touch keld-wv handles. Adding that edge would make keld-native a second UI-thread owner. |
| keld-runtime `RoleRegistry` | rejected. It owns role identity, not windows. KEL-75 keeps window affinity as a separate host fact. |
| New keld-core module for the registry | chosen. It is a pure state machine with no AppKit, unit-testable without `EventLoop::new` (learnings 2026-08-12). `app_session.rs` composes it and stays the one lifecycle and router owner (keld-core `AGENTS.md`). |
| Existing `AppWindowCommand`/`AppWindowEvent` queues and tao proxy | reused for core ↔ UI. They gain variants; the `WebEngine` trait is unchanged. |
| KEL-136 shared transport plus gh527 `setStateApplier`/`callBlocking` | reused. No second reader. |

Named unmet requirement for the new code: no live component mints window identity,
holds a close pending, or carries window state to Bun (FACT: the only window signals
today are `NavigationReady` and `LastWindowClosed`, `engine.rs:100-105`).
Compatibility fallback: renderer-declared boots keep today's close path (§4.c). No
performance claim is made (§9).

### 4.a Identity

```rust
// crates/keld-ipc/src/window.rs (wire types; keld-ipc owns codecs like lifecycle.rs)
/// App-visible window number. Host-minted, nonzero, never reused in an app session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowId(pub u32);
/// Opaque window-incarnation identity (KEL-75). Host-minted, never reused in an app
/// session; compared only for equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowGeneration(pub u64);
/// Every window-addressed message carries both halves. Both are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowRef { pub id: WindowId, pub incarnation: WindowGeneration }
```

- The registry is the keld-core app-session object, not a per-role coordinator, so a
  RoleInstance generation rotation cannot reset it (criterion 3). It keeps two
  counters: WindowId starts at 1, with a checked `u32` increment that fails closed;
  WindowGeneration starts at 1, with a checked `u64` increment. INFERENCE: KEL-75's
  "the monotonic counter" sentence refers to RoleInstance identity; the WindowGeneration
  counter is a separate counter with the same owner and the same no-reset rule.
- `WindowId` is a `u32` because Electron's `id` is a JS number and the facade exposes
  it unchanged. `WindowGeneration` is a `u64` decoded as `bigint` in TypeScript. It is
  never shown to app code.
- Pair minted at `Create` admission, before the native build. A failed build retires
  the pair (`Destroyed` tombstone with no `Created` event), and `Create` answers `ERR`.
- Why both halves: the facade exposes `WindowId` as Electron's `id`. KEL-75
  `window-bound` declarations bind to the WindowGeneration. In this spec every
  incarnation also gets a fresh WindowId, so the check is redundant today. It still
  makes a forged or stale half fail (criterion 5) and keeps KEL-75's binding key.
- A native window number (tao `WindowId`, `NSWindow.windowNumber`) or keld-wv's
  `WebviewId` is a diagnostic only. The registry maps `WindowId → WebviewId` privately.

### 4.b Wire

**Channel.** One new app-link channel named `window`, with its id allocated by the
X05-T2 rule in the keld-ipc table (X05-T4 #597). The name is host-neutral. The entry's
receive class is `HostCallWithEvents`, like lifecycle. Its authority is
`Authority::HostInternal` (§4.g). It is the primary link's third channel, so it does
not lengthen `ReceivePolicy::also_channel` (`receive.rs:132-136`: "a third channel is a
new spec row, not a longer list"). App side: the window id is listed in #610's
per-frame `WorkerReceiveTable.eventChannels`, and replies to the role's own window
calls are admitted by correlation, as gh527 §4.7 defines. Host side: the primary
reader selects the KEL-133 policy per frame by channel from a fixed table of named
constructors (today's echo and lifecycle row, plus one new window row) under the
KEL-133 owner; no second `also_channel` slot is added. This spec does not
choose the number: F02-T2 appends the entry under #508's rule (draft PR #613), which
assigns the id (D5, §4.i).

**Messages** (postcard; discriminants pinned in the vector file, append-only):

```rust
/// Outer-frame bounds in logical points, top-left origin of the primary display.
pub struct Rect { pub x: i32, pub y: i32, pub width: u32, pub height: u32 }
pub struct WindowState { pub bounds: Rect, pub focused: bool, pub maximized: bool, pub full_screen: bool }

/// Host → app EVENTs on the window channel.
pub enum WindowEvent {
    Created { window: WindowRef, state: WindowState },   // 0: first record for a window
    // `Created` also carries `pending_close: Option<u64>` (D6): the close_seq of a close
    // entered during the recovery gap, sent only in a replay; None on every ordinary create.
    Bounds { window: WindowRef, bounds: Rect },           // 1: move or resize, on change
    Focus { window: WindowRef, focused: bool },            // 2
    Maximized { window: WindowRef, maximized: bool },      // 3
    FullScreen { window: WindowRef, full_screen: bool },   // 4
    CloseRequested { window: WindowRef, close_seq: u64 },  // 5
    Destroyed { window: WindowRef },                       // 6: last record for a window
}

/// App → host CALLs.
pub enum WindowRequest {
    Create { width: u32, height: u32, x: Option<i32>, y: Option<i32> },            // 0, blocking
    SetMaximized { window: WindowRef, maximized: bool },                           // 1
    SetFullScreen { window: WindowRef, full_screen: bool },                        // 2
    RequestClose { window: WindowRef },                                            // 3, never blocking
    CloseReply { window: WindowRef, close_seq: u64, verdict: CloseVerdict },       // 4
    Destroy { window: WindowRef },                                                 // 5, blocking
}
pub enum CloseVerdict { Allow, Veto }

/// Host → app REPLYs.
pub enum WindowResponse {
    Created { window: WindowRef },       // 0; written after the Created EVENT
    Accepted,                            // 1; setters (the state change arrives as an EVENT)
    CloseRequested { close_seq: u64 },   // 2; RequestClose started a request
    Merged { close_seq: u64 },           // 3; RequestClose merged into the pending one
    AlreadyClosing,                      // 4
    ReplyApplied,                        // 5
    ReplyStale,                          // 6; plus one host diagnostic
    Destroyed,                           // 7; written after the Destroyed EVENT
}
```

Every `ERR` is a `CallError` with a registered code (02 §2). This spec reserves the
next free numbers. FACT (2026-10-07): `origin/main`'s registry ends at
`KELD-CORE-037`, and no open PR's diff adds a higher `KELD-CORE-*` code (#609 and #287
only cite `KELD-CORE-037`). The implementing PR re-checks main and open PRs before it
registers each code in `docs/engineering/keld-error-codes.md` with a fix sentence.

| Code | Meaning | Registered by |
|---|---|---|
| `KELD-CORE-038` | stale or forged `(WindowId, WindowGeneration)`, or an app call in a state that refuses it | F02-T2 |
| `KELD-CORE-039` | a registry counter is exhausted (`WindowId`, `WindowGeneration` or `close_seq`; the detail names it) | F02-T2 |
| `KELD-CORE-040` | registry not ready | F02-T2 |
| `KELD-CORE-041` | native create failed (names the failed stage; the keld-wv `WvError` text goes to the host log) | F02-T2 |
| `KELD-CORE-042` | window calls unsupported on this platform (Windows/Linux, F02-T12) | F02-T2 |
| `KELD-CORE-043` | window not owned by the calling RoleInstance generation | F02-T3 |
 The facade's own
"Object has been destroyed" and triage errors are `KELD-COMPAT-*`.

**Ordering contract** (the stream the mirror consumes):

1. For one window: `Created` first, `Destroyed` last, nothing after `Destroyed`.
2. Facts are written in the order the registry linearizes them. All window frames for
   a link pass through that link's single writer, so app arrival order is host
   linearization order (A4).
3. Facts are written only on change. The host reads tao state on the UI thread after
   each `Moved`, `Resized`, `Focused` or full-screen transition and forwards a fact only
   when the value differs from the last one sent.
4. Reply placement: a `REPLY` is written after every `EVENT` that its call caused
   (`Created` before `Created`; `Destroyed`, then any `LastWindowClosed`, before
   `Destroyed`, T11). With gh527 step 1, the
   caller therefore sees host state as of the reply.
5. Cross-channel: for the last window, `Destroyed` (window channel) is written before
   `LastWindowClosed` (lifecycle channel) on the same link (criterion 32).

**Version handling.** No frame, flag, kind or `HELLO` change, so `PROTOCOL_VERSION`
stays 2 (keld-ipc `AGENTS.md`). The new channel and payloads take the wire-protocol and
public-API review gates. The host and `@keld/electron` ship as one build, and v0 has no
channel-table exchange. So a mismatched peer fails closed: an unknown channel is
`KELD-IPC-005` at the KEL-133 validator, and an unknown discriminant ends the link
(criterion 11). Evolution is append-only: a new variant takes the next discriminant;
changing an existing variant's fields is a public-API review plus a vector update.
Handshake-time resolution stays destination work (02 §4).

**Contract-authoring owner (decided here).** Extend the existing thin codec adapters
over the KEL-136 shared transport. Rust types live in `keld-ipc/src/window.rs` (as
`lifecycle.rs` does). The TypeScript codec is one hand-written adapter in `@keld/api`.
Both replay one checked-in golden-vector file, the single oracle in the KEL-133 corpus
pattern. The channel constant comes from the X05-T4 generated file. Rejected options:

- *Amend KEL-98.* Its approved non-goals exclude general contract authoring, and it
  generates type declarations only; the codec would still be hand-written. Amending a
  closed spec to carry one channel widens it for no reduction in parsers.
- *Wait for the separately specified schema generator* (`@keld/schema`, `keld gen`).
  It has no approved spec or owner, so it would block the first proof (YAGNI).

### 4.c Window #1

This spec consumes F01-T2's rule (#446) and adds no adoption shim.

- Facade boot (the declaration F01-T2 chooses): the host writes Ready after
  authenticated `HELLO` and creates no window. The registry is constructed before Ready
  is written (criterion 14). Window #1 is the first `Create`, so its WindowId is 1.
- Renderer-declared boot: unchanged. The host-created initial window is not entered in
  the registry, no window-channel frame is written, and its `CloseRequested` keeps
  today's teardown and `LastWindowClosed` behaviour (criterion 15).
- The close path is selected per boot kind, not per window: in one session every
  window takes one path. Temporary adapter: the legacy teardown arm. Its owner is
  F01-T2. Removal condition: F01-T2 decision (b) ("whether renderer-declared boots keep
  `CreateInitialWindow` before Ready"). If those boots move onto the registry, the
  legacy arm is deleted in that change.
- Ready meaning per RoleInstance generation is F01-T2's. Successor roles (D6, §4.i):
  a retired generation's windows stay alive, and ownership transfers explicitly to the
  recovered successor of the same declared role, which receives a state replay
  (criterion 30). Old-generation handles stay invalid (`KELD-CORE-043`).

### 4.d Mirror primitive

F02 owns one primitive in `@keld/api`. F06-T5 reuses it for display and theme facts.

```ts
// packages/@keld/api/src/mirror.ts (internal to Keld packages; not an app export)
export interface MirrorSource<Fact> {
  /** Channel id from the X05-T4 generated constants. */
  readonly channel: number;
  /** Throws on an unknown discriminant or bad payload; the link closes with cause KELD-IPC-005 (criterion 11). */
  decode(payload: Uint8Array): Fact;
  /** Synchronous; mutates mirror state only; never calls user code. */
  apply(fact: Fact): void;
  /** Runs user listeners; called only in the dispatch phase. */
  dispatch(fact: Fact): void;
}
export function attachMirror<Fact>(link: WorkerLink, source: MirrorSource<Fact>): void;
```

`attachMirror` registers `apply` through gh527's `setStateApplier` (at most one per
channel) and `dispatch` on the same channel's event delivery. The contract:

1. **Ordering.** For every record, `apply` runs before any listener for that record or
   any later record (criterion 16). Listeners run in issue order.
2. **Zero traffic per read.** A getter reads mirror state only (criterion 17).
3. **Staleness.** Unparked: state reflects every record the dispatch cursor has
   reached. Parked: gh527 §4.6 step 1 applies every fact up to and including the reply
   before `callBlocking` returns; listeners run after resume (criterion 19). A read is
   never fresher than the last applied host fact. ▲ divergence: Electron reads native
   state synchronously, so a user resize in progress is visible to Electron sooner.
4. **Terminal on failure.** A throwing `apply` or `decode` closes the link with the
   cause `KELD-IPC-005` (gh527 §4.4 and §4.6; criterion 11), and parked or pending
   calls throw `KELD-IPC-022` naming that cause. keld-ipc's meanings are kept:
   `KELD-IPC-003` is the codec failure inside `decode` and is never surfaced, and
   `KELD-IPC-005` ("unexpected kipc frame or session state") is the link-close cause,
   the same layering 02 §2 uses for an undecodable `ERR` payload. gh527's other wake
   codes pass through unchanged. A mirror that missed a fact must not serve state.
5. **Admission.** Facts reach only roles the host admits (criterion 20). For the first
   proof, that is the current primary RoleInstance generation.
6. **Writes.** Setters are non-blocking `CALL`s, legal in `Open` and in
   `ClosePending` (T12). The mirror changes only when the host
   `EVENT` arrives (#449). So `isMaximized()` immediately after `maximize()` may still
   be false. Inside the `'maximize'` listener it is true.

The window mirror entry keeps the host facts (`WindowState`, live or host-destroyed,
pending `close_seq`). The Electron-facing tombstones are separate (§4.e).

### 4.e Close state machine

Host registry, per window:

| State | Meaning | Native view |
|---|---|---|
| `Opening` | pair minted, native build in progress on the UI thread; not live for the last-window policy | being created |
| `Open` | live, no request pending | live |
| `ClosePending(close_seq)` | `CloseRequested` written, reply awaited | live |
| `Closing` | KEL-75 `WindowClosing(w)` linearized; teardown in progress (or awaiting a late build result) | bridge destroyed, then webview, then window |
| `Destroyed` | tombstone; pair retired for the session | none |

Per-window registry state is `(WindowGeneration, WebviewId, owner_role_generation,
state, last close_seq)`. `Create` admission mints a new entry in `Opening` with
`owner_role_generation` set to the calling link's RoleInstance generation.

**Admission before the table.** Every app call that names a window passes two checks,
in order, before the transition table: (1) the pair resolves to an entry with the same
`WindowGeneration`, else `KELD-CORE-038` (criteria 4, 5); (2) the calling link's
RoleInstance generation equals the entry's `owner_role_generation`, else
`KELD-CORE-043` (criterion 30). Neither changes state.

**Inputs.** Each is distinct; none is an alias of another:

| Input | Source |
|---|---|
| `BuildOk` / `BuildFail` | UI loop result of the native build (late if it arrives after T9 or T10) |
| `Native close` | tao `CloseRequested` for the window |
| `RequestClose` | app call |
| `Reply current` | app `CloseReply(s, v)` with `s` equal to the pending `close_seq` |
| `Reply stale` | app `CloseReply(t, _)` with `t` not the pending `close_seq` (superseded, or no request pending) |
| `Setter` | app `SetMaximized` / `SetFullScreen` |
| `Destroy` | app call |
| `Released` | UI loop reports bridge, webview and window released |
| `Bounds fact` | UI loop `WindowBounds` (move or resize) |
| `Focus fact` | UI loop `WindowFocus` |
| `Maximized fact` | UI loop `WindowMaximized` |
| `FullScreen fact` | UI loop `WindowFullScreen` |
| `Role loss` | the `owner_role_generation` is retired (KEL-75 `RevokeAll`) |
| `Session end` | accepted `Quit`, or no successor role will be provisioned |

**Transition table.** Total and deterministic: one row per (state, input) pair
(76 rows; `Reply current` in `ClosePending` has one row per verdict), and nothing
else. "ERR 038" is the stale-window `ERR`; it changes no state. A pure registry unit
test enumerates every pair and asserts its row.

| # | State | Input | Next state | Writes / effects |
|---|---|---|---|---|
| O1 | `Opening` | `BuildOk` | `Open` | `Created`, then `REPLY Created` (T1) |
| O2 | `Opening` | `BuildFail` | `Destroyed` | `ERR 041`; no `Created` (T2) |
| O3 | `Opening` | `Native close` | `Opening` | one host diagnostic; ignored (no `Created` yet) |
| O4 | `Opening` | `RequestClose` | `Opening` | ERR 038 |
| O5 | `Opening` | `Reply current` | — | impossible: no request pending; classified as `Reply stale` |
| O6 | `Opening` | `Reply stale` | `Opening` | `REPLY ReplyStale`, one host diagnostic (T7) |
| O7 | `Opening` | `Setter` | `Opening` | ERR 038 |
| O8 | `Opening` | `Destroy` | `Opening` | ERR 038 |
| O9 | `Opening` | `Released` | `Opening` | one host diagnostic (invariant violation); ignored |
| O10 | `Opening` | `Role loss` | `Closing` | the build result is awaited as a late input (C1, C2); no frame; no pair was ever delivered, so nothing is adopted (T9) |
| O11 | `Opening` | `Session end` | `Closing` | as the `Role loss` row; the blocked `Create` caller gets gh527's `KELD-IPC-024` from the host drain (T10) |
| O12 | `Opening` | `Bounds fact` | — | impossible: a fact names a `view`, and an `Opening` window has no view until O1 maps it; classified as an unmapped-view fact (see State facts) |
| O13 | `Opening` | `Focus fact` | — | impossible: a fact names a `view`, and an `Opening` window has no view until O1 maps it; classified as an unmapped-view fact (see State facts) |
| O14 | `Opening` | `Maximized fact` | — | impossible: a fact names a `view`, and an `Opening` window has no view until O1 maps it; classified as an unmapped-view fact (see State facts) |
| O15 | `Opening` | `FullScreen fact` | — | impossible: a fact names a `view`, and an `Opening` window has no view until O1 maps it; classified as an unmapped-view fact (see State facts) |
| P1 | `Open` | `BuildOk` | `Open` | one host diagnostic (invariant violation); ignored |
| P2 | `Open` | `BuildFail` | `Open` | one host diagnostic (invariant violation); ignored |
| P3 | `Open` | `Native close` | `ClosePending(n)` | `n` = last `close_seq` + 1 by checked `u64` add; `CloseRequested(n)` (T3). On overflow: stays `Open`, one diagnostic, no write (T3x) |
| P4 | `Open` | `RequestClose` | `ClosePending(n)` | as the `Native close` row, plus `REPLY CloseRequested(n)` (T3). On overflow: stays `Open`, ERR 039 (T3x) |
| P5 | `Open` | `Reply current` | — | impossible: no request pending; classified as `Reply stale` |
| P6 | `Open` | `Reply stale` | `Open` | `REPLY ReplyStale`, one host diagnostic (T7) |
| P7 | `Open` | `Setter` | `Open` | `REPLY Accepted`; the fact follows as an `EVENT` (T12) |
| P8 | `Open` | `Destroy` | `Closing` | `WindowClosing(w)`; teardown command; the `REPLY Destroyed` is deferred to C9 (T8) |
| P9 | `Open` | `Released` | `Open` | one host diagnostic (invariant violation); ignored |
| P10 | `Open` | `Role loss` | `Open` | no frame; the recovery gap starts; ownership transfers to the recovered successor of the same declared role, with a `Created` replay (criterion 30) (T9) |
| P11 | `Open` | `Session end` | `Closing` | `WindowClosing(w)`; teardown command; no `CloseRequested` (T10) |
| P12 | `Open` | `Bounds fact` | `Open` | applied to the registry's last-sent state; if the value differs, one `EVENT` of that kind, in arrival order (§4.b ordering rules 2 and 3); otherwise nothing |
| P13 | `Open` | `Focus fact` | `Open` | applied to the registry's last-sent state; if the value differs, one `EVENT` of that kind, in arrival order (§4.b ordering rules 2 and 3); otherwise nothing |
| P14 | `Open` | `Maximized fact` | `Open` | applied to the registry's last-sent state; if the value differs, one `EVENT` of that kind, in arrival order (§4.b ordering rules 2 and 3); otherwise nothing |
| P15 | `Open` | `FullScreen fact` | `Open` | applied to the registry's last-sent state; if the value differs, one `EVENT` of that kind, in arrival order (§4.b ordering rules 2 and 3); otherwise nothing |
| Q1 | `ClosePending(s)` | `BuildOk` | `ClosePending(s)` | one host diagnostic (invariant violation); ignored |
| Q2 | `ClosePending(s)` | `BuildFail` | `ClosePending(s)` | one host diagnostic (invariant violation); ignored |
| Q3 | `ClosePending(s)` | `Native close` | `ClosePending(s)` | merged; nothing written (T4) |
| Q4 | `ClosePending(s)` | `RequestClose` | `ClosePending(s)` | `REPLY Merged(s)` (T4) |
| Q5 | `ClosePending(s)` | `Reply current`, `Veto` | `Open` | `REPLY ReplyApplied` (T5) |
| Q6 | `ClosePending(s)` | `Reply current`, `Allow` | `Closing` | `REPLY ReplyApplied`; `WindowClosing(w)`; teardown command (T6) |
| Q7 | `ClosePending(s)` | `Reply stale` | `ClosePending(s)` | `REPLY ReplyStale`, one host diagnostic (T7) |
| Q8 | `ClosePending(s)` | `Setter` | `ClosePending(s)` | `REPLY Accepted`; the fact follows as an `EVENT`; `s` stays pending (T12) |
| Q9 | `ClosePending(s)` | `Destroy` | `Closing` | `s` abandoned; then as the `Open`/`Destroy` row (T8) |
| Q10 | `ClosePending(s)` | `Released` | `ClosePending(s)` | one host diagnostic (invariant violation); ignored |
| Q11 | `ClosePending(s)` | `Role loss` | `Open` | `s` gets the one terminal outcome `Abandoned`; then as the `Open`/`Role loss` row (T9) |
| Q12 | `ClosePending(s)` | `Session end` | `Closing` | `s` gets the one terminal outcome `Abandoned`; then as the `Open`/`Session end` row (T10) |
| Q13 | `ClosePending(s)` | `Bounds fact` | `ClosePending(s)` | applied to the registry's last-sent state; if the value differs, one `EVENT` of that kind, in arrival order (§4.b ordering rules 2 and 3); otherwise nothing; `s` stays pending |
| Q14 | `ClosePending(s)` | `Focus fact` | `ClosePending(s)` | applied to the registry's last-sent state; if the value differs, one `EVENT` of that kind, in arrival order (§4.b ordering rules 2 and 3); otherwise nothing; `s` stays pending |
| Q15 | `ClosePending(s)` | `Maximized fact` | `ClosePending(s)` | applied to the registry's last-sent state; if the value differs, one `EVENT` of that kind, in arrival order (§4.b ordering rules 2 and 3); otherwise nothing; `s` stays pending |
| Q16 | `ClosePending(s)` | `FullScreen fact` | `ClosePending(s)` | applied to the registry's last-sent state; if the value differs, one `EVENT` of that kind, in arrival order (§4.b ordering rules 2 and 3); otherwise nothing; `s` stays pending |
| C1 | `Closing` | `BuildOk` (late, after a `Role loss` or `Session end` in `Opening`) | `Closing` | teardown command for the just-built view; no frame |
| C2 | `Closing` | `BuildFail` (late, as above) | `Destroyed` | nothing to release; no frame (no `Created` was ever written) |
| C3 | `Closing` | `Native close` | `Closing` | nothing written |
| C4 | `Closing` | `RequestClose` | `Closing` | `REPLY AlreadyClosing`, the only writer of `AlreadyClosing` |
| C5 | `Closing` | `Reply current` | — | impossible: no request pending in `Closing`; classified as `Reply stale` |
| C6 | `Closing` | `Reply stale` | `Closing` | `REPLY ReplyStale`, one host diagnostic (T7) |
| C7 | `Closing` | `Setter` | `Closing` | ERR 038 |
| C8 | `Closing` | `Destroy` | `Closing` | one more deferred `REPLY Destroyed`, written at the `Released` row |
| C9 | `Closing` | `Released` | `Destroyed` | T11 below |
| C10 | `Closing` | `Role loss` | `Closing` | nothing; already closing (an earlier `Destroy` reply is not written, because the owner link is retired) |
| C11 | `Closing` | `Session end` | `Closing` | nothing; already closing |
| C12 | `Closing` | `Bounds fact` | `Closing` | dropped, never written; counted in a saturating per-window `u64` drop counter that the T11 diagnostic reports once |
| C13 | `Closing` | `Focus fact` | `Closing` | dropped, never written; counted in a saturating per-window `u64` drop counter that the T11 diagnostic reports once |
| C14 | `Closing` | `Maximized fact` | `Closing` | dropped, never written; counted in a saturating per-window `u64` drop counter that the T11 diagnostic reports once |
| C15 | `Closing` | `FullScreen fact` | `Closing` | dropped, never written; counted in a saturating per-window `u64` drop counter that the T11 diagnostic reports once |
| D1 | `Destroyed` | `BuildOk` | `Destroyed` | one host diagnostic (invariant violation); ignored |
| D2 | `Destroyed` | `BuildFail` | `Destroyed` | one host diagnostic (invariant violation); ignored |
| D3 | `Destroyed` | `Native close` | `Destroyed` | ignored |
| D4 | `Destroyed` | `RequestClose` | `Destroyed` | ERR 038 |
| D5 | `Destroyed` | `Reply current` | — | impossible; classified as `Reply stale` |
| D6 | `Destroyed` | `Reply stale` | `Destroyed` | ERR 038 (a tombstone answers no close reply) |
| D7 | `Destroyed` | `Setter` | `Destroyed` | ERR 038 |
| D8 | `Destroyed` | `Destroy` | `Destroyed` | ERR 038 |
| D9 | `Destroyed` | `Released` | `Destroyed` | one host diagnostic (invariant violation); ignored |
| D10 | `Destroyed` | `Role loss` | `Destroyed` | nothing |
| D11 | `Destroyed` | `Session end` | `Destroyed` | nothing |
| D12 | `Destroyed` | `Bounds fact` | `Destroyed` | dropped, never written; counted in the registry's saturating session-level `u64` drop counter, reported at session end |
| D13 | `Destroyed` | `Focus fact` | `Destroyed` | dropped, never written; counted in the registry's saturating session-level `u64` drop counter, reported at session end |
| D14 | `Destroyed` | `Maximized fact` | `Destroyed` | dropped, never written; counted in the registry's saturating session-level `u64` drop counter, reported at session end |
| D15 | `Destroyed` | `FullScreen fact` | `Destroyed` | dropped, never written; counted in the registry's saturating session-level `u64` drop counter, reported at session end |

The "impossible" rows are not gaps: a `CloseReply` is classified as `Reply current` only
when a request is pending and the sequence matches, so in every other state it is
`Reply stale` by definition.

**T11 (row C9).** In one step on the owner link's single writer, and only while that
link is live and not retired (after a `Role loss` row nothing is written): `Destroyed`, if a
`Created` was written for this window; then `LastWindowClosed`, if no live window
remains and the step is outside a session end; then every deferred `REPLY Destroyed`.
*Live* means `Open`, `ClosePending` or `Closing`; an `Opening` window is not live until
it reaches `Open`.

The facade runs no last-window policy for that `LastWindowClosed` until the `Destroy`
caller has emitted `'closed'` (§4.e facade rules).

**State facts.** A fact names a keld-wv `view`, and the registry maps a view to a
window only at O1, from `WindowBuilt`. keld-wv writes `WindowBuilt` before any fact for
that view on the one FIFO UI → core channel, and its snapshot is the window's initial
state, so an `Opening` window can never receive a fact (O12-O15 are impossible rows,
enumerated like O5). A fact whose view is unmapped (only a keld-wv ordering bug can
produce one) matches no window: it is dropped with one host diagnostic, and a unit
test feeds one to prove it. Drops in `Closing` and `Destroyed` are counted, never
logged per fact, so a resize storm during teardown cannot flood the log.

**Recovery gap (D6).** From `Role loss` until the recovered successor's
admitted `Subscribe`, `owner_role_generation` names a retired generation. Every row
that would write to the owner link (P3 and P12-P15, and their `ClosePending`
counterparts) writes nothing; its effect stays in the registry's host snapshot, which
holds the latest value of each fact (coalesced, never a stream). App calls during the
gap fail admission with `KELD-CORE-043`, because no live link owns the window, so P4,
P7 and P8 cannot occur in the gap. A native close enters `ClosePending(n)` (P3) with
the veto held: nothing closes the window, and `n` is delivered as
`Created.pending_close` in the replay, so the successor answers it. The gap ends only
with the successor's `Subscribe` (transfer and replay, criterion 30) or with `Session
end`, including a failed recovery in which KEL-75 provisions no successor; P11 or Q12
then applies on the quit path. No timer ends it. The replay sends `Created` only for
windows in `Open` or `ClosePending`. A window already in `Closing` finishes its
teardown, is not replayed, and reaches the successor only as nothing at all, because
it is never adoptable (criterion 30 (iv) and NC 8).

The T-numbers used elsewhere in this spec name these rows: T1 = O1, T2 = O2, T3 =
P3/P4, T3x = their overflow branch, T4 = Q3/Q4, T5 = Q5, T6 = Q6, T7 = O6/P6/Q7/C6, T8 =
P8/Q9, T9 = O10/P10/Q11 (role loss), T10 = O11/P11/Q12 (session end), T11 = C9, T12 =
P7/Q8.

Rules:

- **`close_seq` never wraps.** It is a per-window `u64`, starting at 1, advanced only
  by T3's checked add. Each advance needs one written `CloseRequested` and its reply, so
  even 10^9 close round trips per second (well beyond a kipc round trip) take about 584
  years to exhaust it. A `u32` would not be enough: a hostile app looping
  `RequestClose` and `Veto` at 10^6 round trips per second exhausts it in about 72
  minutes. Exhaustion is still defined (T3x, criterion 26) because a library must not
  panic or wrap. It decodes as `bigint` in TypeScript, like `WindowGeneration`, and is
  never shown to app code.
- **No timer.** No transition has elapsed time as its input. The registry has an
  injected clock only so the test can prove that (criterion 21). The gh527 call
  deadline on blocking `Create` and `Destroy` is the transport's per-wait I/O deadline
  (02 §7). It never completes or vetoes a close; on expiry the facade throws
  `KELD-IPC-006` and emits no `'closed'`.
- **Three exits (PANEL-D19).** `ClosePending` leaves only by the reply (T5, T6), by
  RoleInstance generation loss (T9, which keeps the window for adoption; D6), or by
  the session terminal end (T10). `Destroy` (T8) is the app's own exit and is not a host completion.
- **Fail-closed.** Absence of `Allow` never closes a window. A veto, a stale reply and
  a throwing listener (criterion 29) all keep it open. Only `Allow`, `Destroy`, role
  loss and session end close it, and each is an explicit input.
- **Re-emission and merge.** Each native close attempt made while no request is pending
  writes a fresh `CloseRequested` (T3). An attempt made while one is pending merges
  into it (T4). This reconciles F02-T3's "every native attempt re-emits `close`" with
  #419's merge rule. Electron cannot observe an attempt during a pending request,
  because its `'close'` runs synchronously inside `windowShouldClose:`
  (`electron_ns_window_delegate.mm:411-414`). AppKit itself never delivered
  `performClose:` to the hook during the modal (10/10 in the harness).
- **KEL-75 tombstone.** `WindowClosing(w)` is linearized at T6, T8, T9 and T10, never
  at T3. Window-bound roles (KEL-75/T4) hook there later.
- **Teardown order.** At `Closing` the UI loop destroys the renderer bridge, then drops
  the `View`, whose field order releases the webview before the window (A3). Page
  `unload` dispatch during WKWebView teardown is unknown (A19). The machine never waits
  for a page callback, so a hung page cannot hold a close.
- **Run-loop modes.** Commands reach the UI loop through tao's `EventLoopProxy`. Its
  `CFRunLoopSource` is added in `kCFRunLoopCommonModes` (FACT, tao 0.35.3
  `event_loop.rs:336`), which includes `NSModalPanelRunLoopMode`. Whether tao invokes
  the user callback inside a nested modal loop is unknown (A12). Criterion 28 is the
  falsifier. If it fails, F02-T3 must add a common-modes delivery path in the keld-wv
  macOS loop under keld-wv's existing CFRunLoop `unsafe` allowance and its review.
- **Quit (F01-T3 consumes).** The registry has no cross-window quit state (criterion
  33). The facade's quit loop sends `RequestClose` to every window in reverse creation
  order (newest first, as PANEL-P2 E5 recorded `w2` then `w1`) and keeps going after
  a veto (#419: continue asking). `RequestClose` is never blocking, so
  `app.quit()` can return before any `'closed'` (criterion 35). `window-all-closed`
  suppression during quit is facade state (criterion 34).

Facade (`@keld/electron`), per `BrowserWindow`:

- Two Electron-facing tombstones: `winDestroyed` (read by `isDestroyed()` and every
  getter) and `wcDestroyed` (read by `webContents.isDestroyed()`). Each flips only
  immediately before its own event is emitted, and only after the host fact exists.
- `'close'`: emitted in the dispatch phase of `CloseRequested(s)`, with an event object
  whose `preventDefault()` sets the verdict. After every listener has run (an exception
  is caught, counted as one diagnostic and forces `Veto`), the facade sends exactly one
  `CloseReply(s, verdict)`.
- `close()`: a non-blocking `RequestClose`. ▲ divergence: Electron emits `'close'`
  inside `close()`; Keld emits it after `close()` returns, in issue order.
- `destroy()`: a blocking `Destroy`. After it returns: if `!winDestroyed`, flip it and
  emit `'closed'`; then, if the wake drain applied a `LastWindowClosed`, run the
  last-window policy (below); then return. No `'close'` is emitted. `wcDestroyed`
  stays false.
- Last-window policy: one facade function. While the role is parked, the lifecycle
  applier only records that a `LastWindowClosed` is pending; it never emits
  `'window-all-closed'` and never sends `Quit` (gh527 §4.6: appliers call no user code).
  The policy consumes the pending record exactly once. While the facade quit state is
  active it emits nothing and sends nothing (criterion 34). Otherwise it emits
  `'window-all-closed'` if a listener exists, or, with no listener, sends the default
  non-blocking `Quit`. It
  runs from `destroy()` after `'closed'` on the destroy path, and from the
  `LastWindowClosed` dispatch on the normal close path; whichever runs second finds
  nothing pending. So on the destroy path the step order is `REPLY Destroyed` read,
  `'closed'`, last-window policy (and any `Quit`), `destroy()` returns, as Electron E1
  recorded: `'closed'` fires inside `destroy()` and the quit cascade follows within
  that call. No `Quit` can be written before the `Destroy` reply is read.
- `Destroyed` dispatch: if `!wcDestroyed`, flip it and emit webContents `'destroyed'`.
  Then, if `!winDestroyed`, flip it and emit `'closed'`. This yields `destroyed` →
  `closed` on the normal path and `closed` (inside `destroy()`) → `destroyed` on the
  destroy path, as Electron E1 recorded 5/5. Neither event is ever emitted twice.
- KEL-139 T1b: `'closed'` is emitted only after the host's `Destroyed` fact, so the
  facade fabricates no native completion. ▲ divergence recorded as a cell: on the
  destroy path, Electron emits webContents `'destroyed'` after app `'quit'`; Keld emits
  it in the dispatch phase after `destroy()` returns, and its order relative to
  `'quit'` is F01-T3's quit cell. `window-all-closed` inside `destroy()` now matches
  Electron.

### 4.f Constructor triage

Triage runs in the facade before any frame is written (F02-T4 owns the table data).

| Field (draw.io set) | Outcome |
|---|---|
| `width`, `height`, `x`, `y` | mapped to `Create` |
| `backgroundColor`, `icon` | accepted, one diagnostic each, until F02-T5 / F02-T10 map them |
| `webPreferences.preload`, `.additionalArguments` | handed to F07-T1; scoreboard ▲ "no Node access" |
| `webPreferences.webviewTag:false`, `.contextIsolation:true`, `.nodeIntegration:false`, `.webSecurity:true` | accepted |
| `webPreferences.disableBlinkFeatures`, `.spellcheck` | engine-only: accepted and ignored, one diagnostic each |
| `nodeIntegration:true`, `nodeIntegrationInWorker:true`, `nodeIntegrationInSubFrames:true`, `contextIsolation:false`, `webSecurity:false`, `allowRunningInsecureContent:true` | refused at construction with a `KELD-COMPAT-*` error naming the safe value; no `Create` frame |
| any other field | accepted, one diagnostic, until F02-T10's table |

- No `keld.compat.ts` quirk, flag or authority profile is consulted for the six values.
  A quirk may narrow authority, never widen it. `sandbox:false` never selects a Keld
  profile (KEL-78).
- Defence in depth: `Create` has no field that could carry any `webPreferences` value
  (criterion 40). A facade bug therefore cannot widen host authority over this channel.
- The same triage applies to `overrideBrowserWindowOptions` (F03-T4 owns the handler).

### 4.g Guard

Security decomposition:

| Concern | Rule |
|---|---|
| Identity | the caller is the primary RoleInstance generation bound to this link (KEL-75); payload bytes never name a principal |
| Authentication | the existing 32-byte `HELLO` possession proof (02 §2); no change |
| Authorization | first proof: window `CALL`s admitted only on the primary app link (receive policy), and each window call only from the window's `owner_role_generation`; window `EVENT`s only after a `Subscribe` admitted for the declared `primary` role (#613 criterion 16). The table entry is host-internal, like lifecycle: window UI is the app's own surface and grants no OS resource (01 principle 2). No `keld-guard` evaluation runs, and no capability name is added |
| OS containment | unchanged; window calls touch only host-owned UI state on the UI thread |
| Lifecycle and revocation | RoleInstance generation revocation (KEL-75 `RevokeAll`) drops the link; that generation's windows take T9 and stay alive; ownership transfers explicitly to the recovered successor with a replay (D6); a link that is not the owner is refused (`KELD-CORE-043`) |
| Evidence provenance | registry transitions are host facts; every app-supplied pair and `close_seq` is validated (criteria 4, 5, 27) |

Webview-originated window operations (the renderer bridge, page `window.close()`)
stay denied until KEL-102's per-window grant amendment (F04-A1). The bridge admits only
echo today (`macos_bridge.rs:383`), and this spec does not add the window channel to
it. `docs/architecture/03-security.md`'s sentence on ungated lifecycle control gains
the window channel when F02-T2 makes it live, in that same change.

### 4.h Drift

- FACT: `crates/keld-core/src/lib.rs:3-5` says keld-core "Owns the platform event loop,
  the window/webview registries". keld-wv owns the event loop
  (`WkWebViewEngine::run_app_until_quit`) and the native view map. keld-core has no
  window registry: `rg -c "WindowId" crates` matches only tao's native
  `tao::window::WindowId` in `webview2/mod.rs:2157`, and `rg WindowGeneration crates`
  matches nothing.
- Correction, in F02-T2 (code; not this PR): the crate doc becomes "Owns the window
  registry (WindowId and WindowGeneration minting, window-state events, the close state
  machine), application lifecycle, and dispatch between kipc links and native modules;
  keld-wv owns the platform event loop and native window and webview handles." In the
  same change, F02-T2 updates the `crate.keld-core` row of
  `docs/engineering/product-status.tsv`.
- Correction, in this PR (architecture): 05 §3 gains the `window` ownership paragraph
  (§5).
- Checked and unchanged: the 01 crate table already assigns the window registry to
  keld-core. The keld-native `MODULES` list names `window` for doctor and manifest
  tooling; it is a surface name, not an owner, so it is left as is.

### 4.i Delegated decisions (reversible; recorded 2026-10-07)

The orchestrator took each decision below under the owner's delegation. Each one is
reversible, and each names the observation that reopens it.

- **D1. Blocking `Create` and `Destroy`.** `new BrowserWindow()` and `destroy()` are
  blocking host calls over the F04-T18 transport (#528, gh527). The constructor
  returns the host-minted `id` synchronously, and `'closed'` fires synchronously inside
  `destroy()`, as PANEL-P2 E1 recorded 5/5. The orchestrator adds the native edges
  #449 ← #528 and #450 ← #528. Rejected: an asynchronous constructor whose `id` is unset
  until `Created` (breaks Electron's synchronous `id` and getters); host-pushed reserved
  pairs consumed by the constructor (adds reservation state, and getters before
  `Created` serve the requested size, not the actual one). *Falsifier:* an approved
  design that meets id equality (criterion 1) and synchronous `'closed'` (criterion 24)
  without a blocking call.
- **D2. Throwing `'close'` listener = `Veto`.** The window stays open, the error is
  reported as one diagnostic, and exactly one `Veto` reply is sent (criterion 29). This
  is fail-closed and consistent with the veto contract. Marked ▲, because it is not yet
  verified against Electron. An Electron 44.4.5 oracle cell records the real behaviour.
  If Electron lets the close proceed, the cell becomes a recorded divergence.
  *Falsifier:* the oracle shows the close proceeds, and a first-proof step depends on
  that.
- **D3. Real-Mac close stimulus = Accessibility click.** Tests press the window's
  close button through an AX click, the method PANEL-P2's oracle used successfully. No
  debug `performClose:` hook and no new `unsafe` are added. *Falsifier:* AX is
  unavailable in the CI or OS lane that runs the real-Mac cells.
- **D4. Merge only while pending.** Repeat close requests merge only while one is
  pending (T4). F02-T3's "every native attempt re-emits" applies outside a pending
  prompt (T3). This matches #419. *Falsifier:* an Electron 44.4.5 observation of a
  second `'close'` emitted for an attempt made while the first `'close'` is unanswered.
- **D6. Successor roles. D6 decided by the owner 2026-10-07: Option B.** This is an
  architecture decision (it sets the crash ownership of windows), so the owner, not the
  orchestrator, took it. A window stays alive across a Bun-role crash. Ownership is
  handed explicitly to the recovered successor of the same declared role, old-generation
  handles stay invalid (`KELD-CORE-043`), and the host replays the authoritative window
  state (`Created` with `pending_close` for each window in `Open` or `ClosePending`) so
  the successor rebuilds its mirror. The recovery gap holds a pending close, resolves a
  failed recovery through the quit path, and has no timer (§4.e, criterion 30). This
  slice therefore takes F02-T11's adoption work. *Rejected:* option A, destroying a
  retired generation's windows and refusing successors. It is simpler, but facade boots
  would then deviate from the approved KEL-139 AC5 ("recovery keeps the same window and
  document") until KEL-143. Option B keeps one invariant across renderer-declared and
  facade boots, consistent with AC5, and root `AGENTS.md` forbids silently deviating
  from an approved contract. *Falsifier:* KEL-143 or F02-T11 shows that a successor
  cannot safely adopt a window it did not create (for example, renderer state that
  cannot be re-bound).
- **D5. Channel id.** F02-T2 appends the window-state channel entry under #508's rule
  (draft PR #613). This spec fixes no number. *Falsifier:* #613's approved rule
  assigns ids by a mechanism other than an appended table entry.

### Public surface

**TypeScript (`@keld/electron`, public; Electron-shaped).** This is the #449 member
subset plus the close members F02-T3 adds. No public signature uses `any`.

```ts
export interface Rectangle { x: number; y: number; width: number; height: number }
export interface Point { x: number; y: number }
export interface Size { width: number; height: number }

/** draw.io's known fields. Any other key is typed `unknown`, accepted, and reported by
 *  one diagnostic (§4.f); it never widens to `any`. */
export interface WebPreferences {
  preload?: string;
  additionalArguments?: readonly string[];
  webviewTag?: boolean;
  contextIsolation?: boolean;
  nodeIntegration?: boolean;
  nodeIntegrationInWorker?: boolean;
  nodeIntegrationInSubFrames?: boolean;
  webSecurity?: boolean;
  allowRunningInsecureContent?: boolean;
  disableBlinkFeatures?: string;
  spellcheck?: boolean;
  readonly [unknownField: string]: unknown;
}
export interface BrowserWindowOptions {
  width?: number;
  height?: number;
  x?: number;
  y?: number;
  backgroundColor?: string;
  icon?: string;
  webPreferences?: WebPreferences;
  readonly [unknownField: string]: unknown;
}

export interface CloseEvent { preventDefault(): void; readonly defaultPrevented: boolean }
export interface BrowserWindowEventMap {
  close: [event: CloseEvent];
  closed: [];
  maximize: [];
  unmaximize: [];
  resize: [];
}
export interface WebContentsEventMap { destroyed: [] }

/** The single host-created content view; F03-T2 owns its wider event vocabulary. */
export interface WebContents {
  isDestroyed(): boolean;
  on<K extends keyof WebContentsEventMap>(event: K, listener: (...args: WebContentsEventMap[K]) => void): this;
  once<K extends keyof WebContentsEventMap>(event: K, listener: (...args: WebContentsEventMap[K]) => void): this;
  off<K extends keyof WebContentsEventMap>(event: K, listener: (...args: WebContentsEventMap[K]) => void): this;
}

export declare class BrowserWindow {
  constructor(options?: BrowserWindowOptions);       // blocking Create (D1)
  readonly id: number;                               // host WindowId
  readonly webContents: WebContents;
  static getAllWindows(): BrowserWindow[];           // creation order
  static getFocusedWindow(): BrowserWindow | null;
  getSize(): number[];
  getPosition(): number[];
  isMaximized(): boolean;
  isFullScreen(): boolean;
  isDestroyed(): boolean;
  maximize(): void;
  setFullScreen(flag: boolean): void;
  close(): void;                                     // non-blocking RequestClose
  destroy(): void;                                   // blocking Destroy
  on<K extends keyof BrowserWindowEventMap>(event: K, listener: (...args: BrowserWindowEventMap[K]) => void): this;
  once<K extends keyof BrowserWindowEventMap>(event: K, listener: (...args: BrowserWindowEventMap[K]) => void): this;
  off<K extends keyof BrowserWindowEventMap>(event: K, listener: (...args: BrowserWindowEventMap[K]) => void): this;
}
```

`getSize()` and `getPosition()` return `number[]`, as Electron's own declarations do,
so migrated code type-checks unchanged. App events added by F02-T2:
`'browser-window-created': [event: { preventDefault(): void }, window: BrowserWindow]`
and `'web-contents-created': [event: { preventDefault(): void }, contents: WebContents]`.
Every member outside this list is F02-T8 or F03.

**keld-wv (`crates/keld-wv/src/engine.rs`, public).** The new variants carry only
`Copy` data (ids, rectangles as integers, flags), so the existing
`#[derive(Debug, Clone, Copy, PartialEq, Eq)]` on both enums (`engine.rs:90-105`) stays.
keld-wv never sees a `WindowId` or `WindowGeneration` type (it has no keld-ipc
dependency): it addresses views by its own `WebviewId`, and a build by an opaque `u64`
token that the registry chooses.

```rust
pub enum AppWindowCommand {
    Quit,
    Fatal,
    /// Build one native window plus webview; answered by `WindowBuilt` or `WindowBuildFailed`.
    CreateWindow { build: u64, width: u32, height: u32, x: Option<i32>, y: Option<i32> },
    SetMaximized { view: WebviewId, maximized: bool },
    SetFullScreen { view: WebviewId, full_screen: bool },
    /// Tear down bridge, then webview, then window; answered by `WindowReleased`.
    DestroyWindow { view: WebviewId },
}
pub enum AppWindowEvent {
    NavigationReady,
    LastWindowClosed,
    /// `snapshot` is read from the built native window on the UI thread, so it holds the
    /// OS-chosen position when `CreateWindow` had `x`/`y` as `None`.
    WindowBuilt { build: u64, view: WebviewId, snapshot: WindowSnapshot },
    /// `stage` is a `Copy` enum (`Window` | `Webview`); the `WvError` text is written to
    /// the host log on the UI thread, so the event stays `Copy`.
    WindowBuildFailed { build: u64, stage: BuildStage },
    WindowCloseRequested { view: WebviewId },
    WindowBounds { view: WebviewId, x: i32, y: i32, width: u32, height: u32 },
    WindowFocus { view: WebviewId, focused: bool },
    WindowMaximized { view: WebviewId, maximized: bool },
    WindowFullScreen { view: WebviewId, full_screen: bool },
    WindowReleased { view: WebviewId },
}
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSnapshot { pub x: i32, pub y: i32, pub width: u32, pub height: u32,
    pub focused: bool, pub maximized: bool, pub full_screen: bool }
```

`WindowSnapshot` is the only source of `Created.state` (criterion 1).

For facade boots the existing `LastWindowClosed` variant is not used. The registry
derives last-window from its own states (T11), because an `Opening` window is not
live and keld-wv cannot see registry states.

### Capabilities and manifest changes (spec 03)

None. No capability name or manifest key is added. The window channel's table entry
carries the host-internal marker (§4.g).

### Wire/protocol changes (spec 02)

One new channel and its payloads (§4.b). No frame, kind, flag or `HELLO` change, so
there is no `PROTOCOL_VERSION` bump. Review gates: wire protocol and public API.

### Platform notes

- macOS: tao 0.35.3's `windowShouldClose:` emits `CloseRequested` and returns `NO`
  (`window_delegate.rs:326-331`, FACT). The veto primitive exists; the change is host
  policy. tao registers no `applicationShouldTerminate:` (FACT, #419 packet), which is
  F01-T3's concern.
- Windows and Linux: window `CALL`s answer the unsupported-platform `ERR`. Backends
  never receive the new commands. F02-T12 owns these platforms.

### Runtime seam

- Before: the macOS UI loop owns close outright and tears down on `CloseRequested`.
  keld-core only relays `NavigationReady`/`LastWindowClosed`
  (`app_session.rs` `coordinate_window_events`).
- After: keld-core's registry owns window identity and close policy. The UI loop
  executes commands and reports facts. OS grants: none new. Crash domain: the host
  process; a Bun crash takes T9. Handle and callback lifetime: native handles stay on
  the UI thread and die at T11. Value, error and order semantics: §4.b and §4.e.
  Configuration is captured when `Create` is admitted.

### Migration unit

- Callers: `@keld/electron` gains `BrowserWindow` and `webContents` handles.
  `@keld/api` gains the mirror and the window adapter.
- Handlers: keld-core `coordinate_window_events` handles the new `AppWindowEvent`
  variants. The primary router writes window frames.
- Generated contracts: the X05-T4 constant for the channel id.
- Persisted state: none.
- Temporary adapter: the legacy teardown arm for renderer-declared boots (owner
  F01-T2; removal condition in §4.c).
- Permanent compat facade: `@keld/electron` `BrowserWindow`.

## 5. Boundaries

- Implement in:
  - `crates/keld-ipc/src/window.rs` (new; wire types and vectors), plus receive-policy
    rows in `crates/keld-ipc/src/receive.rs` (a per-frame channel selection with one new
    window row, not a longer `also_channel`) and
    `crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv` (new rows only);
  - `crates/keld-ipc/src/lifecycle.rs`: the #613 `LifecycleRequest::Subscribe`,
    `LifecycleResponse::{Subscribed, SubscribeRefused}` and the closed `SubscribeRefusal`
    enum, with pinned postcard byte tests; `crates/keld-core/src/app_session.rs`'s
    lifecycle `CALL` arm as the `Subscribe` handler and per-link subscription state;
    `crates/keld-core/src/lifecycle.rs`'s `LifecycleSession` answering
    `SubscribeRefused { reason: NotSubscribable }`;
  - a new keld-core registry module composed by `crates/keld-core/src/app_session.rs`,
    and the `crates/keld-core/src/lib.rs` doc;
  - `crates/keld-wv/src/engine.rs` (the `Copy` `AppWindowCommand`/`AppWindowEvent`
    variants listed under "Public surface", `BuildStage` and `WindowSnapshot`),
    `crates/keld-wv/src/wkwebview/mod.rs` (macOS loop);
  - `packages/@keld/api/src/` (mirror, window adapter), `packages/@keld/electron/src/`
    (`BrowserWindow`, triage);
  - `docs/engineering/keld-error-codes.md`, `docs/engineering/product-status.tsv`,
    `docs/architecture/03-security.md` (when the channel goes live).
- Must not touch: the `WebEngine` trait; frame layout, `PROTOCOL_VERSION`, `HELLO`;
  `keld-guard` evaluation; keld-runtime's `RoleRegistry`; the Windows and Linux
  backends beyond exhaustive-match arms; the workspace `Cargo.toml` (no dependency).
- Adoption (D6): no KEL-139 amendment; AC5 holds for every boot mode. The transfer and
  replay in the keld-core registry, the `Created.pending_close` field and its vector,
  and the facade's replay handling in `@keld/electron` are in this slice's T2.
- Architecture sentences changed in this PR
  (`docs/architecture/05-webview-and-native.md` §3, after the module table): one new
  paragraph, "**Window ownership.**", stating that (1) no keld-native broker owns the
  `window` row, because keld-native depends only on ipc and guard while native windows
  and webviews are keld-wv UI-thread handles; (2) the keld-core window registry owns
  window identity (the host-minted `WindowId` and `WindowGeneration`), window-state
  events and the two-phase close state machine, and drives keld-wv through its UI-loop
  command and event queue; (3) the app-link `window` channel carries them and is
  unreachable from webview principals until KEL-102 per-window grants exist. No other
  architecture sentence changes.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 = F02-T2 (#449). The keld-ipc window types and vectors; the X05-T4 table
  entry; the `lifecycle.rs` `Subscribe` variant, its responses and `SubscribeRefusal`
  with pinned byte tests, the `app_session.rs` handler and per-link subscription
  state, the `LifecycleSession` refusal, and their criterion 20 tests; the keld-core
  registry with T1, T2, T11 and T12, `owner_role_generation` and the admission checks;
  the reserved `KELD-CORE-038` to `042`; `Create` (blocking via #528)
  and the state events; the mirror primitive; getters, setters, `id`, `getAllWindows`
  in creation order, `getFocusedWindow`; `browser-window-created` and
  `web-contents-created` emitted once per window by the constructor after `Create`
  returns (their mutual order is cited from v44.4.5 source in that PR); the retired
  and forged pair errors; the lib.rs and product-status drift fix; the 03 sentence.
  Interim: a native close in a facade boot still tears down at once and writes
  `Destroyed` (no veto yet), so criterion 4 is exercisable. Criteria 1–14, 16–17, 19–20,
  41–44.
- [ ] T2 = F02-T3 (#450). T3–T10; `RequestClose`, `CloseReply`, `Destroy` (blocking);
  facade `'close'`, `close()`, `destroy()`, both tombstones, `'closed'`; common-modes
  delivery; removal of T1's interim teardown; the full transition table with its
  every-pair unit test; `KELD-CORE-043`; the D6 adoption (ownership transfer, the
  `Created` replay with `pending_close`, the recovery gap). Criteria 15, 18, 21–33.
- [ ] T3 = F02-T4 (#455). The triage table. Criteria 36–40.
- F01-T3 (#451) owns criteria 34–35 and the `before-quit` half of 33. They are listed
  here only as consumed contracts.

## 7. Test plan

| Criteria | Test | Kind |
|---|---|---|
| 2–6, 21, 25–27, 30, 31, 33 | pure registry state-machine tests in keld-core with a fake UI port and an injected clock; no AppKit; one test enumerates all 76 (state, input) rows of §4.e | unit |
| 20 | `lifecycle.rs` pinned `Subscribe` bytes; `app_session.rs` link tests for an unsubscribed primary, a subscribed primary and a refused `app-bound` role | unit, integration |
| 8, 10, 11 | the golden-vector file replayed by `cargo test -p keld-ipc` and `bun test`; existing vectors and the KEL-133 corpus unmodified | unit, cross-language |
| 9, 43 | the X05-T4 drift check and table validation | unit |
| 1, 13, 14, 16, 17, 20, 22–24, 32, 36–38 | real-Mac host + Bun fixture (macOS GUI session); the frame counter is read from the host link | integration |
| 15 | existing KEL-96 macOS close and `LastWindowClosed` tests, unchanged | integration |
| 18, 19 | gh527 harness with the window applier; step log `applier*, return, continuation, listener*` | integration (#528) |
| 28 | macOS keld-wv test: run a nested `NSModalPanelRunLoopMode` loop on the UI thread and post a command | integration (macOS GUI) |
| 29 | facade unit test with a throwing listener and a recording link | unit |
| 34, 35 | F01-T3's quit cells | conformance (F01-T3) |
| 39, 40 | facade unit test with a hostile `keld.compat.ts`; `Create` vector | unit |
| 41, 42 | KEL-133 corpus rows for a second role link; bridge admission test | unit |
| 7, 44, 45 | text checks named in the criteria | review check |

Anti-flake: no sleeps. The veto test advances an injected clock. Orders are asserted
from recorded step logs, not timestamps (the oracle found cross-process `hr_ns`
inversions up to 2.35 ms). GUI tests run only on a real macOS desktop session; unit
tests never call `EventLoop::new`. The real-Mac native close stimulus is an
Accessibility (AX) click on the window's close button (D3, §4.i), retried until the
transcript shows the expected record, as the PANEL-P2 oracle did. Electron cells cite the PANEL-P2 transcripts at `1c2e8945`.
Counts, not durations, are the oracle.

## 8. Review gates triggered

- **Wire protocol:** yes. A new app-link channel, payloads and receive-policy rows; no
  `PROTOCOL_VERSION` bump (§4.b).
- **Public API:** yes. `@keld/electron` `BrowserWindow`, `WebContents`,
  `BrowserWindowOptions`, `WebPreferences` and the event maps ("Public surface"); the
  `@keld/api` internal mirror; keld-ipc public wire types and the lifecycle
  `Subscribe` messages; the keld-wv `AppWindowCommand`/`AppWindowEvent` variants and
  `BuildStage` and `WindowSnapshot`; `KELD-CORE-038` to `043`.
- **Permission model:** yes. The host-internal marker and app-process-only admission;
  webview denial kept (§4.g). Listed on the ticket (#531 "Ownership and gates").
- `unsafe`: none in production. Criterion 28's test, and the fallback path if it
  fails, use keld-wv's existing CFRunLoop allowance and need its review. The native
  close stimulus is an AX click: no debug `performClose:` hook and no new `unsafe`
  (D3).
- Dependency: none.

## 9. Perf impact

No performance claim. Criterion 17 counts frames for correctness; it is not a speed
measurement. Window #1 now waits for Bun module evaluation and one blocking `Create`
round trip, so architecture 01 §5 "cold start → first paint" can move for facade boots.
F01-T2 measures that delta under X05-T1's registered metric. Mirror event volume
(`move`/`resize` while parked) counts against gh527's ring bound. Its default is open
there (gh527 §10 Q1).

## 10. Open questions

None. The questions raised in the first draft were decided under the orchestrator's
delegation, and D6 by the owner; all are recorded, with falsifiers, in §4.i.
