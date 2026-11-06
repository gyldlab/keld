# Spec: Windows native dialogs and synchronous dirty-close results

- Status: draft
- Tracker: GH-1189
- Owner: Monish Khandelwal (@0monish; approval pending)
- Updated: 2026-11-05
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Extend the shared dialog owner with a Windows native adapter. The adapter returns real Open, Save, and synchronous Message results. It binds each operation to the admitted
role and current parent. It suppresses late presentation and proves physical cleanup.

FACT: current main tests the absence of synchronous Message, Open, and Save exports.
See `packages/@keld/electron/src/dialog.test.ts:41`, `packages/@keld/electron/src/dialog.test.ts:49`, and `packages/@keld/electron/src/dialog.test.ts:56`.
The candidate macOS implementation is in #1306/#1308. This document creates no Windows implementation or acceptance result.

Non-goals:

- Replacing the shared dialog broker, WorkerLink, or window registry.
- Implementing Windows W1-W4 or taking their active source ownership.
- Adding picker-derived filesystem authority or weakening Strict.
- Reusing macOS native proof as Windows qualification.
- Adding synchronous Open/Save, arbitrary shell pickers, bookmarks, or certificate dialogs.
- Approving a merge, package, or full draw.io compatibility claim.

## 2. Spec refs

FACT: these are the inspected source boundaries.

| Owner | Source | Contract consumed |
|---|---|---|
| W2 | `docs/specs/gh1188-windows-window-adapter.md:48` | W5 owns dialogs and Windows button mapping |
| Window registry | `docs/specs/gh531-window-registry-close-state-machine.md:1` | Shared identity and close ownership |
| WorkerLink | `docs/specs/gh527-worker-owned-blocking-call-transport.md:475` | Existing bounds and abandoned-call custody |
| Windows transport | `docs/specs/gh527-worker-owned-blocking-call-transport.md:1417` | Landed terminal-byte and Quit-drain fixes |
| Shared dialog contract | #1308 `docs/specs/gh478-guarded-macos-dialog-broker.md:845@526c504dbdbcbc383649d6243cec047c7b1f39da` | Approved macOS design, separate implementation gates |
| Shared wire | #1308 `crates/keld-native/src/dialog.rs:207@526c504dbdbcbc383649d6243cec047c7b1f39da` | Existing request/reply variants |
| Shared custody | #1308 `crates/keld-core/src/app_session/dialog_custody.rs:57@526c504dbdbcbc383649d6243cec047c7b1f39da` | Reservation, commit, terminal, quiescence |
| Shared result check | #1308 `crates/keld-native/src/dialog.rs:404@526c504dbdbcbc383649d6243cec047c7b1f39da` | Current selected-path rule is POSIX-specific |
| Sync consumer | #1163 `docs/specs/gh564-macos-sync-message-box.md:101@0853635db8bd9348ee716e9f3b46d548d8c3b46d` | One connected WorkerLink, no second transport |
| Current Windows loop | `crates/keld-wv/src/webview2/mod.rs:3292` | Existing Tao event-loop owner |

FACT: #1073 is closed. The landed W2 spec records its fixes at `docs/specs/gh1188-windows-window-adapter.md:9`. Do not reopen that completed repair as a blanket dependency.
The new dialog path still needs its own Windows correlation tests.

FACT: the exact upstream Windows caller uses three ordered buttons.
Source: [draw.io main](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L858-L887), retrieved 2026-11-05.
Verbatim: `buttons = ['&Save', '&Discard Changes', 'Cancel'];`
#1183 retains the actual Windows app/artifact pin.
An app patch must appear in that artifact record.

## 3. Acceptance criteria

### AC-1: guard admission precedes native effect

- **Given:** the current role lacks the exact dialog grant but supplies otherwise valid options and parent.
- **When:** it requests each supported dialog kind.
- **Then:** the existing guard returns its typed denial and no native panel appears.
- **Negative control:**
  - **Single fault:** bypass the dialog guard for Message.
  - **At-risk precondition:** role, parent, profile, and message admission pass.
  - **Expected RED:** the independent native observer sees unauthorized UI.

### AC-2: parent identity survives queued work

- **Given:** an admitted request waits while its parent generation retires.
- **When:** the same numeric HWND becomes available again before presentation.
- **Then:** the old request cannot attach to the replacement window.
- **Negative control:**
  - **Single fault:** resolve the parent by numeric HWND alone.
  - **At-risk precondition:** the fixture proves reuse before releasing the presentation barrier.
  - **Expected RED:** UI attaches to the replacement or the stale request returns a result.

### AC-3: Open and Save return genuine selected paths

- **Given:** real file and directory fixtures contain distinct sentinel names.
- **When:** the operator selects or cancels each admitted Open/Save case.
- **Then:** exact native results reach the original caller once.
  Cancel returns empty paths.
  Save creates or truncates no application file.
- **Negative control:**
  - **Single fault:** return the previous selection on user Cancel.
  - **At-risk precondition:** a successful selection precedes a separate cancellation.
  - **Expected RED:** the canceled result contains the sentinel path.

### AC-4: Windows option branches match their oracle

- **Given:** valid options exercise default file mode, directory mode, combined flags, multiple selections, and selected filters.
- **When:** each request reaches a real Windows picker.
- **Then:** the selected kind and path list match the pinned Windows Electron observation.
- **Negative control:**
  - **Single fault:** force file mode when both file and directory flags are present.
  - **At-risk precondition:** the combined-flags fixture attempts its expected directory selection.
  - **Expected RED:** the selection or returned kind differs from the oracle.

### AC-5: Message returns the selected synchronous index

- **Given:** the unchanged Windows dirty-close request has a live parent and authenticated connected singleton.
- **When:** Save, Discard Changes, and Cancel are each selected in separate runs.
- **Then:** the caller receives primitive integers 0, 1, and 2 respectively before its next statement.
- **Negative control:**
  - **Single fault:** use the macOS Cancel/Discard index mapping.
  - **At-risk precondition:** the operator selects Windows Discard Changes.
  - **Expected RED:** the observed index or resulting application continuation is wrong.

### AC-6: synchronous park preserves event semantics

- **Given:** the main Bun thread waits for Message while the renderer and transport remain active.
- **When:** a state fact and ordered events precede the reply.
- **Then:** the existing mirror applies the fact before return and user listeners run afterward in order.
- **Negative control:**
  - **Single fault:** dispatch a queued user listener before the blocking call returns.
  - **At-risk precondition:** the parked stream contains that listener event before the reply.
  - **Expected RED:** the independent sequence records application reentry.

### AC-7: cancellation prevents late presentation

- **Given:** an admitted request waits behind a native presentation barrier.
- **When:** caller cancellation or parent retirement wins before that barrier opens.
- **Then:** no late panel appears and the original operation reaches one terminal disposition.
- **Negative control:**
  - **Single fault:** ignore the committed tombstone at final presentation admission.
  - **At-risk precondition:** caller cancellation has committed. Parent, role, link, and queued native work remain live. Every other admission predicate passes at the final tombstone check.
  - **Expected RED:** the native observer sees the abandoned panel.

### AC-8: cancellation proves physical release

- **Given:** each supported dialog is visibly open.
- **When:** timeout, parent destruction, role loss, link loss, or generation replacement wins.
- **Then:** the native panel closes within the shared cleanup bound.
  Its callback cannot later publish a successful result.
- **Negative control:**
  - **Single fault:** acknowledge cancellation without issuing native dismissal.
  - **At-risk precondition:** the panel remains visible after the cancellation receipt.
  - **Expected RED:** the retained native witness crosses the cleanup bound.

### AC-9: original and Cancel correlations stay distinct

- **Given:** one live link repeatedly times out admitted dialogs and sends cleanup Cancel calls.
- **When:** 257 successive cleanup cycles complete.
- **Then:** each admitted original has one terminal frame and its abandoned reservation drains.
  A subsequent healthy operation succeeds.
- **Negative control:**
  - **Single fault:** omit the original terminal while retaining the Cancel reply.
  - **At-risk precondition:** native custody is quiescent and released before the wire-only omission. The same link stays healthy.
  - **Expected RED:** the retained abandoned count grows and the link reaches its existing cap.

### AC-10: modal UI does not trap cancellation dispatch

- **Given:** a dialog originates during a WebView2 event and becomes visible after that event returns.
- **When:** Core sends cancellation, parent retirement, and Quit in separate runs.
- **Then:** the owning UI path processes each command without a nested WebView2 callback or deadlock.
- **Negative control:**
  - **Single fault:** enter native modal presentation inside the WebView2 event handler.
  - **At-risk precondition:** the fixture requires a later engine callback to complete cancellation.
  - **Expected RED:** callback ordering or the bounded cleanup observer fails.

### AC-11: Windows paths use the shared result boundary

- **Given:** selected paths include spaces, Unicode, drive roots, and declared supported network paths.
- **When:** the host validates and the existing SDK decodes the result.
- **Then:** both preserve the same selected spelling or return the approved typed refusal.
  No path is silently converted into a different file identity.
- **Negative control:**
  - **Single fault:** retain the POSIX-only leading-slash test on the Windows result path.
  - **At-risk precondition:** a real drive-qualified file is selected.
  - **Expected RED:** a valid Windows selection is refused before the caller receives it.

### AC-12: dirty-close follows the actual app continuation

- **Given:** a modified document runs through the approved Windows artifact.
- **When:** Cancel, failed Save, successful Save, and Discard are exercised separately.
- **Then:** Cancel and failed Save retain the window.
  Successful Save waits for its matching result.
  Discard waits for required draft cleanup before W2 destruction.
- **Negative control:**
  - **Single fault:** destroy the window immediately after selecting Save.
  - **At-risk precondition:** the renderer's save result is held behind a barrier.
  - **Expected RED:** native destruction occurs before that result.

## 4. Design

### D-1: independent atoms

This table is the proposed Windows amendment. Shared owners keep their existing state and policy.

| Atom | Owner and boundary | Input to output | Failure and falsifier | Explicit edge |
|---|---|---|---|---|
| Authorization | Guard/Core | Authenticated call to admitted work | Ungranted UI, AC-1 | Role/profile admission |
| Parent | W2 registry | WindowRef to live parent | Recycled HWND, AC-2 | Final UI admission |
| Native operation | Native Windows adapter | Valid options to actual outcome | Synthetic path/index, AC-3/AC-5 | Authorized work |
| Modal dispatch | W2 UI loop | Deferred work to callback progress | Trapped cleanup, AC-10 | Native operation |
| Custody | Existing DialogSession | Cancel/retire/click to one winner | Late panel or stale result, AC-7/AC-8 | Parent and native |
| Terminal delivery | Existing WorkerLink/Core | Winner to correlated frame | Abandoned leak, AC-9 | Custody |
| App continuation | Existing compat/app | Index to save or close path | Premature destroy, AC-12 | Terminal and W2 |

OS cancellation and wire settlement are different observables. A returned error does not prove the panel disappeared. No new principal, broker process, or private transport is
proposed.

### D-2: reuse the current shared implementation

FACT: #1308 already has `DialogRequestV1`, `DialogReplyV1`, and `DialogCustody`. Those are candidate-branch source facts, not landed Windows behavior. Extend their existing
owners after B-1. Do not copy their policy or state into a Windows broker.

Use the existing guarded dialog channel and generated TypeScript codec. Preserve append-only variant order. Reuse `WindowRef` and authenticated role metadata. No Bun-supplied
HWND, pointer, permission result, or role generation is accepted. The native adapter retains only the operation it owns.

FACT: the shared result validator currently requires a POSIX path. See #1308 `crates/keld-native/src/dialog.rs:404@526c504dbdbcbc383649d6243cec047c7b1f39da`. Extend that
single validator for the host-selected platform. Update its actual SDK consumers through their existing owner. Do not accept a caller-selected path platform or bypass
validation on Windows.

The first lane uses the admitted Bun primary role. Additional role concurrency needs shared-owner acceptance. One role retains at most one active dialog reservation.
Cancellation remains admissible when that reservation is occupied.

### D-3: native APIs and callback ownership

Proposed primitives: Common Item Dialog for Open/Save and TaskDialogIndirect for Message. Keep native objects and callbacks on their owning single-threaded apartment. Use the
existing Windows host UI thread unless B-2 proves that choice cannot work. Do not transfer raw COM pointers across threads.

FACT: Microsoft's `IModalWindow::Show` takes the owner HWND. Its documentation states: "The handle of the owner window. This value can be NULL." Source:
[IModalWindow::Show](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-imodalwindow-show), retrieved 2026-11-05.

Resolve a supplied parent through W2 immediately before native presentation. Never substitute the currently focused window for a stale supplied parent. Retain the registry
generation throughout custody. The first admitted dirty-close lane requires its visible parent.

Parentless and hidden-parent behavior needs the Windows oracle in B-3. A hidden TaskDialog parent cannot be treated as automatically valid. FACT: Microsoft says, "The parent
window should not be hidden or disabled when this function is called." Source:
[TaskDialogIndirect](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-taskdialogindirect), retrieved 2026-11-05. Until B-3 closes, those cases receive
the existing explicit unsupported disposition. No silent parent retargeting is permitted.

FACT: WebView2 does not support modal reentry inside its callbacks. Its prescribed sequence is: "schedule the appropriate work to take place after completion of the event
handler". Source: [WebView2 threading](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/threading-model), retrieved 2026-11-05.

Native entry must occur after the WebView2 callback returns. That condition alone does not prove Tao command delivery. The W2 owner must also prove cancellation while native
modal code holds the stack. Do not hold a guard, registry, or custody lock across native calls. Do not add a second message loop, sleeper poller, or per-dialog Worker.

B-2 owns the exact deferred-entry and cancellation primitive. The recommended experiment uses the existing host UI dispatcher. A dedicated apartment thread is an alternative
only after that route demonstrably fails. Such a thread would need a separate handle/callback lifetime decision.

### D-4: operation and option mapping

Preserve the existing shared option envelope and count limits. Do not grow those limits to accommodate a Windows adapter. Handle absent options and present-`undefined` through
the shared normalization owner. Reject unsupported keys before allocation.

Open consumes the existing file, directory, and multiselection variants. Use the selected filter group, not a union of every filter. Save returns a location without creating
or truncating the selected application file. Do not synthesize an extension without the accepted Windows oracle.

FACT: pinned Electron documentation treats combined file/directory selection differently on Windows. Its example states: "the directory selector will be shown". Source:
[pinned Electron dialog](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/dialog.md#L168-L179), retrieved 2026-11-05. Apply that
Windows branch without changing the macOS adapter.

Message uses the caller's ordered button array. Map native button identifiers back to the original array index. Respect `defaultId`, `cancelId`, access keys, and
`noLink:true`. For the pinned Windows caller: Save is 0, Discard Changes is 1, and Cancel is 2. Test Enter, Escape, the close button, and actual button clicks. Do not confuse
native failure with button index zero.

FACT: the pinned API states: "Returns `Integer` - the index of the clicked button." Source: [pinned Message
API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/dialog.md#L324-L327), retrieved 2026-11-05. The synchronous facade consumes
the already-connected WorkerLink. Before readiness, it throws the shared typed not-ready outcome synchronously. No Promise or synthetic Cancel may replace that result.

### D-5: cleanup and the commit race

Reuse the shared custody winner. Test native completion before cancellation and cancellation before completion. Only the winner can choose the original terminal result. A
later callback cannot change that result.

For an admitted queued request, commit the tombstone before releasing its presentation barrier. For a visible request, request native dismissal on its owning thread. Do not
release custody until native callbacks and physical UI are quiescent. Parent loss, role loss, and link loss enter that same cleanup owner.

FACT: `IFileDialog::Close` supports invocation while the dialog is open. Microsoft states: "An application can call this method from a callback method or function while the
dialog is open." Source: [IFileDialog::Close](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifiledialog-close), retrieved 2026-11-05. That
contract does not prove Keld can deliver the callback. B-2 and AC-10 must establish delivery.

A TaskDialog adapter retains its real callback-provided HWND until destruction. A programmatic dismissal keeps its cleanup cause. It must not become an application Cancel
click. FACT: `TDM_CLICK_BUTTON` "Simulates the action of a button click in a task dialog." Source:
[TDM_CLICK_BUTTON](https://learn.microsoft.com/en-us/windows/win32/controls/tdm-click-button), retrieved 2026-11-05. The adapter must therefore distinguish cleanup from
genuine user selection.

Use the shared 600-second interaction deadline. Keep the 10-second live-link timeout cleanup requirement. Keep the 5-second Host-observed role/link retirement cleanup
requirement. FACT: #1308 records these approved design bounds at `docs/specs/gh478-guarded-macos-dialog-broker.md:853@526c504dbdbcbc383649d6243cec047c7b1f39da`. They are
Windows acceptance targets, not measured Windows results. Do not extend existing Host, IPC, or shutdown clocks.

A healthy link gets independent Cancel and original terminal frames. A dead link gets native cleanup without fabricated delivery. Do not retry an original dialog after an
ambiguous outcome. A cleanup failure enters the existing fatal/custody path.

### D-6: paths, grants, and migration seam

Preserve the selected Windows spelling as data. Reject embedded NUL, malformed conversion, relative paths, and over-limit results. Do not use lossy UTF-16 conversion. The
accepted Windows spelling set is decided in B-3. UNC, extended-length, device, and shell-namespace forms need explicit dispositions. A raw spelling cannot bypass the shared
result validator.

Dialog selection mints no filesystem grant, handle, bookmark, or scope. A guarded file operation still needs its independently approved permission. Legacy does not bypass the
Host guard. The role's ambient filesystem authority remains the profile owner's separate disclosure.

The migration unit includes native callbacks, shared reply validation, SDK decoding, and the actual app continuation. Keep #564's synchronous facade and #478's asynchronous
facade under their existing owners. There is no temporary copied facade. No protocol version, second schema, or new configuration key is proposed.

## 5. Boundaries

Future implementation extends `crates/keld-native/src/dialog`. Core's existing `dialog_session` and `dialog_custody` retain shared authority. W2 owns changes to
`crates/keld-wv/src/webview2/mod.rs`. The existing API and Electron owners retain their facade/codegen files. Coordinate every shared-file edit through one designated writer.

Do not change W1 preload, W3 navigation, W4 facts, packaging, or Strict grants here. Do not assume the current native unsafe allowlist covers Win32 dialogs. A narrowly scoped
native/FFI amendment precedes production calls.

## 6. Tasks

### T-1: qualify the deferred native primitive

- **Blocked by:** B-5
- Obtain separate scoped test-only approval for this mechanism fixture.
- Use an independently scoped Windows mechanism fixture.
- Prove callback exit, visible native UI, cancellation delivery, and subsequent healthy presentation.
- Exercise AC-7, AC-8, and AC-10 without enabling product dispatch.
- Use that evidence to resolve Q-1 before full B-1 approval.

### T-2: deliver guarded Open/Save end to end

- **Blocked by:** T-1, B-1, B-2, B-3, B-4
- Extend the shared native adapter, result validation, and actual SDK consumer.
- Run AC-1 through AC-4, AC-7 through AC-11.
- Preserve the macOS branch and its existing tests.

### T-3: deliver the synchronous Message consumer

- **Blocked by:** T-2, B-6
- Integrate the existing WorkerLink path and native button mapping.
- Run AC-5 through AC-10 and the retained denial/parent cases.
- Keep the bounded event stream and original-correlation checks.

### T-4: qualify the actual Windows dirty-close workflow

- **Blocked by:** T-3, B-7
- Run AC-12 on the approved app artifact with real W2 windows.
- Publish the exact source, engine, Host, app, and profile evidence.
- Keep product claims blocked until this task's results pass.

## 7. Test plan & evidence

| Criteria | Planned fixture and independent witness |
|---|---|
| AC-1 | Guarded requests plus native window census for every dialog kind |
| AC-2 | Registry generation fixture and observed HWND reuse |
| AC-3/AC-4 | Windows UI automation over isolated file/directory/filter sentinels |
| AC-5 | Real buttons, Enter/Escape/access keys, primitive return and sequence |
| AC-6 | Existing WorkerLink stream over 8 KiB, mirror and listener-order witnesses |
| AC-7 | Final presentation barrier released after observed tombstone |
| AC-8 | Visible panel, independently timestamped retirement, physical disappearance |
| AC-9 | 257 real correlation cycles and healthy follow-up |
| AC-10 | WebView2 callback-depth witness plus cancellation during real modal UI |
| AC-11 | Native path fixtures through Host validation and actual SDK decoding |
| AC-12 | Pinned app save/draft protocol, native lifetime, and saved-file bytes |

Use isolated subprocesses for lifetime and failure controls. Use observable barriers, never sleeps as synchronization. Record each dialog kind and each applicable retirement
cause separately. A screenshot, API return, or acknowledged cancel alone cannot prove cleanup. A watchdog expiry is a failed run. If HWND reuse is not observed, AC-2's reuse
cell is not run. Do not count an ordinary stale-id test as that native reuse proof. A controlled reuse hook still needs independent proof that the OS assigned the same HWND to
a different live window.

### E-1: source inspection

- **Run:** GitHub GET of the cited main files, #1308 shared sources, #1189, and pinned draw.io/Electron files.
- **n:** one source inspection per cited file, with no native executions.
- **Environment:** cloud review on 2026-11-05 at the header and explicitly cited candidate commits.
- **Limits:** Mac source approval, Windows transport fixes, and source button order do not prove Windows dialog behavior.

### E-2: current platform documentation receipt

- **Run:** Context7 Windows App SDK query for dialog ownership, apartment rules, and cancellation.
- **n:** one discovery query followed by official Microsoft source confirmation.
- **Environment:** Microsoft Learn pages retrieved 2026-11-05, URLs and short quotes in D-3 through D-5.
- **Limits:** discovery returned unrelated DesktopChildSiteBridge material.
  Official sources confirmed the cited claims.
  No Keld callback-delivery or physical cleanup experiment ran.

## 8. Review gates triggered

- Unsafe: exact Windows COM, callback, and native dialog operations.
- Public API: path values, button indexes, option admission, and synchronous return.
- Permission model: one guard check and no selection-derived authority.
- Dependency: any additional Windows API features require the existing dependency owner.
- Wire protocol: preserve existing variants and prove unchanged framing and correlation.

All five gates remain explicit where the implementation touches their contracts. No numeric channel or error code is allocated here.

## 9. Perf impact

Use the existing transport buffers, panel capacity, and deadlines. Do not add a second ring or expand capacity to hide abandoned calls. Measure cancellation latency and
retained handles separately from user interaction time. No responsiveness or memory improvement is claimed.

## 10. Open questions, blockers & unknowns

### Q-1: modal delivery mechanism

Prefer the existing Windows host UI thread with deferred entry outside engine callbacks. Require direct cancellation evidence while the real native dialog is active. If that
path fails, compare a dedicated apartment with W2. Record the new handle and crash ownership before selecting that alternative.

### B-1: exact Windows amendment approval

- **Status:** open
- **Owner:** Monish Khandelwal and #478/#564/W2 architecture reviewers.
- **Impact:** production Windows dialog implementation cannot start from this draft.
- **Check:** approve the exact adapter contract, task scope, and shared-owner integration sequence.

### B-2: modal dispatch and cleanup primitive

- **Status:** open
- **Owner:** W2 UI-loop owner and the #1189 Windows native implementer.
- **Impact:** T-2 and public presentation remain blocked.
- **Check:** T-1 proves AC-7, AC-8, and AC-10 on the exact Windows source.

### B-3: Windows option, parent, and path oracle

- **Status:** open
- **Owner:** #1189 native operator with #478 compatibility owner.
- **Impact:** parentless/hidden parents, full option mapping, and accepted Windows path forms remain unqualified.
- **Check:** commit pinned Windows Electron observations and explicit support/refusal decisions for every branch.

### B-4: W2 and role/profile integration

- **Status:** open
- **Owner:** #1188 W2 and #1187 W6 owners.
- **Impact:** a standalone panel cannot establish product admission, parent identity, or role teardown.
- **Check:** accept the exact-head W2 parent and W6 profile artifacts and their approved integration interfaces before T-2.
  T-2 produces the later real dialog integration proof. That proof is not this prerequisite.

### B-5: native safety and operator scope

- **Status:** open
- **Owner:** native safety reviewer and proposed operator Amisha Ramani.
- **Impact:** this document authorizes neither native production calls nor execution of T-1's test-only fixture.
- **Check:** approve the exact test-only scope, operator/device claim, and narrow FFI/feature plan before T-1.
  Production integration separately requires B-1 and the production native-safety review.

### B-6: synchronous shared facade admission

- **Status:** open
- **Owner:** #564 and the existing API/WorkerLink owners.
- **Impact:** no Windows synchronous compatibility result can be claimed.
- **Check:** approve the exact existing-link facade contract and designate its implementation owner before T-3.
  T-3 produces AC-5, AC-6, and AC-9 evidence afterward. Those results gate claims, not task entry.

### B-7: app artifact and continuation prerequisites

- **Status:** open
- **Owner:** #1183 artifact owner and #1184/#1185 IPC/navigation owners.
- **Impact:** T-4 cannot qualify the real save/close path from isolated dialog tests.
- **Check:** bind the approved Windows artifact and working save/draft result path before the app run.

### U-1: native cancellation during real modal execution

UNKNOWN: the existing Windows dispatcher can satisfy every native cancellation order. T-1 resolves this with real dialogs and separate event/callback witnesses. Microsoft's
API contract alone is insufficient.

### U-2: application recovery after a thrown dialog failure

UNKNOWN: the pinned app recovers its private close state after every typed runtime failure. Do not synthesize Cancel or clear private app state in the Host. The app/compat
owner must qualify recovery before claiming repeated dirty-close after faults.
