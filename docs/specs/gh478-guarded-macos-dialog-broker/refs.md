# Spec: GH-478: references and provenance

Part of: [index](index.md)
Owns: section 2, authority and exact predecessors

## 2. Spec refs: provenance

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
  [draw.io electron.js lines 4191-4219](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L4191-L4219)
  constructs Open/Save options with `defaultPath`, `filters` and Open
  `properties`, including keys whose value is `undefined`.
  [Lines 876-887](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L876-L887)
  construct the synchronous message options including `type:'question'`,
  `title:'Confirm'` and `noLink:true`.
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

