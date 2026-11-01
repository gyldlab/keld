# Spec: macOS display and appearance facts on the existing window mirror (GH-481)

Status: bounded macOS development S1 contract approved; full GH481 remains draft
Linear: GH-481 (tracker under #517; KEL-102 reference) · Owner: gyldlab/keld · Updated: 2026-11-01

## 1. Goal & non-goals

**Minimum implementation S1 (contract approved):** first capture at genuine
native Init, before any native window exists; the existing mirror carries those
facts before public Ready. Implement only the three getters reached at pinned
draw.io `src/main/electron.js:717,740–741`. Reuse the same owners below.
S1 is Init-sampled data with explicit unavailability, not a construction-time
freshness guarantee. Existing per-window Tao events do not prove global screen,
work-area or pre-window theme observation. Full D1–D12 qualification, including
global observation/recovery and native comparisons, remains required before
closing GH481 or flipping its corpus cells. The bounded S1 decisions are approved
in [the initial owner decision](https://github.com/gyldlab/keld/issues/481#issuecomment-6556306946)
and [the platform/watchdog follow-up](https://github.com/gyldlab/keld/issues/481#issuecomment-6557264649).
These decisions do not approve the remaining global-observation implementation,
claim both physical appearance modes or secondary monitors, or close D1–D12.
S1 also requires the actual windowless desktop variant to survive both existing
window-only consumers and their dispatch path; a codec-only pass is insufficient.

**S1 platform and launch qualification (approved follow-up):** the
first-window implementation is enabled for an authenticated **macOS development
facade** whose existing Host-selected `nativeFacadeBootSelected` is true and
whose canonical preloaded transport retained nonempty, frozen
`hostLaunchFacts`. The current `DevelopmentLaunchFacts::from_validated_development`
producer belongs to the macOS Host and emits this non-authority record only
with a validated development lease; Windows/Linux do not emit that record.
This is a bounded *selection fact*, never a grant or a second permission.
The application does not choose it through a new environment variable, mutable
global, feature probe or a guessed `screen` value. Windows/Linux facade boots
keep their existing `raw Ready + window replay + Subscribed` public barrier,
and their unimplemented `screen`/theme getters retain the GH481
`KELD-COMPAT-013` refusal. macOS installed/lease-free boots are not
qualified by this S1 selector; extending to installed or other OS paths
requires a separately approved Host-owned selection and real OS proof.
When this development S1 is selected, missing `WindowSource` fails the
public barrier with its existing 045 cause; missing/invalidated native
desktop facts fail with 049. A valid snapshot is required at Ready commit.

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
At the first valid snapshot, the sole writer enqueues `DesktopSnapshot(e) →
Created* → Subscribed(c)` in that order. On the actual macOS S1 path, the
UI samples at genuine `NewEvents(Init)` **before** enqueuing `HostInitialized`;
Core therefore cannot write raw Host Ready before it has observed this sample
on the same FIFO. A pre-Init Subscribe may finish replay before raw Ready; a
post-Init Subscribe may follow raw Ready. Public `app.whenReady()`/`app.isReady()`/`ready`
activate exactly once **after** raw Ready, application of the complete desktop
snapshot and window replay, and processing of that same-correlation accepted
`Subscribed(c)` reply. The Ready commit must still see a valid current mirror
snapshot, not merely a previously applied one invalidated later in that drain.
The first reads return the actual captured facts synchronously; a later applied
invalidation can make them refuse. Neither Ready-first nor Ready-last publishes
early Ready or an immediate pre-Init ERR while its existing caller deadline is
live. The artificial order `raw Ready → Subscribe → native Init sample` is a
**synthetic adversarial-order test**, never a positive native macOS trace;
it cannot justify moving the real Init observation or publishing Ready early.
S1 does not promise success after that caller deadline expires.

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
The existing adapter also passes `APP_LINK_IO_DEADLINE_MS = 5_000` to the
Subscribe call itself. Keep that exact caller deadline and its current call-start
anchor; it is not extended to cover the full native Init watchdog. If it expires
first, its typed transport failure rejects public Ready and the existing close/
retirement path fences the pending correlation. Late Init or Subscribed cannot
revive that role. The 15-second watchdog remains a separate outer native-startup
bound. This deliberately permits failed startup when five seconds of Subscribe
waiting expires before an otherwise healthy Init; it does not mean five seconds
of total Init time. Root must approve this narrower availability decision (§10).
Only a newly authenticated successor may retry.
Renderer-declared boots retain their existing behavior.
At genuine Init the macOS owner posts exactly one native `DesktopObserved`
before `HostInitialized` on the same FIFO. The timestamp used to adjudicate
the 15-second native-init bound is taken **after** that sample has completed,
before either event is enqueued; timestamping before native capture would
accept a sample that itself overran the deadline. Core may exempt only the
first unconsumed desktop EVENT from its ordinary late-non-Init receive rule,
so it can still examine an on-time `HostInitialized` after coordinator
descheduling. A second pre-Init desktop EVENT fails immediately; an absent
`HostInitialized` expires at the original clock; a late timestamp remains
expired. No repeating desktop stream can postpone this bound.

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

**D9 — bounded representation.** A populated snapshot admits **1–32 displays**,
unique nonzero native u32 IDs, one primary-id match, signed i32 coordinates,
positive u32 DIP dimensions and finite positive scale. The **32 KiB** limit
counts the complete encoded WindowEvent payload, excluding its KIPC header,
before it enters the existing app-link ring. Its checked limit must be applied
in the Core producer and in incoming mirror decoding; it is independent of the
ordinary KIPC 16 MiB frame cap. A null snapshot carries no display
record and invalidates only desktop availability. In a populated snapshot, count 0,
max+1, duplicate ids, invalid/nonfinite scale, integer overflow,
truncation, malformed tag and **contradictory or regressing on-wire epochs
that cannot arise from the single authoritative writer** are rejected with
typed errors. Duplicate same-epoch/same-value delivery is idempotent; a
same-epoch different-value record is an error. Epoch 0 is reserved; the Core
writer increments checked u64 epochs for each changed Some/None value and never
wraps; changing the sample after epoch `u64::MAX` is a typed terminal refusal
before a new publication. None advances the same watermark, so an older Some
cannot resurrect data.
Legitimate concurrent Mac observer updates are serialized, not mislabeled stale. The *existing*
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
struct DesktopFacts {
    primary_id: u32,
    displays: Vec<DesktopFact>,// 1..=32; validated complete payload <= 32 KiB
    should_use_dark_colors: bool,
}
// Candidate WindowEvent tag 7, pending GH531 owner approval:
// DesktopSnapshot { epoch: u64, snapshot: Option<DesktopFacts> }
// Some = complete native capture; None = loss of usable desktop facts.
// The epoch is on the envelope so invalidation advances the same watermark.
```

The current WindowEvent source at `076a09ce` confirms tags 0–6 are occupied;
tag 7 is the proposed append. WindowResponse::Adopted uses tag 8 in a separate
enum. Preserve every old tag and the eight existing event vectors. Postcard
field order follows the declarations above; f64 uses eight little-endian
IEEE-754 bytes. TypeScript uses `variant: "DesktopSnapshot"`, bigint epoch
and explicit null, following the current codec. Do not add a redundant payload
version field: the appended event tag identifies this schema. Existing KIPC
frame/HELLO protocol versions remain unchanged; this is payload/API review.

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
not unbounded buffering or an extra queue. Do not truncate an oversized display
set or synthesize a primary/scale to fit. Current WindowEvent Copy/Eq derives
cannot survive Vec/f64 unchanged; use the owning code's borrowed/owned handling
and supported traits. The current 64-byte router scratch only covers old events;
provide bounded storage for this event without expanding legacy reply buffers.

Internal `WindowSource.desktop()` returns a deeply frozen DesktopFacts or
undefined while live. Decode freezes its graph, apply copies it and atomically
replaces the prior snapshot; both remain free of user callback dispatch. Handle
this windowless variant before any `fact.window` access in existing consumers.
In particular `packages/@keld/api/src/window-close-receipts.ts:32–33`
(`WindowCloseReceipts.observe`, called by `app.ts:onWindowFact`) and
`packages/@keld/electron/src/browser-window.ts:804,827` (the static
`onWindowFactApplied` listener's held-close flush and map lookup) currently
assume every fact has a `window`. Both must explicitly ignore the desktop
variant before dereferencing it. They must neither mint a fake WindowRef nor
advance close receipts, dispatch a window listener, or send an outbound CALL.
The actual `WindowSource.decode → apply → dispatch → app.ts →
WindowCloseReceipts + BrowserWindow` path needs a real consumer regression
that fails if either early guard is removed; direct `WindowSource` tests alone
do not exercise this integration.
Use the existing captured state intrinsics and preserve prior phase/retention
gates. Same-epoch identical values remain idempotent under D9. A newer null
clears desktop facts while retaining the watermark and live window state.
Malformed/contradictory/regressing facts retain the source's sticky KELD-IPC-005
behavior. End clears facts and callbacks, preserving the terminal code/message;
a delayed fact cannot revive them. No Error-object identity guarantee is added.
Facade getters read only this source after the existing readiness gate, return
ordinary isolated copies/the actual boolean, and issue no calls or waits.

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
  arrive before Subscribe or after the replay batch, but in real S1 follows Init sample
Core writer checks existing channel/role/generation admission
  REFUSED: existing SubscribeRefused reason; no pending c or desktop EVENT
  ALLOWED + validated sample already retained: capture latest epoch e
  ALLOWED + Init still pending, no sample: reserve at most one pending Subscribe(c)
    no subscribed bit, desktop/window EVENT, Subscribed(c) or public Ready
  ALLOWED + completed capture currently unavailable: same-correlation typed ERR,
    no waiting for another Init, no replay of the last superseded Some
Mac UI genuine Init captures desktop state before HostInitialized (15-second outer bound)
  Core writer processes the captured value first and resumes pending c if present
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
  Existing 5-second Subscribe call expires or role/link dies: typed terminal
    fence pending c; no late ERR, Subscribed, subscriber state or public Ready
  Duplicate Subscribe while c pending: reject under existing lifecycle policy
    without replacing c, reserving a second pending call or replaying state
Next queued observer update e+1 → enqueue DesktopSnapshot(e+1) AFTER Subscribed
Existing GH527 applier applies snapshot + window replay, no user listeners
Facade public Ready opens only after raw Ready + current valid desktop +
  window replay + same-correlation successful Subscribed; exactly once
Applied newer None before the Ready commit: typed readiness failure, never Ready
After Ready, newer None: desktop getters refuse locally; window facts remain live

Real macOS S1 positive ordering A: Subscribe(c) pending → Init sample(e) →
  DesktopSnapshot(e) → Created* → Subscribed(c) → HostInitialized/raw Ready
Real macOS S1 positive ordering B: Init sample(e) → HostInitialized/raw Ready →
  eligible Subscribe(c) → DesktopSnapshot(e) → Created* → Subscribed(c)
Synthetic-only robustness: raw Ready → Subscribe(c) → sample(e) does not
  establish a possible macOS Init trace; it must neither bypass gating nor
  be substituted for the two real positive orders
```

**Pre-Init absence is pending; definitive/current unavailability is a failure.**
GH508's `SubscribeRefusal` remains a closed enum for channel/role checks,
without any invented `DesktopUnavailable` reason. The existing GH527
`lifecycleReplyWaiter` admits an authenticated lifecycle `ERR` encoded as a
`CallError` for the **same correlation `c`**, so no new frame/channel is
needed. Recommend **KELD-CORE-049** for **DesktopSnapshotUnavailable**, pending
the error-registry owner's decision; this draft does not reserve or allocate it.
At source `076a09ce`, the registry ends at Core-047; KEL75 already proposes
Core-048 as an alternative. Core-049 has no current source/registry reference.
Recommended message: "The native desktop snapshot is unavailable". Fix:
"Use a live session whose admitted Ready barrier has completed; after an Init
or capture failure, correct the Host's native display/appearance error and
relaunch. Do not synthesize a display or theme."
Reuse the existing lifecycle CallError envelope and terminal teardown. Core-040
names the window registry, Core-045 the missing window subscription and Core-037
the broader app-session failure; repurposing them would hide this distinct
passive-facts failure, including post-Ready invalidation. Existing transport or
sticky protocol errors retain their own cause. Recheck collisions immediately
before allocation and land the registry entry with implementation. An absent *pre-Init* snapshot is an ordinary pending
state and cannot cause an immediate ERR on a healthy early Subscribe within its
existing five-second caller budget. Completed Init with a
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
The existing LifecycleLink passes the same 5,000 ms constant to its Subscribe
call; retain that call-start bound unchanged. The earliest completed failure
wins: definitive Init capture ERR, that caller's typed timeout/link failure,
or the existing 15-second native watchdog. A timeout closes/retires the current
link and consumes/fences its pending correlation before any late success can be
accepted. Do not retain an unowned pending subscription or respond into a retired
generation. This S1 decision permits a healthy but late native Init to lose to
the shorter caller bound; it does not enlarge Host/Guardian/IPC clocks or promise
all 15-second startup orders succeed. No polling, hidden retry or additional
post-Init sampling deadline is introduced.

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
The Core pending slot is keyed to the authenticated current primary owner and
original nonzero lifecycle correlation; a duplicate (same or different
correlation) cannot replace that slot, install a subscriber or replay facts.
Its only completions are one accepted ordered batch, one definitive
same-correlation ERR before replay, or generation/link retirement that releases
the slot without writing a late reply. The caller's unchanged 5,000-ms timeout
closes its Bun link; the Host must discard the slot on that observed link
retirement even if an otherwise healthy Init arrives later. This does not
imply that the Core can observe the precise Bun call-start timestamp.

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

A restart uses the Core writer's latest desktop state and follows the same
fence, while the old role/generation receives no new frame. If that state is
None, refuse the new Subscribe with DesktopSnapshotUnavailable; do not replay
an older Some or wait for an Init that has already completed. A later genuine
valid capture can serve a newly authenticated eligible successor. No
observer callback and no continuation in another thread may forge a
`Subscribed` reply or reset the public facade Ready state.

**Dependency edge to GH446:** its approved readiness gate already waits
for raw Ready + accepted Window Subscribe/full replay. GH481 proposes that
the gate also require a **currently valid applied** `DesktopSnapshot`, complete
Created replay and processed same-correlation `Subscribed(c)`, in either
Ready-first or Ready-last order. It must not create a second App Ready
source, resolve a waiter for a merely *pending* c, or set `isReady()`
from a fabricated default. GH446/GH531 owners must explicitly approve
this extension and the unchanged five-second caller availability decision before adoption.

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

**S1 native reuse candidate (still requires the paired qualification above):**
use safe typed objc2-app-kit 0.3.2 calls at genuine Init, with no hidden window.
Read one NSScreen array, its first full frame as the existing placement anchor,
and each screen's frame, visibleFrame, backingScaleFactor and NSScreenNumber
NSNumber from deviceDescription. That ID source matches pinned Tao 0.35.3;
check that `NSNumber.doubleValue` is a finite, strictly positive integer
not above u32::MAX, and its checked `unsignedLongLongValue` gives exactly
that integer before nonzero/uniqueness admission. An unexpected fractional
NSNumber must not be truncated to a plausible display id. Never use array positions or
promise cross-reboot ID stability. Reuse the current
`appkit_registry_bounds(frame, primary_full_frame)` checked point-space
conversion for full/work frames; do not divide Tao physical dimensions or
multiply AppKit points by scale. Read NSApplication.effectiveAppearance and
bestMatchFromAppearancesWithNames using a verified safe `NSString` representation
of the Aqua/DarkAqua appearance names; absent/unknown results refuse rather than
default Light. **Pinned generated-binding inspection:** `NSScreen::deviceDescription`
is gated on `objc2-app-kit/NSGraphics`; `NSApplication::effectiveAppearance`
needs `NSApplication`, `NSResponder` and `NSAppearance` features; the
device-description value is `AnyObject`, so numeric class/type and u32 range
must be checked before using the ID. These features are not all explicitly
enabled by current `crates/keld-wv/Cargo.toml`. The generated
`NSAppearanceNameAqua/DarkAqua` exports are **extern statics**, whose direct
read would require Rust unsafe; never claim those constants are safe typed
values. Obtain equivalent appearance names through a verified safe NSString
path (and compare against a native Electron sample), or seek separate exact
unsafe authorization first. No unreviewed unsafe or dependency change is
authorized by this draft. Apple's `NSScreen.visibleFrame`,
`NSApplication.effectiveAppearance` and `NSAppearance.bestMatch` documentation
define the OS API, not KELD/Electron parity by themselves.

S1 may recapture the complete sample on native Moved/Resized/Focused/
ScaleFactorChanged and per-window ThemeChanged observations already available
to the UI owner. A failed recapture publishes higher-epoch None through the same
writer; valid changed facts publish Some. These signals are **not** a qualified
global monitor/work-area/theme observer, especially before any window exists.
Do not claim they complete D7/D8 or reflect OS state at every getter. General
freshness needs a separately approved lifetime-owned native observer and its
shutdown proof. No new public GH558 events, polling or fake global invalidation
are authorized. Host/link retirement clears locally even if no final None can
be delivered.

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
| D2 | **Real macOS:** (A) `Subscribe(7) → Init sample(1) → DesktopSnapshot(1) → Created* → Subscribed(7) → raw Ready`; (B) `Init sample(1) → raw Ready → Subscribe(7) → DesktopSnapshot(1) → Created* → Subscribed(7)`. Both resolve public Ready only after the current applied snapshot and reply. Independently emulate `raw Ready → Subscribe → sample` as **synthetic malformed ordering** and prove it cannot publish public Ready early; never call it observed macOS. No/malformed final sample → one same-correlation ERR, zero replay, sticky Ready rejection; 15-second missed Init → `KELD-CORE-037`; wrong role or unsubscribed generation → zero facts; duplicate pending c cannot consume another slot; 5-second caller timeout/link retirement → no late Ready/reply/replay. | Mutate pending admission to immediate pre-Init ERR: real schedule A fails. Allow raw Ready/partial replay to publish: synthetic case fails. Delete slot-fence, duplicate guard or zero-replay failure path: targeted Core framed negatives fail |
| D3 | Real Mac Display array/primary id, finite DIP rectangles/scales, one-primary set | Duplicate id/nonfinite scale/zero displays accepted: exact boundary fails |
| D4 | Paired pinned Electron 44.4.5 physical primary and negative-offset secondary workArea | Leave AppKit bottom-left y-coordinate raw; comparison fails |
| D5 | Paired light + dark actual Mac; `shouldUseDarkColors` boolean matches both | Hardcode `false`; dark run fails |
| D6 | Real WorkerLink/Host count before and after 1,000 getters of each API | Replace getter with per-read CALL; actual frame count increases |
| D7 | Force two deterministic writer-queue interleavings (observer e+1 before Subscribe capture and immediately after). Verify initial e+1 OR replay e→Subscribed→live e+1 in order; parked GH527 reply receives applied fact before listener. Exercise real `WindowSource → app.ts → WindowCloseReceipts + BrowserWindow` on a desktop event with an existing close receipt/window, followed by a real Bounds/CloseRequested event; the desktop event changes no close/window observable or Host CALL. | Let native observer write outside Core writer or enqueue e+1 before replay e: monotonic order/Ready/parked listener tests fail. Remove either windowless consumer guard: actual dispatch fails |
| D8 | Bun crash→same app's successor current snapshot+survivor windows; duplicate same-epoch snapshot idempotent, contradictory same epoch refused; retired link fenced | Apply old epoch to successor or classify legitimate e+1 Subscribe race as a terminal stale error: tests fail |
| D9 | 1..32 display count, full encoded EVENT payload at most 32,768 bytes, malformed/truncated/overlong, zero/duplicate primary or display IDs, nonfinite scale; Core writer increments checked epochs for every Some↔None transition and refuses a change after u64::MAX; the Bun mirror refuses stale/contradictory same-epoch wire facts while admitting identical duplicates. Both producer and receiver enforce the cap; the finite 32-record valid encoding is normally far below 32KiB, so the size check is defensive and needs a separately exercised over-cap input/serialization guard. | Accept 33rd record/no primary, strip over-cap guard or wrap u64::MAX to zero: matching host/codec negative fails |
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
   the **currently valid applied** desktop snapshot to its existing `raw Ready +
   accepted Subscribe + full replay + same-correlation Subscribed` gate.
   Record (a) capture finalized at genuine Init before HostInitialized,
   (b) unchanged five-second caller timeout from Subscribe initiation, accepting
   failed startup when it expires before the separate 15-second Init bound,
   and (c) typed terminal settlement plus pending-correlation fencing for
   unavailable/malformed facts, timeout and role death. No clock increase or
   late same-generation retry is proposed.
   The GH531 owner must separately approve this pending/ordering amendment;
   no second Ready source or delayed-entry assumption is permitted.
3. **Snapshot limits, transaction and epoch:** approve/revise proposed max
   **32** displays, max **32 KiB** encoded bytes, monotonic Core-writer
   epoch and last-sent deduplication. Confirm one atomic Subscribe capture/
   replay/Subscribed batch and the two deterministic interleavings; a
   legitimate OS update must not become a stale-epoch terminal error.
   Approve candidate event tag 7, Some/None envelope and full-payload byte counting;
   retain D9 identical-epoch idempotence and checked exhaustion. These values/
   policies are **not authorized** by this draft alone.
4. **macOS coordinate/effective-appearance selection:** choose the exact
   pinned OS display/appearance API and conversion only after a same-
   machine Electron 44.4.5 paired native experiment; raw NSScreen
   coordinates and a convenience appearance flag do not prove parity.
5. **Pre-Ready/terminal getter and native-snapshot failure:** obtain the
   GH531/GH446 and error-registry owners' approval for *definitive Init-time*
   correlated lifecycle CallError ERR `DesktopSnapshotUnavailable` (Core-049
   recommended after the recorded collision check, **not allocated**), atomic
   zero-replay settlement, sticky public-ready
   failure and typed unavailable getters. An early healthy Subscribe is
   **pending**, not erroneous; absent/malformed final sample is terminal,
   never an empty headless display or light-mode false. Waiters must settle.

This spec is **DRAFT** and approval is a separate tracked decision. It
neither merges/edits the active GH449/GH446 branches, implements any
native module, qualifies real macOS product behavior, closes #481 nor
flips the four pending GH477 conformance cells by itself.
