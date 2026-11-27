# Design

Part of: [index](index.md)
Owns: section 4

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
If Subscribe(c) is admitted before native UI-loop entry, require loop entry
  within 10 seconds of that first admission; expiry is terminal KELD-CORE-037
Mac UI Init reports first valid native desktop state within 15 seconds of
  actual UI-loop entry; expiry is terminal KELD-CORE-037
  Core writer validates and retains latest e, resumes pending c if present
  For validated sample + eligible c: one uninterruptible ordered enqueue batch
    DesktopSnapshot(e) EVENT on existing window channel
    all surviving-window Created replay EVENTs
    correlated Subscribed(c) REPLY on lifecycle
    commit eligibility and last-sent-epoch=e only after queued barrier
  The batch has one absolute 5-second settlement deadline from validated e
  through acceptance of Subscribed(c); per-frame I/O deadlines do not reset it
  Caller deadline is 30 seconds from first admitted c: 10s loop + 15s Init
    + 5s batch, absolute and never reset by retry, duplicate or progress
  Init complete, definitive no sample or malformed sample + pending c:
    exactly one ERR(c, KELD-CORE-049 DesktopSnapshotUnavailable); zero replay/bits
  Eligible Subscribe(c) after that terminal Init outcome: same-correlation
    ERR(c, KELD-CORE-049) immediately; no pending or replay
  Role death/link loss or replay-batch timeout: GH527 typed terminal,
    fence pending c, no late ERR or successful Subscribed
  Duplicate Subscribe while c pending: reject under existing lifecycle policy
    without replacing c, reserving a second pending call or resetting deadlines
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
needed. Owner decision [#6962777320](https://github.com/gyldlab/keld/issues/446#issuecomment-6962777320)
reserves `KELD-CORE-049` for **DesktopSnapshotUnavailable**, consistent with
the owner-approved Windows S1 scope. The GH531/Core error-registry owner must
add its canonical message and fix before implementation consumes the code.
An absent *pre-Init* snapshot is an ordinary pending state and cannot fail a
healthy early Subscribe. Completed Init with a definitively missing or
malformed first sample instead emits one same-correlation KELD-CORE-049 ERR,
zero replay records and no successful `Subscribed`; the same-generation
pending correlation is consumed once. The failed-Init state is sticky even
if no Subscribe was pending at the moment of failure: a later eligible
Subscribe receives its own single correlated ERR, never a new pending c
or a late unvalidated snapshot. If any replay frame is partially
written or the link dies, GH527 transport teardown is terminal: no replay
retry or fake successful `Subscribed` may follow.

**Deadline ownership:** Owner decision [#6962777320](https://github.com/gyldlab/keld/issues/446#issuecomment-6962777320)
sets one absolute 30-second caller deadline from first admitted Subscribe(c),
with no reset. If Subscribe precedes native UI-loop entry, that loop must
enter within 10 seconds of the same admission; expiry fails the current
authenticated app session with KELD-CORE-037 and no late HostInitialized/Ready.
The existing 15-second `INITIAL_MILESTONE_DEADLINE` remains anchored to actual
macOS UI-loop entry through successful Init; expiry also uses KELD-CORE-037.
After the first valid sample, one absolute 5-second aggregate settlement
deadline covers the complete DesktopSnapshot/Created*/Subscribed batch through
acceptance of the same-correlation reply. It is distinct from the existing
5-second per-frame APP_LINK_IO_DEADLINE and cannot reset per frame or progress.
The 10 + 15 + 5 second subdeadlines compose the 30-second caller bound.
Completed Init with no/malformed sample uses reserved KELD-CORE-049;
role/link death or replay-batch expiry uses GH527's terminal path. No partial
subscriber state, late ERR or late successful Ready is permitted.

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
from a fabricated default. The pending-Subscribe ordering and caller-clock
coordination are owner-decided in [#6962777320](https://github.com/gyldlab/keld/issues/446#issuecomment-6962777320);
implementation must enforce the exact subdeadlines and terminal ownership above.

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
