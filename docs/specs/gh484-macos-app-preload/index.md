# Spec: macOS app preload runtime (GH-484 / F07-T1)

Status: draft
Tracker: GH-484
Owner: GYLDLAB / @0monish
Updated: 2026-11-17
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

**Approval/production qualification: BLOCKED** by authenticated
first-inline callable publication, verified package-image authority
and #540/#542 dual-world binding.

**Approval scope:** This proposes an extension to KEL-142. The original KEL-142 approval does not approve an app-owned third world, native handler, preload source authority or renderer facade. No implementation, native acceptance or merge follows from this draft.
## 1. Goal & non-goals

Give macOS draw.io a secure, explicitly **first-proof-subset** Electron preload runtime. Execute its **unmodified** preload at document start in its own app-owned `WKContentWorld`, distinct from both the untrusted page and the existing KEL-142 trusted renderer-bridge world. Register a world-scoped native app handler on the **same per-view
`WKUserContentController`** and consume the KEL-142 bridge's current frame/view checks plus #542's proposed **single** navigation-eligibility owner. Named Electron `el:<topic>` messages must consume **#540's approved v2 contract and #465's canonical decoder extension**, with the host grant contract from #541/#552. The [GH467 selected MVP Option
A](https://github.com/gyldlab/keld/issues/467#issuecomment-6390880361) uses an **at-document-start, predeclared four-method page facade** (`request`, `registerMsgListener`, `sendMessage`, `listenOnce`) and copied/frozen `window.process.type` and real `.versions` facts. Its calls must resolve to those methods **actually registered by the authenticated
unmodified app preload**, not a page-supplied function table. The **conditional** `.configuration` adaptive-colors result for a valid flag and previously nullish setting must also be observable by the **first inline page script**. Arbitrary dynamic Electron `contextBridge.exposeInMainWorld` graph publication is a **deferred, explicitly unqualified
divergence**, not an Option A compatibility PASS. Test-only static publication timing has been observed; production callable binding, Host/Bun dispatch and security approval remain unproved.

Non-goals: #467 owns the bounded Option A page method/argument/callback facade, `ipcRenderer` event semantics, future separately reviewed full dynamic `contextBridge` graph and the complete draw.io two-request round trip. #473 owns page resources/CSP, not preload source admission. #567 owns F07 conformance cells and expected-fail policy; #540 owns the v2
wire/carriage spec, #465 the canonical decoder/send extension, #541/#542/#552 the grant and navigation-generation contracts, **#543 owns host-to-renderer event delivery**, and #464 owns the related IPC conformance cells. No new KIPC channel, second parser, nonce, grant store or ambient Node resolver in #484; no general dynamic graph parity, synchronous
`sendSync`, bundled multiple preloads, WebView2/WebKitGTK implementation, production performance win, or changes to the KEL-142 `window.keld` public facade.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [refs.md](refs.md) | Section 2. | Reviewing source pins, upstream owners or evidence provenance. |
| [contract.md](contract.md) | Section 3. | Reviewing binary first-proof criteria. |
| [design.md](design.md) | Section 4. | Reviewing A2, native receiver, source custody and lifecycle. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Reviewing path ownership, tasks, tests, performance or blockers. |

## Current blockers

- B-1
- B-2
- B-3
- B-4
- B-5
- B-6

## 8. Review gates triggered

### unsafe

- **Current draft:** **none in this docs diff**

- **Implementation condition:** Conditional YES for new native Objective-C paths; sanctioned per-crate allowlist and named independent unsafe evidence on exact code.

### public API

- **Current draft:** **YES (proposed contract)**

- **Implementation condition:** `require`, process, preload error behavior and first-inline facade availability; #467 independently owns copy/proxy shape.

### permission model

- **Current draft:** **YES**

- **Implementation condition:** Third-world handler must apply enumerated `el:` grants and #542 checked document lifecycle, with hostile-preload negative controls.

### dependency addition

- **Current draft:** **none proposed**

- **Implementation condition:** Existing workspace-pinned objc2/WebKit and host primitives reused; a future new dependency reopens gate.

### wire protocol

- **Current draft:** **YES (consume approved extension)**

- **Implementation condition:** `keld.wv-link/v1` is numeric-only. #540 owns v2 schema and carriage, #465 the canonical named-send decoder, #543 the host-event implementation and #464 conformance. #540/#542 must approve two world-scoped binding credentials under one checked navigation owner (or amend v2), and #484 must prove no duplicate
  parser/channel/document authority.

An independent reviewer must assess the **final** spec diff; future code needs fresh named independent architecture/security evidence, not approval borrowed from old KEL-142 work.
