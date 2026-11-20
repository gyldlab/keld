# Spec: GH-478: acceptance criteria

Part of: [index](index.md)
Owns: section 3, AC-1 to AC-15

## 3. Acceptance criteria

Each ID keeps its approved A-label in the subtitle. Given and When restate the approved
lead-in, and Then keeps the binary observations.

### AC-1: A1a - immutable-policy boot preflight

- **Given:** a policy file with malformed JSONC, duplicate object keys, invalid bytes or
  a mismatched verified digest.
- **When:** the existing retained-handle policy loader runs before an app child,
  bootstrap listener or application window/panel starts.
- **Then:** the loader rejects the policy with its existing typed startup error and
  resource-absence proof. No authenticated dialog CALL exists at this cut, so no
  correlated Guard ERR is promised. The already opened policy handle is an input, not a
  newly permitted app resource.
- **Negative control:**
  - **Single fault:** move policy verification after app startup.
  - **At-risk precondition:** an app child, listener or panel can start from a malformed
    or mismatched policy.
  - **Expected RED:** independent resource counters become nonzero, or the typed startup
    error is lost.

### AC-2: A1b - exact grant on a valid live CALL

- **Given:** a well-formed digest-verified manifest, an authenticated current app role
  and a valid dialog CALL.
- **When:** guard admission evaluates the call.
- **Then:** exactly `{"app":{"system":["dialog"]}}` reaches native admission once.
  Missing or empty lists, wrong node types, duplicate array literals, unknown literals
  and wildcard-like strings fail the guard-owned evaluator with canonical typed
  `KELD-GUARD` ERR on that CALL and pointer `/app/system`, with zero native panel
  creation. Duplicate JSON object keys belong to AC-1. No developer shortcut or
  automatic legacy enablement exists.
- **Negative control:**
  - **Single fault:** remove guard evaluation, or admit a wildcard or duplicate literal.
  - **At-risk precondition:** a live authenticated dialog CALL reaches the dispatcher.
  - **Expected RED:** a denied CALL creates a native panel, or the error omits
    `/app/system`.

### AC-3: A1c - caller identity before authorization

- **Given:** an unauthenticated, stale or wrong-role link, and independently a
  non-AppProcess principal.
- **When:** the link bootstrap/session and guard evaluator admit the call.
- **Then:** the existing bootstrap/session rule refuses the link with zero dialog
  admission and no fabricated CALL response. The guard evaluator rejects non-AppProcess
  principals before grant lookup. A valid app link with an invalid parent is tested
  under AC-7; it does not bypass identity or permission checks.
- **Negative control:**
  - **Single fault:** reuse app grants for another principal, or admit an untrusted link.
  - **At-risk precondition:** a link reaches dialog admission without current
    authenticated role authority.
  - **Expected RED:** the identity owner check fails, or dialog admission occurs.

### AC-4: A2 - open dialog result

- **Given:** grant and a real macOS Open panel with an ordinary file selected.
- **When:** `dialog.showOpenDialog()` completes; then Cancel; then `multiSelections`
  with multiple files.
- **Then:** selected returns
  `{canceled:false,filePaths:[<actual selected absolute POSIX path>]}`. Cancel,
  including an empty selection, returns `{canceled:true,filePaths:[]}`.
  `multiSelections` preserves selection order, and the returned path count is bounded
  before a response is allocated.
- **Negative control:**
  - **Single fault:** return the last selected path on Cancel, or force
    directories-only when both `openFile` and `openDirectory` are present.
  - **At-risk precondition:** a native Open panel returns a selection or Cancel.
  - **Expected RED:** ordinary-file selection/result fails, or the directory case
    fails.

### AC-5: A3 - save dialog result

- **Given:** a real selected save destination and `securityScopedBookmarks` absent or
  false.
- **When:** `dialog.showSaveDialog()` completes, or Cancel is chosen, or
  `securityScopedBookmarks:true` is requested.
- **Then:** selected returns `{canceled:false,filePath:<absolute path>}`. Cancel returns
  `{canceled:true,filePath:""}`. The supported mode creates or writes no file, opens no
  scope and implies no later `node:fs` write. `securityScopedBookmarks:true` is rejected
  before presenting a dialog; it is not silently accepted, and no fake bookmark data is
  returned.
- **Negative control:**
  - **Single fault:** create or truncate the target in supported mode, or silently
    accept bookmarks.
  - **At-risk precondition:** a Save panel or bookmark option reaches native
    presentation.
  - **Expected RED:** the sentinel file changes, or the bookmark denial control passes.

### AC-6: A4 - message dialog result

- **Given:** buttons and no checkbox label, then a checkbox label.
- **When:** a native button is selected in async `showMessageBox()`.
- **Then:** no-checkbox returns
  `{response:<actual selected zero-based index>,checkboxChecked:false}`. With a label,
  the actual native checked state is returned. A wrong index, Promise-returned-as-value,
  synthetic click or JS-only mock does not pass.
- **Negative control:**
  - **Single fault:** always return `defaultId`.
  - **At-risk precondition:** a real native message panel reports a non-default click.
  - **Expected RED:** the non-default click fails.

### AC-7: A5 - parent identity

- **Given:** a host-minted `WindowRef` under the current authenticated caller role
  generation and a real native UI object, or an absent parent, or a stale, destroyed,
  cross-app or forged parent.
- **When:** the host admits the dialog and presents it, then rechecks owner and ref at UI
  claim and before publishing a result.
- **Then:** a valid parent uses the async upstream sheet API whether visible or hidden;
  only an absent parent uses an independent alert/panel. A stale, destroyed, cross-app or
  forged parent is rejected before native presentation. Retirement cancels or drains the
  owned panel rather than retargeting its result. Hidden-parent async
  appearance/completion remains UNKNOWN until the pinned Electron and Keld native oracle
  cases run; source branch equality does not prove parity.
- **Negative control:**
  - **Single fault:** use a public numeric id, getter or `instanceof`; bypass the Host
    incarnation/role check; or replace a hidden sheet with a detached modal.
  - **At-risk precondition:** a supplied parent or forged wrapper reaches parent lookup
    or result publication.
  - **Expected RED:** a forged parent is admitted, a result attaches to the wrong owner,
    or the selected source branch changes.

### AC-8: A6 - independent native custody

- **Given:** an admitted dialog with one native object, one generation-bound original
  CALL and one terminal outcome.
- **When:** app or Bun role death, link failure, parent destruction, user cancellation,
  interaction expiry or session terminal Quit occurs; then a late completion arrives.
- **Then:** each admitted dialog owns exactly one native object and one terminal
  outcome. A later cleanup-only Cancel CALL is separate and never creates a second
  dialog. The event causes observable physical panel closure and native callback
  quiescence even if the caller is gone. A late completion is ignored after cleanup and
  cannot answer or mutate a replacement role/window. The next valid native dialog
  succeeds in the surviving host.
- **Negative control:**
  - **Single fault:** drop the callback owner without closing the panel.
  - **At-risk precondition:** a role or parent dies while a native panel is outstanding.
  - **Expected RED:** the native census detects a leaked panel.

### AC-9: A7 - UI-thread delivery

- **Given:** a visible native dialog and the normal current-host UI command path.
- **When:** terminal cleanup or panel cancellation must be delivered, including while
  `NSModalPanelRunLoopMode` is active.
- **Then:** no AppKit object is constructed or operated off the macOS main UI thread.
  The real `AppWindowCommand` or panel cancellation is delivered in the active mode. A
  default-mode-only completion source fails the test. No registry, guard or generation
  lock is held across presenting an OS panel or invoking user callbacks. A simulated
  CFRunLoop block alone is not proof.
- **Negative control:**
  - **Single fault:** deliver UI cleanup only in default mode.
  - **At-risk precondition:** a native modal owns the main run loop.
  - **Expected RED:** the watchdog or control fails.

### AC-10: A8 - path-only authority

- **Given:** the explicitly supported non-bookmark profile and an OS-selected path.
- **When:** Open or Save returns, and an `@keld/api` guarded `fs.read/write` immediately
  follows.
- **Then:** results carry only OS-selected absolute POSIX paths, with no backslash, NUL,
  native bookmark, file descriptor, retained capability or new manifest entry. Explicit
  `securityScopedBookmarks:true` and unimplemented Electron options are rejected with a
  typed unsupported outcome before panel creation. Unrepresentable selections produce a
  typed failure with no fabricated path. The following FS operation remains denied
  unless an independently declared grant authorized it. Aliases and symlinks require
  separately observed OS fixtures before parity claims.
- **Negative control:**
  - **Single fault:** treat the selected path as an FS scope.
  - **At-risk precondition:** a selected path exists without an independent FS grant.
  - **Expected RED:** the denied guarded read succeeds.

### AC-11: A9 - bounded inputs and effects

- **Given:** oversized strings, too many buttons or filters, an invalid result index,
  too many selected paths, unknown enum discriminants, a wrong correlation or an
  unallocated dialog channel.
- **When:** the request is admitted or retried, including after an ambiguous transport
  outcome (IPC-022/023).
- **Then:** each fails before unbounded allocation or presentation. One request cannot
  allocate a second panel on retry. No unlimited queue exists.
- **Negative control:**
  - **Single fault:** remove the selected-path ceiling.
  - **At-risk precondition:** the boundary value plus one reaches allocation.
  - **Expected RED:** the max+1 value is accepted.

### AC-12: A10 - non-macOS and strict behavior

- **Given:** no implemented native backend on Windows/Linux, or a Strict session with a
  granted UI dialog.
- **When:** a dialog is requested, or filesystem access follows a selection.
- **Then:** Windows/Linux return typed unsupported, not a fake `canceled:true` or a
  KELD-compatible pass. Legacy path-only operations disclose forfeited `node:fs`
  containment. Under Strict, even a granted UI dialog does not implicitly authorize
  arbitrary filesystem access after selection.
- **Negative control:**
  - **Single fault:** return a fabricated Cancel on an unsupported platform.
  - **At-risk precondition:** a non-macOS call or a post-selection strict FS read occurs.
  - **Expected RED:** the unsupported-platform cell passes, or the strict read succeeds.

### AC-13: A11 - first-proof conformance

- **Given:** the actual #477 expected-fail async cells and the GH566 registry.
- **When:** the cells are registered and executed before the corresponding
  implementation, then the real Mac result passes.
- **Then:** the implementing PR inverts only its owned cells together with observed
  native acceptance. The one GH566 parser, platform labels and frozen 44.3.0 lifecycle
  bytes stay unchanged.
- **Negative control:**
  - **Single fault:** remove the mapped test from execution.
  - **At-risk precondition:** a cell flips without native proof.
  - **Expected RED:** manifest admission fails.

### AC-14: A12 - source-exact option admission

- **Given:** the unmodified pinned draw.io Open, Save and Message argument objects,
  including present-`undefined` and absent values.
- **When:** each call is presented on macOS, and each of the three Message buttons is
  chosen, cancelled or selected.
- **Then:** Open accepts `defaultPath`, `filters` and `properties`; Save accepts
  `defaultPath` and `filters`; present-but-`undefined` optional values normalize as
  absent. Message accepts `type:'question'`, `title:'Confirm'`, `noLink:true`, three
  buttons and indexes; `noLink` is validated but inert on macOS. A supported option with
  the wrong type, or an unknown key even with an `undefined` value, fails before a panel
  appears. The source-exact fixtures actually select, cancel and choose each of the
  three buttons.
- **Negative control:**
  - **Single fault:** treat all present-`undefined` values as unsupported, remove the
    `noLink` allowlist, or accept unknown `bogus:undefined`.
  - **At-risk precondition:** a source-exact draw.io options object reaches admission.
  - **Expected RED:** a valid call fails before the panel, or the unknown-key pre-UI
    denial control passes.

### AC-15: A13 - caller timeout and reentrancy custody

- **Given:** a blocking caller whose local deadline expires, an outstanding admitted
  original CALL, and a same-link Cancel.
- **When:** the Host admits the original late, the Cancel arrives, and the native result
  races the tombstone; a concurrent or reentrant call also arrives.
- **Then:** the native panel is physically retired within the approved short cleanup
  bound, even if the Host admits the original late. Exactly one panel may be outstanding
  per role; concurrent or reentrant attempts are typed `DialogBusy` before another panel
  exists. A purely local Worker `abandon` or typed `KELD-IPC-006` is insufficient: an
  independently observed same-link Host cancellation and a real AppKit
  close/callback-quiescence witness are mandatory. `CALL(c_orig)` and
  `Cancel(c_cancel)` are distinct correlations. A `REPLY(c_cancel)` proves only the
  Cancel disposition. A live Host separately terminates `c_orig` exactly once with
  `ERR(c_orig)` if cancellation won, or preserves an already committed
  `REPLY(c_orig)` if the native result won. The Worker discards the late original
  terminal and releases its abandoned entry, without treating either frame as proof of
  rollback. The 257th successful timeout/cancellation on one healthy link must not hit
  `KELD-IPC-027`.
- **Negative control:**
  - **Single fault:** remove the Host Cancel relay or tombstone, suppress modal-mode
    physical cleanup, permit a stale callback, or remove `DialogBusy`.
  - **At-risk precondition:** a caller times out while the original is pending, or a
    second call overlaps.
  - **Expected RED:** a late panel or leaked handle crosses the bound despite transport
    ACK/ERR, or two native panels appear.
