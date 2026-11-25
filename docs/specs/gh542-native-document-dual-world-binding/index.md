# Spec: native checked-navigation and document-correlated dual-world binding (GH542)

Status: draft
Tracker: GH-542
Owner: 0monish / GYLDLAB
Updated: 2026-11-17
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

## 1. Goal & non-goals

The target is one Host-minted, checked-`u32` navigation generation and one qualified current committed application-document identity per live macOS `WKWebView`.
The eventual design has two distinct native credentials under that same document generation: one for the KEL-142 trusted bridge world and one for the separately
decided #441/#484 app-preload world. However, the current GH540 v2 `bind` request is tokenless and cannot identify the committed document that emitted it. Until
GH540 approves a document-correlated handshake and GH542 proves it on real WKWebView, the GH542 dual-world binding stays disabled: no second realm credential,
no `bound.document` response for the new app realm, and no Host/Bun forwarding based on an uncorrelated Bind. Existing KEL-142 behavior remains governed by its
own approved contract and is not requalified by this draft.

The first demonstrable result, after the owner and native gates pass, is a real macOS WKWebView with a pinned eligible application document, a
document-correlated bind for each approved realm, one admitted operation in each appropriately authorized realm, and **zero native/Host/Bun forbidden effects**
after independently injected stale/cross-realm/page-origin faults. Early document-start script publication is inert setup; it does not authorize Bind.

Non-goals: implementing code or changing current #541/#540/#441/#484/#552 sources in this PR; allowing page-world native handlers or treating `BroadcastChannel`
as authenticated sender provenance; app-script/DOM/URL self-certification; a second Core generation mint, new renderer principal, new `el:*` grants, unbounded
prebind queue, codec/credit owner, extra KIPC channel, shadow renderer transport, arbitrary remote/subframe IPC or general navigation policy (#460/KEL-79);
claiming native success, CodeQL coverage, Windows/Linux, installation or draw.io product PASS from this document.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [contract.md](contract.md) | Sections 2 and 3. | Reviewing upstream contracts, source facts or acceptance rules. |
| [design.md](design.md) | Section 4. | Reviewing trust, identity, Bind, lifecycle or dispatch. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Planning implementation, tests, performance or blockers. |

## Current blockers

- B-1
- B-2
- B-3
- B-4
- B-5
- B-6

## 8. Review gates triggered

- **Public API/native:** YES for future typed current-document/role/handler accessor and `Principal::Webview` adapter; independent native/WebKit and
  KEL-102/KEL-142 architecture review needed. This document exports no API.
- **Permission model / security:** YES; actual committed app eligibility, two realm credentials, separate native KEL-142/#441 worlds, final GH-552 Guard grant
  and role ownership are a security boundary. Require independent named security reviewer on exact source with direct forbidden-effect negatives and existing
  `GUARD003` unchanged.
- **Unsafe:** YES for future macOS Objective-C WebKit callback/retained-handle and any new native unsafe. List exact unsafe operations, main-thread/ownership
  lifetime proofs and independent unsafe gate on *implementation*; none added by this docs PR.
- **Wire protocol:** **No new wire bytes or KIPC version authorized.** GH540 owner must approve a document-correlated Bind handshake and the per-world
  `bound.document` interpretation. The current tokenless Bind request cannot prove the emitting committed document. If incompatible, this is a blocking GH540
  wire-review amendment, not permission to extend v2 ad hoc.
- **Dependency addition:** none in spec; no new third-party runtime or crypto dependency assumed.
- **Exact approval:** request Amisha independent source/public/native/security review of the DRAFT specification, plus GH #540 and GH #541 owners for shared
  interfaces and GH #484 for unresolved A1. Owner-approved exact content is required before any production mutation. A green docs-only `CI required` is **not**
  CodeQL coverage or OS acceptance. **DO NOT MERGE.**
