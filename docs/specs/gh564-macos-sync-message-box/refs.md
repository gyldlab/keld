# Spec: GH-564 source references
Part of: [index](index.md)
Owns: section 2 source/approval and native prerequisites

## 2. Spec refs: factual boundary

- Approved `docs/specs/gh527-worker-owned-blocking-call-transport.md` §4.5
  (same role's sole WorkerLink), §4.6 (apply framework facts before return,
  deliver listeners **after** resume), §4.7 (typed correlation and receiver
  admissibility), §4.9 (nonblocking app.quit), the actual
  `packages/@keld/kipc/src/transport.ts` `WorkerLink.callBlocking`.
- [#528 reviewed macOS transport evidence](https://github.com/gyldlab/keld/issues/528#issuecomment-6370844113),
  `MAX_BLOCKING_CALL_DEADLINE_MS=24h` in current source, and existing ring
  byte/record limits. A **human dialog budget** is a new caller-specific
  deadline **below** that existing maximum, not an increase of it.
- Approved `docs/specs/gh531-window-registry-close-state-machine.md`:
  Host-minted WindowRef and role-owned `ClosePending` → `Veto` → `Open`
  before the application's later dirty/modal/save choices. Native release
  and `closed` belong to #449/#450; this API does not authorize closing.
- GH478's shared native broker contract is **approved** by the
  [October 28 owner-delegated v2 receipt](https://github.com/gyldlab/keld/issues/478#issuecomment-6476354568),
  input SHA-256 `a3137e6d826d93788845b57c4295917b34be54b61f7fcc939903ab3432fc73c2`.
  The approved text is present at
  [PR1306's exact source](https://github.com/gyldlab/keld/blob/16e519091992d551853b6d0434b85000499a3d3c/docs/specs/gh478-guarded-macos-dialog-broker.md).
  The integrating tree now contains the generated Message/Cancel codec,
  guarded Core route, native sheet owner and shared nonce cancellation.
  Older PR1161 draft descriptions are not the current approval receipt.
  Contract approval and source presence do not establish this consumer's
  native acceptance. Use the generated channel constant and existing Guard.
- GH532/GH566 single conformance owner; the current
  `crates/keld-compat/fixtures/native-corpus/corpus.json` maps the sync
  expected-fail case to `packages/@keld/electron/src/dialog.test.ts`.
  Historical [GH477 PR1156](https://github.com/gyldlab/keld/pull/1156)
  receipts do not prove the new synchronous consumer.
- Pinned Electron **v44.4.5** tag commit
  `694f45852a0f1726cd23bfd379854de489cccb65`,
  [`docs/api/dialog.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/dialog.md),
  [`message_box_mac.mm`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/message_box_mac.mm),
  and the [October 24 real-Mac oracle](https://linear.app/gyldlab-keld/document/electron-4445-macos-drawio-closequit-oracle-24-oct-2026-1718a506d0c7).
  The 75 source-shaped Electron traces are an **upstream** oracle, not a
  KELD native or unmodified installed draw.io acceptance run.
- Exact draw.io Desktop
  [`jgraph/drawio-desktop@2edf9fb:src/main/electron.js:849-939`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js)
  sets `modifiedModalOpen` **before** `showMessageBoxSync`, calls it
  with `type:'question'`, `title:'Confirm'` and `noLink:true` at
  lines 876–887, and destroys the window on an application
  `saveAndClose-result.success` at lines 896–905. These are distinct
  source facts from native user choice and durable file persistence.

No architecture amendment to GH527/GH531 is authorized. GH478's approved
broker contract does not approve its GH564 consumer. The bounded consumer
needs the separate receipt in §1.1; the broader target remains draft.
Resolve any conflict with the approved native broker in that owner before
expanding the consumer's scope.
