# References and provenance

Part of: [index](index.md)
Owns: section 2

## 2. Spec refs

| Source | Contract or observed state |
| --- | --- |
| [Architecture 04 §3](../../architecture/04-electron-compat.md#3-how-the-shim-works-layers) | A preload runs before page scripts; the renderer shim is distinct from native policy. |
| [Architecture 05 §2](../../architecture/05-webview-and-native.md#2-renderer-bridge-contract-windowkeld) | The generic bridge and engine seams; add an app-world clarification only after owner coordination. |
| [Architecture 01 §5](../../architecture/01-overview.md) and [03](../../architecture/03-security.md) | Performance budgets and default-deny native authority. |
| [Approved KEL-142](../kel142-macos-renderer-bridge-api/index.md) §§4–8, KEL-142 AC-2/AC-5/AC-6 | Trusted isolated world, host-minted document nonce, bounded `keld.wv-link/v1` parser and real macOS isolation tests. |
| [PANEL-D21 #441](https://github.com/gyldlab/keld/issues/441) | Decided **A**: third app-owned world and separate world-scoped native handler on the same per-view controller; reject sharing KEL-142 trusted world. |
| [#484 technical handoff](https://github.com/gyldlab/keld/issues/484#issuecomment-6381705299) | Proposed contracts and falsifiers; **not** a spec approval or native test result. |
| [KELD native macOS bridge at
`b4e9f4f2`](https://github.com/gyldlab/keld/blob/b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50/crates/keld-wv/src/wkwebview/macos_bridge.rs),
[`wv_link.rs`](https://github.com/gyldlab/keld/blob/b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50/crates/keld-wv/src/wv_link.rs) |
Existing `defaultClientWorld` + page facade scripts and one native handler at `AtDocumentStart`; native checks
world/view/frame/controller. **Current `decode_wv_link` is v1 only: bind and numeric-u16 invoke, no named `el:` send/event.** Bind
and `evaluate_in_world` are asynchronous; no app-world preload exists. |
| [#540 IPC v2 spec](https://github.com/gyldlab/keld/issues/540), [#465 canonical send
extension](https://github.com/gyldlab/keld/issues/465), [#543 host-event
implementation](https://github.com/gyldlab/keld/issues/543), [#464 IPC conformance](https://github.com/gyldlab/keld/issues/464),
[#541 grant amendment](https://github.com/gyldlab/keld/issues/541), [#542 navigation
mint](https://github.com/gyldlab/keld/issues/542) | Dependency contracts: v2 named send/event, bounded credit, one canonical
parser, exact `el:` grants, and generation rotation at cross-document commit plus earlier host-initiated-load invalidation. #540's
approved specification is **not** an implementation in current v1. World-scoped credential admission for *both* app/trusted
handlers is unresolved under #540/#542. |
| [KELD boot walker](https://github.com/gyldlab/keld/blob/b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50/crates/keld-core/src/app_session.rs#L1746-L1820) | Uses directory-relative `openat`, `O_NOFOLLOW`, regular-file and bounded read. It currently **drops** the root handle after boot and rejects symlinks, so is not yet a preload loader. |
| [Electron v44.4.5
`init.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/sandboxed_renderer/init.ts),
[`preload.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/sandboxed_renderer/preload.ts)
| Immutable sandboxed preload module map, wrapper and error wording, pinned to Electron commit
`694f45852a0f1726cd23bfd379854de489cccb65`. |
| [draw.io
preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js) |
Unmodified source at `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`: seeds `.configuration.defaultAdaptiveColors` **only** for a
`--initial-adaptive-colors=` flag with value `auto`, `simple` or `none` when the existing `defaultAdaptiveColors` is nullish,
catches malformed storage, registers IPC callbacks and exposes `electron` **and** `process`. |
| [Apple WKContentWorld](https://developer.apple.com/documentation/webkit/wkcontentworld), [world-scoped
handler](https://developer.apple.com/documentation/webkit/wkusercontentcontroller/add(_:contentworld:name:)) | Isolated named
world and handler API availability. These sources do **not** by themselves guarantee synchronous cross-world `contextBridge`
publication before the first inline script. |
| [2026-10-24 physical macOS F0 baseline/negative control](https://github.com/gyldlab/keld/issues/484#issuecomment-6382021064) |
macOS 26.5.1 ARM64 real WKWebView: first-inline page observed `typeof window.electron === 'undefined'` after app-world JS
assignment, while a **static** page-world injection was visible. Mutating only the static bootstrap's world changed the page's
first-inline method `function → undefined` with native handler still firing. **Partial isolation evidence only:** no dynamic
`contextBridge` proxies, authentic draw.io preload, source-root admission or grant proof. |
| [2026-10-24 native F0 16/16 first-inline proof](https://github.com/gyldlab/keld/issues/484#issuecomment-6382401714) | Real macOS
WKWebView + **unmodified pinned draw.io preload** with a **test-only** two-way synchronous publication handshake: first-inline
page observed all four callable methods, copied process subset, frozen API and adaptive-colors seeding for the tested
missing-preference/valid-flag fixtures, **16/16** across both app/page registration orders. An unguarded one-shot publication
showed nondeterministic order; changing to `AtDocumentEnd` made first-inline API unavailable, and suppressing exposure gave **3/3
negative**. This proves local timing feasibility, **not** authentic `contextBridge` or Host authorization. |
| [2026-10-24 nested callback F0 6/6 with RED](https://github.com/gyldlab/keld/issues/467#issuecomment-6382468009) | Same
immutable unmodified preload in real WKWebView, but **synthetic test-only IPC**: **6/6** positive runs exercised real preload
`request` and `watchFile` with two nested callback deliveries and no serialized function; suppressing only nested callback-token
revival caused **6/6** missing change callbacks, while top-level callbacks still worked. Direct forged DOM events against
unexposed functions were refused in that fixture, but shared DOM events are **page-forgeable**; no genuine Bun effect, native
credential, grant, teardown or release was proved. |
| [2026-10-24 dual-world native credential F0](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584) | Real
WKWebView fixture with **one test-owned checked-u32 epoch** and separate app/trusted native credentials: 33 native receipts per
run, 3/3 corrected baselines, independent credential-check and wrong-role-operation RED mutants. Its initial missing/missing
optional credential equality was a fixture defect; the repair requires an existing stored binding and supplied credential. This
supports only native-realm feasibility, **not** KELD #542 document authority, #552 guard, Bun link, source integrity or
first-inline callable dispatch. |

**Current-documentation receipt (2026-10-24):** Applicability: pinned sandboxed Electron and Apple WKContentWorld API. Context7:
`/electron/electron` resolved and queried for sandbox preload and contextBridge; results index evolving `main` docs and are
discovery only. Official primary: immutable Electron commit above and Apple API documents. Supported claim: pinned nine-module
wrapper, error behavior and world-scoped installation are available as design inputs. Supplemental **native test-only** F0
receipts above establish first-inline scheduling and local nested callback feasibility on this Mac, with no security authority.
Fallback/blocker: approved production `contextBridge` publication, unforgeable method dispatch, native original-document bind and
Host grant proof remain unproved. `origin/main` was `b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50` on 2026-10-24, equal to the handoff
pin.
