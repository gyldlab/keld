# Spec: bounded contextBridge return shapes for Zettlr
Status: draft
Linear: GH-549 (#517) · Owner: Monish Khandelwal · Updated: 2026-11-06

## 1. Goal & non-goals

Extend the existing facade owner with the distinct return shapes used by the pinned Zettlr preload:
an immediately callable unsubscribe handle, Promise results, and same-task local mirrored data where the engine qualifies it.
Pass setMaxListeners to the existing EventEmitter semantics without turning a warning threshold into a resource grant.

Non-goals: arbitrary dynamic object-graph exposure, constructors, all function-valued returns, computed synchronous host calls,
a new cross-world carrier, raising bridge caps, full Zettlr scoring, or claiming Windows isolation.
This is a gated extension proposal, not an implementation or native-conformance receipt.

## 2. Spec refs

- [GH549](https://github.com/gyldlab/keld/issues/549) and [D-Z21 scope](https://github.com/gyldlab/keld/issues/549#issuecomment-6556337180) distinguish the three return shapes.
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md) governs isolated authority.
- [Architecture 04](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md) §§3,4 governs preload facade and evidence.
- [GH467 proposal](https://github.com/gyldlab/keld/blob/b8b15392cce51fbf848f5a47841250100df7535f/docs/specs/gh467-contextbridge-ipcrenderer.md) owns publication, proxy identity and its existing graph/callback budgets.
- [GH540 proposal](https://github.com/gyldlab/keld/blob/16b00a2e388449c4525a30913937bc2f6e11c472/docs/specs/gh540-compat-ipc.md) owns transport lifetime; a returned proxy is not a transport credit.
- [Electron v44.4.5 contextBridge](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/context-bridge.md) supports Function/Promise return positions and describes copied errors.
- [Electron renderer](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/renderer/api/ipc-renderer.ts) inherits EventEmitter.
- [Node v24.0.0 events contract](https://github.com/nodejs/node/blob/c5349f43cd66d2aa02d86414c9ed426f71d3ae48/doc/api/events.md) describes setMaxListeners as a warning threshold and chainable method.
- [Zettlr preload](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/common/modules/preload/index.ts) returns a closure from ipc.on, exposes async invoke, and calls setMaxListeners(100).
- [Zettlr config store](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/pinia/config.ts) performs a synchronous read during store construction.

Node v24 is an upstream semantic reference, not a claim that this is Electron's embedded Node or Keld's Bun version.
GH399's source receipts do not yet qualify native same-task return readback.
GH1375 owns mirrored data; GH548 the fallback; GH547 invokes; GH542 realm retirement; GH484 loader publication.
Current main exports app/BrowserWindow and its native floor exposes invoke only. The extended preload facade is not shipping at this pin.

## 3. Acceptance criteria (binary, each becomes a test)

### AC-1: Publish separate qualified return-shape cells

Record Function return, Promise fulfillment/rejection, local synchronous data, unsupported computed sync and isolation separately.
Cite pinned Electron source and actual per-engine native observations where available.
A callable unsubscribe does not qualify arbitrary returned functions, synchronous host calls or isolated-world security.

### AC-2: An unsubscribe handle is callable immediately

The real unmodified ipc.on call returns a local callable in the same stack, never a Promise.
It is tied to that exact subscription and invokes the actual preload-created off closure once bound.
Calling it twice removes no other subscription and returns no invented application result.

### AC-3: Early disposal and late binding cannot leak a listener

Call the returned handle before the asynchronous preload closure is bound.
When binding later succeeds, the actual closure is immediately disposed without reactivating the subscription.
Late bind, duplicate bind, bind failure and navigation each leave bounded state; a disposed entry never becomes live again.

### AC-4: Event ordering and duplicate registrations are explicit

Register the same page callback twice with two subscriptions, then dispose one.
Only the remaining registration receives subsequent admitted events.
The owner-approved registration barrier decides when delivery begins; publishing a local handle alone is not evidence of registration.
No historical event is buffered for a late registration; GH543's no-listener rule remains intact.

### AC-5: Promise values settle once with approved copy semantics

The actual preload invoke wrapper returns a Promise that fulfills with an admitted copied value or rejects with approved copied error fields.
A fulfilled Error value remains a value; it is not converted to a rejection.
Navigation or failure cannot settle a successor's Promise, and retained proxy/callback records are released at the owning terminal boundary.

### AC-6: Local synchronous data remains in the same task

After GH399 and GH1375 qualification, an exposed config.get returns the mirrored primitive/object before the next microtask.
The witness uses the actual preload and first-page-script path; no Promise or host round trip substitutes for the value.
A stale or unavailable mirror follows its declared policy and cannot return a fabricated default.

### AC-7: setMaxListeners preserves emitter semantics and resource caps

setMaxListeners(100) updates the emitter's warning threshold and returns that emitter for chaining.
Valid zero/Infinity and invalid values follow the selected existing EventEmitter implementation's contract.
Changing the warning threshold neither allocates subscriptions nor increases the bridge's independent hard limits.

### AC-8: Function-valued returns do not confer authority

Proxy identifiers and shared-DOM messages never supply native identity, grants or host callable names.
Wrong document, stale binding, fabricated endpoint or denied channel produces zero privileged effects.
The genuine admitted path still succeeds so the refusal is not explained by a globally broken bridge.

### AC-9: Computed citation returns stay explicitly unsupported

The outer getCitationCallback shape does not imply support for its inner citeproc-provider synchronous computation.
The inner unsupported call throws the GH548/ GH1375-selected typed error with an async migration fix.
No stale config value, Promise or canned citation is returned as a synchronous result.

## 4. Design

Reuse GH467's publication and per-document proxy owner; a second proxy registry would split lifetime and budget authority.
The initial draw.io four-slot proof does not authorize arbitrary Zettlr graph exposure.
A separately approved finite Zettlr publication profile must name ipc, config and the selected functions before enabling them.

| Shape | Local operation | Remote or native requirement |
| --- | --- | --- |
| ipc.on return | Mint bounded callable handle immediately | Bind actual preload disposer; order registration under existing relay |
| invoke return | Return Promise | Existing approved invoke/handler route plus approved value/error copy |
| config.get | Read approved local mirror | GH399 same-task cross-world readback; no synchronous host request |
| getCitationCallback inner call | Explicit typed refusal | Computed sync provider is outside mirror scope |
| setMaxListeners | Delegate to existing emitter | No native call and no resource-quota change |

Subscription state is bounded and monotonic: pending-bind, bound, disposed or failed.
The local callable marks disposed even before bind. The eventual preload closure observes that state and removes its actual callback.
Do not free an ID for reuse while a late bind can still reference it; retirement follows GH542 and the existing proxy owner.
Maintain explicit registration identity; JavaScript function equality across worlds cannot identify the preload's wrapper closure.

The preload callback intentionally replaces Electron's event argument with undefined before calling page listeners.
Preserve that actual behavior. Do not expose a native sender/event capability to make the test pass.
Two registrations of the same function need two handles. Cleanup removes exactly the matching wrapper.

Immediate handle publication and effective registration are different events. The implementation must prove the owner-approved ordering
between a queued registration, events and early disposal using the existing bounded per-document relay.
If that order cannot match Electron's synchronous registration behavior, record the narrow divergence and block the relevant parity cell.
No unbounded pre-registration event queue is allowed.

Promise handling reuses approved proxy/copy rules and one terminal owner.
Never treat a callback Promise as permission to refund native transport retention early.
Error name/message and approved code fields follow the bridge owner; upstream Electron can discard custom error properties.
Argument copies remain mutable where the app requires them; exposed data copy/freeze rules must not be generalized to all argument objects.

The existing GH467 proposal has finite depth/key/string/endpoint/callback/accounted-byte limits.
Its 64 callback registrations are a hard resource budget separate from setMaxListeners(100).
This spec does not claim a 100-listener workload fits that profile; actual demand and any larger owner-approved profile are pending.
Do not silently make setMaxListeners a hard listener cap or a mechanism to disable bridge accounting.

Capabilities: none new. All actual IPC effects retain exact existing el:channel checks.
Wire: no new KIPC frame or native side channel; any local relay shape extension requires GH467/GH542 approval.
Migration unit: published profile, callback/proxy owner, actual preload invocation and tests move together.
Permanent compatibility facade remains. No application source rewrite or temporary native endpoint is selected.

macOS must prove publication and same-task readback on actual WKWebView.
Windows route D may provide ordinary same-world JavaScript return semantics while declaring isolation divergence;
that source/design expectation is not native acceptance. Linux needs its own cross-world evidence.
If same-task readback is unavailable, keep that cell unsupported; do not weaken isolation to manufacture a pass.

## 5. Boundaries

Implement through the existing preload/contextBridge proxy owner and @keld/electron EventEmitter facade.
Coordinate native publication with GH484/GH542, Promise invoke settlement with GH547 and mirrored values with GH1375.
Do not edit transport error policy, grants, arbitrary graph schema, hard resource limits or another platform's proof.
This draft changes only this spec.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

### T-1: Qualify the finite publication and return-shape profile

Approve named Zettlr exposure, oracle cells and registration ordering before extending the relay.
Provide failing fixtures for AC-1/AC-4/AC-8/AC-9 with actual app functions.

### T-2: Implement bounded disposer lifetime and emitter passthrough

Connect immediate local handles to real preload unsubscribe closures and retire them safely.
Prove AC-2/AC-3/AC-4/AC-7 with early disposal, duplicates and quota exhaustion.

### T-3: Integrate Promise settlement through the existing proxy owner

Implement fulfillment, rejection and cleanup while preserving copied-value distinctions.
Prove AC-5 and stale-binding refusal without a second invoke or transport owner.

### T-4: Enable qualified synchronous local reads

Only after GH399/GH1375 approval, connect the same-task mirror result to the actual config.get publication.
Prove AC-6 on native first-script fixtures; keep computed citations unsupported under AC-9.

## 7. Test plan

| Criteria | Independent witness | Single mutation that must fail |
| --- | --- | --- |
| AC-1 | Per-shape/platform provenance audit | Use unsubscribe success as all-function parity |
| AC-2 | Same-stack typeof function and actual off counter | Return Promise of disposer |
| AC-3 | Held bind barrier and final listener count | Bind a previously disposed subscription as live |
| AC-4 | Distinct registration tokens and callback counts | Remove both entries by page function identity |
| AC-5 | Fulfilled Error versus thrown error and one settlement | Reject every Error-valued fulfillment |
| AC-6 | First-script value plus microtask sentinel | Schedule mirror result in a Promise |
| AC-7 | Emitter threshold/chaining plus unchanged hard quota | Use warning threshold as bridge admission limit |
| AC-8 | Valid path and denied/stale path side-effect counters | Accept page-supplied endpoint authority |
| AC-9 | Computed-provider marker with synchronous refusal | Return cached unrelated value |

Use deterministic bind and retirement barriers. Do not infer same-task readback from synchronous event dispatch alone.
No runtime, native cross-world or quota measurements were executed for this draft.

## 8. Review gates triggered

Public API, permission boundary, isolated-world publication and lifecycle review.
An extension beyond GH467's fixed profile needs its explicit owner approval; this draft does not waive it.
Dependency/unsafe/wire gates apply if the later implementation introduces them.
Official GitHub sources above were read 2026-11-06; Context7 is not applicable under the user's GitHub-only scope.

## 9. Perf impact

Returned proxies, Promise records and deferred disposer bindings increase framework retention.
Count live/pending/retired endpoints and bytes under the existing owner, including late-binding overlap.
No CPU, memory or native allocation improvement is claimed; demand for more than the current callback budget is unmeasured.

## 10. Open questions

- GH467/GH484/GH542 owners: approve a finite Zettlr publication profile and secure private binding before implementation.
- GH549 owner: approve the registration barrier and any unavoidable registration-timing divergence.
- GH399/native operator: prove actual same-task return readback; synchronous dispatch source evidence alone is insufficient.
- Resource owner: measure Zettlr callback demand; decide any shared budget amendment without coupling it to warning thresholds.
- GH1375 owner: supply approved mirror availability and stale-read policy; GH547 owns Promise invoke qualification.
