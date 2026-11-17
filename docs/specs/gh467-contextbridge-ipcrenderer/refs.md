# References and provenance

Part of: [index](index.md)
Owns: section 2

## 2. Spec refs

| Normative owner | Consumed contract, no duplicate owner |
| --- | --- |
| [KEL-142](../kel142-macos-renderer-bridge-api/index.md) / architecture [05](../../architecture/05-webview-and-native.md) §§2–3 | Existing **trusted** named world + frozen `window.keld.invoke` page surface. New app world needs a *separately approved* amendment under GH-484. |
| [GH-441 option A](https://github.com/gyldlab/keld/issues/441#issuecomment-6022098697) and [GH-484 handoff](https://github.com/gyldlab/keld/issues/484#issuecomment-6381705299) | Page / application preload / KELD trusted worlds; App preload only, main frame, at document start. |
| [GH-540 spec PR #1122](https://github.com/gyldlab/keld/pull/1122), `docs/specs/gh540-compat-ipc.md` at `3d72e492916259d89b1bf4723329e602ea6e1691` | Approved-but-unmerged **bounded** one-`compat-control` message contract, host-opaque values, original/current document destinations, terminal consumer + release receipts, single application link. |
| [GH-541 spec PR #1123](https://github.com/gyldlab/keld/pull/1123), pinned at `72266ab2383403db3ec201d8388df66a2945862d` | Approved-but-unmerged spec only: checked u32 navigation, native document eligibility, byte-exact window grants and final Host dispatch. GH-542/GH-552 implementations remain prerequisites. |
| [Architecture 02](../../architecture/02-ipc.md), [03](../../architecture/03-security.md), [04](../../architecture/04-electron-compat.md) | Canonical link/guard boundary, explicit compatibility subset/divergence score, default deny. GH-540/GH-541 own proposed amendments. |
| [GH-464](https://github.com/gyldlab/keld/issues/464), [GH-567](https://github.com/gyldlab/keld/issues/567), [GH-532](../gh532-first-proof-evidence-rules.md), [GH-566](../gh566-corpus-manifest-owner.md) | Pinned conformance cells, expected-fail baseline before implementing, implementation-ticket keys, digest/validator/negative-control admission. |
| [GH-465](https://github.com/gyldlab/keld/issues/465), [GH-543](https://github.com/gyldlab/keld/issues/543), [GH-544](https://github.com/gyldlab/keld/issues/544), [GH-466](https://github.com/gyldlab/keld/issues/466) | Native one-way send, addressed events, sole endpoint IPC codec and ipcMain/webContents owner. |
| [GH-473](https://github.com/gyldlab/keld/issues/473), [GH-449](https://github.com/gyldlab/keld/issues/449), GH-452/GH-570 | Authentic committed `file://` and CSP, existing keyed window/facade, canonical process facts. |
| [GH-1095](../gh1095-request-error-codes.md), [KEL-74](../kel74-compat-evidence-schema.md) | Typed failure/settlement owner, no result or compatibility-score laundering. |

Pinned upstream oracle: Electron **v44.4.5**
`694f45852a0f1726cd23bfd379854de489cccb65`, notably
[contextBridge documentation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/context-bridge.md),
[native converter](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/renderer/api/electron_api_context_bridge.cc),
[upstream cases](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/spec/api-context-bridge-spec.ts),
and IPC documentation. Source blob hashes observed:
context-bridge.md `d5bddc8526c0cf7d60f5efdb24547e9af4646a62`;
converter `ade25537bc54a242f772461d946524ad0f396fef`;
spec `c549e304c7f1fc0f945317157321bded28748b2e`.

Pinned draw.io desktop
`2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`,
[unmodified preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js)
(blob `44f2f22f159a8a845ff7e573dd10b816383703ec`)
and webapp submodule `24b76c2cbd55d88e354042e8d329a2e4708972bc`.
Preload installs `ipcRenderer.on('mainResp')` and
`ipcRenderer.on('fileChanged')` before exposing `electron` and `process`;
`request` *mutates* `msg.reqId`; `watchFile` retains and deletes
`msg.listener` before `send('rendererReq', msg)`.

**2026-10-25 pinned page-caller correction (evidence, not approval):**
The [original current webapp `ElectronApp.js`](https://github.com/jgraph/drawio/blob/24b76c2cbd55d88e354042e8d329a2e4708972bc/src/main/webapp/js/diagramly/ElectronApp.js#L694-L715)
calls `requestSync('isFullscreen')` in `EditorUi.init`; the helper
converts it to `{action:'isFullscreen'}`, the unchanged preload assigns a
dynamic `reqId` and sends `rendererReq`, and the desktop main
[`rendererReq` listener](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L4431-L4543)
replies on `mainResp` with a Boolean. The `isPluginsEnabled` main
switch case is a legacy fallback: none of the 406 checked-out pinned
current webapp JavaScript files invokes it. Prior two-request native
`isPluginsEnabled` fixtures called the preload explicitly and sent
synthetic responses; do not score them as genuine page-origin traffic.
A second *natural* page `isFullscreen` request follows the fullscreen
UI action's `sendMessage('toggleFullscreen')`, which needs a **separate
approved** `el:toggleFullscreen` grant; never infer or widen it from
`el:rendererReq`. A conditional `getLocalFonts` request may occur
earlier; do not assume `reqId=1`. See [source owner receipt](https://github.com/gyldlab/keld/issues/467#issuecomment-6392657686).


Pinned page-listener census: the unmodified `ElectronApp.js` registers `isModified`, `removeDraft`, `saveAndClose` and `args-obj`.
It uses `listenOnce` for `export-success` and `export-error` from `mxElectronRequest('export')`. See [pinned page
source](https://github.com/jgraph/drawio/blob/24b76c2cbd55d88e354042e8d329a2e4708972bc/src/main/webapp/js/diagramly/ElectronApp.js#L458-L500),
[startup
registration](https://github.com/jgraph/drawio/blob/24b76c2cbd55d88e354042e8d329a2e4708972bc/src/main/webapp/js/diagramly/ElectronApp.js#L893-L899)
and [one-shot export
listeners](https://github.com/jgraph/drawio/blob/24b76c2cbd55d88e354042e8d329a2e4708972bc/src/main/webapp/js/diagramly/ElectronApp.js#L2634-L2650).

The pinned preload registers `mainResp` and `fileChanged` internally. Its `request` stores `reqInfo[reqId]` before sending
`rendererReq`. Its `mainResp` handler calls the success/error callback, then deletes that key. See [pinned preload lines
61–123](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js#L61-L123).
Owner decision: the GH-484 issue owner selected A2 as the data-only page carrier and superseded the earlier A2 write hold. See [A2 override](https://github.com/gyldlab/keld/issues/484#issuecomment-6560086477) and [A2 correction handoff](https://github.com/gyldlab/keld/issues/484#issuecomment-6560285327). This selects a design direction only; it does not claim native qualification or production approval.

Working baseline: KELD origin/main
`ec29b7ce2b155a8c98f96036cbb9c54488b5c302`, 2026-11-17.
`packages/@keld/electron/src/index.ts` currently exports the
main-role app/link, not a renderer-side contextBridge or ipcRenderer.
Existing native `keld-wv` bridge is KEL-142 v1; its page facade is not
an Electron preload runtime.

**Physical Mac F0 evidence update (2026-10-24; not product acceptance):**
Real WKWebView on macOS 26.5.1 executed the **unmodified pinned draw.io
preload** in a named app world under a deliberately **test-only**
contextBridge/ipcRenderer shim. Eager page-world API publication was
unreliable (each of two injection-order variants passed only 4/8 runs).
An order-independent, synchronous, two-way document-start publication
prototype exposed the four app-registered methods and copied, frozen
process metadata at the first inline script **16/16** times; suppressing
the `electron` registration removed them **3/3** times. A bounded
local page-to-preload proxy then invoked the actual unmodified
`request` method, observed preload-mutated `reqId=1,2,3` in a
**simulated** send log, and delivered two nested `watchFile` callbacks
in **6/6** runs. Omitting only nested callback revival removed those
two change events, leaving ordinary callbacks intact (**6/6** negative
controls). The independent native findings and source hashes are
recorded on [#484](https://github.com/gyldlab/keld/issues/484#issuecomment-6382401714)
and [#467](https://github.com/gyldlab/keld/issues/467#issuecomment-6382468009).
These fixtures use page-visible, forgeable DOM metadata/events and
synthetic `mainResp` values; **they do not prove an approved bridge,
native Host identity/grants, Bun handlers, transport release, actual
application round-trip or Electron parity**.

**Separate native credential feasibility, still not product proof:** the
[2026-10-24 physical-Mac two-world experiment](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584)
observed 33 native receipts per run under one test-owned checked-u32
document epoch and two distinct world-scoped 128-bit credentials (3/3
corrected baselines; credential-check and role-operation mutants RED).
It exposed a fixture bug where absent stored and supplied optional tokens
compared equal, then corrected the fixture to require an actual stored
binding and supplied string. This does **not** implement KELD's #542
document admission, #540 v2 binding or #552 guard, nor authenticate the
separate page-to-preload callable publication mechanism.
