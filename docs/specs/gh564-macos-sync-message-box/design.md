# Spec: GH-564 ownership and design
Part of: [index](index.md)
Owns: section 4 and six original design subsections

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

The current `@keld/api/src/app.ts` already owns `connectedLink`,
`linkPromise`, `hostReady` and sticky `linkDead`. Its
`onConnected` callback installs the same `LifecycleLink` before runtime
initialization/subscription and public Ready finish. Conversely,
`hostReady` can remain true after link death. Neither a cached instance
nor `app.isReady()` alone proves usable admission.

Add only these package-private typed consumer methods:

```ts
// packages/@keld/api/src/app.ts: existing singleton owner
export function invokeMessageDialogBlocking(
  parent: WindowRef | null,
  options: MessageOptions,
): DialogReplyV1;

// packages/@keld/api/src/link.ts: same connected LifecycleLink
callMessageDialogBlocking(
  parent: WindowRef | null,
  options: MessageOptions,
): DialogReplyV1;
```

The API entry checks the existing Ready state and preserves the sticky
terminal cause. Absent or connected-but-not-Ready sessions refuse
synchronously with existing `KELD-CORE-040`, without starting
`whenReady()`, `ensureLink()` or another connection. Closed/quitting
sessions retain the existing dialog owner's typed refusal. WorkerLink's
terminal-state check remains authoritative even before an `onEnd`
listener has run.

The new link method uses the same nonce allocator, generated Message
codec, `DIALOG_CHANNEL`, interaction deadline, expected-reply validation
and timeout-to-Cancel policy as the existing asynchronous dialog methods.
Share those decisions in the existing private dialog owner; do not copy
a second policy. Only the actual transport invocation differs:
`WorkerLink.callBlocking` returns the original reply bytes directly.
An `async` function or Promise cannot wrap this synchronous method.

The Electron overloads preserve `readDialogParentRef` before
`normalizeMessageOptions`, then require a Message reply and return
its actual numeric `response`. A cleanup ACK is not a clicked index.
The existing index already exports the identical `dialog` object on
named and default exports, so this slice needs no new index owner.

### 4.3 Native UI, lifecycle and result identity

GH478 owns Guard, native parent resolution, NSAlert/sheet creation,
UI-thread native closure and callback cancellation. The bounded consumer
retains its supplied-parent sheet branch; it adds no visibility selector.
GH564 asks only for the **message-panel** operation (not file-open/file-save). `WindowRef`,
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
- Interaction deadline: reuse GH478's already-approved `600_000` ms
  human budget unchanged. It is finite, below existing
  `MAX_BLOCKING_CALL_DEADLINE_MS=86_400_000`, and is **not** the
  `APP_LINK_IO_DEADLINE_MS` (currently 5,000 ms for frame I/O).
  Do **not** extend Host20/Guardian10/Cleanup5/KIPC policy clocks.
- Caller deadline and native deadline are measured on distinct clocks.
  `WorkerLink.callBlocking` currently sends `abandon` only to its
  transport Worker; a `KELD-IPC-006` throw does not dismiss AppKit.
  Before the normal blocking CALL, GH564's existing-link adapter must
  generate one bounded nonce in the shared GH478 request payload.
  If `callBlocking` expires, that same adapter posts GH478's existing
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
  The approved observable limits are **10 s from local timeout while
  link remains live** and **5 s from Host-observed role/link loss**.
  Measured delivery and physical cleanup still require the GH478/Core
  acceptance gates, including forced-late-admission controls. Current
  `LifecycleLink` catches a rejected Cancel Promise; an ordinary
  Cancel-call timeout does not itself prove link retirement. Native's
  independent 600 s timer starts at Host admission. Neither fact proves
  the 10 s caller-clock bound. Withheld/delayed Cancel and original
  admission must therefore be exercised before claiming this criterion.
  Failed bounded cleanup remains GH478's existing fatal/custody-owner
  obligation; do not add a consumer deadline, silently waive the bound,
  or fabricate Cancel. A Cancel request/ACK without native release fails.
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
