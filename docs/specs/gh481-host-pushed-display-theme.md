# Spec: macOS display and appearance facts on the existing window mirror (GH-481)

Status: draft
Linear: GH-481 (tracker under #517; KEL-102 reference) · Owner: gyldlab/keld · Updated: 2026-10-28

## 1. Goal & non-goals

Publish one authoritative, generation-bound **macOS display and appearance snapshot**
through the *existing* GH-531 window-mirror subscription, so the Electron facade's
`screen.getAllDisplays()`, `screen.getPrimaryDisplay()`, `Display.workArea` and
`nativeTheme.shouldUseDarkColors` are **synchronous local reads**. The first
snapshot must be applied before the facade reports public Ready or constructs its
first BrowserWindow. A role not admitted to the window subscription receives no
such fact. Subsequent system changes must update the mirror through the existing
ordered host EVENT path, not by polling on every getter. The target is the pinned
Electron **44.4.5** first-proof draw.io application, not an invented new KELD
platform-wide API guarantee.

Non-goals:
- A second host app link, channel, Worker, router, mirror primitive or Bun-to-Host
  request per getter. No new `app.system` permission literal or native allowlist.
- Redefining GH-531 `WindowRef`, close state, role ownership, restart and
  `Subscribe`/`Subscribed` handling, or altering the already-approved GH-527
  WorkerLink ring and parked-call ordering.
- All remaining `screen` APIs, display-change listeners and `nativeTheme.updated`
  events, `themeSource` setter, OS accessibility appearance, or a universal
  menu/window-style change. GH-558 separately owns those later members.
- Windows/Linux native backends, native display capture permission, renderer-origin
  requests, arbitrary application-Worker subscriptions or system preference writes.
- An Electron percentage, an installed draw.io compatibility pass, or a purported
  native OS test from a TS import/mocked display. Source-only parity is not OS proof.

## 2. Spec refs and current-source facts

- `docs/architecture/03-security.md` §2, `docs/architecture/04-electron-compat.md`
  §3–4 and `docs/architecture/05-webview-and-native.md` §3: app principal,
  host-owned UI handles, platform-native facts and provenance.
- **Approved** `docs/specs/gh531-window-registry-close-state-machine.md`,
  especially §4.d `MirrorSource<Fact>`/`attachMirror`, the *single* authenticated
  window subscription (AC20) and retained window/generation/replay contract.
  Source of truth: `@keld/api` mirror is host-pushed, not a native-provider copy
  in Electron facade. The spec explicitly already says F06-T5 reuses it.
- **Approved technical design** in [GH446 draft PR #657](https://github.com/gyldlab/keld/pull/657),
  exact approval/evidence on [GH445](https://github.com/gyldlab/keld/issues/445#issuecomment-6371519139):
  raw Host Ready alone is insufficient; public `app.whenReady()/isReady()` and
  `ready` wait for **accepted** window Subscribe, full replay and the correlated
  `Subscribed` REPLY. Its source implementation is still **unmerged**. A fake
  ready-before-subscription is an independent negative control.
- `docs/specs/gh527-worker-owned-blocking-call-transport.md` §4.6: one
  `setStateApplier` per channel; host facts before blocking REPLY apply before Bun
  synchronous continuation, but user listeners run after that continuation.
- [GH449 active window registry/consumer stack](https://github.com/gyldlab/keld/issues/449),
  including [draft #1153](https://github.com/gyldlab/keld/pull/1153). **Not on
  main:** there is no live screen/theme mirror or compatible facade today.
- Pinned upstream Electron **v44.4.5** tag commit
  [`694f45852a0f1726cd23bfd379854de489cccb65`](https://github.com/electron/electron/tree/694f45852a0f1726cd23bfd379854de489cccb65),
  exact [`docs/api/screen.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/screen.md),
  [`docs/api/native-theme.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/native-theme.md)
  and [`docs/api/structures/display.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/display.md).
- [GH477 test-only draft #1156](https://github.com/gyldlab/keld/pull/1156)
  already has four GH481 **expected-fail**, macOS-only cells for the two getters,
  `Display.workArea` as DIP Rectangle and dark-mode boolean (current head
  `260974757fe92490d11bbcbf579901a263af32a4`), *not* KELD native
  acceptance. GH-566 is the sole corpus manifest/parser/runner owner.

**Spec relationship:** This draft proposes extending the **same** GH-531 window
EVENT vocabulary with *one new global desktop fact*; GH531 currently lists
per-window event discriminants 0–6 and limits those to their owner generation.
No production code may append discriminants or change the replay grammar until
its exact owner approves the **narrow global-fact amendment**. Merely declaring
GH481 approved without that independent GH531 composition review is not enough.

## 3. Acceptance criteria (binary)

**D1 — one authority and one subscription.** An authenticated *current primary*
Bun role admitted through the **existing** GH-531 window subscription receives
one desktop snapshot. The host sends **zero** desktop/window facts to an
unsubscribed or wrong-role connection, even if HELLO succeeded. No second
permission/channel/Worker exists. A forged frame or old generation is refused
before the mirror can apply it.

**D2 — initial snapshot before public Ready.** Given a facade boot, a valid
authenticated current-primary `Subscribe(c)` is legal **before** the first
native sample and before raw Host Ready. With no sample yet, retain **one
bounded pending Subscribe correlation**: do not commit a subscriber bit,
publish a desktop/window `EVENT`, send `Subscribed(c)` or resolve public Ready.
At the first valid snapshot, whether raw Ready was already delivered or
arrives later, the sole writer enqueues `DesktopSnapshot(e) → Created* →
Subscribed(c)` in that order. Public `app.whenReady()`/`app.isReady()`/`ready`
activate exactly once **after** raw Ready, application of the complete desktop
snapshot and window replay, and processing of that same-correlation accepted
`Subscribed(c)` reply. The first `screen`/theme read then sees actual values
before the first BrowserWindow constructor. Neither Ready-first nor Ready-last
can publish an early Ready or an early lifecycle `ERR` on a healthy sample.

**Finite terminal bounds.** This proposal requires the first native sample
to be definitively valid or unavailable by the GH446 macOS UI Init milestone,
bounded by its existing **15-second** startup deadline. If Init completes
with no sample or a malformed first sample, settle an already-pending
`Subscribe(c)` or any eligible later same-generation Subscribe with exactly
one same-correlation lifecycle `ERR(CallError
DesktopSnapshotUnavailable)`, zero replay/subscriber state, sticky rejected
`whenReady()` waiters, `isReady() === false` and no `ready`. If Init itself
misses that deadline, use GH446 `KELD-CORE-037` startup/role-link terminal
failure, not an invented late `ERR` after death. Role death or link loss uses
the existing GH527 typed transport failure and fences the pending correlation.
The caller's finite Subscribe reply deadline must cover the permitted pending
Init window **plus** the bounded replay/transport completion; the current 5s
per-frame I/O deadline is not such a caller deadline. The GH446 owner must
bind the clock anchor and exact caller bound before implementation (§4.3,
§10); no clock change is authorized here. Any failure remains terminal for
the current generation; only a newly authenticated successor may retry.
Renderer-declared boots retain their existing behavior.

**D3 — real primary and all-displays shape.** A fresh true native Mac
snapshot produces one primary Display and a synchronously returned array of
all available displays, each with a unique id, finite DIP `bounds`/
`workArea` Rectangle (`x`, `y`, `width`, `height`) and positive finite
scale factor. The primary appears in the list exactly once. A headless/no
observable-display failure is typed, **not** a synthetic all-zeros display.

**D4 — actual OS work areas.** On the **same physical Mac**, same layout and
scale configuration, each returned `Display.workArea` and primary
`workArea` agrees with a pinned native Electron 44.4.5 run (including
multi-display offsets and menu-bar/dock usable area). AppKit's native
coordinate-origin conventions are explicitly converted to Electron's
logical top-left DIP representation; raw AppKit `visibleFrame` is not assumed
identical. Require a second non-primary/negative-offset configuration before
a cross-monitor claim. Tests preserve a genuine toolbar/dock inset.

**D5 — actual effective appearance.** On the same physical Mac, pinned
Electron and KELD see the same boolean `shouldUseDarkColors` under **both**
light and dark system effective appearance, with an actual native settings
transition. It is a synchronous boolean, never an async Promise, a hardcoded
false or a string. This first slice does not install an Electron `updated`
listener or change native appearance preferences.

**D6 — zero IPC on reads.** After an admitted initialized snapshot, at least
**1,000** consecutive calls *each* to `getAllDisplays`,
`getPrimaryDisplay`, workArea access and dark mode getter cause **zero
new app-link frames**. The counters are on the actual WorkerLink and Host
sides; a mocked function call count or a hidden transport spy does not prove
absence of frames. Reads use an already-applied local mirror copy only.

**D7 — ordered updates and parked callers.** When the host observes a changed
display or effective appearance, it emits one snapshot for the new epoch;
the Bun mirror applies it in existing GH-527 issue order. When a separate
blocking dialog CALL is parked and a desktop fact precedes its REPLY, the
synchronous continuation sees the updated fact **before returning** without
running user JS during the park. A fact arriving after REPLY is not applied
prematurely. Host-owned subscription replay and live desktop observer
publication have **one writer/linearization point** (§4.3): observer update
`e+1` committed before Subscribe is captured by initial replay; committed
after Subscribe follows its `Subscribed` barrier. Neither e+1-before-e
regression nor lost e+1 is allowed, and a legitimate observer race must
**not** cause a terminal stale-epoch error. `nativeTheme.updated` remains
unimplemented.

**D8 — lifetime and recovery.** Native monitor observer subscriptions and
Host-owned cached facts survive a Bun-role restart but retire on Host
shutdown. A successor of the same declared primary role gets the *current*
authoritative desktop snapshot and window replay at its first eligible
Subscribe; a callback from the old role cannot answer or target the new one.
A malformed/replayed contradictory epoch fails closed; a legitimate
subscribe-vs-observer race is serialized and is **not** a desynchronization.
No silent reset to initial window or primary Display identity. No numeric OS display id is a KELD
role or capability identity.

**D9 — bounded representation.** The first-proof proposal admits at most
**32 displays**, one primary-id match, finite integer DIP dimensions and
finite positive scale factor, and limits encoded desktop snapshot payload
to **32 KiB** before it enters the existing app-link ring. Count 0,
max+1, duplicate ids, invalid/nonfinite scale, integer overflow,
truncation, malformed tag and **contradictory or regressing on-wire epochs
that cannot arise from the single authoritative writer** are rejected with
typed errors. Duplicate same-epoch/same-value delivery is idempotent; a
same-epoch different-value record is an error. Legitimate concurrent Mac
observer updates are serialized, not mislabeled stale. The *existing*
GH-527 channel/credit/aggregate memory bounds still apply;
no new queue or per-getter allocation budget is added. These proposed
limits are approval-dependent, not currently shipped.

**D10 — default-deny role admission, not new resource authority.** `D1`'s
host-internal window-fact subscription authorizes passive desktop/theme
facts only for its current primary role. There is **no** new
`app.system` literal, FS/media grant, app-initiated request, webview
principal exception or session-wide broadcast. A nonadmitted role sees
neither event nor snapshot; a forged renderer frame fails before Bun.

**D11 — explicit unsupported boundaries.** Unsupported themeSource writes,
`nativeTheme.updated`, screen cursor/event/matching APIs and Windows/Linux
native execution do not silently return fabricated values or a KELD
compatibility pass. A pre-Ready getter either reads a fact legitimately
already applied for that same admitted role or throws a stable typed
not-ready error; it must not block on a synchronous host CALL.

**D12 — conformance flip only alongside native proof.** The four GH477
expected-fail cells remain `fail` until GH481's actual Mac native paired
results, mirror/transport 0-frame evidence and generation/Ready fences pass.
Then the implementing GH481 PR inverts its **own** mapped tests and flips
each owning cell to `pass` in the *same* diff. It preserves the frozen
v44.3.0 lifecycle manifest, other GH477 ticket cells, GH566 single parser,
and the full source/hosted review gates.

## 4. Design

### 4.1 First principles: one host snapshot and one mirror source

**OS owner:** the existing macOS `keld-wv`/Host UI-loop owner samples
actual `NSScreen` display geometry and effective appearance through
pinned objc2 APIs (or an explicitly justified already-pinned OS primitive).
It records one owned, versioned **DesktopSnapshot** in Host state, not in
an Electron facade module and not in a keld-native second window owner.
Any native notification/callback that delivers a changed screen/appearance
enters the Host's *existing* UI-to-Core command/event path. Host serializes
updates with its current window fact writer. It never trusts Bun to provide
an OS frame or principal.

**Subscription owner:** GH-531's primary role window subscription remains
the **single** admitted stream. The proposed appended
`WindowEvent::DesktopSnapshot` carries *global* desktop state but is sent
only on that same already-subscribed, authenticated current primary link.
It is not a per-window response; the existing per-window owner rule is
**amended narrowly** to authorize this source-owned global fact. A role
without current accepted Subscribe receives no fact, and every other
existing event/REPLY ordering guarantee is preserved.

**Read owner:** the GH-531 `MirrorSource<Fact>`/`attachMirror` generic applier
is reused *within one channel*. Because GH-527 admits at most one
`setStateApplier` per channel, window facts **and** desktop facts must use
one dispatch/decode and apply step, then the public Electron facade reads
only the applied desktop substate. Implementing a second applier on the
same channel would fail its approved receive-table contract.

### 4.2 Proposed additive wire shape — not an allocation or approval

The following is a **sketch** of an *append-only* extension to the existing
window EVENT enum, not a checked-in IDL or permission to modify GH-531:

```rust
struct DesktopFact {
    id: u32,                   // physical display identity for native equivalence only
    bounds: Rect,              // GH531 logical points, top-left
    work_area: Rect,           // DIP, not raw NSScreen bottom-left
    scale_factor: f64,         // finite, > 0
}
struct DesktopSnapshot {
    epoch: u64,                // Host-owned ordered observation generation
    primary_id: u32,
    displays: Vec<DesktopFact>,// 1..=32, 32 KiB total encoded budget
    should_use_dark_colors: bool,
}
// APPEND to GH531 WindowEvent, after existing 0..6; discriminant is
// chosen/pinned by the GH531 source owner, never guessed in this draft.
// WindowEvent::DesktopSnapshot { snapshot: DesktopSnapshot }
```

There is **no new channel number**, no new role ID, no Host WINDOW grant and
no change to GH-527 framing. GH-531 already owns the current codec and
wire-golden-vector source; its owner appends the variant *only after both*
GH481 approval and the amendment approval, re-running the hostile-frame,
drift, unknown-discriminant and complete record-order tests. A too-old host
that cannot send a required desktop snapshot cannot report desktop-ready;
it fails visibly instead of publishing empty data.

**Memory ownership:** one bounded Host latest-snapshot copy and one
bounded per-admitted-role mirror snapshot. A snapshot update replaces the
last value; it does not retain an unbounded history or every screen change
while Bun is parked. GH527 queued records remain bounded by the existing
credit/retention rules; over-budget emission is a typed terminal failure,
not unbounded buffering or an extra queue.

### 4.3 Exact first-read and recovery order

The selected **proposal**, dependent on the accepted GH446/GH531 readiness
composition, has **one Core/Host writer actor** that serializes all window
EVENTs, native observer update reports, subscription admission, outgoing
reply order and generation retirement. The macOS UI observer only posts a
bounded source fact to this writer; it cannot write frames or mutate the
subscriber registry independently. The actor's Subscribe handling is one
non-awaiting, non-reentrant turn. If the native sample is not ready, the
actor retains **only one pending Subscribe(c)** for the authenticated current
primary generation; this is a correlation reservation, **not** successful
subscription admission. Its replay **linearization point** is the validated
latest `DesktopSnapshot(e)` capture **under that writer**, before the first
subscription bit or outgoing replay frame. The complete native snapshot
(bounds/primary/epoch/shape) must validate before replay is attempted.

```text
Authenticated current-primary HELLO → Subscribe(window,c); raw Ready may
  arrive before Subscribe, before sample or after the replay batch
Core writer checks existing channel/role/generation admission
  REFUSED: existing SubscribeRefused reason; no pending c or desktop EVENT
  ALLOWED + validated sample already retained: capture latest epoch e
  ALLOWED + no validated sample: reserve at most one pending Subscribe(c)
    no subscribed bit, desktop/window EVENT, Subscribed(c) or public Ready
Mac UI Init reports first valid native desktop state (within 15-second bound)
  Core writer validates and retains latest e, resumes pending c if present
  For validated sample + eligible c: one uninterruptible ordered enqueue batch
    DesktopSnapshot(e) EVENT on existing window channel
    all surviving-window Created replay EVENTs
    correlated Subscribed(c) REPLY on lifecycle
    commit eligibility and last-sent-epoch=e only after queued barrier
  Init complete, definitive no sample or malformed sample + pending c:
    exactly one ERR(c, CallError DesktopSnapshotUnavailable); zero replay/bits
  Eligible Subscribe(c) after that terminal Init outcome: same-correlation
    ERR(c, DesktopSnapshotUnavailable) immediately; no pending or replay
  Init 15-second milestone missed: GH446 KELD-CORE-037 terminal startup
  Role death/link loss: GH527 typed terminal, fence pending c, no late ERR
  Duplicate Subscribe while c pending: reject under existing lifecycle policy
    without replacing c, reserving a second pending call or replaying state
Next queued observer update e+1 → enqueue DesktopSnapshot(e+1) AFTER Subscribed
Existing GH527 applier applies snapshot + window replay, no user listeners
Facade public Ready opens only after raw Ready + desktop/window applied +
  same-correlation successful Subscribed accepted; exactly once
First BrowserWindow constructor + sync screen/nativeTheme reads are usable
```

**Only definitive Init-time unavailability is a desktop Subscribe failure.**
GH508's `SubscribeRefusal` remains a closed enum for channel/role checks,
without any invented `DesktopUnavailable` reason. The existing GH527
`lifecycleReplyWaiter` admits an authenticated lifecycle `ERR` encoded as a
`CallError` for the **same correlation `c`**, so no new frame/channel is
needed. The error-code owner must allocate a stable `KELD-CORE-*`
**DesktopSnapshotUnavailable** code/fix before implementation; this draft
reserves no number. An absent *pre-Init* snapshot is an ordinary pending
state and cannot fail a healthy early Subscribe. Completed Init with a
definitively missing or malformed first sample instead emits one typed ERR,
zero replay records and no successful `Subscribed`; the same-generation
pending correlation is consumed once. The failed-Init state is sticky even
if no Subscribe was pending at the moment of failure: a later eligible
Subscribe receives its own single correlated ERR, never a new pending c
or a late unvalidated snapshot. If any replay frame is partially
written or the link dies, GH527 transport teardown is terminal: no replay
retry or fake successful `Subscribed` may follow.

**Deadline ownership:** GH446's existing 15-second `INITIAL_MILESTONE_DEADLINE`
is measured from macOS UI-loop entry through successful Init, not from an
arbitrary pre-Init Subscribe. That clock is the outer native-Init outcome
bound for pending `c`, **conditional on a bounded path to UI-loop entry**.
The GH446 caller/adapter owner must ensure its *overall* lifecycle Subscribe
reply deadline outlives `(Init deadline - eligible Subscribe time) + bounded
writer/replay/transport time` for every admitted early Subscribe, including
the pre-UI-loop interval; a 5-second per-frame APP_LINK_IO_DEADLINE does
not meet this. Until that start anchor and finite reply bound are approved,
this remains a non-implementable proposal, not permission to extend any
existing clock. If native sampling can occur only *after* completed Init,
owners must specify a separate finite observation bound and caller clock
before replacing this proposal's Init-time terminal rule. No indefinite
pending correlation is permitted.

The Bun singleton consumes a definitive Init-time `ERR` as rejection of
the *current Subscribe* attempt. It rejects **all** public `whenReady()`
waiters with
the same typed cause, retains a sticky public-ready-failed state for this
role generation, reports `app.isReady() === false`, emits **no** `ready`,
and refuses subsequent getters/constructors with the typed cause.
No same-generation auto retry can silently create a second subscription;
the next authenticated successor role may attempt a fresh admission.
This is public-facade terminal readiness, not permission to detach the
Host native owner or wait unboundedly for a user-visible display.

**Concurrent publication is linearized.** A native OS change is placed
on the same Core writer queue. If it commits *before* Subscribe's capture,
`e+1` becomes the initial replay snapshot; no earlier `e` is sent. If it
commits *after* capture, the writer enqueues replay `e`, full window
replay and `Subscribed(c)` **as one ordered batch**, then emits live
epoch `e+1` to the newly eligible subscriber. No separate observer
thread may bypass this writer or interleave a live event in the batch.
Subscriber state stores a last-sent epoch; same-epoch/same-payload
notifications are idempotently coalesced, and a later epoch is appended
in order. `e+1` may become visible after the Subscribe reply, consistent
with GH527's reply-time staleness rule, but is never lost or followed by
an older snapshot. Host role retirement fences any not-yet-written
batch and carries no callback to a successor.

A restart uses the Core writer's latest validated epoch and follows the
same fence, while the old role/generation receives no new frame. No
observer callback and no continuation in another thread may forge a
`Subscribed` reply or reset the public facade Ready state.

**Dependency edge to GH446:** its approved readiness gate already waits
for raw Ready + accepted Window Subscribe/full replay. GH481 proposes that
the gate also require the **first applied** `DesktopSnapshot`, complete
Created replay and processed same-correlation `Subscribed(c)`, in either
Ready-first or Ready-last order. It must not create a second App Ready
source, resolve a waiter for a merely *pending* c, or set `isReady()`
from a fabricated default. GH446/GH531 owners must explicitly approve
this extension and the finite caller-clock coordination before adoption.

### 4.4 Actual display and appearance conversion

The native provider's observed display IDs, virtual desktop bounds, usable
frames, scale and effective appearance are **platform inputs**, not authority.
Electron's `Display.workArea` is a DIP Rectangle. AppKit's native screen
coordinate system is not assumed identical. One conversion owner in the
existing UI backend takes the complete screen set and a selected primary
anchor, converts to GH531 logical coordinates, then checks integer/range
bounds **before** publishing. It must handle multi-screen negative offsets,
nonuniform scaling, Dock/menu exclusions and floating-point rounding.

**DO NOT** select a formula from first-monitor-only math or assume
`NSScreen.visibleFrame == Electron workArea`: the owning implementation must
first retain paired Electron 44.4.5 and KELD OS observations on the same Mac
and configuration, with one negative-mutant conversion (raw bottom-left
unchanged) proven to fail. If Apple exposes multiple plausible effective
appearance sources, choose by actual foreground AppKit/renderer behavior,
not by a convenient process preference. Dark-mode validation runs in light
and dark mode and against a deliberate hardcoded-false mutation.

### 4.5 Permission and crash boundaries

The **host-owned mirror grant** is the authenticated current declared
primary RoleInstance's successful GH531 Subscribe. Sending passive desktop
facts introduces **no** FS, location, camera, power, or system API authority
to Bun; it is data, not a capability. A browser renderer, app-bound role,
retired generation or unauthenticated link cannot call or subscribe to
the window channel to obtain these facts.

GH527 owns link loss and replay ordering. When a native observer stops or
Host dies, the mirror cannot keep claiming fresh data. The retired role's
local snapshot is terminal, not proof of current authority. A successor
restarts only with the existing GH531 enrollment; callbacks are Host-owned
and quiesced as part of the accepted Host teardown. No PID, display ID or
WindowId is used as replacement authorization.

### 4.6 Reuse and alternatives rejected

| Alternative | Disposition and falsifier |
|---|---|
| Call Host on every synchronous getter | **Reject.** It parks Bun and breaks 1,000 reads/zero-frames; adds unnecessary latency and another authority edge |
| Open a new channel or Worker for desktop facts | **Reject.** GH531 already has the admitted mirror stream and record ordering; a second link forks the source of truth |
| Publish via the lifecycle Ready channel | **Reject.** Changes the distinct lifecycle receive policy and readiness authority, contrary to GH531 §4 |
| Copy facts into `keld-native` under its planned `screen` module | **Reject as a second owner.** Native UI/thread/display lifetime stays with keld-wv/Host; a pure data conversion helper is permitted only with explicit ownership and no new window handles |
| Hardcode primary Display or dark mode from environment | **Reject.** Physical multi-display/appearance and negative controls immediately refute a placeholder |
| Add an unguarded shared `app.system.screen` authority | **Reject.** GH481 delivers passive facts only to already-admitted primary; no new manifest entry is required |
| Extend existing `WindowEvent` with a global snapshot | **Propose, gated.** One event applier/subscription reused, but needs GH531 owner's explicit approval of the global-fact and ordered replay amendment |

No performance/safety equivalence is inferred from mock state tests.

## 5. Boundaries

When (and only when) approved:
- `keld-wv` macOS UI backend and existing `AppWindowEvent`/UI-thread
  observation owner: bounded real OS snapshots and controlled teardown;
  no raw AppKit objects leave the UI owner.
- `keld-core` GH531 window/subscription producer and current primary role
  writer: authoritatively serializes the desktop fact before the accepted
  `Subscribed` REPLY and fences old generations.
- `keld-ipc` **GH531-owned** window event enum/codec/golden vectors:
  one append-only EVENT variant after explicit amendment; not a new ID.
- `packages/@keld/api` **GH531-owned** existing mirror primitive and
  authenticated role adapter: one decode/applier and one latest snapshot.
- `packages/@keld/electron/src/screen.ts`, `native-theme.ts` and `index.ts`:
  synchronous `screen` and `nativeTheme` read-only facade, with no Electron
  runtime dependency.
- GH477's existing `electron-native-v1` mapped tests must invert their four
  corresponding cells in the implementation change, with real Mac proof.

**Must not edit** any of these under *this spec-only PR*: all production
Rust/TS, GH449/#1150/#1153 source worktrees or live GH446 readiness code,
GH527 transport, fixed GH531 spec without approved amendment, generated
channel table/allocations, KELD_KEL143_* handshakes, Host20/Guardian10/IPC
clocks, root Cargo/workflow/input policy or Windows/Linux production.
Any shared file requires the winning incumbent owner's claim plus
independent exact-diff review and canonical generated-input rebinding.

## 6. Tasks (ordered, each bounded to a PR)

- [ ] T1. Independent GH481 architecture/security review of the exact
      public-read/mirror design; approve this spec and the narrow GH531
      **global desktop fact / before-Subscribed** amendment (not a unilateral
      change to GH531).
- [ ] T2. Reuse the existing GH477 expected-fail pinned 44.4.5 conformance
      cases (no new parser or duplicate source). Preserve other cells.
- [ ] T3. Native macOS display/appearance observer and conversion in the
      existing UI owner with exact retained bounds, light/dark, negative
      offset, changing displays and physical callback teardown tests.
- [ ] T4. GH531 owner appends one wire event variant and extends current
      Subscribe replay/ack envelope with a **first desktop snapshot**, using
      one host-controlled epoch and bounded payload; hostile frame/failure
      and old-code golden-vector tests.
- [ ] T5. GH531 singleton mirror consumes the new fact in **one** state
      applier, current primary/admitted role only, before its public Ready
      barrier; recovery/reordered Ready and deferred user listeners tested.
- [ ] T6. Thin Electron `screen`/`nativeTheme` synchronous getters,
      zero-frames/read and no electron runtime dependency; atomic current
      snapshot, explicit pre-Ready/unsupported behavior.
- [ ] T7. Real paired Electron/KELD Mac evidence for primary and
      non-primary monitor, workArea DIP and light/dark; verify exact
      event/reader/role lifecycle controls, flip only owned four GH477
      cells, complete full required CI and independent review.

T3 can be developed as an isolated native observation test after T1;
T4–T5 share **one** GH449/GH531 owner and cannot race its active branch.
GH446 final implementation integration is a dependent review, not a
license for a second Ready/subscribe policy.

## 7. Test plan (failing-first and independent negative controls)

| AC | Positive observable and owner | One independently refutable mutation |
|---|---|---|
| D1 | Host authenticated primary/Subscribed → desktop EVENT; app-bound/unsubscribed receives none | Broadcast same frame to all authenticated links: nonprimary sees fact, test fails |
| D2 | Ready-first `raw Ready → Subscribe(7) → valid native Init snapshot(1)` and Ready-last `Subscribe(7) → valid native Init snapshot(1) → raw Ready`: one bounded pending c, no early output; exactly `DesktopSnapshot(1) → Created* → Subscribed(7)` then one public Ready after application and acceptance. Controls: sample-first orders, no sample or malformed first sample at completed Init (before or after Subscribe) → one same-correlation typed ERR/zero replay/sticky rejected Ready; 15-second missed Init → GH446 `KELD-CORE-037`; role death → GH527 terminal; unauthorized/second Subscribe cannot consume the pending slot | Mutate pending admission to the original **immediate ERR on pre-Init missing sample**: both otherwise healthy Ready-first and Ready-last orders fail; removing terminal settlement hangs `whenReady`, admitting a second c or partial replay creates ghost subscription |
| D3 | Real Mac Display array/primary id, finite DIP rectangles/scales, one-primary set | Duplicate id/nonfinite scale/zero displays accepted: exact boundary fails |
| D4 | Paired pinned Electron 44.4.5 physical primary and negative-offset secondary workArea | Leave AppKit bottom-left y-coordinate raw; comparison fails |
| D5 | Paired light + dark actual Mac; `shouldUseDarkColors` boolean matches both | Hardcode `false`; dark run fails |
| D6 | Real WorkerLink/Host count before and after 1,000 getters of each API | Replace getter with per-read CALL; actual frame count increases |
| D7 | Force two deterministic writer-queue interleavings (observer e+1 before Subscribe capture and immediately after). Verify initial e+1 OR replay e→Subscribed→live e+1 in order; parked GH527 reply receives applied fact before listener | Let native observer write outside Core writer or enqueue e+1 before replay e: monotonic order/Ready/parked listener tests fail |
| D8 | Bun crash→same app's successor current snapshot+survivor windows; duplicate same-epoch snapshot idempotent, contradictory same epoch refused; retired link fenced | Apply old epoch to successor or classify legitimate e+1 Subscribe race as a terminal stale error: tests fail |
| D9 | 1..32 display count, 32KiB bytes, malformed/truncated, epoch exhaustion, invalid scale | Accept 33rd record or no primary: typed boundary test fails |
| D10 | Absent/forged/app-bound role receives zero frames, no new permission | Add `screen` to public unguarded role receives: refusal test fails |
| D11 | Pre-Ready typed refusal and unsupported API typed errors, no synthetic values | Return false/empty array as an unqualified default: observer test fails |
| D12 | Real native proof and GH566 4-cell pending→pass flip in one implementation diff | Flip based on a TS mock/without platform evidence: corpus + product-acceptance gate fails |

Actual Mac tests require an environment with native display access, two
physical layout configurations, and both light/dark modes; a headless
CoreGraphics/TCC denial is **an environment failure**, never evidence of
an empty display or correct conversion. All OS fixtures retain the
Electron binary/source pin, KELD commit, monitor identity/layout,
appearance epoch, actual raw/native and normalized rectangle values,
requested/observed event order and false/positive controls. Use actual
OS/UI callbacks and compare exact state after an observed condition;
never sleep to synchronize. Explicitly record unavailable physical
configurations as **unknown** rather than silently weakening tests.

## 8. Review gates triggered

- **Public API:** yes, new `screen` getters and `nativeTheme` read-only
  property; exact return type/shape and pre-Ready behavior.
- **Wire protocol:** yes, an **append-only** GH531 WindowEvent discriminant,
  new typed snapshot and strict codec. Frame protocol version and existing
  discriminants stay unchanged; golden-vector and hostile-frame reviews.
- **Permission model:** yes, explicit amendment to the existing
  host-internal window mirror admission guarantee; no new `app.system`
  grant, and nonprimary denies all facts.
- **Unsafe:** none authorized by this spec. Any native ObjC/FFI operation
  requiring new production unsafe must be separately permitted by the
  exact nearest crate AGENTS.md and independently reviewed first.
- **Dependency:** no new package is proposed; reuse pinned OS-native
  APIs. Any new feature/dependency discovered requires that owner’s
  exact change/review gate.
- **Real OS:** mandatory physical macOS comparisons for both appearance
  modes, primary/multi-display geometry, AppKit origin/scale, native
  observer teardown and paired Electron version evidence. A stub is not
  OS acceptance.

## 9. Perf impact

No performance gain is claimed before measurement. Target **zero kipc
frames per synchronous read** is an externally verified correctness
condition. The cold native observer, one bounded snapshot serialization
and host subscription replay may affect Startup and retained memory,
so report exact frame counts/bytes, snapshot capacity, peak bounded
copy budget, allocation count, first public-Ready latency and Host
shutdown cleanup in the real Mac fixture. Compare with architecture 01
§5; a >5% regression needs the existing written waiver process. No
latency workaround may widen Host/Guardian/IPC deadlines or GH527 ring
budgets.

## 10. Open questions (blocking for Status: approved)

1. **GH531 wire owner:** explicitly approve the global
   `WindowEvent::DesktopSnapshot` appended variant and initial snapshot
   before window-entity replay/`Subscribed`; currently GH531 describes
   per-window event ownership. Its owner must sign the narrow exception
   and test the old discriminants and subscribed-only recipient rule.
2. **GH446 facade barrier composition:** approve/reject B's **one bounded
   pending authenticated Subscribe(c)** before first native sample, and add
   the first **applied** desktop snapshot to its existing `raw Ready +
   accepted Subscribe + full replay + same-correlation Subscribed` gate.
   Explicitly bind (a) first sample finalized no later than native Init,
   (b) the bounded path to UI-loop entry for an early Subscribe and a finite
   caller reply deadline covering the entire 15-second Init window plus
   replay/transport, and (c) typed terminal settlement for no sample,
   malformed sample, Init timeout and role death. If sampling may legitimately
   follow Init, instead decide the *additional* finite observation deadline.
   The GH531 owner must separately approve this pending/ordering amendment;
   no second Ready source or delayed-entry assumption is permitted.
3. **Snapshot limits, transaction and epoch:** approve/revise proposed max
   **32** displays, max **32 KiB** encoded bytes, monotonic Core-writer
   epoch and last-sent deduplication. Confirm one atomic Subscribe capture/
   replay/Subscribed batch and the two deterministic interleavings; a
   legitimate OS update must not become a stale-epoch terminal error.
   These values/policies are **not authorized** by this draft alone.
4. **macOS coordinate/effective-appearance selection:** choose the exact
   pinned OS display/appearance API and conversion only after a same-
   machine Electron 44.4.5 paired native experiment; raw NSScreen
   coordinates and a convenience appearance flag do not prove parity.
5. **Pre-Ready/terminal getter and native-snapshot failure:** obtain the
   GH531/GH446 and error-registry owners' approval for *definitive Init-time*
   correlated lifecycle CallError ERR `DesktopSnapshotUnavailable` (no numeric
   code allocated), atomic zero-replay settlement, sticky public-ready
   failure and typed unavailable getters. An early healthy Subscribe is
   **pending**, not erroneous; absent/malformed final sample is terminal,
   never an empty headless display or light-mode false. Waiters must settle.

This spec is **DRAFT** and approval is a separate tracked decision. It
neither merges/edits the active GH449/GH446 branches, implements any
native module, qualifies real macOS product behavior, closes #481 nor
flips the four pending GH477 conformance cells by itself.
