# Spec: host-opaque Electron endpoint value codec v0 (GH544)

- Status: draft
- Tracker: GH-544
- Owner: GYLDLAB / `@keld/electron`
- Updated: 2026-11-13
- Pin: 69f85cf376a8087bf028e3acafe17f243da5d5aa

## 1. Goal & non-goals

Provide **one** bounded TypeScript codec shared unchanged by the Bun
Electron facade, isolated renderer compatibility runtime and future
`executeJavaScript` result-value consumer. An endpoint value survives
encode → Host-opaque forwarding → decode with the selected first-proof
domain: JSON values, `undefined`, `Date`, ordinary `Error`
(name/message/optional stack), and class instances flattened to their
own enumerable data properties. Independent golden bytes and a
renderer-build identity check must refute a JSON-only replacement.

**Non-goals:** `ipcMain`/`ipcRenderer` implementation, native routing,
contextBridge callback proxies, envelope/result ownership, binary and
MessagePort transfer, general structured clone, new KIPC channel,
second transport or conformance score changes. Actual
`rendererReq → mainResp` belongs to GH465/GH543/GH466/GH467/GH484.
**Spec-only draft; no implementation authorized.**

## Parts

| Part | Owns | Load when |
|---|---|---|
| [contract.md](contract.md) | Sections 2, 3, 4 | Review value grammar, acceptance and goldens |
| [delivery.md](delivery.md) | Sections 5, 6, 7, 9, 10 | Review scope, tasks, tests and gates |

## Current blockers

- B-1
- B-2
- B-3
- B-4

## 8. Review gates triggered

- **Public API:** yes, Electron-visible value domain and
  synchronous typed refusal contract.
- **Wire protocol:** yes, endpoint-private **value** byte
  format under GH540's unmerged opaque Host envelope;
  no KIPC frame, channel or version change is claimed.
- **Permission model:** no change; GH541/GH552 own Host grants.
- **Unsafe:** none. **Dependency addition:** none.
- Independent Amisha/nonauthor review required on exact
  draft; CI-green documentation is **not** spec approval.
  No self-approval, merge or conformance-cell flip.

