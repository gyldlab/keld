# Design

Part of: [index](index.md)
Owns: section 4

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
specified in AC-8. Neither
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
  allocate an additional ring or enable T-4 credit without measurements.
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
