# Integration and verification

Part of: [index](index.md)
Owns: integration and verification.

## 4. Design: Integration and evidence

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


## 5. Boundaries

Original section scope: Boundaries.

Before §1.1's independent exact-content review and owner receipt, this
amendment permits only assigned documentation and failure-first tests.
After that receipt, the bounded production implementation belongs only in:

- `packages/@keld/api/src/app.ts`: synchronous access to the existing
  Ready singleton and its sticky terminal state.
- `packages/@keld/api/src/link.ts`: blocking Message on the existing
  WorkerLink, shared nonce/codec/deadline/result/cancellation policy.
- `packages/@keld/electron/src/dialog.ts`: existing brand/options
  admission and actual synchronous numeric result mapping.

`src/index.ts` already shares the dialog object. GH478's existing
Native/Core/WV owners and generated dialog codec are consumed unchanged;
this consumer does not implement the missing parentless/hidden fallback.
The mapped GH477 test/corpus may change only after its declared real
native proof; source-only tests cannot qualify a broader cell.

MUST NOT modify `packages/@keld/kipc/src/transport.ts`, GH527/GH531
specs, Host/Core/WV/Native production, Guard policy, channel/schema/error
allocation, generated codec, Windows/Linux production, Guardian timeouts,
CI policy or root dependencies. Preserve sibling async behavior while
sharing its existing private policy. Any further owner change needs its
own assignment and review; this bounded receipt cannot authorize it.


## 6. Tasks

Original section scope: Tasks (one concern per held PR; ordered dependencies).

Original ordered implementation dependencies remain below. Existing fail-closed holds apply immediately,
not only after another task completes. T-3 through T-6 retain T-1's required exact-content approval receipt before production work.
That prerequisite does not block permitted test preparation.

### T-1: Legacy T1

- **Blocked by:** none

- [ ] T1: obtain §1.1's independent exact-content review and owner receipt
      for the bounded GH564 consumer; consume the existing approved GH478
      v2 contract without re-approving it. Full-target approval remains open.
### T-2: Legacy T2

- **Blocked by:** none

- [ ] T2: consume the current GH477 mapped expected-fail sync cell;
      keep it fail before qualified implementation, no duplicate
      parser/denominator and no 44.3.0 lifecycle re-pin.
### T-3: Legacy T3

- **Blocked by:** T-1

- [ ] T3: add a private **connected-link** sync access on the existing
      singleton/WorkerLink, and tests that distinguish absent/connecting/
      live/terminal states without a second HELLO.
### T-4: Legacy T4

- **Blocked by:** T-1

- [ ] T4: consume GH478's guarded real NSAlert operation, actual parent and
      clicked-index mapping, bounded native cleanup on role loss,
      local `callBlocking` expiry and late Host admission, plus both the
      distinct Cancel ACK and original-correlation terminal obligations.
### T-5: Legacy T5

- **Blocked by:** T-1

- [ ] T5: compose the sync UI result and GH527 parked event/mirror semantics
      with >8KiB incoming events; test native default-only completion and
      actual Host Quit/role loss separately.
### T-6: Legacy T6

- **Blocked by:** T-1

- [ ] T6: flip GH477's single implementing-ticket cell to pass **only in
      this implementing diff** and prove real unmodified draw.io
      Close→Cancel/Save/Discard acceptance on a Mac, with independently
      reopened saved content and a clearly labelled unsupported error-
      recovery repeat-close scenario.
### T-7: Legacy T7

- **Blocked by:** none

- [ ] T7: run actual final full local/hosted required CI, CodeRabbit or
      independent nonauthor exact-head review, permission/public API/wire
      review and OS acceptance before any merge or compatibility score.

The broader draft target is not an implementation authorization. Only
§1.1's specifically reviewed and recorded bounded slice may proceed after
its owner receipt. No receipt is claimed by this pending amendment.
Do not mark this issue done when T3/T4 or a source fixture passes.


## 7. Test plan & evidence

Original section scope: Failing-first test plan.

The bounded source regressions are
`packages/@keld/electron/src/dialog-sync-consumer.test.ts` and
`packages/@keld/api/src/dialog-sync-consumer.test-role.ts`. Their eight
isolated child cases cover unconnected and connected-before-Ready
refusal, exact source options and 0/1/2 primitive mapping, brand-first
admission, parentless refusal, sticky terminal state despite Ready,
Quit admission, original typed-error preservation and wrong-reply refusal.
They control only the LifecycleLink Host seam and establish no native
click, Guard, visibility, WorkerLink park or physical-cleanup acceptance.
They were prepared before production and are **unrun** in this amendment.

Keep the existing unsupported-only test unchanged until stronger real
native positive and negative proof permits its explicit bounded update.
A 257-cycle successful-click test does not satisfy S8's 257 local expiries.
Root owns actual Bun/Cargo/native execution and must retain the observed
RED, corrected GREEN and independent mutation failures. S2 and S13 remain
unqualified as stated; no source-only result flips full compatibility.


- **AC:** S1
- **Positive observable:** Submit the exact draw.io `type:'question'`, `title:'Confirm'`, `noLink:true`
  object, observe actual macOS 0/1/2 clicks; sync primitive and no Promise
- **Independent negative control:** Reject inert `noLink`, drop `title` or substitute Promise/defaultId: real
  source admission or non-default click fails

- **AC:** S2
- **Positive observable:** visible parent sheet and hidden/absent fallback
- **Independent negative control:** Stale WindowRef attaches to recycled parent and fails

- **AC:** S3
- **Positive observable:** one cached authenticated realm link; pre-Ready throws synchronously
- **Independent negative control:** Open second Worker/HELLO or await Promise; one-link/return-shape test
  fails

- **AC:** S4
- **Positive observable:** main JS suppressed during real modal; async positive control interleaves
- **Independent negative control:** Execute a timer/IPC listener while parked and parity trace fails

- **AC:** S5
- **Positive observable:** real >8KiB queued facts, pre-reply state applied before return, listeners later
- **Independent negative control:** Dispatch listener during wake or drop last EVENT; exact ordering fails

- **AC:** S6
- **Positive observable:** exact guard denial, native panel creation census zero
- **Independent negative control:** Remove guard before UI and denial/no-panel fails

- **AC:** S7
- **Positive observable:** Kill authenticated role while real alert outstanding, panel reaped and next call
  works; old generation Cancel cannot close successor's panel
- **Independent negative control:** Drop native cancellation; typed wake still occurs but native census fails

- **AC:** S8
- **Positive observable:** Transport/Core: on one healthy link repeat **256+1 = 257** sequential timed-out
  `CALL(c_orig)` plus nonce-bound fresh `Cancel(c_cancel)`; observe separate ACK/`REPLY(c_cancel)` and exactly
  one typed `ERR(c_orig)` (or prior genuine `REPLY(c_orig)`), abandoned count returns to zero and next CALL
  succeeds. Native Mac: delay Host admission past local timeout, physically dismiss/suppress its panel within
  the approved bound, enforce nonce/role fencing, and keep link-loss/terminal Host cleanup bounded
- **Independent negative control:** ACK `c_cancel` but omit `ERR(c_orig)`: the 257th abandoned original
  triggers `KELD-IPC-027`, even if every Cancel ACKs and every panel closes. Double-terminal the original or
  treat ACK as its disposal: exact correlation/once-only assertion fails. Independently drop native Cancel
  relay, restrict UI dispatch to default-mode only or synthesize clicked Cancel: visible panel/handle or wrong
  result fails

- **AC:** S9
- **Positive observable:** real clicked Cancel=1 while wrong terminal outcome is error
- **Independent negative control:** Treat kernel/role failure as ESC press; distinguishable witness fails

- **AC:** S10
- **Positive observable:** Unmodified draw.io dirty text survives Cancel/failed Save; correlate app
  Save-success ACK to destruction, then independently reopen the saved file and compare modified bytes
- **Independent negative control:** Treat the app ACK as disk proof, omit immediate preventDefault, cross-bind
  stale Save or skip the independent file read: fail

- **AC:** S11
- **Positive observable:** GH566 corpus flips only on real Mac pass, frozen lifecycle unchanged
- **Independent negative control:** Remove mapped test/keep implementing ticket after pass; validator fails

- **AC:** S12
- **Positive observable:** exact existing Host/Guardian/link limits, no global buffer growth
- **Independent negative control:** Inflate existing deadlines/add second link; contract and resource diff
  fail

- **AC:** S13
- **Positive observable:** Verify current Ready/grant/parent **before** triggering dirty-close, then force an
  after-flag dialog error: dirty window/bytes survive, repeat Close's known blocked state is recorded as
  unsupported
- **Independent negative control:** Pretend the failed dialog clicked Cancel or score a later close as working
  without the source-exact recovery adapter: negative fails

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


## 9. Perf impact

Original section scope: Perf impact.

A synchronous user prompt legitimately parks Bun's main JS, but the host
and transport Worker remain responsive. No RTT, speedup or RSS benefit is
claimed. Measure retained ring records/bytes, native panel lifetime,
cleanup tail and Host Quit latency with the real Mac workload. Do not
increase existing deadlines or activate extra GH527 T4 credit machinery
without the owner’s measured evidence. The transport still uses one
bounded queue and record order, no second buffering layer.


## 10. Open questions, blockers & unknowns

Original section scope: Open questions (must be closed before full-target Status: approved).

The full target remains draft. For §1.1, the selected design reuses the
approved GH478 v2 clocks, Guard, Message/Cancel schema, registered native
errors and source options. These are existing owner decisions, not new
questions for repeat approval. Independent review of this canonical
bounded amendment and the recorded owner receipt are still pending.
The historical questions below remain full-target acceptance work where
not answered by that owning contract; they do not authorize new Native,
protocol or application-recovery work.

1. **Shared dialog interaction and cancellation bounds:** reuse GH478's
   approved **600,000 ms** distinct human UI budget, **10 s live-link
   local-timeout native close** and **5 s Host-observed role/link-loss
   native close**. Validate the existing same-channel
   `Cancel { nonce }`, distinct Cancel and original CALL terminal frames,
   257 sequential abandoned-correlation release control, bounded
   tombstone and late-admission ordering on real macOS with GH478/GH527/Core
   owners. Local Worker `abandon` is
   insufficient; never replace or extend Host20/Guardian/KIPC clocks.
2. **GH478 platform-native completion:** the bounded consumer uses the
   current supplied-parent sheet. Parentless and synchronous hidden-parent
   fallback remain open in their native owner, with measured UI-loop cleanup
   and any required independent unsafe review. No visibility inference or
   unqualified CFRunLoop scheduling claim fills that gap.
3. **Pre-Ready synchronous API behavior:** confirm the proposed *synchronous
   typed not-ready refusal* before the existing singleton is connected,
   rather than allowing a call to invent a second connection or return a
   Promise. The first-proof normal draw.io caller runs after Ready.
4. **Native error-code allocation:** consume GH478's existing
   `KELD-NATIVE-009` through `KELD-NATIVE-016` outcomes. They are registered
   in `docs/engineering/keld-error-codes.md` at the owning PR1306 source
   `16e519091992d551853b6d0434b85000499a3d3c`. The inspected integrating
   tree has those native source outcomes but lacks these eight registry
   sections. Its designated root owner must reconcile the existing rows
   before claiming code/spec consistency; this consumer allocates no code
   and this amendment does not edit that separately owned registry.
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
6. **Pinned message-argument compatibility:** reuse GH478's approved
   admission of `type:'question'`, `title:'Confirm'` and macOS-inert
   `noLink:true` alongside the exact draw.io buttons and indexes.
   Run unchanged caller and invalid-type/unknown-field controls; until
   both specs agree and a native Mac panel reproduces the result,
   keep the GH477 synchronous cell expected-fail.

A bounded receipt under §1.1 does **not** approve the full GH564 target,
complete GH450 native close, qualify Windows/Linux, flip a corpus cell,
resolve formal review or authorize a merge. It permits only the three-file
consumer after independent exact-content approval, the existing approved
dependency contract and winning source/OS assignments. Native acceptance,
generated-doc gates and broader issue completion remain separately required.

Structural migration does not close or approve the original open decisions and execution blockers above.
