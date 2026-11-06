# Spec: app-declared sendSync reply mirrors
Status: draft
Linear: GH-1375 (#517) · Owner: Monish Khandelwal · Updated: 2026-11-06

## 1. Goal & non-goals

Serve explicitly declared, cacheable sendSync reads from a bounded local snapshot produced by the actual main listener.
Make refresh-before-listener order, optimistic writes and staleness visible compatibility choices.
The first-page-script seed must have a qualified identity and publication barrier before the feature can be enabled.

Non-goals: general synchronous RPC, the parked synchronous-XHR lane, computed citeproc results,
page-supplied snapshots, wider grants, automatic migration edits, unrestricted dynamic keys, or a claimed full Zettlr pass.
This draft supplies the missing contract and blockers; it is not approval or proof that the seed barrier exists.

## 2. Spec refs

- [GH1375](https://github.com/gyldlab/keld/issues/1375) owns the mirror proposal and declared divergence.
- [GH548 decision](https://github.com/gyldlab/keld/issues/548#issuecomment-6556349025) keeps undeclared channels on typed synchronous failure.
- [GH549 decision](https://github.com/gyldlab/keld/issues/549#issuecomment-6556337180) separates unsubscribe, mirrored local return and computed citation shapes.
- [Architecture 02](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/02-ipc.md) §§1,5,7 governs existing carriage and lifetime.
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md) governs default-deny host authority.
- [Architecture 04](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md) §§3,4 describes app quirks and honest divergence.
- [Current bridge](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/macos_bridge.rs) has no mirror or qualified first-script seed mechanism.
- [GH540 contract](https://github.com/gyldlab/keld/blob/16b00a2e388449c4525a30913937bc2f6e11c472/docs/specs/gh540-compat-ipc.md) owns quotas, document routing, physical release and ordered invalidation.
- [GH542 binding](https://github.com/gyldlab/keld/blob/c889a0f054b89de9b838a1a3451238cf638f86e2/docs/specs/gh542-native-document-dual-world-binding.md) owns host-authenticated document identity.
- [GH467 publication](https://github.com/gyldlab/keld/blob/b8b15392cce51fbf848f5a47841250100df7535f/docs/specs/gh467-contextbridge-ipcrenderer.md) remains gated on secure first-inline publication.
- [Electron v44.4.5 sendSync](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/ipc-renderer.md) is the synchronous-reply reference.
- [Electron event identity](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/ipc-main-event.md) documents nullable senderFrame.
- [Zettlr preload](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/common/modules/preload/index.ts) exposes synchronous config reads and two different write return positions.
- [Zettlr store](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/pinia/config.ts) seeds from sendSync and re-reads after throttled update events.
- [Zettlr provider](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/config/index.ts) captures get-config/set-config-single returnValue and broadcasts update invalidations.

Source correction: the provider broadcasts command: update with a changed option or undefined.
It does not push the complete configuration snapshot. The store re-fetches it. A mirror therefore needs an authoritative
refresh capture before forwarding the invalidation; copying that event payload cannot implement this feature.

GH541/GH552 own exact channel grants; GH543 delivery; GH545 larger payload evidence; GH399 same-task cross-world return;
GH592 may propose declarations but is optional for a hand-authored artifact.
No pending predecessor approval or architecture amendment is transferred by this draft.

## 3. Acceptance criteria (binary, each becomes a test)

### AC-1: Declaration selects behavior without granting authority

Only an exact, validated app declaration selects a mirror. The guard never reads that declaration.
Undeclared, disabled, unsupported or unavailable channels throw synchronously under GH548.
Similar-prefix channels, page-forged declarations and unapproved argument shapes cannot select another snapshot.

### AC-2: The first page script sees an authoritative seed

The unmodified preload and config store construct synchronously from a snapshot captured from the actual main listener's returnValue.
An independent main-side value/digest and first-inline page result are deep-equal.
The capture uses the approved host-minted sender/document policy and only declared channels.
No fake committed senderFrame, page-provided snapshot or async Promise can satisfy this criterion.

### AC-3: Invalidation refresh precedes application listeners

Change a main-side config value, emit config-provider update, then read inside the first page event listener.
The read sees an authoritative snapshot captured for that admitted refresh, never the update payload or the previous value.
Two refreshes retain their logical order; an old delayed capture cannot overwrite a newer snapshot.
The existing event payload is still delivered unchanged after the mirror state is applied.

### AC-4: Reads remain synchronous and local

An admitted get-config read returns the selected snapshot or property in the same task.
No native call, host request, filesystem read or Promise is created by that read.
Snapshot objects follow the approved value-copy policy; mutation of one returned object cannot silently rewrite authoritative mirror state.

### AC-5: Write-through remains ordinary guarded IPC

An approved set-config-single pattern sends its original arguments through the normal el:config-provider route.
The main listener sees the actual write only if the grant passes. The local optimistic value is a declared divergence, not success evidence.
A later authoritative refresh reconciles the mirror. Denial, failure or retirement cannot leave an optimistic overlay labeled confirmed.

### AC-6: Missing or oversize seed fails before a false-ready page

No listener, capture exception, unsupported value, absent identity barrier or oversize snapshot prevents mirror readiness.
The system produces the approved bounded diagnostic and does not start the page with a fabricated empty/default config.
It neither truncates nor silently drops the update while promising mirror-before-listener behavior.

### AC-7: Retirement prevents stale state and callbacks

Navigate, destroy, revoke the role or close the link during seed/refresh/write.
Old captures, queued events and optimistic overlays cannot mutate the successor mirror.
Physical buffers remain charged until the owning queue proves release; logical invalidation alone is not a refund.

### AC-8: Staleness and optimistic return values are separate cells

Qualify the observed interval between a main change and its admitted refresh as a declared staleness divergence.
Separately record optimistic write return and unmodified preload return behavior.
The preload config.set discards the sendSync result; the store's setConfigValue returns it. Do not claim both return true to their caller.

### AC-9: Computed synchronous citation channels stay unsupported

The pinned citeproc-provider get-citation-sync shape has no declared cacheable snapshot rule.
It fails synchronously with an explicit asynchronous migration fix.
A working configuration mirror cannot qualify computed citation results or workflows that exercise them.

### AC-10: Capacity and authorization controls independently fail

A denied write reaches no main handler even when its mirror declaration and payload are valid.
A valid granted path works. A maximum admitted snapshot works, while maximum-plus-one fails before excess framework retention.
All snapshot, pending refresh and optimistic-write state stays within approved per-document and aggregate budgets.

## 4. Design

Use one mirror owner in the existing preload compatibility runtime and one main-role capture adapter around existing ipcMain registration.
The mirror is framework data, not a new native principal or transport.
The existing authenticated host route supplies document identity; the application quirk cannot mint it.

The finite declaration must identify exact channel, read argument match/projection, canonical snapshot request,
refresh channel/match, optional write pattern/local optimistic projection and explicit staleness behavior.
Approve that closed schema before implementation. Do not evaluate arbitrary page-provided key functions.
Capture validated app configuration once at app/role admission; a page cannot hot-swap it.

For Zettlr, get-config without a property captures the complete authoritative config.
The read rule covers absent key and supported literal/dotted property keys; unknown/prototype-sensitive paths need an approved result.
The refresh rule matches config-provider command: update. Its payload is an invalidation hint, never a replacement snapshot.
The write rule recognizes set-config-single and preserves key/val arguments on the normal send path.

### Seed identity and publication gate

The issue's "before document loads" seed and "senderFrame is its first document" cannot be assumed simultaneously.
At window construction an admitted first-document identity and callable frame may not yet exist.
The current native script path does not demonstrate holding page execution while an asynchronous main capture completes.

Before implementation, GH542/GH484 and this owner must select and qualify one of these explicit policies:

- Capture after authentic document admission, with a demonstrated native publication barrier that prevents the first page script
  from running until the seed is privately installed.
- Amend the issue's seed-event contract to permit a host-authenticated window sender and null/unavailable senderFrame before load,
  proving the app accepts it and then bind the resulting snapshot to the authentic document before publication.

The second policy changes the requested seed identity and requires an explicit owner decision; it is not selected by this draft.
A reserved numeric ID is not proof of a committed frame. A later async overwrite is not a first-script seed.
If neither policy qualifies, keep the feature unavailable and preserve the typed fallback.

### Capture, refresh and ordering

The main-role adapter invokes the actual registered listener under a bounded capture context and records its returnValue.
The approval must define absent/multiple listeners, repeated assignments, thrown exceptions and asynchronous assignment.
Do not borrow invoke's separate handler map or substitute an app-specific config getter.
The capture is host-initiated only for the declared admitted window; it must pass the owner's normal channel eligibility policy.

For refresh, the producer intercepts the declared invalidation before delivery, captures an authoritative snapshot,
and binds snapshot plus original event to one bounded ordered transaction through the existing owner.
Mirror apply occurs before page listener dispatch. The original event is unchanged.
Any required private metadata/envelope extension needs GH540/GH543 review; this spec authorizes no new wire encoding.

A monotonically ordered internal refresh identity is bound to the existing role/document; do not create a new authority epoch.
Serial capture/application or equivalent owner-proved sequencing must prevent stale overwrite.
If capture is asynchronous, define what state the snapshot observes and prevent completion order from reversing event order.
This draft does not claim a snapshot exactly at mutation time unless that boundary is demonstrated.

A failed refresh invalidates readiness according to the approved availability policy.
It must not forward a success-implying update whose first listener can read falsely fresh state.
Use the bounded diagnostic/retirement path; never hide failure behind last-known-good state without the declared staleness contract.

### Writes, budgets and value lifetime

Optimistic writes are bounded local overlays, distinct from the last authoritative snapshot.
Apply only after local validation/admission succeeds; send ordinary IPC and handle its internal terminal failure.
Track overlay ownership so a denial or older refresh cannot incorrectly confirm a later write.
The issue proposes true as the optimistic sendSync result; this differs from actual validation and remains a separate divergence.
No optimistic return authorizes retry, grants execution or proves persistence.

Snapshot copies, current/next refresh overlap, queued invalidations, pending overlays and retired generations all count.
Reuse GH540/GH467/GH623 accounting. The full Zettlr snapshot size is unmeasured; GH545 must qualify any profile beyond existing limits.
Do not create an unbounded map of arbitrary argument keys or retain complete history.
Reads return approved copies from the authoritative/overlay view, with no sharing that lets page mutation alter trusted state.

Capabilities: no new permission. In particular the declaration cannot bypass existing exact el:config-provider checks.
Wire: consume existing carriage; any additional logical packet or readiness fact is a gated owner amendment.
Migration unit: declaration validation, capture adapter, seed/publication barrier, ordered refresh, local reads and optimistic reconciliation.
No automatic app edits are included. The fallback remains permanently for undeclared/unsupported channels.
macOS requires its own first-inline and cross-world proof; Windows/Linux inherit no acceptance from engine-neutral design.

## 5. Boundaries

Future changes belong to existing @keld/electron main/preload owners, app quirk validation, GH543 delivery and GH484/GH542 publication.
Keep permission checks in the guard and lifecycle/physical charges in their existing owners.
Do not implement a synchronous carrier, arbitrary citation cache, separate native link, app-specific Zettlr handler replacement,
migration write mode or a new product scoring denominator. This PR edits only this specification.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

### T-1: Approve declaration, seed identity and capture semantics

Build failing fixtures around the real pinned listener and first-inline store.
Select a qualified identity/publication policy and precise returnValue capture rules for AC-1/AC-2/AC-6.
No mirror runtime may be enabled while that decision remains open.

### T-2: Seed and read a bounded authoritative snapshot

Implement the approved barrier and bounded value/copy contract in existing owners.
Prove first-script equality, synchronous local reads and typed oversize/unavailable failure for AC-2/AC-4/AC-6/AC-10.

### T-3: Apply ordered authoritative refreshes

Connect actual invalidation events to main capture and mirror-before-listener delivery.
Prove AC-3/AC-7 using reordered capture completion, navigation and independent native release barriers.

### T-4: Add guarded optimistic write reconciliation

Implement only the approved write grammar and bounded overlay lifecycle.
Prove AC-5/AC-8/AC-10 with denial, delayed refresh and overlapping update controls; preserve preload/store return differences.

### T-5: Publish narrow native compatibility evidence

Qualify macOS first-script seed, update order, staleness and unsupported citation cells for AC-8/AC-9.
Record actual source/runtime/profile/grants and retain other engines or workflows as unknown.

## 7. Test plan

| Criteria | Independent witness | Single mutation that must fail |
| --- | --- | --- |
| AC-1 | Exact declaration/channel plus no privileged effects | Permit prefix declaration match |
| AC-2 | Independent main snapshot and first-inline page digest | Seed from page-supplied data |
| AC-3 | Read inside first listener after controlled update | Dispatch event before mirror apply |
| AC-3 | Two captures with independently held completion barriers | Let older result overwrite newer state |
| AC-4 | Same-task value and zero host call counter | Return a Promise or shared mutable snapshot |
| AC-5/AC-10 | Denied and allowed actual main-handler counters | Treat quirk as an el: grant |
| AC-6/AC-10 | Max/max-plus-one values and readiness marker | Truncate oversize snapshot and mark ready |
| AC-7 | Native retired-generation marker and charged bytes | Install delayed old seed into successor |
| AC-8 | Distinct preload config.set and store return observations | Manufacture true from preload's void wrapper |
| AC-9 | Computed citation argument marker and immediate refusal | Reuse unrelated cached value |

Each negative control must reach its targeted granted/ready path first; a missing facade is not a valid denial oracle.
No snapshot measurement, native barrier, host listener capture or application workflow was executed for this draft.

## 8. Review gates triggered

Public quirk/API schema, permission composition, secure publication, lifecycle/resource accounting and any owner-approved wire extension.
Require GH467/GH484/GH542/GH540/GH543/GH541/GH552 review of their exact seams.
Official Electron/Zettlr GitHub sources above were read 2026-11-06; Context7 is not applicable under GitHub-only scope.
No approval, code allocation or native proof is supplied by these source receipts.

## 9. Perf impact

Local reads avoid a new host request but snapshot capture, copies and refreshes still consume resources.
Measure full config size, peak overlap, refresh frequency and retained overlays before choosing limits.
No latency claim or memory ceiling is made without the approved profile and actual owner census.

## 10. Open questions

- GH1375/GH542/GH484 owners: resolve real senderFrame identity versus pre-load seeding and demonstrate a first-script barrier.
- API owner: approve finite declaration/key grammar and multi-listener/returnValue capture policy.
- GH540/GH543 owners: approve any snapshot-plus-event metadata and readiness/availability changes through the existing transport.
- Resource owner: measure full snapshot and overlay demand; approve any GH545/GH467 budget extension.
- Mirror owner: approve stale-read, failed-refresh and optimistic-denial reconciliation behavior before implementation.
- Native/evidence owner: qualify same-task page return, seed and refresh order; computed citations stay explicitly unsupported.
