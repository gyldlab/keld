# Contract and acceptance

Part of: [index](index.md)
Owns: sections 2 and 3

## 2. Spec refs

Tracker context: GH-478 is tracked under #517; KEL-80 is its reference.

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
- GH532 (`docs/specs/gh532-first-proof-evidence-rules.md`) and
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
  `src/main/electron.js:4191-4219@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e` constructs Open/Save options with
  `defaultPath`, `filters` and Open `properties`, including keys whose
  value is `undefined`; `src/main/electron.js:876-887@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e` constructs the synchronous message
  options including `type:'question'`, `title:'Confirm'` and `noLink:true`.
  This is caller-source evidence, not proof that a KELD broker works.
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

The migration maps legacy A1–A13 to AC-1–AC-13 one-to-one. The acceptance meaning is unchanged.

### AC-1: exact grant and source

- **Given:** One real app role has a verified policy snapshot and the exact system dialog grant.
- **When:** The caller submits one dialog request.
- **Then:** Given one real app role and verified manifest
  `{"app":{"system":["dialog"]}}`, a valid dialog CALL reaches native admission
  **once**. With an absent, empty, malformed, duplicated or wildcard-like literal,
  non-app principal, unverified digest or wrong role, admission returns an existing
  canonical **typed KELD-GUARD denial** whose fix names `/app/system`, with **zero**
  NSPanel/NSAlert creation. No developer shortcut and no automatically enabled legacy.
- **Negative control:**
  - **Single fault:** Remove guard evaluation
  - **At-risk precondition:** One real app role has a verified policy snapshot and the exact system dialog grant.
  - **Expected RED:** denied call shows native UI and test fails

### AC-2: open dialog result

- **Given:** The exact grant is admitted and a real macOS open panel is visible.
- **When:** The user selects a file or cancels the open panel.
- **Then:** Given grant and a selected ordinary file on a real
  Mac, `dialog.showOpenDialog()` resolves a Promise to
  `{canceled:false,filePaths:[<actual selected absolute POSIX path>]}`. For native
  Cancel (including an empty selection), `{canceled:true,filePaths:[]}`. When
  `multiSelections` is allowed, the result preserves selection order; the maximum
  number of returned paths is bounded before a response is allocated.
- **Negative control:**
  - **Single fault:** Return last selected path on Cancel, or force directories-only when both flags are present
  - **At-risk precondition:** The exact grant is admitted and a real macOS open panel is visible.
  - **Expected RED:** ordinary-file selection/result fails; a files-only mutant fails the directory case.

### AC-3: save dialog result

- **Given:** The exact grant is admitted and a real macOS save panel is visible.
- **When:** The user selects a save destination or cancels the save panel.
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
  - **Single fault:** Create/truncate target in supported dialog mode or silently accept bookmarks
  - **At-risk precondition:** The exact grant is admitted and a real macOS save panel is visible.
  - **Expected RED:** sentinel/denial control fails

### AC-4: message dialog result

- **Given:** The exact grant is admitted and the real native message panel has the requested buttons.
- **When:** The user selects a message button and optional checkbox state.
- **Then:** Given buttons and no checkbox label, a native
  selected button produces `{response:<actual selected zero-based index>,
  checkboxChecked:false}` from async `showMessageBox()`. A wrong button index,
  Promise-returned-as-value, synthetic click or JS-only mock does **not** pass.
  With a checkbox label, the actual native checked state is returned.
- **Negative control:**
  - **Single fault:** Always return defaultId
  - **At-risk precondition:** The exact grant is admitted and the real native message panel has the requested buttons.
  - **Expected RED:** a non-default click fails

### AC-5: parent identity

- **Given:** The request uses a Host-minted WindowRef and the current authenticated role.
- **When:** The Host resolves the requested parent before native presentation.
- **Then:** The host resolves a supplied **host-minted**
  `WindowRef` under the *current authenticated caller role generation* and real
  native UI object, not by caller-controlled numeric OS handle. A shown parent
  uses an attached modal **sheet**; no parent or a valid but hidden parent uses
  an independent alert/panel. A stale, destroyed, cross-app or forged parent is
  rejected **before** native presentation (not silently retargeted to a different
  window). The hidden-parent fallback itself is not a grant widening.
- **Negative control:**
  - **Single fault:** Accept recycled WindowId with old generation
  - **At-risk precondition:** The request uses a Host-minted WindowRef and the current authenticated role.
  - **Expected RED:** UI attaches and fails

### AC-6: independent native custody

- **Given:** One dialog is admitted and its native object, callback and generation are retained.
- **When:** The role, link, parent or session retires, or the user cancels.
- **Then:** Each admitted dialog owns exactly one
  native object, one generation-bound original CALL and its one terminal outcome.
  A later cleanup-only Cancel CALL is separate and never creates a second dialog.
  App/Bun role death, link failure, parent destruction, user cancellation,
  interaction expiry or session terminal Quit causes an observable physical panel
  closure and native callback quiescence, even if the caller is gone. A late
  completion is ignored after cleanup and cannot answer or mutate the replacement
  role/window. The next valid native dialog succeeds in the same surviving host.
- **Negative control:**
  - **Single fault:** Drop callback owner without closing panel
  - **At-risk precondition:** One dialog is admitted and its native object, callback and generation are retained.
  - **Expected RED:** native census detects leak

### AC-7: UI-thread delivery

- **Given:** A native modal is active on the macOS main UI thread.
- **When:** A guarded UI command or cancellation must reach the active panel.
- **Then:** No AppKit object is constructed or operated off the
  macOS main UI thread. While a dialog is visible, the normal current-host UI
  command path can still receive the required terminal cleanup. If any supported
  path enters `NSModalPanelRunLoopMode`, delivery of **the real**
  `AppWindowCommand`/panel cancellation is witnessed there. A default-mode-only
  completion source must fail the test; a simulated CFRunLoop block alone is not
  proof of the KELD bridge. Do not hold registry, guard or generation locks
  across presenting an OS panel or invoking user callbacks.
- **Negative control:**
  - **Single fault:** Deliver UI cleanup only in default mode
  - **At-risk precondition:** A native modal is active on the macOS main UI thread.
  - **Expected RED:** watchdog/control fails

### AC-8: path-only authority

- **Given:** The broker returns only an OS-selected path and no independent FS grant exists.
- **When:** The app attempts a guarded filesystem operation on the selected path.
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
  - **Single fault:** Treat selected path as FS scope
  - **At-risk precondition:** The broker returns only an OS-selected path and no independent FS grant exists.
  - **Expected RED:** denied read succeeds and fails

### AC-9: bounded inputs/effects

- **Given:** The request or reply is subject to the declared finite envelope and count limits.
- **When:** The caller supplies an invalid or over-limit input, result or correlation.
- **Then:** Oversized strings, too many buttons/filters,
  invalid result index, overly many selected paths, unknown enum discriminants,
  wrong correlation and an unallocated dialog channel fail before unbounded
  allocation/presentation. One dialog request cannot allocate a second panel on
  retry or on an ambiguous transport outcome (IPC-022/023). No unlimited queue.
- **Negative control:**
  - **Single fault:** Remove selected-path ceiling
  - **At-risk precondition:** The request or reply is subject to the declared finite envelope and count limits.
  - **Expected RED:** max+1 is accepted and fails

### AC-10: non-macOS and strict behavior

- **Given:** The caller runs on an unsupported OS or under Strict containment.
- **When:** The caller invokes a dialog or accesses a selected path.
- **Then:** Without an implemented native backend,
  Windows/Linux are typed unsupported, **not** a fake `canceled:true` or a
  KELD-compatible pass. Legacy path-only operations must disclose forfeited
  `node:fs` containment. Under Strict, *even a granted UI dialog* does not
  implicitly authorize arbitrary filesystem access after selection.
- **Negative control:**
  - **Single fault:** Return fabricated Cancel on unsupported platform
  - **At-risk precondition:** The caller runs on an unsupported OS or under Strict containment.
  - **Expected RED:** cell fails

### AC-11: first-proof conformance

- **Given:** The pinned GH566 registry contains the expected-fail compatibility cells.
- **When:** An implementing PR evaluates and flips its owned conformance cells.
- **Then:** The actual #477 expected-fail async cells
  are registered and executed before corresponding implementation. When this
  broker's real Mac result passes, the implementing PR inverts only its owned
  cells **together with** observed native acceptance; the one GH566 parser,
  platform labels and frozen 44.3.0 lifecycle bytes stay unchanged.
- **Negative control:**
  - **Single fault:** Remove mapped test from execution
  - **At-risk precondition:** The pinned GH566 registry contains the expected-fail compatibility cells.
  - **Expected RED:** manifest admission fails

### AC-12: source-exact option admission

- **Given:** The unmodified pinned draw.io caller supplies its real Open, Save or Message options.
- **When:** The facade validates and forwards the source-exact options.
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
  - **Single fault:** Treat all present-`undefined` as unsupported or remove `noLink` allowlist
  - **At-risk precondition:** The unmodified pinned draw.io caller supplies its real Open, Save or Message options.
  - **Expected RED:** source-exact valid call fails before panel; accept unknown `bogus:undefined`: pre-UI denial control fails

### AC-13: caller timeout and reentrancy custody

- **Given:** The caller deadline can expire before Host admission or while a panel is visible.
- **When:** The caller times out and sends a distinct same-link Cancel.
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
  - **Single fault:** Omit the original c_orig terminal after acknowledging c_cancel, or remove the Host Cancel relay/tombstone.
  - **At-risk precondition:** The caller deadline can expire before Host admission or while a panel is visible.
  - **Expected RED:** The abandoned-ID limit is exhausted, or a late panel/stale callback survives cancellation.
