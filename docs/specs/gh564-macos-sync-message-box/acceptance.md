# Spec: GH-564 acceptance
Part of: [index](index.md)
Owns: section 3, AC-1–AC-13 and existing S1–S13 contracts

## 3. Acceptance criteria: one independent oracle per criterion

### AC-1: S1 — actual clicked synchronous result

- **Given:** A ready admitted Bun main role has the verified dialog grant and a live owned Mac parent.
- **When:** The unchanged draw.io caller opens Save, Cancel and Discard Changes with exact options.
- **Then:** A real click returns primitive index 0, 1 or 2 before the next main statement.
- **Negative control:**
  - **Single fault:** Replace the native clicked index with the configured default index.
  - **At-risk precondition:** The native three-button dialog accepts a nondefault choice.
  - **Expected RED:** The independent clicked-index trace differs from the returned primitive.

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

### AC-2: S2 — parented and independent alert identity

- **Given:** A genuine current Host-owned window is visible or a documented hidden/absent parent is supplied.
- **When:** The caller requests a native synchronous alert using the approved parent semantics.
- **Then:** A live visible parent uses its sheet; full-target hidden/parentless fallback remains unqualified.
- **Negative control:**
  - **Single fault:** Retarget a stale or cross-app parent to an unrelated live window.
  - **At-risk precondition:** The current window registry has distinct native parent identities.
  - **Expected RED:** The native window ownership and panel census expose an incorrect parent.

**S2 — parented and independent dialogs.** A real visible, caller-owned
Host WindowRef displays one parent-attached macOS **sheet**. A request
without parent or with a valid hidden parent displays one independent
native alert. A stale/destroyed/forged/cross-app parent refuses without
retargeting. In all supported cases the return remains synchronous to Bun,
even if AppKit uses an asynchronous completion internally.

**Bounded slice status:** S2 remains open. Section 1.1 qualifies only the
visible-parent product case over the existing supplied-parent sheet;
hidden synchronous fallback and parentless presentation are not implemented.

### AC-3: S3 — one already-connected WorkerLink

- **Given:** The role owns one authenticated Ready singleton with a GH478 dialog grant.
- **When:** The sync facade issues the guarded dialog CALL and observes correlated completion.
- **Then:** Only the existing WorkerLink drains frames; pre-Ready access throws synchronously.
- **Negative control:**
  - **Single fault:** Create an extra Worker or perform a second HELLO for the dialog.
  - **At-risk precondition:** The role has an already-connected singleton before the dialog.
  - **Expected RED:** The one-link census or sync return-shape test fails.

**S3 — one existing transport owner.** `dialog.showMessageBoxSync` uses the
already **connected** role singleton in `packages/@keld/api`, and only
`WorkerLink.callBlocking` on the GH478 allocated `dialog` channel.
The existing Worker-owned socket and ring deliver one correctly correlated
REPLY or typed ERR. No new HELLO, endpoint, Worker, link, wake thread,
transport copy parser or per-dialog queue is created. Before the singleton
is authenticated/ready, calling the synchronous facade throws a typed
*not ready* error **synchronously**: it must not return a Promise or wait
for a newly started async connect.

### AC-4: S4 — no application callbacks while parked

- **Given:** A real native dialog is open and the main Bun role is blocked.
- **When:** Timers, IPC listeners and renderer tasks are scheduled during the park.
- **Then:** Main application callbacks remain quiescent; independent renderer and Worker progress continues.
- **Negative control:**
  - **Single fault:** Run a Bun main-process application listener before the native reply.
  - **At-risk precondition:** The modal remains visibly open while a callback becomes runnable.
  - **Expected RED:** The traced callback ordering or main-role quiescence assertion fails.

**S4 — listener isolation under park.** While the native synchronous alert
is up, **no Bun main-process application** timer, microtask, `ipcMain`
listener or second `close` listener executes. An independent renderer timer
continues. A positive asynchronous-dialog control *does* deliver main JS
callbacks while the panel is visible. After the native result, queued
host EVENTs and CALLs are delivered in original GH527 order; no lost,
duplicate or reentrant user listener. Other Bun Workers are separate and
are **not** asserted paused.

### AC-5: S5 — framework facts precede user continuation

- **Given:** More than 8 KiB of host facts can queue ahead of the correlated dialog reply.
- **When:** WorkerLink resumes a blocked synchronous call with framework state to apply.
- **Then:** Pre-reply mirror facts apply before return; user listeners run only on later main tasks.
- **Negative control:**
  - **Single fault:** Deliver the queued user listener during the synchronous return.
  - **At-risk precondition:** The real event stream precedes the native reply.
  - **Expected RED:** Mirror-versus-callback ordering assertions fail.

**S5 — framework state before user continuation.** When a host state fact
is queued **before** the dialog REPLY, the existing WorkerLink framework
state applier updates the mirror before `callBlocking` returns, **without
calling user code**. Event listeners run in order only on a later regular
main-event-loop task. State facts after REPLY are not applied as though
known at the reply. The real consumer must exercise an **over-8-KiB**
parked EVENT stream, reusing #528's tested ring rather than substituting a
single small mocked event.

### AC-6: S6 — Guard denial makes no native panel

- **Given:** The caller lacks the current exact app.system dialog grant or correct principal.
- **When:** A synchronous showMessageBox request reaches the existing GH478 route.
- **Then:** It throws canonical Guard denial and creates zero native panels.
- **Negative control:**
  - **Single fault:** Bypass the existing GH478 Guard check before native panel creation.
  - **At-risk precondition:** The denied request is otherwise syntactically valid.
  - **Expected RED:** The denial and zero-native-panel census both fail.

**S6 — denied grant never presents UI.** Without the exact verified
`app.system:["dialog"]` grant, with a non-app principal, unverified policy
or wrong generation, a call throws an existing canonical typed
`KELD-GUARD*` denial whose fix names `/app/system`. **Zero** AppKit panel
objects become visible/retained; the call cannot move through an
unguarded fallback or mint a path/FS scope. This is GH478's guard,
**not** a second facade-side permission evaluator.

### AC-7: S7 — native panel retired on role or parent loss

- **Given:** A genuine panel is open under an authenticated role and parent generation.
- **When:** The role, parent, or link terminates before a click.
- **Then:** GH478 dismisses the real panel once and never delivers to a successor generation.
- **Negative control:**
  - **Single fault:** Omit native dismissal on the terminal role-loss path.
  - **At-risk precondition:** The real panel outlives its role while the Host remains active.
  - **Expected RED:** Native panel census or the healthy next-panel operation fails.

**S7 — generation/parent/native custody.** During an outstanding real
panel, Bun role loss, Worker death, app link loss, destroyed parent or
Host terminal Quit physically dismisses/releases it through GH478's
UI-thread owner, with no callback to any successor or stale window and
no second answer. If the Host survives, the next admitted role can
open/close a fresh panel normally. A typed JS wake without physical
panel dismissal is **failure**. No numeric PID/window handle serves as
authority.

### AC-8: S8 — expiry retires both correlations without consent

- **Given:** One healthy WorkerLink has an admitted timed-out original CALL and separate Cancel nonce.
- **When:** The deterministic Core/transport test repeats 257 caller expiries and cancellations.
- **Then:** Each original gets one terminal disposition; abandoned count reaches zero and next CALL succeeds.
- **Negative control:**
  - **Single fault:** Acknowledge Cancel without terminating the abandoned original CALL.
  - **At-risk precondition:** The Cancel ACK is emitted while the Worker still tracks c_orig.
  - **Expected RED:** The 257th original triggers KELD-IPC-027 or the exact correlation census fails.

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

### AC-9: S9 — user Cancel differs from terminal failure

- **Given:** A real Mac panel has a configured cancelId and actual Cancel button.
- **When:** The user clicks Cancel or an independent terminal failure is injected.
- **Then:** Only a user click returns its numeric index; terminal failures throw typed errors.
- **Negative control:**
  - **Single fault:** Map a link-loss or role-death outcome to the clicked Cancel index.
  - **At-risk precondition:** The caller has an outstanding dialog when the role dies.
  - **Expected RED:** The user-click witness and terminal error trace disagree.

**S9 — user Cancel versus terminal failure.** A *real* macOS Cancel
button/Escape mapped by Electron's `cancelId` is the actual numeric
selected index (1 in draw.io's buttons). It is distinguishable from
closing a panel because the role died, a timer expired or the native
provider failed. None of those failure paths can impersonate Cancel.

### AC-10: S10 — actual draw.io dirty-close Save/Cancel/Discard

- **Given:** One unmodified pinned draw.io window is dirty under an admitted Mac role.
- **When:** The real close handler invokes the native sync panel and Save, Cancel or Discard.
- **Then:** Native identity and saved-file bytes match the separately witnessed application outcome.
- **Negative control:**
  - **Single fault:** Treat the application Save-success ACK as disk persistence without reopening bytes.
  - **At-risk precondition:** The app acknowledges Save but the file is an independently observable resource.
  - **Expected RED:** A real reopened file comparison or exact close-order trace fails.

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

### AC-11: S11 — single GH477/GH566 conformance owner

- **Given:** The exact Electron v44.4.5 mapped expected-fail sync cell is selected.
- **When:** A separately qualified real Mac implementation produces its lifecycle evidence.
- **Then:** The mapped cell flips only in the implementing diff after real product proof.
- **Negative control:**
  - **Single fault:** Remove the mapped expected-fail cell instead of proving it.
  - **At-risk precondition:** The canonical conformance validator checks that pinned cell.
  - **Expected RED:** The missing or unqualified corpus update fails validation.

**S11 — conformance lifecycle.** The implemented GH564 change inverts the
GH477 pinned v44.4.5 expected-fail synchronous cell and flips
`implementing_ticket: GH-564` to an actual pass **in the same diff**.
The one GH566 validator and the v44.3.0 lifecycle manifest stay unchanged.
Mac native result is separately source/CI qualified. Windows/Linux remain
unknown/unsupported; no product pass is inferred from a TS import test.

### AC-12: S12 — no inherited deadline or authority expansion

- **Given:** GH527 transport, Host, Guardian and FS deadlines are held fixed.
- **When:** The bounded human dialog call and its cleanup are exercised.
- **Then:** No existing deadline or authority widens; only the approved human budget applies.
- **Negative control:**
  - **Single fault:** Increase an existing Host or transport timeout for the dialog.
  - **At-risk precondition:** A genuine user interaction is slow while the service clocks remain bounded.
  - **Expected RED:** The exact deadline/source contract and resource test fail.

**S12 — no deadline or authority widening.** The existing transport, Host,
Guardian, Core shutdown and real FS-service deadlines are byte-identical
or behaviorally proven unchanged. The dialog-specific human clock must
fit within `MAX_BLOCKING_CALL_DEADLINE_MS`; no infinite wait, `sleep`
workaround, ambient permission, new global exception or unbounded buffer.

### AC-13: S13 — failed-dialog repeat-close remains explicitly blocked

- **Given:** Ready, grant and live owned parent preflight succeeded before dirty-close.
- **When:** A genuine error occurs after draw.io sets modifiedModalOpen.
- **Then:** No consent is forged and unsaved content remains; repeat-close stays unsupported.
- **Negative control:**
  - **Single fault:** Synthesize a Cancel click when the dialog actually fails.
  - **At-risk precondition:** The dirty-close handler has already set its application-private modal flag.
  - **Expected RED:** The native error/content trace or explicit known-fail repeat-close case fails.

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
