# Spec: macOS draw.io contextBridge and ipcRenderer first-proof facade

Status: draft
Tracker: GH-467
Owner: GYLDLAB / @0monish
Updated: 2026-11-17
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

**Approval/production qualification: BLOCKED** by unproved secure first-inline callable publication and owner-approved dual-world binding.

**Approval boundary:** This draft is not approved for implementation. It grants no app API or security authority. It claims no native qualification or compatibility PASS. Independent owner approval is required. Keep this PR DRAFT / DO NOT MERGE until every gate passes.
## 1. Goal & non-goals

Make the pinned **unmodified** draw.io desktop preload expose its narrow page
API through Electron-like `contextBridge.exposeInMainWorld` and exchange
`rendererReq` / `mainResp` messages with the actual main-role listener using
`ipcRenderer.send/on/once`, on physical macOS WKWebView. A successful first
proof is **one genuine pinned page-origin `isFullscreen` request** from
`EditorUi.init`, observed at the original page callback, live main
`rendererReq` handler, current native document and transport release.
A separately labelled **test-triggered** second `isFullscreen` request
checks distinct IDs and credit reuse; it is **not** a second natural page
call. Include the nested long-lived `watchFile` callback and copied/frozen
exposed data; protect KELD's trusted renderer bridge with Host grants.

This issue owns the **application-level facade and cross-world value behavior**,
not the WebKit application-preload loader. GH-484 owns the loader, world selection,
handler and source containment; GH-540 owns transport and credits. Landing a
successful JS unit test or a synthetic response is not sufficient.

**Selected first-proof product scope (Option A, security approval pending):**
use a **predeclared document-start page facade** with exactly four callable
methods `request`, `registerMsgListener`, `sendMessage`, `listenOnce`
bound, after private registration, to the **actual unmodified pinned draw.io
preload** in the separate app content world. Copy/freeze only the pinned
`process.type` and real session-derived `process.versions` values. A page
world message never authenticates a method or its caller and never grants
native or Bun authority. General arbitrary/dynamic
`contextBridge.exposeInMainWorld(key, api)` graph publication is **Option B,
not accepted by this first proof**. Every unimplemented, doc-cited
GH464 `contextBridge` cell remains **expected-fail** (or an expressly
owner-approved and recorded divergence), not UNKNOWN or PASS merely
because the fixed draw.io facade works. UNKNOWN is reserved for
genuinely unqualified/uncited engine observations.
This is an explicit, draw.io-specific Electron compatibility divergence,
not a claim of full contextBridge parity. The scoped product direction is
recorded on [GH467](https://github.com/gyldlab/keld/issues/467#issuecomment-6390880361);
it does **not** satisfy this specification's independent approval gate.

Non-goals for this first-proof slice:

- Arbitrary graph publication under the full dynamic Electron
  `contextBridge` API, generic third-party preload parity,
  `ipcRenderer.invoke`, `sendSync`, synchronous
  or function-valued **cross-world returns**, `contextBridge.executeInMainWorld`
  or `exposeInIsolatedWorld`, arbitrary Promise crossing, Node integration or
  renderer native OS grants.
- Expanding the approved GH-544 IPC value codec, cloning arbitrary
  Map/Set/TypedArray/BigInt/cyclic object graphs, or transferring MessagePorts.
- Native window registry, Ready lifecycle, all browserWindow methods, broad
  plugin/open/save/close UI journeys, unconstrained IPC burst traffic, larger
  than the GH-540 admitted 4 KiB first-proof subset, or speed claims.
- A second KIPC link, ad-hoc renderer protocol, JS-side authority checks as
  substitutes for Rust Host enforcement, or an automatic legacy profile.
- Windows/Linux parity. Their observed cells remain UNKNOWN.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [refs.md](refs.md) | Section 2. | Reviewing upstream source pins, owner boundaries or evidence provenance. |
| [contract.md](contract.md) | Section 3. | Reviewing binary acceptance criteria C01–C23. |
| [design.md](design.md) | Section 4. | Reviewing world identity, callable publication, dispatch and lifecycle. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Reviewing path ownership, tasks, tests, performance or open decisions. |

## Current blockers

- B-1
- B-2
- B-3
- B-4
- B-5
- B-6
- B-7
## 8. Review gates triggered

- **Public API / compatibility:** yes; cross-world graph, method
  return shape, EventEmitter and Electron source-vs-test contract.
- **Security / permission model:** yes; hostile preload/page,
  app-world handler separation, qualified document and exact grant;
  reviewers from GH-541/542/552 retain owner decision authority.
- **Native engine:** yes; independent real macOS WKWebView
  first-inline and message handler tests, not V8-only simulation.
- **IPC wire:** no change owned by this issue. Consume GH-540
  reviewed one-link message envelope; any requested wire mutation
  returns to GH-540.
- **Dependency addition / unsafe:** none proposed for this spec;
  a future implementation must request their exact gate if needed.
- **Review:** no self-approval; exact-head CodeRabbit or documented
  independent adversarial review/refutation; full applicable
  repository gates before qualification.
