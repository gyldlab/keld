# Spec: macOS synchronous Electron message box over the existing WorkerLink (GH-564)

Status: draft
Linear: GH-564 (tracker under #517; reference KEL-80) · Owner: gyldlab/keld · Updated: 2026-10-27

## 1. Goal & non-goals

Make the pinned draw.io application's **unmodified**
`dialog.showMessageBoxSync([window, ]options)` usable under KELD's explicit
macOS legacy compatibility profile. It must return the **actual integer
button index synchronously** to a Bun main-process `ipcMain` listener while
the **one existing GH527 WorkerLink** continues draining host frames. The
Host presents the real guarded dialog through **GH478's native broker** and
retains native cancellation/cleanup independent of Bun lifecycle. This
closes only the synchronous message-dialog compatibility atom; it does not
by itself make the whole draw.io app or two-phase close shippable.

Non-goals:
- Inventing a second Worker, socket, IPC link, ring, credit protocol,
  Node synchronous request trick or nested JS event pump. Never modify the
  GH527 transport just because a dialog consumer needs to park JS.
- Implementing the native dialog broker, permission grammar or host window
  registry again. GH478, #449/#450 and GH531 are their sole owners.
- Renderer-side `ipcRenderer.sendSync`, `showOpenDialogSync`,
  `showSaveDialogSync`, `showErrorBox`, `showCertificateTrustDialog`, a
  universal Electron module or an app-specific draw.io IPC workaround.
- Returning an invented Cancel or clicked default on link loss, role death,
  denial or expiry. Such a guess can discard unsaved user data.
- Changing Host 20 s / Guardian 10 s / cleanup 5 s / kipc 2 s and other
  existing service deadlines or public `app.quit()` Promise divergence.
- Windows/Linux real native support or permission-sandbox containment
  claims; only macOS first-proof is accepted here.

## 2. Spec refs and factual boundary

- Approved `docs/specs/gh527-worker-owned-blocking-call-transport.md` §4.5
  (same role's sole WorkerLink), §4.6 (apply framework facts before return,
  deliver listeners **after** resume), §4.7 (typed correlation and receiver
  admissibility), §4.9 (nonblocking app.quit), the actual
  `packages/@keld/kipc/src/transport.ts` `WorkerLink.callBlocking`.
- [#528 reviewed macOS transport evidence](https://github.com/gyldlab/keld/issues/528#issuecomment-6370844113),
  `MAX_BLOCKING_CALL_DEADLINE_MS=24h` in current source, and existing ring
  byte/record limits. A **human dialog budget** is a new caller-specific
  deadline **below** that existing maximum, not an increase of it.
- Approved `docs/specs/gh531-window-registry-close-state-machine.md`:
  Host-minted WindowRef and role-owned `ClosePending` → `Veto` → `Open`
  before the application's later dirty/modal/save choices. Native release
  and `closed` belong to #449/#450; this API does not authorize closing.
- Shared native broker **proposed, not approved or implemented** in
  [GH478 draft PR #1161](https://github.com/gyldlab/keld/pull/1161),
  `docs/specs/gh478-guarded-macos-dialog-broker.md`. One `dialog` grant
  through canonical keld-guard and one append-only GH508 channel table;
  no speculative numeric channel ID in this specification.
- GH532/GH566 single conformance owner; [GH477 first expected-fail
  cell](https://github.com/gyldlab/keld/pull/1156) is draft, hosted
  **22/22 successful at `be42686e`**, not merged and not an implemented API.
- Pinned Electron **v44.4.5** tag commit
  `694f45852a0f1726cd23bfd379854de489cccb65`,
  [`docs/api/dialog.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/dialog.md),
  [`message_box_mac.mm`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/message_box_mac.mm),
  and the [October 24 real-Mac oracle](https://linear.app/gyldlab-keld/document/electron-4445-macos-drawio-closequit-oracle-24-oct-2026-1718a506d0c7).
  The 75 source-shaped Electron traces are an **upstream** oracle, not a
  KELD native or unmodified installed draw.io acceptance run.
- Exact draw.io Desktop
  [`jgraph/drawio-desktop@2edf9fb:src/main/electron.js:849-939`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js)
  sets `modifiedModalOpen` **before** `showMessageBoxSync`, calls it
  with `type:'question'`, `title:'Confirm'` and `noLink:true` at
  lines 876–887, and destroys the window on an application
  `saveAndClose-result.success` at lines 896–905. These are distinct
  source facts from native user choice and durable file persistence.

No architecture amendment to GH527/GH531 is authorized by this draft.
GH478 and this specification need separate approval before their respective
production implementations. Any conflict with an approved upstream native
broker contract must be resolved *in that owner* before GH564 writes code.

## 3. Acceptance criteria (one independent oracle per criterion)

**S1 — actual synchronous value.** With a ready admitted Bun main role,
verified exact `dialog` grant and live parent window, a real Mac alert with
buttons `['Save','Cancel','Discard Changes']`, `defaultId=0`, `cancelId=1`
returns the **actual clicked** index **0/1/2**, respectively, as a primitive
`number` **before** the caller executes the next statement. It is finite,
integer, in the admitted button range, and **not thenable**. Each of three
choices is a separately witnessed native click; echoing the default does
not pass. The exact caller also supplies `type:'question'`,
`title:'Confirm'`, `noLink:true` and `normalizeAccessKeys:true`.
`noLink` must be accepted as a validated macOS **no-op**, not rejected
because its visual effect is Windows-only in Electron 44.4.5;
`title` is accepted with measured native presentation semantics, even if
macOS does not visually show it. These required keys cannot be discarded by
an over-restrictive shared GH478 broker allowlist.

**S2 — parented and independent dialogs.** A real visible, caller-owned
Host WindowRef displays one parent-attached macOS **sheet**. A request
without parent or with a valid hidden parent displays one independent
native alert. A stale/destroyed/forged/cross-app parent refuses without
retargeting. In all supported cases the return remains synchronous to Bun,
even if AppKit uses an asynchronous completion internally.

**S3 — one existing transport owner.** `dialog.showMessageBoxSync` uses the
already **connected** role singleton in `packages/@keld/api`, and only
`WorkerLink.callBlocking` on the GH478 allocated `dialog` channel.
The existing Worker-owned socket and ring deliver one correctly correlated
REPLY or typed ERR. No new HELLO, endpoint, Worker, link, wake thread,
transport copy parser or per-dialog queue is created. Before the singleton
is authenticated/ready, calling the synchronous facade throws a typed
*not ready* error **synchronously**: it must not return a Promise or wait
for a newly started async connect.

**S4 — listener isolation under park.** While the native synchronous alert
is up, **no Bun main-process application** timer, microtask, `ipcMain`
listener or second `close` listener executes. An independent renderer timer
continues. A positive asynchronous-dialog control *does* deliver main JS
callbacks while the panel is visible. After the native result, queued
host EVENTs and CALLs are delivered in original GH527 order; no lost,
duplicate or reentrant user listener. Other Bun Workers are separate and
are **not** asserted paused.

**S5 — framework state before user continuation.** When a host state fact
is queued **before** the dialog REPLY, the existing WorkerLink framework
state applier updates the mirror before `callBlocking` returns, **without
calling user code**. Event listeners run in order only on a later regular
main-event-loop task. State facts after REPLY are not applied as though
known at the reply. The real consumer must exercise an **over-8-KiB**
parked EVENT stream, reusing #528's tested ring rather than substituting a
single small mocked event.

**S6 — denied grant never presents UI.** Without the exact verified
`app.system:["dialog"]` grant, with a non-app principal, unverified policy
or wrong generation, a call throws an existing canonical typed
`KELD-GUARD*` denial whose fix names `/app/system`. **Zero** AppKit panel
objects become visible/retained; the call cannot move through an
unguarded fallback or mint a path/FS scope. This is GH478's guard,
**not** a second facade-side permission evaluator.

**S7 — generation/parent/native custody.** During an outstanding real
panel, Bun role loss, Worker death, app link loss, destroyed parent or
Host terminal Quit physically dismisses/releases it through GH478's
UI-thread owner, with no callback to any successor or stale window and
no second answer. If the Host survives, the next admitted role can
open/close a fresh panel normally. A typed JS wake without physical
panel dismissal is **failure**. No numeric PID/window handle serves as
authority.

**S8 — failure and expiry do not forge consent.** A denied, stale,
expired, native-failed or effect-unknown call throws a typed error,
**not** Save/Cancel/Discard. An ambiguous IPC-022/023 is never silently
retried and a late native click is fenced. A finite **human-interaction**
deadline is separate from the existing bounded 5 s frame I/O and
Host/Guardian shutdown deadlines; expiry cancels the native panel.
Host shutdown does **not** wait the full user-interaction duration.
Existing GH527 `WorkerLink` local `abandon` is **not** an AppKit
dismissal; the GH478 same-link nonce/Cancel proposal must satisfy its
separately approved physical cleanup bound after caller timeout, even
with delayed Host admission and when JS has already resumed by throwing.
The expired original `CALL(c_orig)` and the new asynchronous
`Cancel(c_cancel)` have **distinct correlations and terminal obligations**.
The Host's ACK/`REPLY(c_cancel)` proves the Cancel disposition only; it
does **not** retire the WorkerLink's abandoned `c_orig`. For an admitted
original still pending when native cancellation wins, the same Host
writer must separately send exactly one canonically registered typed
`ERR(c_orig)` after its owned cleanup/tombstone, or preserve the genuine
already-committed `REPLY(c_orig)` if the native click won first. GH527's
Worker discards that late original terminal from the expired caller but
releases its abandoned correlation exactly once. On one healthy link,
**256+1 = 257** sequential caller expiries followed by acknowledged
cancellation in a deterministic transport/Core test must leave no
abandoned originals and permit a healthy next call without
`KELD-IPC-027`. No new user-visible clicked
index, duplicate original terminal, retry or forged response on a dead
link is permitted. Physical panel dismissal is an **independent**
real-Mac acceptance obligation; two well-formed replies cannot prove it.

**S9 — user Cancel versus terminal failure.** A *real* macOS Cancel
button/Escape mapped by Electron's `cancelId` is the actual numeric
selected index (1 in draw.io's buttons). It is distinguishable from
closing a panel because the role died, a timer expired or the native
provider failed. None of those failure paths can impersonate Cancel.

**S10 — exact draw.io integration.** On real Mac under explicit legacy,
loading the pinned unmodified draw.io main and renderer yields:

1. Native/window close event is immediately `preventDefault()`-vetoed,
   and Host remains open while `isModified(uniqueId)` is outstanding.
2. Dirty renderer reply enters the actual registered `ipcMain` handler;
   that handler invokes **this** synchronous native alert, not a mock.
3. Cancel retains same dirty native window, same renderer document content
   and clears the app's modal guard via the normal successful return path.
4. Save returns index 0 but cannot itself destroy the window; only an
   **application-correlated Save success acknowledgement** leads draw.io
   to call `destroy` and produce one `closed`. This app acknowledgement
   alone proves neither persisted file contents nor `fsync` durability.
   Independently reopen and compare the actual saved document bytes on
   disk in native Mac acceptance; failed/canceled Save retains the dirty
   window and original unsaved content. Never substitute the app's boolean
   for a persistent-content receipt.
5. Discard returns index 2; application draft deletion is an independent
   contract. A cross-window global `draftRemoved` acknowledgement is a
   **pinned draw.io application-protocol defect hypothesis**, not
   permission for KELD to cross-bind Host WindowRefs or to claim durable
   draft deletion without a receipt.

**S11 — conformance lifecycle.** The implemented GH564 change inverts the
GH477 pinned v44.4.5 expected-fail synchronous cell and flips
`implementing_ticket: GH-564` to an actual pass **in the same diff**.
The one GH566 validator and the v44.3.0 lifecycle manifest stay unchanged.
Mac native result is separately source/CI qualified. Windows/Linux remain
unknown/unsupported; no product pass is inferred from a TS import test.

**S12 — no deadline or authority widening.** The existing transport, Host,
Guardian, Core shutdown and real FS-service deadlines are byte-identical
or behaviorally proven unchanged. The dialog-specific human clock must
fit within `MAX_BLOCKING_CALL_DEADLINE_MS`; no infinite wait, `sleep`
workaround, ambient permission, new global exception or unbounded buffer.

**S13 — preflight and fail-closed error recovery limitation.** Before the
first dirty-close test, the harness must prove an authenticated Ready
singleton, exact current `app.system:["dialog"]` policy and a live
owned parent, then submit the unchanged draw.io event. This preflight
prevents a known deterministic test-setup refusal; it does **not** change
the app's `modifiedModalOpen` assignment. On a genuine denial, expiry,
or link failure after that assignment, the native dialog never forges
Cancel and the dirty window/content survive. Pinned draw.io may leave
`modifiedModalOpen=true` and ignore the **next** close. Repeatable close
after such a failure is explicitly **unsupported/blocked** until a
separately approved app-recovery compatibility owner proves a source-exact
adapter or upstream fix. A test must observe the stranded flag effect
as a known fail; it must not silently score recovery as passed.

## 4. Design

### 4.1 First-principles ownership and event sequence

Use the approved GH531/#450 separation of **Host close attempt** and
**application dirty/modal decision**:

```text
Native CloseRequested(s) → Host retains window → facade close listener
  → draw.io preventDefault() → Host CloseReply(s,Veto) → Open
Renderer isModified(uniqueId) → ipcMain isModified-result
  → dialog.showMessageBoxSync(parent, options)
  → API reads EXISTING authenticated LifecycleLink synchronously
  → WorkerLink.callBlocking(dialog-channel, bounded bytes, interactionDeadline)
  → transport Worker drains host EVENTs; Bun main application JS parked
  → Host authorizes exact dialog grant, verifies parent and opens real panel
  → Native click, UI owner releases panel, host sends correlated REPLY
  → GH527 mirror state appliers (no user callbacks), callBlocking returns number
  → draw.io Save/Cancel/Discard branch; queued listeners run after JS stack
  → any later destroy is a NEW app action, never consent inferred by Host
```

The invariant is **one Bun role singleton, one Host-minted parent identity,
one GH478 native panel owner and one correlated original CALL terminal**;
a timed-out original also requires the distinct fresh Cancel CALL response
specified in S8. Neither
facade nor renderer mints authority. WebView-to-Bun IPC is separate from
Bun-to-Host native CALL; renderer `sendSync` is explicitly out of scope.

### 4.2 Smallest change to the existing singleton

The current `@keld/api/src/app.ts` owns `linkPromise` and `ensureLink()`;
`LifecycleLink.connect` yields an already authenticated WorkerLink.
Because this API is **synchronous**, it cannot safely `await ensureLink()`.
The approved implementation must cache the *same* established
`LifecycleLink` instance as an **internal ready-to-use reference** on
successful connection, while preserving the existing sticky failed-connect
and onLinkDead semantics. It cannot create another role link or retry a
closed one. Readiness for a synchronous call is **stronger** than merely
having the cached Promise: it needs a live authenticated/usable session.

Proposed private TypeScript seam (sketch, not landed API):

```ts
// packages/@keld/api/src/app.ts (only singleton owner)
export function invokeDialogBlocking(bytes: Uint8Array, timeoutMs: number): Uint8Array;

// packages/@keld/api/src/link.ts (the already connected LifecycleLink)
class LifecycleLink {
  callDialogBlocking(bytes: Uint8Array, timeoutMs: number): Uint8Array;
}

// packages/@keld/electron/src/dialog.ts (thin compatibility mapping)
interface DialogSyncFacade {
  showMessageBoxSync(options: MessageOptions): number;
  showMessageBoxSync(parent: BrowserWindow, options: MessageOptions): number;
}
```

The `app.ts` owner must preserve its failed-connect and terminal state
without introducing another parser or publicly exposing transport Worker
internals. Require the same existing `WorkerLink.callBlocking()` and
**generated** dialog channel constant from the approved GH478 channel
allocation; the typed operation/result comes from **GH478's codec**. No
hand-coded numeric channel and no new ipc frame version.

If the app invokes the synchronous method before the current role's link
is authenticated/ready, it must throw a typed synchronous KELD error
rather than return or internally start a Promise. The ready mechanism
must use the existing host Ready/Subscribe contract from #445/#446/#449;
no new app-wide latch is invented as a workaround.

### 4.3 Native UI, lifecycle and result identity

GH478 owns guard, current parent visibility, NSAlert/sheet creation,
UI-thread native closure and callback cancellation. GH564 asks only for
the **message-panel** operation (not file-open/file-save). `WindowRef`,
role generation and correlation bind the pending native work; return the
actual clicked button index after the native panel has settled, not

defaultId. Multiple messages must not queue indefinitely, and one
role's abandoned panel never belongs to its next generation.

The **entire actual unmodified draw.io options object** at pinned lines
876–887 must reach the **same** GH478 native message broker:

```ts
{
  type: "question",
  buttons: ["Save", "Cancel", "Discard Changes"],
  defaultId: 0,
  cancelId: 1,
  noLink: true,
  normalizeAccessKeys: true,
  title: "Confirm",
  message: "The document has unsaved changes. Do you want to save them?",
}
```

GH478's shared typed allowlist must admit **all** these keys and their
correct values before panel creation; `type` is a real enum,
`title` a bounded string and `noLink` a boolean that has **no native
effect on macOS** under the pinned Electron 44.4.5 contract. Electron
allows title even if the OS does not show it. GH564 must not silently
drop a key or invent a platform-specific rejection. Unknown additional
keys, wrong types and out-of-range indexes remain typed failures before
UI. Each original native button click (0/1/2) must be observed on a
visible Mac panel, independently of a mocked fixture.

Before exercising that call, the installed-app harness verifies
`Ready`, the already established role link, current exact
`app.system:["dialog"]` grant, host-owned window and approved native
message backend. Failure of this **external preflight** marks the product
test **blocked** before driving dirty close; it does not change the
unmodified application's own state transitions. Once draw.io has set
`modifiedModalOpen=true`, a genuine typed permission/lifecycle/native
failure must **never** be swallowed into synthetic Cancel. Pinned lines
849–939 only clear that flag on normal return branches, so a thrown
failure can strand it and make a later Close a no-op. The dirty window
and bytes must survive; **post-failure repeatable Close is an explicit
unsupported dependency** requiring a separately approved upstream
fix or application compatibility adapter with a source-exact failing
test. No KELD Host state reset or cross-window workaround may clear
an application-private variable.

### 4.4 Error, deadline and backpressure contract

- Guard denial: existing `KELD-GUARD*` code/fix, no visible panel.
- Invalid parent, parent retired, UI failure, expired interaction: GH478's
  typed outcome; do not allocate collision-prone codes in this consumer.
- Link/Worker death and ambiguous effect: existing `KELD-IPC-022/023/025`
  terminal semantics, no replay/auto-retry and no fabricated integer.
- Reentrant `callBlocking` from a framework state applier: existing
  `KELD-IPC-005` refusal, no user-code reentrancy introduced by the shim.
- Concurrent dialog from another role-owned async task: GH478
  Host-owned `DialogBusy` before a second panel, distinct from the
  transport's `KELD-IPC-005`.
- Interaction deadline: **proposal** `600_000` ms, borrowed from unapproved
  GH478 spec; must be jointly approved before code. It is finite, below
  existing `MAX_BLOCKING_CALL_DEADLINE_MS=86_400_000`, and is **not** the
  `APP_LINK_IO_DEADLINE_MS` (currently 5,000 ms for frame I/O).
  Do **not** extend Host20/Guardian10/Cleanup5/KIPC policy clocks.
- Caller deadline and native deadline are measured on distinct clocks.
  `WorkerLink.callBlocking` currently sends `abandon` only to its
  transport Worker; a `KELD-IPC-006` throw does not dismiss AppKit.
  Before the normal blocking CALL, GH564's existing-link adapter must
  generate one bounded nonce in the shared GH478 request payload.
  If `callBlocking` expires, that same adapter posts GH478's proposed
  `Cancel { nonce }` as a **new bounded asynchronous CALL over the same
  WorkerLink**, without waiting in user JS, masking the original
  exception or retrying the original side effect. If the existing link
  has died, GH478's Host role-retirement owner must close its panel.
  The original timed-out `CALL(c_orig)` is distinct from the fresh
  `Cancel(c_cancel)`: the Host must send the latter's authenticated
  ACK/`REPLY(c_cancel)` or refusal **and**, for an admitted original
  still pending, a separate exactly-once registered typed `ERR(c_orig)`
  when cancellation wins, or retain the actual committed
  `REPLY(c_orig)` if the native result won first. GH527's Worker
  discards that late original terminal and releases its abandoned
  `c_orig` entry; ACK for `c_cancel` alone does not release it.
  `MAX_ABANDONED_CALLS=256` makes the missing-original-frame mutation
  observable: on the **257th** sequential timeout/Cancel ACK with no
  original terminal on one healthy link, it fails with `KELD-IPC-027`.
  Native cancellation still requires independent physical-dismissal
  evidence even when both correlation replies were written. On link
  death, retire Host/native custody without fabricating either reply;
  unknown nonce or wrong role/generation cannot retire another call.
  Host-side cancellation checks the authenticated role/generation,
  retains a bounded tombstone before queued presentation, prevents
  late UI and releases the real native handle on the UI thread. No
  second socket, Worker, protocol version or custom transport is allowed.
  The proposed observable limits are **10 s from local timeout while
  link remains live** and **5 s from Host-observed role/link loss**;
  final values and delivery correctness are still GH478/GH527/Core
  approval gates, including forced-late-admission and modal-run-loop
  negative controls. A local Cancel request/ACK without native release
  is a failed test, not a completion receipt.
- Retained event traffic stays under GH527's existing byte/record budgets;
  >8 KiB is a correctness test for drain/replay, not authorization to
  allocate an additional ring or enable T4 credit without measurements.
- Host-side execution remains nonblocking for Bun user input: the Worker
  and real native UI callback own progress; the Host must continue
  processing Quit and role loss while the Bun app JS waits.

### 4.5 Cross-platform, permission and compatibility boundaries

Only macOS with a current exact dialog grant and explicit legacy profile
is supported for the first product proof. Strict profile continues to
default-deny unless independently declared and OS-containment admitted;
GUI selection does not grant `node:fs`. Do not claim universal Electron
parity, Mac Store bookmark/alias support, Windows/Linux support or native
cleanup from an Electron oracle. No new permissions, boot profile grammar,
IPC table entry or native type is allocated by **this** consumer spec.

Unsupported options and impossible states fail with typed diagnostics;
`@keld/electron` never imports actual Electron at runtime. On genuine
successful user selection, Electron's numeric return is reproduced even
though the KELD Host's native presentation/transport are asynchronous.
Observable behavior, not identical Electron ObjC source, is the contract.

### 4.6 Reuse decision and rejected alternatives

Existing GH527 WorkerLink satisfies the blocking transport/liveness
atom and is already macOS-accepted as a **foundation** under #528. The
native broker, guarded channel and Host-minted window service are owned
by GH478/GH531, so expanding those implementations *in place* is smaller
and safer than parallel copies. Rejected alternatives:

1. `dialog.showMessageBoxSync` returning a Promise: fails the actual
   synchronous API and draw.io's unsaved-changes modal.
2. Fake native button index or failure mapped to Cancel: permits silent
   loss of unsaved data; cannot pass a real non-default-click test.
3. New connection or Worker opened inside this method: violates
   authenticated single-link ownership and risks duplicate HELLO/replay.
4. Host UI thread waits synchronously for Bun or owns a second modal
   pump/timeout policy: creates circular waits during Quit/role retirement.
5. Letting queued application event listeners run inside `Atomics.wait`
   wake-drain: violates pinned Electron oracle and GH527 two-cursor contract.
6. Reusing stale native parent or numeric OS ID: bypasses GH531 generation
   authority and can target another window.
7. Extending existing Host deadlines to accommodate a human decision:
   hides lifecycle deadlocks and widens shutdown costs.

No rewrite of WorkerLink or measured performance benefit is claimed.

## 5. Boundaries

After both required spec approvals, implementation belongs **only** in:
- `packages/@keld/api/src/app.ts` singleton, package-private direct sync
  access to the *same* connected LifecycleLink; no new connection.
- `packages/@keld/api/src/link.ts` thin guarded-channel wrapper on existing
  WorkerLink; no edit to the transport internals.
- `packages/@keld/electron/src/dialog.ts` and `src/index.ts`: matching
  named/default export and strict Electron result mapping.
- GH478's **future** `crates/keld-native/src/dialog.rs`, to be created
  by the GH478 owner **after draft PR #1161 approval**; this file does
  not exist on the reviewed repo head. GH564 later consumes that
  broker and its approved Core/WV native ownership/permission seam,
  rather than treating an unimplemented file as a ready dependency.
- GH477's `electron-native-v1` mapped cell and test when it is approved/
  landed, with the implementing-ticket flip in the same source change.

MUST NOT modify `packages/@keld/kipc/src/transport.ts`, GH527 or GH531
specs, `crates/keld-core/src/app_session.rs` under GH824/#834 or GH449
owner without coordination, `keld-guard` policy, channel allocation
registry, sibling async file dialogs, native Windows/Linux production,
GuardedPrimary/Guardian timeout logic, CI policy or root dependencies.
Cross-owner integration requires the earlier approved ticket/branch and
fresh winning claim, not a direct patch into another issue's worktree.

## 6. Tasks (one concern per held PR; ordered dependencies)

- [ ] T1: obtain exact-content approval for this GH564 spec and GH478's
      upstream message-broker contract, including source-exact message
      options, architecture path-only rule, same-link cancellation and
      shared interaction/cleanup clocks.
- [ ] T2: consume the already-published GH477 expected-fail sync cell
      (draft #1156); keep it fail before the implementation, no duplicate
      parser/denominator and no 44.3.0 lifecycle re-pin.
- [ ] T3: add a private **connected-link** sync access on the existing
      singleton/WorkerLink, and tests that distinguish absent/connecting/
      live/terminal states without a second HELLO.
- [ ] T4: consume GH478's guarded real NSAlert operation, actual parent and
      clicked-index mapping, bounded native cleanup on role loss,
      local `callBlocking` expiry and late Host admission, plus both the
      distinct Cancel ACK and original-correlation terminal obligations.
- [ ] T5: compose the sync UI result and GH527 parked event/mirror semantics
      with >8KiB incoming events; test native default-only completion and
      actual Host Quit/role loss separately.
- [ ] T6: flip GH477's single implementing-ticket cell to pass **only in
      this implementing diff** and prove real unmodified draw.io
      Close→Cancel/Save/Discard acceptance on a Mac, with independently
      reopened saved content and a clearly labelled unsupported error-
      recovery repeat-close scenario.
- [ ] T7: run actual final full local/hosted required CI, CodeRabbit or
      independent nonauthor exact-head review, permission/public API/wire
      review and OS acceptance before any merge or compatibility score.

Implementation may not begin from a draft. Do not mark this issue done
when only T3 or T4 has a passing mock.

## 7. Failing-first test plan

| AC | Positive observable | Independent negative control |
|---|---|---|
| S1 | Submit the exact draw.io `type:'question'`, `title:'Confirm'`, `noLink:true` object, observe actual macOS 0/1/2 clicks; sync primitive and no Promise | Reject inert `noLink`, drop `title` or substitute Promise/defaultId: real source admission or non-default click fails |
| S2 | visible parent sheet and hidden/absent fallback | Stale WindowRef attaches to recycled parent and fails |
| S3 | one cached authenticated realm link; pre-Ready throws synchronously | Open second Worker/HELLO or await Promise; one-link/return-shape test fails |
| S4 | main JS suppressed during real modal; async positive control interleaves | Execute a timer/IPC listener while parked and parity trace fails |
| S5 | real >8KiB queued facts, pre-reply state applied before return, listeners later | Dispatch listener during wake or drop last EVENT; exact ordering fails |
| S6 | exact guard denial, native panel creation census zero | Remove guard before UI and denial/no-panel fails |
| S7 | Kill authenticated role while real alert outstanding, panel reaped and next call works; old generation Cancel cannot close successor's panel | Drop native cancellation; typed wake still occurs but native census fails |
| S8 | Transport/Core: on one healthy link repeat **256+1 = 257** sequential timed-out `CALL(c_orig)` plus nonce-bound fresh `Cancel(c_cancel)`; observe separate ACK/`REPLY(c_cancel)` and exactly one typed `ERR(c_orig)` (or prior genuine `REPLY(c_orig)`), abandoned count returns to zero and next CALL succeeds. Native Mac: delay Host admission past local timeout, physically dismiss/suppress its panel within the proposed bound, enforce nonce/role fencing, and keep link-loss/terminal Host cleanup bounded | ACK `c_cancel` but omit `ERR(c_orig)`: the 257th abandoned original triggers `KELD-IPC-027`, even if every Cancel ACKs and every panel closes. Double-terminal the original or treat ACK as its disposal: exact correlation/once-only assertion fails. Independently drop native Cancel relay, restrict UI dispatch to default-mode only or synthesize clicked Cancel: visible panel/handle or wrong result fails |
| S9 | real clicked Cancel=1 while wrong terminal outcome is error | Treat kernel/role failure as ESC press; distinguishable witness fails |
| S10 | Unmodified draw.io dirty text survives Cancel/failed Save; correlate app Save-success ACK to destruction, then independently reopen the saved file and compare modified bytes | Treat the app ACK as disk proof, omit immediate preventDefault, cross-bind stale Save or skip the independent file read: fail |
| S11 | GH566 corpus flips only on real Mac pass, frozen lifecycle unchanged | Remove mapped test/keep implementing ticket after pass; validator fails |
| S12 | exact existing Host/Guardian/link limits, no global buffer growth | Inflate existing deadlines/add second link; contract and resource diff fail |
| S13 | Verify current Ready/grant/parent **before** triggering dirty-close, then force an after-flag dialog error: dirty window/bytes survive, repeat Close's known blocked state is recorded as unsupported | Pretend the failed dialog clicked Cancel or score a later close as working without the source-exact recovery adapter: negative fails |

Test fixture pins: Electron v44.4.5 and drawio-desktop
`2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`, webapp submodule
`24b76c2cbd55d88e354042e8d329a2e4708972bc`. Three-button
Save/Cancel/Discard each needs a real native click and separate trace.
The two-dirty-window quit sweep is #443/#451's **next-milestone** owner;
first proof uses one dirty window but must not regress its event ordering.

The real Mac harness must observe actual native panel visibility/click,
window identity, renderer dirty bytes, coherent timestamps, Bun/Host role
terminal status and a healthy subsequent operation. Mocked Rust/TypeScript
stubs alone are not acceptance. The external watchdog is only a kill
switch, never the source of Cancel. Negative controls restore production
bytes after failure and are retained as exact evidence. Every test is
reported with selected count and platform.

## 8. Review gates triggered

- **Public API:** yes — sync overloads, numeric result, typed errors and
  named/default Electron exports.
- **Permission model:** no new grant in this consumer; must independently
  verify that GH478 exact guard admission is reused, not bypassed.
- **Wire protocol:** GH478 owns the new channel/typed message schema;
  this consumer reuses it unchanged, but its exact consumer decode and
  generated constant need wire/receiver-contract review.
- **Unsafe:** none in the consumer; a GH478 native unsafe change needs its
  separately approved explicit allowlist and review, not inferred here.
- **Dependency:** no new package or library required by this spec.
- **Platform:** real macOS native + Bun/WorkerLink and Hosted CI, without
  converting Electron upstream oracle evidence into product proof.

## 9. Perf impact

A synchronous user prompt legitimately parks Bun's main JS, but the host
and transport Worker remain responsive. No RTT, speedup or RSS benefit is
claimed. Measure retained ring records/bytes, native panel lifetime,
cleanup tail and Host Quit latency with the real Mac workload. Do not
increase existing deadlines or activate extra GH527 T4 credit machinery
without the owner’s measured evidence. The transport still uses one
bounded queue and record order, no second buffering layer.

## 10. Open questions (must be closed before Status: approved)

1. **Shared dialog interaction and cancellation bounds:** accept or
   revise GH478's proposed **600,000 ms** distinct human UI budget,
   **10 s live-link local-timeout native close** and **5 s Host-observed
   role/link-loss native close**. Validate the proposed same-channel
   `Cancel { nonce }`, distinct Cancel and original CALL terminal frames,
   257 sequential abandoned-correlation release control, bounded
   tombstone and late-admission ordering on real macOS with GH478/GH527/Core
   owners. Local Worker `abandon` is
   insufficient; never replace or extend Host20/Guardian/KIPC clocks.
2. **GH478 platform-native completion:** approve the exact alert/parentless
   callback scheduling with measured UI-loop cleanup and any required objc2
   unsafe allowance; do not assume all native CFRunLoop modes execute the
   same callbacks.
3. **Pre-Ready synchronous API behavior:** confirm the proposed *synchronous
   typed not-ready refusal* before the existing singleton is connected,
   rather than allowing a call to invent a second connection or return a
   Promise. The first-proof normal draw.io caller runs after Ready.
4. **Native error-code allocation:** consume GH478's eventual accepted
   typed errors and KELD's canonical error registry, never reserve guessed
   `KELD-NATIVE-*` numeric codes in this draft.
5. **Effect-unknown application recovery:** the **source-backed first
   proof limitation is explicit**: pinned draw.io's app-owned
   `modifiedModalOpen` can remain true after a thrown dialog failure,
   and repeatable Close is **unsupported** then. Preflight avoids
   deterministic fixture misconfiguration; it cannot repair a later
   runtime error. The normal 0/1/2 path is required. The GH564/compat
   owner must open and approve a separate, source-exact app-recovery
   ticket before claiming repeatable Close after such errors; it may
   not synthesize Cancel, clear app-private state from Host or silently
   count this as a pass.
6. **Pinned message-argument compatibility:** approve GH478's typed
   admission of `type:'question'`, `title:'Confirm'` and macOS-inert
   `noLink:true` alongside the exact draw.io buttons and indexes.
   Run unchanged caller and invalid-type/unknown-field controls; until
   both specs agree and a native Mac panel reproduces the result,
   keep the GH477 synchronous cell expected-fail.

Approving this specification does **not** approve or implement GH478,
complete GH450 native close, integrate GH445/GH446 Ready, qualify
Windows/Linux, flip any corpus cell or authorize a merge. It permits
implementation **only after both approved dependency contracts** and the
winning source/OS claims are present.
