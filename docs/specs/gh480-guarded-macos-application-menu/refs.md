# References

Part of: [index](index.md)
Owns: section 2

## 2. Spec refs: first-proof source facts
 tracking: GH-480 (parent #517; KEL-102 reference).

- `docs/architecture/03-security.md` §2 (default deny, verified policy,
  principal authority); `docs/architecture/04-electron-compat.md` §3
  (facade over shared Host app link); `docs/architecture/05-webview-and-native.md`
  §3 (guarded native module, actual UI-thread owns native handles).
- **Approved** `docs/specs/gh508-kipc-channel-table.md` §4.5: the
  projected menu table entry is `menu`, app→Host `GuardedCall` against the
  exact `app.system` `menu` literal. Its existing receive-policy table
  **does not authorize menu activation EVENTs** on that guarded channel;
  hence F06-T9 click delivery is explicitly deferred. The implementing
  owner allocates the next real id only when it lands; no speculative id.
- **Approved** `docs/specs/gh531-window-registry-close-state-machine.md`
  §4.d–4.g: host-minted WindowRef, current focused native window,
  app-link authentication and separate actual GH-450 CloseRequested/Veto
  owner. [GH-449 draft stack](https://github.com/gyldlab/keld/issues/449)
  is **unmerged**; menu cannot mint its own window id or shortcut.
- `docs/specs/gh527-worker-owned-blocking-call-transport.md`: the one
  authenticated role WorkerLink. Host writes and native completion share
  that link, no second HELLO/queue or custom framing.
- Shared `app.system` literal decision F06-D3 A and [GH-478 proposed
  dialog broker spec #1161](https://github.com/gyldlab/keld/pull/1161):
  **one** `keld-guard` resource-less system-literal evaluator owns both
  `dialog` and `menu` grants. GH478's implementation/spec is **not yet
  approved or shipped**. GH480 must reuse its accepted primitive or win a
  separately reviewed single owner; it may not introduce a second parser.
- Pinned Electron v44.4.5 tag commit
  [`694f45852a0f1726cd23bfd379854de489cccb65`](https://github.com/electron/electron/tree/694f45852a0f1726cd23bfd379854de489cccb65),
  official [`menu.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu.md),
  [`menu-item.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu-item.md)
  and [`tutorial/menus.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/tutorial/menus.md).
  Exact [`lib/browser/api/menu-item.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/menu-item.ts)
  lowercases role strings; the tutorial
  explicitly defines roles as case-insensitive and supplies default labels
  **and accelerators**. Pinned
  [`menu-item-roles.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/menu-item-roles.ts)
  assigns the built-in
  macOS Cmd+W/Z/C/V/H/Q equivalents, among others. A recognized role
  takes priority over its `click` callback. Source-derived
  no-focused-window fallback remains **unknown/unscored** until a
  dedicated pinned source/native witness.
- Pinned draw.io [`jgraph/drawio-desktop@2edf9fb:src/main/electron.js`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb/src/main/electron.js#L1949-L2069):
  its actual Mac template has **three top-level menus**, twelve role
  items, custom-click items, separators, and `autoCheckForUpdates` with
  `type:'checkbox'`, `checked: store == null || store.get('dontCheckUpdates') !== true`
  and a non-role `click(menuItem)`. Under `disableUpdate`, three update
  items including that checkbox are removed before construction.
  The exact source spells `role:'hideothers'` in lowercase; role-only
  items supply **no explicit keyboard accelerators**.
- [GH480 real macOS AppKit probe (source, command and stdout)](https://github.com/gyldlab/keld/issues/480#issuecomment-6388101245)
  (macOS 26.5.1 arm64, Apple clang 17, SDK 26.2):
  `NSMenuItem.state` retained `NSControlStateValueOn=1` and
  `NSControlStateValueOff=0`; a no-op programmatic checkbox action
  retained its check mark. `NSMenu.autoenablesItems` defaults to
  **YES**, and `[menu update]` with an enabling validator re-enabled
  a manually disabled item; **NO** preserved explicit disabled state.
  Synthetic `NSMenu.performKeyEquivalent(Cmd+W)` returned **YES**
  on a disabled Close item with **zero target calls**, then invoked
  the target when enabled. This is native object/in-process
  dispatch evidence only, **not** real global keyboard routing,
  KELD Host guard, first responder or dirty close-veto proof.
- [GH-477 existing DRAFT corpus #1156](https://github.com/gyldlab/keld/pull/1156)
  contains the first-proof `Menu.buildFromTemplate`, `setApplicationMenu`,
  role ignores-click and role-close **expected-fail** cells (current
  head `260974757fe92490d11bbcbf579901a263af32a4`). The role-quit
  cell belongs to [GH-563](https://github.com/gyldlab/keld/issues/563)
  and **must remain expected-fail** until actual GH-563 implementation.
  Those green Bun *absence* tests are not native menu acceptance.

This document does **not** amend an existing approved API, channel or
window contract by itself. Final cross-owner permission and GH-450
close integration must be explicitly reviewed before the specification
can be marked approved/implemented.
