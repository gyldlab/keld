# Spec: explicit sendSync fallback and conformance cells
Status: draft
Linear: GH-548 (#517) · Owner: Monish Khandelwal · Updated: 2026-11-06

## 1. Goal & non-goals

Give ipcRenderer.sendSync an explicit synchronous failure on every channel without an approved mirror declaration.
The error identifies the unsupported operation and an asynchronous invoke migration path.
Keep Electron source expectations, Keld selected divergence and observed conformance verdicts separate.

Non-goals: synchronous host transport, Atomics or XHR workaround, automatic async rewriting, reply mirror implementation,
returnValue delivery, citation computation, or claiming that a planning comment proves a shipping facade.
Implementation and merge remain gated; this is a future spec.

## 2. Spec refs

- [GH548](https://github.com/gyldlab/keld/issues/548) and [current routing decision](https://github.com/gyldlab/keld/issues/548#issuecomment-6556349025) own this fallback.
- [Architecture 02](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/02-ipc.md) §5 describes the compatibility destination; this spec does not approve a new blocking carrier.
- [Architecture 04](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md) §§3,4 govern explicit subsets and evidence.
- [Current exports](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts) expose app and BrowserWindow, not a shipping ipcRenderer implementation.
- [Native floor](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/macos_bridge.rs) exposes invoke only.
- [GH467 proposal](https://github.com/gyldlab/keld/blob/b8b15392cce51fbf848f5a47841250100df7535f/docs/specs/gh467-contextbridge-ipcrenderer.md) excludes sendSync from its initial four-method proof.
- [Electron v44.4.5 documentation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/ipc-renderer.md) specifies synchronous result and event.returnValue reply.
- [Electron renderer implementation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/renderer/api/ipc-renderer.ts) delegates sendSync directly to native IPC.
- [Electron event structure](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/ipc-main-event.md) owns returnValue and senderFrame semantics.
- [GH1375](https://github.com/gyldlab/keld/issues/1375) owns the declared reply-mirror alternative; GH399 owns same-task cross-world qualification.

GH532/GH566 own evidence schemas and frozen corpora. GH1101 owns error allocation; GH547 owns actual invoke delivery.
The current main source does not substantiate the issue's historical phrase "live state"; this document specifies the intended fallback.
No architecture prose or approved baseline is changed by this draft. Any eventual destination change requires its existing owner amendment.

## 3. Acceptance criteria (binary, each becomes a test)

### AC-1: Record four separate oracle cells

Create separate cells for documented blocking/synchronous return, returnValue set after listener return,
listener-less warning and missing-reply value. Record source, version, platform and unknowns for each.
Source-only cells retain expected_verdict: unknown under GH532; source text is not a native run.
The Keld fallback's selected divergence is recorded independently of whether an oracle verdict is qualified.

### AC-2: Undeclared calls throw before returning

Call sendSync in a try/catch followed by an observable post-call marker.
A typed KELD-COMPAT error is thrown in that same call stack; no Promise, thenable or delayed rejection is returned.
No request, pending correlation, native callback or registered main listener is invoked.

### AC-3: The diagnostic is stable and actionable

The error has the approved machine-readable code, bounded message and fix guidance to use an asynchronous invoke/handle flow.
The message identifies sendSync and its unsupported channel policy without logging application argument values.
The code is reserved by the existing allocation owner before implementation; this spec invents no numeric suffix.

### AC-4: Mirror selection is explicit and exact

An absent, disabled, invalid or unready declaration takes the synchronous fallback.
A declaration is only a selector for GH1375's separately approved path; it grants no permission and starts no host call.
A similar-prefix channel or forged page declaration does not change the selection.

### AC-5: Existing asynchronous APIs retain their own semantics

Adding the fallback does not convert send/on to Promises or make invoke synchronous.
The valid invoke path continues to use its existing handler, settlement and grant checks.
The fallback's fix text is not proof that GH547 invocation is already implemented or qualified.

### AC-6: Missing reply cannot hang the fallback

A channel whose main listener omits returnValue still fails immediately on the undeclared Keld path.
Electron missing-reply and delayed-returnValue behavior is measured only inside a bounded, externally supervised oracle process.
No application main thread is blocked by a test waiting for an unsupported Keld reply.

### AC-7: Publication makes no unearned compatibility claim

The conformance record names the exact proposed/landed Keld revision and each actual native oracle revision.
The fallback is marked as a deliberate unsupported/divergent behavior, never a synchronous Electron pass.
Unrun macOS, Windows and Linux observations stay unknown.

## 4. Design

Reuse the existing preload facade dispatch owner. The operation is local validation followed by a local synchronous throw.
A new transport cannot meet this narrow requirement more safely or simply than an explicit failure.
No boundary change occurs on the fallback path: it neither crosses a process nor enters a native queue.

Proposed control flow, not a new exported schema:

```ts
function sendSync(channel: string, ...args: unknown[]): unknown {
  const selected = declaredReadyMirror(channel);
  if (selected !== undefined) return selected.read(args);
  throw approvedUnsupportedSendSyncError();
}
```

GH1375 owns selector validation, ready state, argument grammar and read/write policy. Until that approved path exists,
the implementation has no ready mirrors and every call fails locally. Never replace the throw with Promise.reject.
Validation must not run arbitrary argument coercions or disclose arguments in the error.

Electron's documented synchronous semantics explain why a silent async return is incompatible.
The delayed-returnValue, listener-less warning and missing-reply strings are separate hypotheses until pinned native observations qualify them.
Do not populate them by copying an issue comment into an observed-result field.

A qualified oracle record includes exact Electron executable/version, source pin, platform/architecture, fixture digest,
stdout/stderr and exit/timeout result, plus whether the handler ran and when returnValue was written.
An external supervisor owns the deadline and teardown; it must not use the potentially blocked renderer to enforce that deadline.
A timeout is a timeout observation, not invented reply bytes.

Error identity follows GH1101/registry rules and the accepted compatibility error carrier.
Do not repurpose a generic transport error or an existing unrelated COMPAT suffix.
Typed errors across a contextBridge boundary retain only the exact fields approved by its owner; Electron itself can lose custom properties.

Capabilities and manifest: none. The future mirror checks its existing el:channel grants independently.
Wire: none for fallback. Native crash domain and resources: unchanged; the fallback owns no pending work.
Migration unit: preload method plus diagnostic, API typing and explicit conformance cells. It does not rewrite application call sites.
The permanent fallback remains when mirrors are later enabled for selected channels.
All engines must preserve synchronous throw; platform qualification is still separate.

## 5. Boundaries

Implement in the existing @keld/electron preload facade with its tests and keld-compat evidence fixtures.
Coordinate new code allocation with the registry owner; do not edit error wire formats or codec behavior.
Do not touch GH1375 mirror implementation, the parked GH401 carrier, frozen baseline records, native queue owners or runtime dependencies.
This draft PR contains only this spec.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

### T-1: Establish honest oracle and divergence records

Add the four separately sourced cells and bounded oracle fixture for AC-1/AC-6/AC-7.
Retain source-only unknowns until actual native execution is available; preserve frozen corpus ownership.

### T-2: Add the local typed fallback

After facade and code-allocation approval, implement synchronous throw and exact field/fix validation.
Prove AC-2/AC-3 without opening a transport or running a main handler.

### T-3: Integrate mirror selection without changing asynchronous APIs

Consume GH1375's approved selector when available; all other channels keep the fallback.
Prove AC-4/AC-5 with absent, forged, invalid and ready declaration fixtures.

## 7. Test plan

| Criteria | Independent witness | Single mutation that must fail |
| --- | --- | --- |
| AC-1/AC-7 | Evidence-schema review against raw oracle artifacts | Promote a source-only cell to an observed pass |
| AC-2 | Same-stack catch marker plus zero transport/handler count | Return Promise.reject instead of throwing |
| AC-3 | Approved code/field assertions and secret argument marker | Interpolate argument values into diagnostics |
| AC-4 | Exact topic/declaration identity and zero undeclared side effects | Match channels by prefix |
| AC-5 | Existing asynchronous handler marker and return-shape tests | Route invoke through sendSync |
| AC-6 | Independent process supervisor and raw timeout/exit log | Make watchdog depend on blocked renderer |

Keep native oracle runs isolated and bounded. Record timeouts without reinterpreting them as a particular Electron value.
No runtime tests, native oracle observations or device qualification were executed while authoring this draft.

## 8. Review gates triggered

Public API and error-carrier review; permission review for the future mirror selector integration.
No dependency, unsafe or wire change is selected by the fallback. GH467/GH548/GH1375 and evidence owners approve their exact boundaries.
Source receipt: official Electron v44.4.5 GitHub files above, retrieved 2026-11-06; Context7 not applicable under GitHub-only scope.

## 9. Perf impact

The fallback allocates only its approved bounded local error and adds no host round trip or pending record.
Measure error-path allocation only if needed by the approved facade budget. No speed or memory improvement is claimed.

## 10. Open questions

- Error owner: reserve the precise KELD-COMPAT suffix and approve public carrier/fix text before implementation.
- Evidence owner: obtain pinned native observations for delayed returnValue, listener-less warning and missing reply.
- Facade owner: approve method publication and cross-world error-field behavior; current main does not provide it.
- GH1375 owner: define exact ready/disabled/invalid selector outcomes before integrating declared mirrors.
