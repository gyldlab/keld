# Spec: macOS synchronous message box: Source requirements and acceptance

Part of: [index](index.md)
Owns: Existing WorkerLink contract and synchronous acceptance

## 2. Spec refs

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
- GH478's shared native broker contract is **approved** by the
  [October 28 owner-delegated v2 receipt](https://github.com/gyldlab/keld/issues/478#issuecomment-6476354568),
  input SHA-256 `a3137e6d826d93788845b57c4295917b34be54b61f7fcc939903ab3432fc73c2`.
  The approved text is present at
  [PR1306's exact source](https://github.com/gyldlab/keld/blob/16e519091992d551853b6d0434b85000499a3d3c/docs/specs/gh478-guarded-macos-dialog-broker.md).
  The integrating tree now contains the generated Message/Cancel codec,
  guarded Core route, native sheet owner and shared nonce cancellation.
  Older PR1161 draft descriptions are not the current approval receipt.
  Contract approval and source presence do not establish this consumer's
  native acceptance. Use the generated channel constant and existing Guard.
- GH532/GH566 single conformance owner; the current
  `crates/keld-compat/fixtures/native-corpus/corpus.json` maps the sync
  expected-fail case to `packages/@keld/electron/src/dialog.test.ts`.
  Historical [GH477 PR1156](https://github.com/gyldlab/keld/pull/1156)
  receipts do not prove the new synchronous consumer.
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

No architecture amendment to GH527/GH531 is authorized. GH478's approved
broker contract does not approve its GH564 consumer. The bounded consumer
needs the separate receipt in §1.1; the broader target remains draft.
Resolve any conflict with the approved native broker in that owner before
expanding the consumer's scope.

## 3. Acceptance criteria

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

**Bounded slice status:** S2 remains open. Section 1.1 qualifies only the
visible-parent product case over the existing supplied-parent sheet;
hidden synchronous fallback and parentless presentation are not implemented.

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
dismissal; the GH478 same-link nonce/Cancel owner must satisfy its
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

### AC-1: Original showMessageBoxSync returns the native selection synchronously

- **Given:** The admitted original Bun caller and its authenticated WorkerLink have a bounded dialog request.
- **When:** The original main process calls showMessageBoxSync and the Host broker presents the native dialog.
- **Then:** The native button index returns to the same Bun call before it continues.
- **Negative control:** Change one native reply correlation while keeping the call active.
- **Single fault:** Replay a reply with another caller's nonce and request identity.
- **At-risk precondition:** The original synchronous call is waiting for the Host dialog result.
- **Expected RED:** The existing link rejects the foreign reply and does not resume the caller with its index.
