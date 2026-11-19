# Dialog references and acceptance

Part of: [index](index.md)
Owns: Sections 2 and 3

## 2. Spec refs: Provenance

- `docs/architecture/03-security.md` §2: immutable manifest, exact-capability
  default-deny and application-principal authority.
- `docs/architecture/04-electron-compat.md` §3–4: main-process facade over the
  single app link and honest per-operation compatibility records.
- `docs/architecture/05-webview-and-native.md` §3: native broker, UI-thread
  ownership and separation of host-minted windows from guard capabilities.
- `docs/specs/gh508-kipc-channel-table.md` §§4–5: **already approved** one
  append-only channel table, the proposed guarded `dialog` row and generated TS
  constant (no speculative numeric allocation or version change).
- `docs/specs/gh527-worker-owned-blocking-call-transport.md`: **already
  approved**, one WorkerLink with bounded frames, ordered wake and typed
  terminal errors. The sync consumer is #564, not this broker.
- `docs/specs/gh531-window-registry-close-state-machine.md`: **approved** host
  `WindowRef=(WindowId,WindowGeneration)` and role-owned close/recovery;
  application-window handle ownership is #449/#450, not this broker.
- `docs/specs/gh532-first-proof-evidence-rules.md` and
  `docs/specs/gh566-corpus-manifest-owner.md`: pinned 44.4.5 expected-fail
  cells and one conformance runner (not the frozen 44.3.0 lifecycle corpus).
- Electron v44.4.5 pinned commit
  `694f45852a0f1726cd23bfd379854de489cccb65`,
  [`docs/api/dialog.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/dialog.md)
  and [`message_box_mac.mm`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/message_box_mac.mm).
  Recorded real-Mac oracle: [KEL-237](https://linear.app/gyldlab-keld/document/electron-4445-macos-drawio-closequit-oracle-24-oct-2026-1718a506d0c7),
  [#442](https://github.com/gyldlab/keld/issues/442#issuecomment-6381736061).
- Pinned, unmodified draw.io Desktop
  [`jgraph/drawio-desktop@2edf9fb`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js):
  Lines 4191-4219 of that external file construct Open/Save options with
  `defaultPath`, `filters` and Open `properties`, including keys whose
  value is `undefined`. Lines 876-887 construct the synchronous message
  options including `type:'question'`, `title:'Confirm'` and `noLink:true`.
  This is caller-source evidence, not proof that a KELD broker works.
- Pinned [macOS file-dialog implementation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/file_dialog_mac.mm)
  `SetAllowedFileTypes` and `PopUpButtonHandler::selectFormat` determine
  initial/group-switch filter behavior; source observation is not a Keld run.
- [Approved GH499 migration contract](https://github.com/gyldlab/keld/blob/9959cb45608164c789e7e2e7fb7ab0e5eed039ca/docs/specs/gh499-drawio-hand-authored-migration.md)
  §4.4 and [exact owner approval](https://github.com/gyldlab/keld/issues/499#issuecomment-6465227830)
  permit the separately gated macOS dirty-close async adaptation.
- [#478 acceptance brief](https://github.com/gyldlab/keld/issues/478),
  [#564 consumer handoff](https://github.com/gyldlab/keld/issues/564#issuecomment-6381751522),
  [#528 transport acceptance](https://github.com/gyldlab/keld/issues/528#issuecomment-6370844113).

This **draft** PR proposes a narrow change to the existing `dialog`
architecture row to reconcile path-only/no-implicit-grant first-proof
behavior. The architecture/FS owner must independently review that
amendment before it is approved. `keld-native::MODULES` listing `dialog`
means planned surface, **not** that a broker currently exists.
`keld-guard` currently supports scoped arrays under `app.<group>.<action>`;
`app.system` literal-list admission does **not** exist yet and must be implemented
by its owning crate. No call may pretend `evaluate("system.dialog", "dialog")`
implements that different grammar.

## 3. Acceptance criteria

The original A1 through A13 map to AC-1 through AC-13. AC-1 retains the A1a, A1b and A1c cases.
The negative controls below come from the source test plan. The full set remains in [delivery.md](delivery.md).

### AC-1: Boot policy, exact grant and caller identity

- **Given:** The retained policy is invalid at boot, or a verified policy is present at the live CALL boundary.
- **When:** Attempt startup with invalid policy, or send a dialog CALL to the live Host.
- **Then:** The three source cases retain independent outcomes.

  **A1a — immutable-policy boot preflight.** Malformed JSONC, duplicate
  object keys, invalid bytes or a mismatched verified digest are rejected by
  the existing retained-handle policy loader before starting an app child,
  bootstrap listener or application window/panel. Preserve the loader's existing
  typed startup error and resource-absence proof. No authenticated dialog CALL
  exists at this cut, so no correlated Guard ERR is promised. Do not move
  preflight after startup to manufacture one. The already opened policy handle
  is an input, not a newly permitted app resource.

  **A1b — exact grant on a valid live CALL.** Given a well-formed digest-verified
  manifest, an authenticated current app role and a valid dialog CALL, exactly
  `{"app":{"system":["dialog"]}}` reaches native admission once. Missing/empty
  system list, wrong node type, duplicate array literals, unknown literals and
  wildcard-like strings fail the proposed guard-owned evaluator with canonical
  typed `KELD-GUARD` ERR on that CALL and fix pointer `/app/system`, with zero
  native panel creation. Duplicate JSON object keys belong to A1a, not this atom.
  No developer shortcut or automatic legacy enablement.

  **A1c — caller identity before authorization.** An unauthenticated, stale or
  wrong-role link is refused by its existing owning bootstrap/session rule with
  zero dialog admission and no fabricated CALL response. Independently, the guard
  evaluator rejects non-AppProcess principals before grant lookup. A valid app
  link with an invalid parent is tested separately under AC-5; it does not bypass
  the identity/permission checks or become another principal.
- **Negative control:**
  - **Single fault:** Move policy verification after app startup.
  - **At-risk precondition:** The retained-policy input is invalid at boot.
  - **Expected RED:** An app child, bootstrap listener or native panel appears.

### AC-2: Open returns the selected paths

- **Given:** A granted caller owns a real macOS Open panel.
- **When:** Select an ordinary file or cancel.
- **Then:** Given grant and a selected ordinary file on a real
  Mac, `dialog.showOpenDialog()` resolves a Promise to
  `{canceled:false,filePaths:[<actual selected absolute POSIX path>]}`. For native
  Cancel (including an empty selection), `{canceled:true,filePaths:[]}`. When
  `multiSelections` is allowed, the result preserves selection order; the maximum
  number of returned paths is bounded before a response is allocated.
- **Negative control:**
  - **Single fault:** Return the last selected path on Cancel.
  - **At-risk precondition:** Select a path, then cancel a subsequent Open.
  - **Expected RED:** The canceled result contains a path.

### AC-3: Save returns a path without creating a file

- **Given:** A granted caller owns a real macOS Save panel without security-scoped bookmarks.
- **When:** Select a destination or cancel.
- **Then:** Given a real selected save destination and
  `securityScopedBookmarks` absent/false, `dialog.showSaveDialog()` resolves to
  `{canceled:false,filePath:<absolute path>}`; Cancel resolves to
  `{canceled:true,filePath:""}`. The supported first-proof mode **does not
  create or write** a file, open a scope or imply a later `node:fs` write.
  `securityScopedBookmarks:true` is **explicitly unsupported and rejected before
  presenting** a dialog: the pinned Electron documentation says this option can
  create a blank selected file in Mac App Store builds. Do not silently accept
  it and claim the no-side-effect guarantee or return fake bookmark data.
- **Negative control:**
  - **Single fault:** Create the selected file during Save.
  - **At-risk precondition:** The selected destination does not exist before selection.
  - **Expected RED:** The destination exists before a separate application write.

### AC-4: Message returns the native button and checkbox state

- **Given:** A granted caller owns a real macOS Message panel.
- **When:** Click a non-default button.
- **Then:** Given buttons and no checkbox label, a native
  selected button produces `{response:<actual selected zero-based index>,
  checkboxChecked:false}` from async `showMessageBox()`. A wrong button index,
  Promise-returned-as-value, synthetic click or JS-only mock does **not** pass.
  With a checkbox label, the actual native checked state is returned.
- **Negative control:**
  - **Single fault:** Return the default button index for every click.
  - **At-risk precondition:** A different button is clicked.
  - **Expected RED:** The result index differs from the native selection.

### AC-5: Only the current host-minted parent can attach a panel

- **Given:** The caller supplies a current, stale or forged parent, or omits the parent.
- **When:** Request a native dialog.
- **Then:** The host resolves a supplied **host-minted**
  `WindowRef` under the *current authenticated caller role generation* and real
  native UI object, not by caller-controlled numeric OS handle. A supplied valid
  parent uses the asynchronous upstream **sheet** API, whether visible or hidden;
  only an absent parent uses an independent alert/panel. This follows the pinned
  async native source branch, not the synchronous method's visibility fallback.
  Actual hidden-parent async appearance/completion remains **UNKNOWN** until the
  pinned Electron and Keld native oracle cases run; source branch equality alone
  does not prove parity. Do not invent a hidden parent, retarget to another window
  or silently replace the sheet with a detached modal. A stale, destroyed,
  cross-app or forged parent is rejected before native presentation. The Host
  rechecks current owner/ref at UI claim and again before publishing a result;
  retirement cancels/drains the owned panel rather than retargeting its result.
- **Negative control:**
  - **Single fault:** Replace the supplied hidden parent sheet with a detached modal.
  - **At-risk precondition:** A genuine hidden parent enters the supplied-parent path.
  - **Expected RED:** The presenter's native branch differs from the pinned async source branch.

### AC-6: Native custody drains independently of the caller

- **Given:** A real panel is admitted under one authenticated generation.
- **When:** Retire the role and observe cleanup.
- **Then:** Each admitted dialog owns exactly one
  native object, one generation-bound original CALL and its one terminal outcome.
  A later cleanup-only Cancel CALL is separate and never creates a second dialog.
  App/Bun role death, link failure, parent destruction, user cancellation,
  interaction expiry or session terminal Quit causes an observable physical panel
  closure and native callback quiescence, even if the caller is gone. A late
  completion is ignored after cleanup and cannot answer or mutate the replacement
  role/window. The next valid native dialog succeeds in the same surviving host.
- **Negative control:**
  - **Single fault:** Drop callback custody without closing the panel.
  - **At-risk precondition:** The panel is still visible when its role dies.
  - **Expected RED:** The native census detects a retained panel.

### AC-7: The main UI thread delivers cleanup in the native mode

- **Given:** A real dialog is visible while the native UI owner is servicing its loop.
- **When:** Send the normal terminal UI command.
- **Then:** No AppKit object is constructed or operated off the
  macOS main UI thread. While a dialog is visible, the normal current-host UI
  command path can still receive the required terminal cleanup. If any supported
  path enters `NSModalPanelRunLoopMode`, delivery of **the real**
  `AppWindowCommand`/panel cancellation is witnessed there. A default-mode-only
  completion source must fail the test; a simulated CFRunLoop block alone is not
  proof of the KELD bridge. Do not hold registry, guard or generation locks
  across presenting an OS panel or invoking user callbacks.
- **Negative control:**
  - **Single fault:** Deliver cleanup only in the default run-loop mode.
  - **At-risk precondition:** An admitted native path is in NSModalPanelRunLoopMode.
  - **Expected RED:** The physical cleanup control times out.

### AC-8: Selected paths confer no guarded filesystem authority

- **Given:** An Open or Save result supplies a path with no independent matching FS grant.
- **When:** Attempt a guarded read or write of the selected path.
- **Then:** Open and save results in the *explicitly supported
  non-bookmark profile* carry **only** the OS-selected absolute POSIX paths,
  no backslash, NUL, native bookmark, file descriptor, retained capability or
  new manifest entry. Explicit `securityScopedBookmarks:true` and any
  unimplemented Electron option are rejected with a typed unsupported outcome
  **before** panel creation, not silently ignored. Unrepresentable path selections
  produce a typed failure with no fabricated path. An immediately following
  `@keld/api` guarded `fs.read/write` of the selected path remains denied unless
  an independently declared FS grant already authorized that operation. Aliases
  and symlinks require separately observed OS fixtures before parity claims.
- **Negative control:**
  - **Single fault:** Treat the selected path as an FS scope.
  - **At-risk precondition:** Guarded FS receives a selection for which no grant exists.
  - **Expected RED:** The guarded read succeeds.

### AC-9: Dialog input and native effects remain bounded

- **Given:** A dialog reaches the owning decoder or result validation.
- **When:** Exercise the input and output limits.
- **Then:** Oversized strings, too many buttons/filters,
  invalid result index, overly many selected paths, unknown enum discriminants,
  wrong correlation and an unallocated dialog channel fail before unbounded
  allocation/presentation. One dialog request cannot allocate a second panel on
  retry or on an ambiguous transport outcome (IPC-022/023). No unlimited queue.
- **Negative control:**
  - **Single fault:** Remove the selected-path ceiling.
  - **At-risk precondition:** A result contains one path over the limit.
  - **Expected RED:** The oversized result is accepted.

### AC-10: Unsupported platforms and Strict keep their authority boundaries

- **Given:** A caller uses a platform without a native backend or exercises an explicit profile.
- **When:** Invoke the dialog and inspect the typed outcome and filesystem authority.
- **Then:** Without an implemented native backend,
  Windows/Linux are typed unsupported, **not** a fake `canceled:true` or a
  KELD-compatible pass. Legacy path-only operations must disclose forfeited
  `node:fs` containment. Under Strict, *even a granted UI dialog* does not
  implicitly authorize arbitrary filesystem access after selection.
- **Negative control:**
  - **Single fault:** Return fabricated Cancel on the unsupported platform.
  - **At-risk precondition:** There is no native backend.
  - **Expected RED:** The unsupported-platform conformance cell receives Cancel instead of the
    typed refusal.

### AC-11: Only exercised native conformance cells may flip

- **Given:** The existing GH566 registry maps the owned #477 async cases.
- **When:** Run the expected-fail cells before implementation and observe the native result
  before flipping.
- **Then:** The actual #477 expected-fail async cells
  are registered and executed before corresponding implementation. When this
  broker's real Mac result passes, the implementing PR inverts only its owned
  cells **together with** observed native acceptance; the one GH566 parser,
  platform labels and frozen 44.3.0 lifecycle bytes stay unchanged.
- **Negative control:**
  - **Single fault:** Remove a mapped test from execution.
  - **At-risk precondition:** The registry still contains the mapped cell.
  - **Expected RED:** Manifest admission fails.

### AC-12: Pinned draw.io options reach the real native panel

- **Given:** The pinned original draw.io call supplies the specified Open, Save or Message
  arguments.
- **When:** Run the unmodified arguments, including known optional values that are present but
  undefined.
- **Then:** The unmodified pinned draw.io
  `showOpenDialog` options (`defaultPath`, `filters`, `properties`) and
  `showSaveDialog` options (`defaultPath`, `filters`) are accepted with
  present-but-`undefined` optional values normalized as absent. The shared
  message broker accepts the actual `type:'question'`,
  `title:'Confirm'` and `noLink:true` alongside the three buttons and
  indexes; `noLink` is validated but inert on macOS. A supported option with
  the wrong type or an unknown key, **even when its value is undefined**,
  fails before a panel appears. Source-exact fixtures must actually select,
  cancel and choose each of the three message buttons on macOS.
- **Negative control:**
  - **Single fault:** Remove noLink from the permitted macOS Message options.
  - **At-risk precondition:** The pinned source submits its genuine noLink:true option.
  - **Expected RED:** The valid call fails before native panel creation.

### AC-13: Caller timeout retires the panel and original correlation

- **Given:** An original dialog CALL reaches a live role and the blocking caller times out.
- **When:** Deliver same-link Cancel and observe both correlations and physical custody.
- **Then:** After a blocking caller's
  local deadline, the native panel is physically retired within the approved
  short cleanup bound, even if the Host admits the original request late.
  Exactly one panel may be outstanding for a role; concurrent/reentrant
  attempts are typed `DialogBusy` failures before another panel exists.
  A purely local Worker `abandon` or typed `KELD-IPC-006` is not sufficient:
  an independently observed same-link Host cancellation and a real AppKit
  close/callback-quiescence witness are mandatory. This is an **unimplemented
  cross-owner gate**, not a current WorkerLink capability.
  The timed-out original `CALL(c_orig)` and subsequent `Cancel(c_cancel)`
  are distinct correlations. An ACK/`REPLY(c_cancel)` proves only the Cancel
  disposition; for a still-pending admitted original, a live Host must
  separately terminate `c_orig` exactly once with a registered `ERR(c_orig)`
  if cancellation won, or preserve an already committed genuine
  `REPLY(c_orig)` if the native result won. The Worker must
  discard that late original terminal frame and release its abandoned entry,
  without treating either terminal frame as proof that a native effect was
  rolled back. The 257th successful timeout/cancellation on one healthy link
  must not hit `KELD-IPC-027` from retained original correlations.
- **Negative control:**
  - **Single fault:** ACK Cancel without emitting the original terminal frame for an admitted
    pending original.
  - **At-risk precondition:** Successive caller timeouts abandon original correlations on a
    single healthy link.
  - **Expected RED:** Retained originals reach the limit and the 257th cleanup closes the link
    with KELD-IPC-027.
