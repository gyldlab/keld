# Spec: addressed host-to-renderer named events
Status: draft
Linear: GH-543 (#517) · Owner: Monish Khandelwal · Updated: 2026-11-06

## 1. Goal & non-goals

Deliver an admitted named event to its captured native document through the existing renderer bridge.
Expose frozen `window.keld.on` registration and preserve issue order, bounded retention and no-listener drop.
Every acceptance criterion is future work. This draft grants no implementation, native qualification or merge authority.

Non-goals: Electron facade registration, invoke/handle, sendSync, subframes, larger payloads, multiple pending invokes,
new transport or registry, priority queues, replay, global order across windows, and Electron conformance claims without an oracle.

## 2. Spec refs

- [GH543](https://github.com/gyldlab/keld/issues/543) and its [route/lifetime clarification](https://github.com/gyldlab/keld/issues/543#issuecomment-6371796168) own this delivery slice.
- [Architecture 02](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/02-ipc.md) §§1,2,5,7 govern carriage, identity and retirement.
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md) governs host authorization.
- [Architecture 04](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md) §4 governs qualified evidence.
- [Native envelope](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wv_link.rs) is currently bind/invoke only, capped at 4096 payload bytes.
- [macOS bridge](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/macos_bridge.rs) owns pending native work and exposes only invoke in its page facade.
- [GH540 contract](https://github.com/gyldlab/keld/blob/16b00a2e388449c4525a30913937bc2f6e11c472/docs/specs/gh540-compat-ipc.md) §§4.3-4.10 supplies the proposed event, consumed and Deliver contract.
- [GH542 binding](https://github.com/gyldlab/keld/blob/c889a0f054b89de9b838a1a3451238cf638f86e2/docs/specs/gh542-native-document-dual-world-binding.md) owns the document identity mint.
- [GH467 facade](https://github.com/gyldlab/keld/blob/b8b15392cce51fbf848f5a47841250100df7535f/docs/specs/gh467-contextbridge-ipcrenderer.md) consumes delivery without owning its native lifetime.
- [Electron ipcRenderer](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/ipc-renderer.md) is the v44.4.5 external API reference, not a Keld-native observation.

GH540's pinned revision is draft after its diagnostic-budget amendment; earlier approval does not cover that revision.
GH541/GH552 own grants, GH544 the value codec, GH449 shared Subscribe, GH623 aggregate outbound admission,
and GH1095/GH1096 settlement. This spec consumes their approved contracts and introduces no architecture amendment itself.
Implementing the new envelope remains gated on the owning GH540/KEL-142 wire amendment.

## 3. Acceptance criteria (binary, each becomes a test)

### AC-1: Ordered delivery uses the real endpoint

Given an admitted subscribed document and a listener, send N distinguishable events under available credits.
The callback observes each payload exactly once in admitted cross-topic issue order.
Count and independent sequence markers must agree; a fabricated host receipt cannot satisfy the page witness.
N includes sequential reuse after physical release, not an unsupported burst above the one-record profile.

### AC-2: Original and current routes remain different

Capture A for DeliverToOriginalDocument, commit B, then attempt A's queued delivery.
A's event reaches neither B nor a recreated window. DeliverToCurrentMain admitted after B captures B once.
A later commit C cannot retarget that queued delivery. Host and native callback identities must agree.

### AC-3: Same-document changes preserve admission

After a native-confirmed same-document hash change, an already addressed event reaches its listener.
Cross-document retirement revokes the old route. Cancelled navigation does not become a speculative commit.
The test obtains these classifications from the GH542 owner, not from an arbitrary page URL assertion.

### AC-4: No listener means no retained application event

An event with no listener is discarded with the canonical NoListener disposition.
Registering a listener later reveals no history. The empty callback path still releases its framework retention.
The Keld behavior test may pass while the uncited Electron no-listener cell remains unknown.

### AC-5: Capacity refusal preserves a healthy route

Hold one admitted DATA operation at a native-completion barrier and attempt another beyond the approved profile.
The internal sender receives the approved typed refusal; no allocation or scheduling exceeds any owner quota.
After genuine release, another valid event arrives. Public webContents.send/event.reply retain their separate void contract.

### AC-6: Consumption cannot refund native retention early

Let the page report consumed while native evaluation remains active.
Normal Deliver settlement waits for both endpoint disposition and independent native release.
Retirement may settle correlation first, but charges remain until owner-proved quiescence.
A delayed completion releases only its own charge and never produces a second terminal response.

### AC-7: Control progress survives full DATA

With DATA full, consumed/retirement/readiness use GH540's reserved CONTROL capacity through the same FIFO owner.
Malformed, duplicate, wrong-direction, stale or future receipts release no unowned charge.
Control exhaustion closes admission and retires through the canonical owner; no silent live continuation is allowed.

### AC-8: Listener registration and cleanup are document local

The frozen facade rejects invalid channels/callbacks before registration.
A disposer removes only its own registration and is idempotent; duplicate callback registrations remain distinct.
Navigation, destroy and role retirement release framework subscriptions through their existing owner.
Late callbacks from a retired document cannot register, mutate or consume capacity in its successor.

### AC-9: Real native and evidence gates stay separate

A macOS fixture proves actual original/current delivery, native navigation and completion barriers.
Mock tests alone cannot qualify native callback ordering. Linux and Windows remain unknown until equivalent lane evidence exists.
Only the covered delivery cells change; no-listener oracle and full product workflow scores stay with their owners.

## 4. Design

The host already owns routing and the native queue already owns evaluation lifetime. Reuse both.
Adding a page listener table solves endpoint registration; it does not authorize a second link or a second credit ledger.
The application-value codec stays opaque to Rust. No page-provided webview, generation or release quantity is trusted.

Proposed floor API, subject to the public API gate:

```ts
type RendererEventListener = (payload: ReadonlyArray<number>) => void;
type DisposeRendererListener = () => void;
interface RendererEventFloor {
  on(channel: string, listener: RendererEventListener): DisposeRendererListener;
}
```

The array contains codec bytes, not decoded Electron arguments. Its transport representation and copy ownership must
match GH540/GH544; the approval record must settle this API shape before implementation. The frozen facade retains invoke.
The Electron facade owns its own EventEmitter-facing argument reconstruction and exception behavior.

The existing canonical compat entry carries DeliverToCurrentMain or DeliverToOriginalDocument.
Current resolves once at admission; original never substitutes a successor. Both retain host-authenticated role/document state.
GH542 supplies the checked non-reusing identity; do not cast the current wrapping u64 navigation counter to u32.

Delivery phases are reserve, capture, queue, final identity check, endpoint dispatch/discard, native completion and terminal release.
Page receipt and native completion are independent facts. A callback Promise is not awaited as a transport receipt.
Listener failure executes the release path without deciding the Electron facade's exception-observation policy.

Use the GH540 sequential profile unchanged: one DATA record per direction across the role, including retired work;
4096 payload bytes; 256 UTF-8 topic bytes; 65536 private DATA envelope bytes.
Each owner retains its own 262144-byte accounting allowance and separate conversion scratch; CONTROL has four bounded records.
These are proposed accounting ceilings, not measured engine-heap bounds. No fresh allowance appears on navigation or role restart.
GH623 must bound the canonical total queues across producers before this feature is enabled.

Native enqueue and completion both revalidate role/document identity. A same-document change retains the existing admitted identity.
Retirement closes producers before drain; successor readiness requires old queue, active work and framework continuations to quiesce.
A timeout abandons correlation, not remote execution or retained native bytes. There is no automatic replay.

Capabilities: no new manifest permission. Renderer-originated setup/control cannot expand exact el:channel grants.
Host delivery uses the existing authenticated route and approved subscription; a public topic name is never routing authority.
Wire: consume the GH540 private v2 and canonical compat-control contract only after its owner approval; add no second format.

Migration unit: the native envelope, queue owner, core Deliver integration, page registration and corresponding facade adapter move together.
Permanent Electron compatibility APIs remain. No temporary forwarding transport is introduced.
macOS is the first native witness; other engines reuse semantics only after their own lifetime bindings qualify.

## 5. Boundaries

Implement in the existing keld-wv envelope/macOS bridge, keld-core compat delivery and canonical @keld/kipc/native queue owners.
Keep @keld/electron integration with GH467/GH465. Place conformance fixtures with keld-compat's existing evidence owner.
Do not touch shared policy, frozen evidence baselines, grants syntax, channel allocation, payload limits or another platform's claimed proof.
This draft PR edits only this specification.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

### T-1: Land one addressed native event end to end

After predecessor approvals, connect canonical Deliver to the existing envelope and frozen page registration.
Prove AC-1/AC-2 with distinct page markers and original/current route fixtures. Keep the sequential profile.

### T-2: Close listener and navigation lifetimes

Integrate the GH542 identity/retirement facts with listener/disposer cleanup, same-document preservation and no-listener drop.
Prove AC-3/AC-4/AC-8 with actual callback barriers and stale continuations.

### T-3: Prove bounded retirement and control progress

Connect independent consumed/native completion to existing retention accounting.
Exercise full DATA, CONTROL exhaustion and ambiguous retirement for AC-5/AC-6/AC-7; preserve public void facade behavior.

### T-4: Qualify the macOS delivery slice

Run real native end-to-end fixtures against the pinned facade and publish immutable evidence for AC-9.
Update only owned covered cells through the existing evidence process; retain unqualified engines and oracle cells explicitly.

## 7. Test plan

| Criteria | Independent witness | Single mutation that must fail |
| --- | --- | --- |
| AC-1 | Page payload count and sequence versus sender manifest | Reverse two admitted event deliveries |
| AC-2 | Unique old/new document markers and native commit barrier | Retarget stale original delivery to current |
| AC-3 | Native same-document/commit classification plus callback | Rotate generation on every hash change |
| AC-4 | Late listener receives zero callbacks | Buffer unhandled event for later replay |
| AC-5 | Queue high-water census and successful post-release event | Omit pre-enqueue credit check |
| AC-6 | Held native barrier and charged bytes after consumed | Refund on page receipt alone |
| AC-7 | DATA-full release/retirement completion and unchanged ownership | Let DATA consume CONTROL reservation |
| AC-8 | Registration identity and retired callback counters | Share a listener entry with successor |
| AC-9 | Native artifact/platform/version and exact cell provenance | Substitute a mock-only receipt |

Use explicit test-controlled barriers, not sleeps or assumed callback names. A mutation must reach its intended live path;
an unrelated permission or subscription refusal is not a valid negative-control result. No tests or native runs were executed for this draft.

## 8. Review gates triggered

Public API, wire protocol, security/permission composition and native lifecycle review.
Require GH540, GH542, GH541/GH552, GH623 and facade-owner review of the exact integration diff.
No dependency addition or unsafe code is proposed here; an implementation introducing either triggers its own gate.

## 9. Perf impact

Queue retention, bridge copies, callback dispatch and wakeups may affect architecture 01 §5 budgets.
Record per-hop high-water bytes/records including retired generations and conversion overlap, plus payload-size workload.
No latency, throughput, memory improvement or native heap ceiling is claimed by this document.

## 10. Open questions

- GH543/GH467 API owners: approve the byte-array callback and disposer signature, listener limits and exception observation.
- GH540/KEL-142 owners: approve the current v2 diagnostic and envelope contract before consuming it.
- GH542/GH623 owners: provide approved identity, aggregate admission and quiescence implementations.
- Native operator: provide actual macOS same-document and consumed-versus-completion evidence; other platforms remain unqualified.
- Evidence owner: keep the Electron no-listener cell unknown until a qualified independent oracle exists.
