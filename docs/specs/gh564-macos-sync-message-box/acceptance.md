# GH-564 acceptance criteria
Part of: [index](index.md)
Owns: section 3

## 3. Acceptance criteria

### AC-1: actual synchronous value
- **Given:** An admitted Bun main role has the exact dialog grant and a live parent window.
- **When:** The user selects Save, Cancel or Discard in the real macOS panel.
- **Then:**
  With a ready admitted Bun main role,
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
- **Negative control:**
  - **Single fault:** Return defaultId instead of the actual clicked button index.
  - **At-risk precondition:** The real panel is shown and a non-default button is clicked.
  - **Expected RED:** The result differs from the native click trace or is not a primitive number.

### AC-2: parented and independent dialogs
- **Given:** The caller supplies a visible, hidden or absent Host-owned parent.
- **When:** The facade receives a request with a visible, hidden, absent or stale parent.
- **Then:**
  A real visible, caller-owned
  Host WindowRef displays one parent-attached macOS **sheet**. A request
  without parent or with a valid hidden parent displays one independent
  native alert. A stale/destroyed/forged/cross-app parent refuses without
  retargeting. In all supported cases the return remains synchronous to Bun,
  even if AppKit uses an asynchronous completion internally.

  **Bounded slice status:** AC-2 remains open. Section 1.1 qualifies only the
  visible-parent product case over the existing supplied-parent sheet;
  hidden synchronous fallback and parentless presentation are not implemented.
- **Negative control:**
  - **Single fault:** Attach the panel to a stale parent instead of the current visible parent.
  - **At-risk precondition:** A request names a hidden, absent, stale or cross-app parent.
  - **Expected RED:** The parent identity or required fallback/refusal oracle fails.

### AC-3: one existing transport owner
- **Given:** The role singleton is connected and authenticated, or is not yet ready.
- **When:** The synchronous facade is called before or after singleton readiness.
- **Then:**
  `dialog.showMessageBoxSync` uses the
  already **connected** role singleton in `packages/@keld/api`, and only
  `WorkerLink.callBlocking` on the GH478 allocated `dialog` channel.
  The existing Worker-owned socket and ring deliver one correctly correlated
  REPLY or typed ERR. No new HELLO, endpoint, Worker, link, wake thread,
  transport copy parser or per-dialog queue is created. Before the singleton
  is authenticated/ready, calling the synchronous facade throws a typed
  *not ready* error **synchronously**: it must not return a Promise or wait
  for a newly started async connect.
- **Negative control:**
  - **Single fault:** Create a second WorkerLink or wait for a new asynchronous connection.
  - **At-risk precondition:** The existing singleton is ready or is not ready.
  - **Expected RED:** The one-link count or synchronous not-ready refusal fails.

### AC-4: listener isolation under park
- **Given:** A real native synchronous alert is visible.
- **When:** The native panel parks the Bun main application event loop.
- **Then:**
  While the native synchronous alert
  is up, **no Bun main-process application** timer, microtask, `ipcMain`
  listener or second `close` listener executes. An independent renderer timer
  continues. A positive asynchronous-dialog control *does* deliver main JS
  callbacks while the panel is visible. After the native result, queued
  host EVENTs and CALLs are delivered in original GH527 order; no lost,
  duplicate or reentrant user listener. Other Bun Workers are separate and
  are **not** asserted paused.
- **Negative control:**
  - **Single fault:** Allow one Bun main-process callback while the panel is visible.
  - **At-risk precondition:** The real synchronous panel is open.
  - **Expected RED:** The main-process event trace records a callback before the result.

### AC-5: framework state before user continuation
- **Given:** A host state fact is queued before the dialog reply.
- **When:** WorkerLink.callBlocking receives the reply and queued host events.
- **Then:**
  When a host state fact
  is queued **before** the dialog REPLY, the existing WorkerLink framework
  state applier updates the mirror before `callBlocking` returns, **without
  calling user code**. Event listeners run in order only on a later regular
  main-event-loop task. State facts after REPLY are not applied as though
  known at the reply. The real consumer must exercise an **over-8-KiB**
  parked EVENT stream, reusing #528's tested ring rather than substituting a
  single small mocked event.
- **Negative control:**
  - **Single fault:** Deliver a user listener before callBlocking returns.
  - **At-risk precondition:** A host fact is queued before the correlated dialog reply.
  - **Expected RED:** The mirror and listener ordering assertion fails.

### AC-6: denied grant never presents UI
- **Given:** The caller has a missing grant, invalid principal, unverified policy or wrong generation.
- **When:** A synchronous dialog request reaches the Host.
- **Then:**
  Without the exact verified
  `app.system:["dialog"]` grant, with a non-app principal, unverified policy
  or wrong generation, a call throws an existing canonical typed
  `KELD-GUARD*` denial whose fix names `/app/system`. **Zero** AppKit panel
  objects become visible/retained; the call cannot move through an
  unguarded fallback or mint a path/FS scope. This is GH478's guard,
  **not** a second facade-side permission evaluator.
- **Negative control:**
  - **Single fault:** Bypass the existing Guard check before opening the panel.
  - **At-risk precondition:** The exact dialog grant or authenticated app principal is absent.
  - **Expected RED:** The canonical denial or zero-panel census fails.

### AC-7: generation/parent/native custody
- **Given:** A real native panel is outstanding for the current role and parent.
- **When:** The Bun role, Worker, app link, parent or Host reaches a terminal state.
- **Then:**
  During an outstanding real
  panel, Bun role loss, Worker death, app link loss, destroyed parent or
  Host terminal Quit physically dismisses/releases it through GH478's
  UI-thread owner, with no callback to any successor or stale window and
  no second answer. If the Host survives, the next admitted role can
  open/close a fresh panel normally. A typed JS wake without physical
  panel dismissal is **failure**. No numeric PID/window handle serves as
  authority.
- **Negative control:**
  - **Single fault:** Skip physical panel dismissal when the owning role is lost.
  - **At-risk precondition:** A real panel is outstanding during role or parent retirement.
  - **Expected RED:** The native panel census or successor-isolation check fails.

### AC-8: failure and expiry do not forge consent
- **Given:** A dialog call can expire, fail or lose its effect before a terminal result.
- **When:** The interaction deadline expires or the native operation fails.
- **Then:**
  A denied, stale,
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
- **Negative control:**
  - **Single fault:** Treat the Cancel acknowledgement as the original CALL terminal.
  - **At-risk precondition:** The original call expires while its Host operation is still pending.
  - **Expected RED:** The 257-expiry abandoned-correlation test fails.

### AC-9: user Cancel versus terminal failure
- **Given:** The user or native provider can close the dialog.
- **When:** The user clicks Cancel, presses Escape, or the provider fails.
- **Then:**
  A *real* macOS Cancel
  button/Escape mapped by Electron's `cancelId` is the actual numeric
  selected index (1 in draw.io's buttons). It is distinguishable from
  closing a panel because the role died, a timer expired or the native
  provider failed. None of those failure paths can impersonate Cancel.
- **Negative control:**
  - **Single fault:** Map a terminal provider or role failure to the Cancel button index.
  - **At-risk precondition:** A native failure, role loss or expiry occurs without a user click.
  - **Expected RED:** The typed error is not distinguishable from a real user selection.

### AC-10: exact draw.io integration
- **Given:** The pinned unmodified draw.io app is running under explicit Legacy.
- **When:** The user closes a dirty document and chooses a native dialog action.
- **Then:**
  On real Mac under explicit legacy,
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
- **Negative control:**
  - **Single fault:** Destroy the dirty window from the button index without the app Save-success acknowledgement.
  - **At-risk precondition:** The real draw.io dirty-close flow reaches Save or Discard.
  - **Expected RED:** The correlated app event or independent saved-file readback fails.

### AC-11: conformance lifecycle
- **Given:** The GH477 mapped synchronous cell is still expected to fail.
- **When:** The implementing change updates the conformance corpus.
- **Then:**
  The implemented GH564 change inverts the
  GH477 pinned v44.4.5 expected-fail synchronous cell and flips
  `implementing_ticket: GH-564` to an actual pass **in the same diff**.
  The one GH566 validator and the v44.3.0 lifecycle manifest stay unchanged.
  Mac native result is separately source/CI qualified. Windows/Linux remain
  unknown/unsupported; no product pass is inferred from a TS import test.
- **Negative control:**
  - **Single fault:** Mark the GH477 corpus cell as passing without native proof.
  - **At-risk precondition:** The conformance validator evaluates the implementing ticket.
  - **Expected RED:** The validator or evidence gate rejects the claimed pass.

### AC-12: no deadline or authority widening
- **Given:** The existing transport, Host, Guardian, Core and FS-service deadlines apply.
- **When:** The dialog consumer is integrated and tested.
- **Then:**
  The existing transport, Host,
  Guardian, Core shutdown and real FS-service deadlines are byte-identical
  or behaviorally proven unchanged. The dialog-specific human clock must
  fit within `MAX_BLOCKING_CALL_DEADLINE_MS`; no infinite wait, `sleep`
  workaround, ambient permission, new global exception or unbounded buffer.
- **Negative control:**
  - **Single fault:** Increase an existing Host, Guardian, Core or frame deadline.
  - **At-risk precondition:** The exact implementation diff and deadline controls run.
  - **Expected RED:** The byte/behavioral deadline comparison fails.

### AC-13: preflight and fail-closed error recovery limitation
- **Given:** The real dirty-close harness has passed its authenticated Ready, grant and parent preflight.
- **When:** A genuine denial, expiry or link failure occurs after the app sets its modal flag.
- **Then:**
  Before the
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
- **Negative control:**
  - **Single fault:** Return Cancel after a genuine dialog failure.
  - **At-risk precondition:** The app has already set modifiedModalOpen and the dialog fails.
  - **Expected RED:** The dirty document survives but the known repeat-close limitation is not recorded.
