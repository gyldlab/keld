# Spec: Completion-backed page-world JavaScript evaluation
Status: draft
Linear: GH-462 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Make executeJavaScript resolve or reject from the actual macOS evaluation result using GH544's one endpoint codec, with bounded
pending state and navigation/lifetime fencing.

Non-goals: New value encoders, isolated-world public APIs, insertCSS and Windows/Linux completion parity are separate. The
page-world eval permission does not expose bridge or preload worlds.

## 2. Spec refs

- [Owning issue GH-462](https://github.com/gyldlab/keld/issues/462) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current fire-and-forget eval](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/engine.rs).
- [Native eval owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/mod.rs).
- [Engine change rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/AGENTS.md).
- [Endpoint codec draft](https://github.com/gyldlab/keld/blob/0437c9104077e6a52c9128e4367e91c57937b22a/docs/specs/gh544-endpoint-value-codec.md).
- [Electron executeJavaScript contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-contents.md).
- [Eval owner issue](https://github.com/gyldlab/keld/issues/462).
- [Retirement distinction](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel143-macos-generation-filesystem-recovery.md): local caller settlement and native quiescence are separate observations; eval retains its own owner.

Verified baseline: Main WebEngine.eval returns Result<(), WvError> and is explicitly fire-and-forget. GH544 is a draft, with a
4096-byte encoded value limit, depth 16, 256 nodes and 256 value/property visits; its Proxy trap and pre-return allocation limits
remain explicit unresolved implementation gates.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given page-world scripts returning 2, strings, arrays, null and undefined, when completion arrives, then the Promise resolves to
the corresponding distinct decoded values.

Independent negative control: Use plain JSON or fire-and-forget success; undefined/null or the result value must diverge.

### AC2

Given a script producing a rejected Promise, when the wrapper settles, then executeJavaScript rejects with the approved
name/message/optional-stack shape.

Independent negative control: Resolve the error envelope as an ordinary successful value.

### AC3

Given Map or another GH544-forbidden result, when encoding runs, then the existing typed unsupported-value outcome rejects and no
lossy plain-object conversion occurs.

Independent negative control: Convert Map to {} and report success.

### AC4

Given evaluation during load, when the selected matching load has not finished, then evaluation waits; a cross-document identity
change before completion rejects stale work, while a proven hash-only change does not.

Independent negative control: Evaluate immediately while loading or compare only current URL text.

### AC5

Given the approved aggregate reservation cap, when queued or native-entered evaluations reach the limit, then one more call
rejects before allocating a queue record, native callback or closure. Locally settled but unquiesced native work still counts.

Independent negative control: Check the cap only after native callback registration, or exclude locally rejected native work.

### AC6

Given the GH544 exact 4096/4097-byte result boundary and independent depth/node/visit boundaries, when decoding runs, then
exact-limit positives pass and over-limit results reject with the limit named.

Independent negative control: Apply a new larger eval-only codec limit or reject exactly 4096 bytes.

### AC7

Given a destroyed view or retired role, when late native completion arrives, then it cannot settle another caller and all owned
callback state is released once.

Independent negative control: Reuse the correlation ID without its lifetime identity.

### AC8

Given a probe for the privileged bridge handler, when executeJavaScript evaluates it in the page world, then it observes no bridge
object.

Independent negative control: Execute the same code in the bridge/preload world.

### AC9

Given native evaluation completion is held, when callers time out, navigate or lose their role and a fresh generation requests
more work, then queued plus native-entered plus locally settled/unquiesced reservations remain within both approved caps.
No generation or replacement-view transition resets the host-session aggregate. After correlated native completion or proved
teardown, the reservation releases once and a healthy follow-up call can enter.

Independent negative control: Release on Promise rejection or reset the aggregate on role restart. Independently count retained
native callbacks/closures and admitted evaluations; the bound must fail even when the active Promise count stays below the cap.

## 4. Design

### 4.1 Completion seam proposal

Add a design-reviewed completion form to the existing engine/Core command queue, implemented by the live macOS backend in the same
change. Proposed logical result is EvalCompletion { view, attempt, navigation_identity, outcome }; use existing identity types and
owning schema allocation rather than raw page-selected IDs. Other engines remain explicitly unsupported until a live backend
implements the contract.

### 4.2 Single envelope and codec

The host-owned async wrapper captures successful value or {name,message,optional stack} failure and delegates serialization to the
unchanged GH544 codec. Preserve undefined independently of null. The wrapper's execution result is untrusted page data;
malformed/envelope/proxy errors reject without creating host authority. Evaluate in the page world, never the privileged bridge or
preload app world.

### 4.3 Budgets

Propose 32 reservations per immutable native-view lifetime and 128 across the host application session, partitioned for
attribution by authenticated generation. Charge before queue/callback allocation. The aggregate spans retired roles and replaced
views; their still-retained work never disappears by resetting a generation counter. Count queued, native-entered and locally
settled but unquiesced records together. Each record owns its callback/closure references and retained script/result charges.

Cancellation before native entry can release after proving queue removal. After native entry, local rejection only revokes
caller settlement authority. Release its reservation on the correlated native callback or independently proved native
teardown/quiescence, once only. If neither is provable, retain bounded retirement state and refuse further admission when the
cap is reached. Do not replay, silently evict retained native work or call a timeout cancellation of the engine.

Code bytes and encoded result bytes also consume their existing transport/codec limits; the owning implementation must account
for queued and retired backing, not only live Promise bytes. GH544's 4096-byte value budget is authoritative. Envelope overhead
has separate transport accounting and must not silently reduce the result allowance. These reservation caps do not bound all
engine allocations or CPU work.

### 4.4 Native allocation limitation

A codec byte/visit cap does not bound engine evaluation work, a malicious Proxy trap or native conversion allocation before
callback return. GH544's admission strategy and this backend's extraction path require source qualification and pressure evidence
before enabling arbitrary results. This draft does not mislabel a bounded JS decoder as a whole-engine memory bound.

### 4.5 Lifetime and ordering

Capture authenticated caller/view/navigation identity at admission. Wait for the corresponding completed document before
evaluating. Rejection, destruction or retirement settles the caller once and fences stale delivery. Queued work must be removed
before releasing its reservation; native-entered work follows section 4.3's independent quiescence rule. Callback completion
posts to the existing owner and cannot touch native handles on an I/O thread. A hung page script is not safely preempted by a
local timeout. No new role may inherit a retired call's correlation identity or freed-looking budget.

### 4.6 Migration

Retain fire-and-forget internal users until explicitly migrated; do not change their meaning incidentally. Add the facade Promise
consumer, live backend callback and Core record owner together. Central registry owns new error codes; this draft uses descriptive
categories rather than inventing numeric codes.

### 4.7 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes stay in the existing engine eval seam, Core completion owner, Electron WebContents and shared GH544 codec call
sites. No second encoder, worker, privileged world or unsanctioned native handle access.

This publication changes only `docs/specs/gh462-completion-backed-evaluation.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Approve the live-backend completion shape, pending budgets and GH544 extraction/admission gates; pin all value and lifetime
cells.

### T2

- [ ] Implement one macOS native-to-Core-to-facade result slice with shared-codec vectors and error/rejection propagation.

### T3

- [ ] Add load-wait and stale-navigation/destruction settlement, independently owned native retirement and host-session budget
accounting with exact-boundary and restart controls.

### T4

- [ ] Run original localStorage/result callers and native late-callback/pressure fixtures; keep other engine cells unknown.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-462;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use independent GH544 golden bytes plus native result fixtures. Do not use encode-then-decode alone as the oracle. Synchronize
navigation/finish/destruction with explicit native attempts. AC9 holds the native completion seam while local terminal paths
and generation turnover run; observe actual retained callback/closure owners separately from Promise and queue counts. Record
releases and successful follow-up admission. A state-model test proves the reservation ledger; actual macOS callback teardown
and release evidence must independently qualify the native boundary. Local rejection never proves native script termination.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API, WebEngine design review and wire owner if completion messages change. Unsafe review applies to any new native call
outside existing sanctioned wrappers. GH544 approval is prerequisite.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Bound owned pending records under the proposed caps and use current codec limits. Native evaluation/Proxy execution remains
outside those simple bounds; no memory/CPU safety or latency number is claimed without evidence.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve the 32-per-view-lifetime/128-per-host-session proposal, timeout semantics and correlated native-quiescence receipt.
   No role restart may reset unresolved native charges; inability to prove quiescence is an explicit admission blocker.
2. Resolve GH544's pre-return extraction/Proxy admission limitations for this live backend.
3. Approve native callback shape, result-envelope transport accounting and error-registry additions.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

