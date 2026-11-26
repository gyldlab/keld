# Spec: Cancelable two-phase application quit
- Status: draft
- Tracker: GH-451
- Owner: @0monish (delegated design choices recorded; joint approval pending)
- Updated: 2026-11-26
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

Original tracking note: GH-451 (#517); joint implementation admission remains pending.
Publication: grouped future implementation pack; docs only, do not merge.
The 26 November technical council selected the conditional Host-owned design. The user delegated GH-451 design decisions
to the council; the agent-authored decision record selects the GH-451-side semantics below. It is not a substitute for
independent human approval, joint owner amendments, actual runtime implementation or native acceptance.

## 1. Goal & non-goals

Define the full macOS quit transaction: synchronous before-quit intent, per-window veto, will-quit veto and one host-owned
terminal exit. A dirty document must survive every vetoing route.

Non-goals: Updater quitAndInstall, app.exit, renderer beforeunload parity, Windows shutdown and Linux qualification remain
separate. GH563 owns the menu adapter.

## 2. Spec refs

- [Owning issue GH-451](https://github.com/gyldlab/keld/issues/451) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Close owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh531-window-registry-close-state-machine.md).
- [Lifecycle owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/lifecycle.rs).
- [Electron browser state machine](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/browser.cc).
- [Observed quit/veto sequence](https://github.com/gyldlab/keld/issues/451#issuecomment-6381751800).
- [Scoped D2 decision, preserved](https://github.com/gyldlab/keld/issues/451#issuecomment-6561125711).
- [W7 terminal review candidate](https://github.com/gyldlab/keld/blob/eb7a18b42ef7e8213a280ec9c76015bf78a0964f/docs/engineering/gh451-terminal-commit-review-w7.md).
- [GH-523 draft AC-7 and shared terminal owner](https://github.com/gyldlab/keld/blob/eb7a18b42ef7e8213a280ec9c76015bf78a0964f/docs/specs/gh523-host-authorized-app-exit.md#L98-L126).
- [User-delegated GH-451 design selection](https://github.com/gyldlab/keld/issues/451#issuecomment-7182471840).

Verified baseline: Current main app.quit returns a Promise and refuses live windows with KELD-COMPAT-012. It does not implement
this full transaction. The public D2 comment approves a narrower local preflight; it is not proof of full native, multiwindow or
Dock behavior.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary; selected GH-451 semantics do not authorize joint contract, schema or permission changes.

## 3. Acceptance criteria

### AC-1: Before-quit veto

- **Given:** a before-quit listener that vetoes
- **When:** quit is requested
- **Then:** it runs once and the canceled Quit issues no window close or terminal Host action. A separately committed
  authorized terminal operation is governed by the shared winner contract, not by this veto.
- **Negative control:** Remove the before-quit veto branch; assert the first native-close counter becomes nonzero.
  - **Single fault:** Remove the before-quit veto branch.
  - **At-risk precondition:** a before-quit listener that vetoes; quit is requested.
  - **Expected RED:** The first native-close counter becomes nonzero despite the veto.

### AC-2: Two clean windows and physical close receipts

- **Given:** two clean windows
- **When:** one quit cycle starts
- **Then:** each receives its own GH531 close request; will-quit and quit follow actual terminal window receipts, and window-all-closed is suppressed only while quitting stays active.
- **Negative control:** Complete the app transaction from close-request issuance instead of destruction receipts.
  - **Single fault:** Complete the transaction from close-request issuance rather than physical destruction.
  - **At-risk precondition:** two clean windows; one quit cycle starts.
  - **Expected RED:** Will-quit or quit precedes authenticated physical Destroyed receipts.

### AC-3: Veto does not stop remaining close requests

- **Given:** a veto from either window in the two-window oracle fixture
- **When:** the sweep runs
- **Then:** the other window still receives its request and active quitting resets. A later explicit last-window destruction emits window-all-closed.
- **Negative control:** Stop the sweep at the first veto, or retain active quitting after that veto; each mutation has a separate assertion.
  - **Single fault:** Stop the sweep at the first veto.
  - **At-risk precondition:** a veto from either window in the two-window oracle fixture; the sweep runs.
  - **Expected RED:** The other window lacks its close request; separately retained Quitting also fails reset.

### AC-4: Asynchronous Save and second Quit

- **Given:** draw.io's veto, asynchronous save and destroy sequence
- **When:** the final window closes
- **Then:** window-all-closed can trigger a second app.quit and a second before-quit, followed by will-quit and quit.
- **Negative control:** Use remembered application intent as the suppression flag; the second before-quit must disappear and fail.
  - **Single fault:** Use remembered application intent as the suppression flag.
  - **At-risk precondition:** draw.io's veto, asynchronous save and destroy sequence; the final window closes.
  - **Expected RED:** The second before-quit is missing following the final window destruction.

### AC-5: Will-quit veto and fresh retry

- **Given:** zero windows, a will-quit veto, and no independently committed terminal operation
- **When:** quit is requested again later
- **Then:** the host is still alive and before-quit/will-quit re-run without invented close events.
- **Negative control:** Leave the application permanently in Quitting after a will-quit veto.
  - **Single fault:** Leave the application permanently in Quitting after a will-quit veto.
  - **At-risk precondition:** zero windows and a will-quit veto; quit is requested again later.
  - **Expected RED:** The host terminates or a later attempt lacks before-quit and will-quit.

### AC-6: Native Dock Quit uses the veto owner

- **Given:** Dock or another non-menu OS quit source and a dirty window
- **When:** quit is requested
- **Then:** it reaches the same veto owner and never directly terminates the host.
- **Negative control:** Allow the native terminate callback to bypass the authenticated quit-intent path.
  - **Single fault:** Allow the native terminate callback to bypass the authenticated quit-intent path.
  - **At-risk precondition:** Dock or another non-menu OS quit source and a dirty window; quit is requested.
  - **Expected RED:** The dirty window loses its veto or Host terminates directly.

### AC-7: Nested synchronous Quit coalesces

- **Given:** a nested quit during a current synchronous callback
- **When:** it re-enters the facade
- **Then:** it coalesces with that active attempt and the caller does not park waiting for native close/save work.
- **Negative control:** Await host shutdown on the calling JS stack or create a second overlapping sweep.
  - **Single fault:** Await host shutdown on the calling JavaScript stack.
  - **At-risk precondition:** a nested quit during a current synchronous callback; it re-enters the facade.
  - **Expected RED:** The caller blocks on native close/save or overlapping sweeps begin.

### AC-8: Replacement window and Opening gate

- **Given:** a closed listener creates a replacement window or a create reservation is still Opening
- **When:** the original sweep has received all destruction receipts
- **Then:** Host admission to WillQuit linearizes at the authenticated Prepare cut after physical release and an authoritative
  empty registry (including Opening and Closing). A later admitted Create permanently invalidates that token and prevents its
  Commit, even if the Create fails before delivery. Suppress the callback if invalidation is already observed. Neither a
  Prepared nor a Status reply proves registry emptiness at a later JS callback start.
- **Negative control:** Treat an exhausted sweep snapshot as an empty current registry; the replacement-window survival counter must fail.
  - **Single fault:** Treat an exhausted sweep snapshot as the current empty registry.
  - **At-risk precondition:** a closed listener creates a replacement window or a create reservation is still Opening; the original sweep has received all destruction receipts.
  - **Expected RED:** Prepare is granted against a nonempty registry, or the invalidated token commits.

Hold the Prepare reply and, separately, a recovered Prepared Status reply; admit Create before JS dispatch. Check that known
invalidation suppresses the callback, and that an unobserved intervening Create still prevents Commit. The proposed Host cut
semantics require explicit approval; if callback-start emptiness is required instead, obtain a separate proven coordination
mechanism before implementation. No stale reply grants terminal authority.

### AC-9: Zero-window terminal/Create serialization

- **Given:** a zero-window app.quit immediately followed by BrowserWindow creation
- **When:** the existing host owner serializes admission
- **Then:** an accepted Create prevents premature terminal quit, or a committed terminal fence rejects Create before a native window exists.
- **Negative control:** Use a stale zero-window snapshot and admit Create after terminal commitment; assert that no orphan native window can survive.
  - **Single fault:** Use a stale zero-window snapshot and admit Create after terminal commitment.
  - **At-risk precondition:** a zero-window app.quit immediately followed by BrowserWindow creation; the existing host owner serializes admission.
  - **Expected RED:** An orphan native window survives after terminal commit.

### AC-10: Reversible preparation, exact retirement and one terminal winner

- **Given:** a qualified Prepared attempt races a newly admitted Create, CreateWithPreload, or authorized GH-523 Exit.
- **When:** the existing lifecycle/registry owner orders admission and terminal commitment.
- **Then:** Create invalidates the old token even if the build fails and the registry returns to zero. Exit committing first
  fixes its cause/code and invalidates Quit; Quit committing first fixes its cause/code. The losing operation cannot claim a
  successful receipt, or report a live-link Retained if an independent terminal winner has committed. No second tail runs.
- **Negative control:**
  - **Single fault:** Permit an invalidated token to reserve terminal shutdown after a transient admitted Create.
  - **At-risk precondition:** the admitted Opening has already failed before Commit checks the census.
  - **Expected RED:** the retired token commits or ends a surviving session.

Exercise both Exit/Quit admission orders, including invalid Exit and exact winning status. Independently mutate cause/code overwrite.

### AC-11: Retained link and same-attempt reconciliation

- **Given:** a qualified conditional operation receives Retained, confirmed Cancel, or loses an acknowledgement.
- **When:** the same current link observes a response or obtains an ordered exact-attempt Status result.
- **Then:** proven nonexecution or cancellation permits a fresh attempt on that link only while no terminal winner exists.
  Prepared authorization is valid at its ordered Host cut, not a later callback-time census; consume it at most once subject to
  observed invalidation and the final Commit recheck. Known Commit acceptance fences once. Both facade and API converge on the
  same disposition. Indeterminate stays blocked; generation retirement is not proof of noncommit.
- **Negative control:**
  - **Single fault:** allow a fresh terminal effect before an ambiguous prior operation has an authoritative disposition.
  - **At-risk precondition:** the original operation may have committed although the reply was lost.
  - **Expected RED:** a second effect, wrong-token continuation, duplicate fence or premature success occurs.

Test separate lost Prepare, Cancel and Commit acknowledgements, mismatch, retirement, stale Prepared and correlation reuse.
Hold the original or recovered Prepared reply across Create or competing Exit; no stale success may admit Commit. Distinguish
proven cancellation from a losing attempt after a committed winner, and unknown effect from actual nonexecution.
The real allocator must prevent reuse before write. An old/unselected peer sees no new conditional choreography or frames;
the existing separately approved legacy path and bytes are unchanged.

### AC-12: Reversible FS drain and one terminal reservation

- **Given:** an admitted FS call still owes a real terminal Reply/Err when conditional Commit begins.
- **When:** the existing owner pauses further reader admission and drains while Running.
- **Then:** the terminal reply and lease settle before the final locked check and one reservation. Retained restores the same
  live dispatch path only when no independent terminal winner exists. No second full Quit call or early reserve can lose the
  admitted reply. A competing Exit must obey the shared owner and the already owed reply/lease obligations, not bypass the pause.
- **Negative control:**
  - **Single fault:** reserve terminal shutdown before the admitted FS terminal reply is written.
  - **At-risk precondition:** an admitted worker has a reply that needs the running-state transition/writer path.
  - **Expected RED:** the reply disappears, a lease remains, or cleanup cannot complete.

Hold an actual FS reply/lease while Exit races Draining, then exercise each winner order after the obligations settle. Prove
one reservation, exact cause/code/receipt and one cleanup tail, or a separately qualified non-success outcome on transport
failure. The isolated shared-winner model does not exercise this real I/O or Exit admission.

### AC-13: Native reply and actual termination are separate witnesses

- **Given:** a qualified native Quit strategy and a real signed disposable original with held cleanup work.
- **When:** the OS requests Quit, including while a native event callback is active.
- **Then:** veto or precommit refusal keeps the app alive when no independent terminal operation commits; the canceled Quit
  never claims the other winner. Success is not reported before the required pre-reply obligations
  complete. If delayed reply is selected, the live Host sends one matching main-thread reply, then its own natural kernel exit
  is observed independently. No successful exit is inferred from a reservation or a model.
- **Negative control:**
  - **Single fault:** publish successful native reply before an owned cleanup worker has finished.
  - **At-risk precondition:** the worker is held by the test and terminal reservation has committed.
  - **Expected RED:** the native success observation precedes the cleanup obligation.

Exercise nonresponsive precommit and failed postcommit cleanup separately; both require approved bounded non-success behavior.

## 4. Design

### 4.1 One transaction owner

Proposed phases are Idle, BeforeQuit, Closing, Prepared, WillQuit, Draining and Terminal. GH531 remains the sole window-close
owner. Before-quit veto has no Host close effect. Before the selected sweep, establish one reversible generation-bound active
quitting/suppression owner; this is not terminal reservation. Veto resets active quitting immediately while already selected
entries still receive their requests. Facade owns public event suppression; Host retains actual GH531 release and
LastWindowClosed facts. Bind each close to the existing exact GH531 window, document, generation and close sequence,
including a Merged result from an earlier close; no older attempt's terminal authority transfers. An exhausted snapshot,
CloseRequested, user-facing closed or Allow is not physical Destroyed. Use the existing bounded receipt owner.

Under the existing ordered lifecycle/registry cut, only authenticated current-role preparation after physical release and
authoritative empty census including Opening/Closing can authorize will-quit. Admission linearizes at Prepare; the reply
does not establish a later JS callback-time census. Prepare is reversible and mints one bounded attempt token. Successful
Create, preload or adoption invalidates it at admission even if the build later fails. This includes the interval between
the original/recovered Prepared reply's Host cut and callback dispatch. Suppress a callback on observed invalidation; the
final Commit check must reject the token regardless of delivery timing. Will-quit veto or throw cancels the exact token.
For Commit, pause further reader admission reversibly, keep Running until admitted FS
terminal replies and leases settle without holding their required locks, then recheck the exact token, current role, registry
and drain under the existing serialization. Reserve once and enter the existing post-reservation tail once. Never run the full
legacy Quit again after reservation. Refused/Retained and unknown terminal effects are distinct.

GH-523 AC-7 and KEL-96 must share this single terminal decision. An independently validated authorized Exit committing first
invalidates the reversible Quit and fixes cause/code; Quit winning first fixes its own cause/code. The losing operation cannot
overwrite the winner, repeat terminal cleanup or claim its receipt. A competing committed terminal is a distinct disposition
from Retained/nonexecution on a live link; if the losing caller cannot obtain authoritative status, its effect stays unknown.
Both terminal paths must respect outstanding FS terminal replies/leases and one reader/cleanup owner. A same-reader Exit
queued during Draining is not yet admitted. An independently eligible contender must satisfy the same outstanding drain
obligations before the first successful final locked reservation. A failed drain is non-success/unknown; resume only on
proved noncommit and settled obligations, otherwise retain exact failure custody under the existing owner;
no early reserve or dropped reply is allowed to manufacture success. Joint owners must approve that ordering in GH-523 and
KEL-96. This adds no Exit API or unilateral policy change to GH-451.
Do not add a second lifecycle/registry owner, terminal timeout or permission grant.

### 4.2 Source and ordering

Facade intent emits before-quit synchronously, then uses the existing authenticated Main link. Host/OS intent enters that same
coordinator. Do not infer full parity from a JS-only state model. The pinned two-window oracle observes W2 then W1; capture
creation order and native receipts so a future backend can reproduce or explicitly record a divergence.

### 4.3 Return contract and recovery

Retain the approved scoped D2 nonblocking Promise<void> interface. Its public nonterminal handoff settlement and the private
active attempt / authoritative terminal outcome are separate. Preserve existing before-quit veto/throw and same-Promise
coalescing, including reentry while the real attempt remains active after the public Promise settles. Do not await native
close/Save from a callback that must itself finish. Matched Veto resets eligibility; already selected sweep requests finish.
Preserve existing typed refusals and single-window handoff. The selected multiwindow rule extends first qualified
handoff, not all acknowledgements: settle the public Promise once at the first authenticated current dispatched close
sequence or a valid correlated CloseRequested/Merged; AlreadyClosing requires an already bound genuine sequence.
A bare send is not settlement. Complete the entire selected newest-first sweep despite settlement or veto, retaining
each exact private receipt. Two council reviewers selected this rule; one preferred all ACKs. Test a held other ACK
while a callback needs the public Promise, and late error custody. No void migration is selected.

Before-quit veto/throw fulfills scoped cancellation with no Host effect by this attempt. A typed error before public
settlement rejects once; after settlement report the original cause through the existing attempt error owner without
resettling or clearing a successor. Existing scoped close-handoff failure retirement stays unchanged. Conditional
unknown retains the original private reconciliation authority instead of blindly retiring the link.
Zero-window success requires known Commit Accepted and local admission/fence completion, not Prepared. Retained or
refusal follows the applicable typed error contract. Known acceptance survives local failure; reject if pending,
otherwise report a late error. Will-quit veto/throw fulfills local cancellation if pending but private Cancel/Status
must establish nonexecution or confirmed cancellation before another effect. One subsequent explicit retry may queue
with its own Promise; coalesce further retry intents and launch fresh before-quit only on the same qualified current
link after proven noncommit and no competing terminal. Unknown/retirement/winner rejects that queued request.

Negotiate conditional support through the existing trusted link owner before starting new conditional choreography.
Do not infer support from Ready, package version or a successful legacy Quit. Preserve existing legacy wire and behavior.
Typed Retained or confirmed Cancel keeps the same live link only when no independent terminal winner has committed.
Commit Accepted proves one irreversible reservation, not clean
exit or public quit event. A known commitment remains known even if subsequent local fencing or cleanup fails.

For an ambiguous reply, the existing allocator and Host owner retain the original operation/identity, current authenticated
link, role, attempt and token when known. Ordered read-only Status on the same link uses a distinct correlation; it returns
the exact authoritative disposition, including invalidated rather than stale Prepared. Do not reuse an unresolved identity,
start a fresh effect, reconnect, replay or downgrade to forceful Quit. The API and facade must converge on the same original
attempt: continue Prepared authorized at the ordered cut at most once, with observed-invalidation and final Commit checks;
settle confirmed Cancel/Retained without repeating callbacks, or fence a known Accepted once. A competing committed winner
is not this attempt's acceptance or Retained. Missing/mismatched/retired/indeterminate status remains unknown. Retirement
revokes the old role's authority but never proves noncommit, erases a known committed cause, or permits replay on a successor.
Keep unknown custody fenced until an authoritative outcome or an approved terminal/retirement disposition establishes what
may proceed, without assuming nonexecution.
Use a bounded existing-owner record: at most one active conditional attempt and one unresolved original effect on
the current authenticated link. Reserve its allocator identity before send, including against ordinary calls.
Serialize Status with a distinct correlation. Select explicit exact receipt-consumption confirmation for reclamation;
release only after ordered proof that the original cannot execute again and late delivery cannot alias. Missing or
ambiguous confirmation retains the record and fences new conditional effects at capacity. Do not evict by time or
pressure. Preserve a known committed winner separately in the lifecycle owner, including after link retirement.
The IPC owners must choose and approve the exact schema, confirmation and retirement mechanism; no tags or deadlines
are allocated here.

### 4.4 Native terminal boundary

Only the ordered Host terminal owner may end the process. Current native cancellation remains the safe scoped behavior;
it is not proof of successful OS Quit. The council selects delayed AppKit reply only as an experiment candidate under GH443.
Reject immediate termination on initial OS intent. A delayed strategy must prove real callback progress in and outside native
modal dispatch. Veto/precommit refusal denies that native request and keeps the app alive absent a separately committed
terminal winner. A live main-thread owner sends one
correlated success reply only after the required actual window release, terminal commitment and pre-reply cleanup obligations
are proven. Observe the Host's own kernel exit independently after that reply; requiring its own exit before reply is circular.
A postcommit failure cannot roll back the committed cause or promise restored windows. The native/cleanup owners must approve
bounded precommit nonprogress and postcommit failure dispositions. No deadline or native strategy is enabled by this draft.

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

## 6. Tasks

### T-1: Record pending conformance and veto-reset fixtures
- **Blocked by:** none

- [ ] Record pending conformance cells and literal phase/order fixtures, including the veto-reset and second-quit oracle; preserve
scoped D2 behavior until the joint contract is approved.

### T-2: Implement the facade-to-Host transaction and GH531 sweep
- **Blocked by:** T-1, Q-1, Q-2, Q-3, Q-4, Q-5, Q-6

- [ ] Implement one facade-to-host quit transaction together with the GH531 per-window sweep and zero-window will-quit branch;
include every veto negative control. Include closed-listener replacement, Opening and same-turn zero-window Quit/Create controls
against the current registry. Implement only the approved existing-owner transaction, retained-link/status reconciliation,
shared terminal decision and FS drain. Verify the API/facade attempt convergence and old-peer controls before wiring.

### T-3: Wire qualified non-menu native Quit and role retirement
- **Blocked by:** T-2, Q-3, Q-7, Q-8

- [ ] Wire the qualified non-menu native quit adapter and retirement handling into the same coordinator; reject bypasses and stale
intent.

### T-4: Run real macOS original-app conformance
- **Blocked by:** T-2, T-3

- [ ] Run real macOS clean/dirty/two-window/zero-window fixtures and original draw.io save/cancel/retry; move only this issue's
evidenced cells to pass.

## 7. Test plan & evidence

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-451;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use a fake authenticated link for deterministic sequencing, then real macOS process/window receipts. A model cannot establish Dock
delivery or natural process exit. Observe native window identity, role generation and terminal cause separately; drive
acknowledgements rather than sleeping.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Full runtime, native and original-app acceptance tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The original documentation-only session was restricted to GitHub. The later isolated model and specification-check evidence
is identified separately; no full runtime or native acceptance is claimed from these documentation checks.

### E-1: Historical documentation-only review; native acceptance not run
- **Run:** The original GitHub-only drafting session reported source readback and documentation checks, not an executed native-app test.
- **n:** 0 native or installed-app acceptance runs.
- **Environment:** GitHub-only spec publication at source baseline 0eb3ce203187f35c5276a4237189c906a5220d5a; no real macOS original app exercised.
- **Limits:** Documentation checks do not establish GH-451 runtime, veto, Dock, terminal/cleanup or signed draw.io conformance.

### E-2: Technical council and local nonshipping evidence
- **Run:** independent six-lane technical review of the combined local draft and its owning contracts; standalone Rust projections
  and Bun scripted model tests were reported in the issue handoff. These are not authenticated/native acceptance.
- **n:** 22/22 current Rust projection/model tests (including four shared-decision additions); earlier combined 66/66 Bun tests.
- **Environment:** isolated macOS prototype `24187ab15e6294493bf8790e35092d214b109ee8`; prior combined `9bfb73eff`.
- **Limits:** no real new wire, OS Quit, physical cleanup, process exit, public API migration or owner approval. The four
  shared-decision model additions use simulated reservation outcomes and already validated input; they do not prove malformed
  Exit-code validation, actual owed FS reply/lease settlement, winner-overwrite mutation, cleanup or parent process status.
  Council approved
  architecture direction, requested contract revision and withheld production/native approval.

## 8. Review gates triggered

Public API and wire protocol; lifecycle/KEL-96 owner amendment. Unsafe review only if the selected native adapter adds a
sanctioned FFI call.

Status remains draft. The agent-authored user-delegated record selects GH-451-side design semantics, not joint owner
approval or independent human sign-off. Implementation admission requires the remaining owning decisions in section 10
and independent exact-content review. Publication cannot approve wire allocation, architecture amendment, native behavior
or release. Isolated test candidates may verify the selected semantics without changing production.

## 9. Perf impact

One bounded snapshot of existing registry IDs per active quit. Measure UI dispatch latency and retained entries during a vetoed
sweep; no latency or startup improvement is claimed.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions, blockers & unknowns

### Q-1: D2 settlement and exception contract
Selected on the GH-451 side: first qualified handoff, exact private outcome and error custody, confirmed-cancellation retry
and the matrix in section 4.3. Joint API review must approve exact error mapping; void migration stays separate.

### Q-2: Reversible active-quitting amendment
Selected on the GH-451 side: generation-scoped reversible suppression before sweep, reset on matched veto, distinct from
terminal reserve. KEL-96 must adopt the corresponding owner amendment. The native mechanism is a separate later gate.

### Q-3: Lifecycle payload and role-loss disposition
Selected on the GH-451 side: one unresolved exact effect, ordered same-link Status, explicit receipt-consumption proof,
bounded fail-closed retention and API/facade convergence in section 4.3. IPC owners approve negotiation, single lifecycle
allocation and exact confirmation/retirement mechanism. No fallback on unknown; preserve existing legacy wire.

### Q-4: Terminal/Create fence and late-created windows
Selected on the GH-451 side: Host Prepare linearization, not JS callback-start emptiness; permanent Create invalidation.
Joint GH531 review must approve exact merged sequences and physical Destroyed integration against the pinned oracle.
No stale snapshot or prior attempt authorizes terminal admission.

### Q-5: One shared terminal winner
Selected on the GH-451 side: first eligible committed terminal fixes immutable cause, code, receipt and one existing tail.
Joint GH-523/KEL-96 owners must adopt the corresponding decision. No unrelated Exit implementation is authorized here.

### Q-6: Running-state FS drain
Selected on the GH-451 side: reversible running-state pause and owed reply/lease drain for either terminal contender,
then final serialized recheck and one reserve. Joint lifecycle/FS/Exit owners must adopt and qualify failure custody;
no bypass or second cleanup tail.

### Q-7: Native cleanup failure and resource qualification
Selected principles: no false precommit success and no postcommit rollback. The exact bounded failure dispositions,
custodian, clock and deadline still require native/lifecycle owner evidence; do not invent them. GH-1404 B-2, GH-1404 B-3,
G0–G12 and applicable release/provenance gates remain separate.

### Q-8: Qualified native mechanism
Current Cancel is the scoped safe baseline. Delayed AppKit reply is selected only as an experiment candidate; no
immediate terminal admission. GH443/KEL-96 must select a qualified mechanism after actual modal dispatch and pre-reply
cleanup/post-reply exit proof. Use a separately qualified disposable experiment before native wiring, not a circular
requirement for the reversible source design.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.
