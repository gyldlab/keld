# Spec: Cancelable two-phase application quit
Status: draft
Linear: GH-451 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Define the full macOS quit transaction: synchronous before-quit intent, per-window veto, will-quit veto and one host-owned
terminal exit. A dirty document must survive every vetoing route.

Non-goals: Updater quitAndInstall, app.exit, renderer beforeunload parity, Windows shutdown and Linux qualification remain
separate. GH563 owns the menu adapter.

## 2. Spec refs

- [Owning issue GH-451](https://github.com/gyldlab/keld/issues/451) and its latest public owner decisions.
- [Architecture
05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md):
native ownership, engine boundary and capability separation.
- [Current
facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Close
owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh531-window-registry-close-state-machine.md).
- [Lifecycle
owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/lifecycle.rs).
- [Electron browser state
machine](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/browser.cc).
- [Observed quit/veto sequence](https://github.com/gyldlab/keld/issues/451#issuecomment-6381751800).
- [Scoped D2 decision, preserved](https://github.com/gyldlab/keld/issues/451#issuecomment-6561125711).

Verified baseline: Current main app.quit returns a Promise and refuses live windows with KELD-COMPAT-012. It does not implement
this full transaction. The public D2 comment approves a narrower local preflight; it is not proof of full native, multiwindow or
Dock behavior.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a before-quit listener that vetoes, when quit is requested, then it runs once and no window close or terminal host action
occurs.

Independent negative control: Remove the before-quit veto branch; assert the first native-close counter becomes nonzero.

### AC2

Given two clean windows, when one quit cycle starts, then each receives its own GH531 close request; will-quit and quit follow
actual terminal window receipts, and window-all-closed is suppressed only while quitting stays active.

Independent negative control: Complete the app transaction from close-request issuance instead of destruction receipts.

### AC3

Given a veto from either window in the two-window oracle fixture, when the sweep runs, then the other window still receives its
request and active quitting resets. A later explicit last-window destruction emits window-all-closed.

Independent negative control: Stop the sweep at the first veto, or retain active quitting after that veto; each mutation has a
separate assertion.

### AC4

Given draw.io's veto, asynchronous save and destroy sequence, when the final window closes, then window-all-closed can trigger a
second app.quit and a second before-quit, followed by will-quit and quit.

Independent negative control: Use remembered application intent as the suppression flag; the second before-quit must disappear and
fail.

### AC5

Given zero windows and a will-quit veto, when quit is requested again later, then the host is still alive and
before-quit/will-quit re-run without invented close events.

Independent negative control: Leave the application permanently in Quitting after a will-quit veto.

### AC6

Given Dock or another non-menu OS quit source and a dirty window, when quit is requested, then it reaches the same veto owner and
never directly terminates the host.

Independent negative control: Allow the native terminate callback to bypass the authenticated quit-intent path.

### AC7

Given a nested quit during a current synchronous callback, when it re-enters the facade, then it coalesces with that active
attempt and the caller does not park waiting for native close/save work.

Independent negative control: Await host shutdown on the calling JS stack or create a second overlapping sweep.

## 4. Design

### 4.1 One transaction owner

Proposed phases are Idle, BeforeQuit, Closing, WillQuit and Terminal. GH531 remains the sole window-close owner. The app
coordinator stores only the active attempt and outstanding registry identities; it does not invent another window registry or a
second close timer. Veto cancels active quitting immediately, while already selected sweep entries still receive their close
request.

### 4.2 Source and ordering

Facade intent emits before-quit synchronously, then uses the existing authenticated Main link. Host/OS intent enters that same
coordinator. Do not infer full parity from a JS-only state model. The pinned two-window oracle observes W2 then W1; capture
creation order and native receipts so a future backend can reproduce or explicitly record a divergence.

### 4.3 Return contract and recovery

Proposed Electron-compatible target is nonblocking app.quit(): void. Current Promise-return callers and the scoped D2 contract
require a joint public-API migration decision before this replacement is enabled. Nested calls during an active callback coalesce;
a new call after veto creates a new attempt. A retired role cannot answer or restart an old attempt; loss-of-role disposition
stays with lifecycle/recovery owners.

### 4.4 Native terminal boundary

Only the ordered host terminal owner may end the process. Native terminate cancellation versus delayed reply remains the
GH443/AppKit experiment gate; neither is presumed equivalent. A close veto or will-quit veto keeps the host alive.
Listener-exception disposition must match the approved D2/public API decision, with no accidental fallthrough to termination.

### 4.5 Contract integration

A host-to-app quit-intent payload must reuse the existing lifecycle schema and its single allocation ledger. Preserve the scoped
GH563 QuitRequested decision; do not independently allocate another tag. KEL-96 pre-Quitting and suppression wording must be
amended by its owner before implementation approval. No architecture changes are approved by this draft.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation spans the Electron app facade, Core lifecycle coordinator and existing macOS native quit adapter. Do not
rewrite GH531, invent another transport, alter deadlines or edit scoped unpublished work.

This publication changes only `docs/specs/gh451-cancelable-two-phase-quit.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Record pending conformance cells and literal phase/order fixtures, including the veto-reset and second-quit oracle; preserve
scoped D2 behavior until the joint contract is approved.

### T2

- [ ] Implement one facade-to-host quit transaction together with the GH531 per-window sweep and zero-window will-quit branch;
include every veto negative control.

### T3

- [ ] Wire the qualified non-menu native quit adapter and retirement handling into the same coordinator; reject bypasses and stale
intent.

### T4

- [ ] Run real macOS clean/dirty/two-window/zero-window fixtures and original draw.io save/cancel/retry; move only this issue's
evidenced cells to pass.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-451;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use a fake authenticated link for deterministic sequencing, then real macOS process/window receipts. A model cannot establish Dock
delivery or natural process exit. Observe native window identity, role generation and terminal cause separately; drive
acknowledgements rather than sleeping.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and wire protocol; lifecycle/KEL-96 owner amendment. Unsafe review only if the selected native adapter adds a
sanctioned FFI call.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

One bounded snapshot of existing registry IDs per active quit. Measure UI dispatch latency and retained entries during a vetoed
sweep; no latency or startup improvement is claimed.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve the full API return/exception migration against the scoped D2 decision.
2. Select the qualified AppKit terminate strategy using GH443 and approve the KEL-96 active-quitting amendment.
3. Approve the single lifecycle payload allocation and exact role-loss disposition before enabling full quit.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

