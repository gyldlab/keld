# Spec: Windows XML Legacy development preload bridge
Status: approved
Tracker: GH-1184 · Owner: repository owner · Updated: 2026-11-11

Approved exact proposal: `5e286fddd945a6ccad8a599d938421afaf14f5ea`,
[human receipt6801094337](https://github.com/gyldlab/keld/issues/1184#issuecomment-6801094337).
The implementation still owes all technical and native qualification gates.

This bounded proposal preserves PR1307/r17 at `d2c48b1b1fe3d42bf08dcc82211671e06f72654f`.
Approval of this file authorizes only the Windows XML development slice below, not
r17's general migration classifier, production release, isolation, or arbitrary apps.
The single human decision is in §10. Technical implementation and independent gates
remain mandatory; historical blocker labels do not create additional human decisions.

## 1. Goal & non-goals

Run the pinned application's unchanged preload before its first page script through
normal `keld dev`, using the already approved explicit Legacy/BOOT8/selected-source
path. Qualify Launch, Open, Edit, Save existing, Save As, dirty-close Cancel, dirty-close
Save, dirty-close Discard, Cleanup and Fresh relaunch. Desktop pin:
`2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`; webapp pin:
`24b76c2cbd55d88e354042e8d329a2e4708972bc`.

Non-goals: isolated preload secrets; preload-only authority; Strict containment;
release publisher authentication; remote or non-entry documents; extra windows for
VSDX/import/export; automatic route-D classification/attestation; new manifest keys,
window selector, parser, endpoint-value codec, ledger, transport, or parallel Host writer.
A closed Windows Host-message carrier serializer is explicitly new wire work in §4.

## 2. Spec refs

Architecture 02 (Host-routed IPC), 03 §1–2 (native identity and immutable grants),
04 §3.3 (documented emulation) and 05 §2 (renderer boundary) govern this slice.
W3's approved `53c95fb2774ef842fad716c92851d96636242b7d` §4.12 and KEL96 amendment
own BOOT8, source selection, page-navigation deny-all and writer/final-admission rules:
[approval](https://github.com/gyldlab/keld/issues/1185#issuecomment-6798699680).
W6's explicit Legacy approval is [6796682432](https://github.com/gyldlab/keld/issues/1187#issuecomment-6796682432).
GH540 owns delivery accounting; GH542/PR1192 owns native document leases; GH484 owns
preload source; GH467 owns the existing first-proof runtime; GH541/GH552 own grants/CSP.
This file amends only their Windows adapter interpretation stated here. Their concrete
existing contracts remain the source of truth, including bounds and typed refusals.

## 3. Acceptance criteria (binary, each becomes a test)

1. A graph-selected preload, exact arguments and genuine Host facts run before first
   inline page code in the top-level selected entry. `process.contextIsolated` is false.
   An iframe, error page, non-entry document or string-loaded about:blank gains no route.
2. Same-document raw page and facade calls have identical Host authorization. A missing
   literal grant, wrong native view/source, old epoch or retired role produces zero Bun
   handler/native effects. Channel permission cannot become a native filesystem permit.
3. A retained source/graph mutation or preload digest mismatch refuses construction;
   the page/app cannot select another root, preload, grant key or expected digest.
4. A held A reply released after B commits never reaches B's raw WebMessage listener.
   Pending navigation holds posts; cancellation without replacement restores A; actual
   replacement revokes A. Native post completion and consumer release each occur once.
5. Missing/late bind receipt, forged/duplicate receipt, source failure, role crash and
   window close end through existing typed failure/revocation and W2 cleanup. No success,
   replay credit or document is fabricated. A healthy successor still works.
6. Ordinary no-flag Host plus the complete provider passes all ten XML workflows above,
   including actual W5 Open/Save/close dialogs and on-disk XML readback. First-inline,
   denied-channel and stale-document mutants fail their independent effect oracles.

## 4. Design

### Evidence and atomic decisions, before implementation

Current shared foundation is `aac5060602aceb63f521bcaf9164d2a6b6337979`; composed W3
source inspected at `7b74a205`. Main remains `f49bc07e16ffb64c7060bc47b05ad755bd2ae4b9`.
`CompatSession`/`CompatLedger`, checked native document/source leases, preload image,
strict renderer decoders, endpoint-value codec, schema5 window grants, and the original
preload runtime exist. Core `compat_router` and Wv `wkwebview` remain macOS-gated:
Windows has neither their production composition nor a production page bridge today.
W3 confirms its Windows resource/document adapter is still implementation work.

| Atom / owner / boundary and inputs → outputs | Status and independent falsifier |
|---|---|
| Identity: GH542 native owner; real view + correlated NavigationId + served selected entry → checked lease | Windows adapter unimplemented. Same-URI iframe response must not qualify top-level commit. |
| Authentication: W1 native callback; native sender + Source + current epoch → admitted request | Unimplemented. Replay A's captured raw envelope after B's commit; no handler entry. |
| Authorization: Guard; Host-resolved source/window binding + exact literal → channel-only permit | Shared implementation exists, Windows consumer unimplemented. Remove one grant; actual handler count becomes zero. |
| OS containment: W6; actual admission → explicit Legacy | Separately approved; no Strict claim. Missing explicit Legacy input refuses this route. |
| Source/memory/I/O: Core graph + W3 retained handles; admitted selection → immutable bounded preload/assets | Shared selection exists; Windows reader pending. Change a leaf/root after selection; no changed bytes execute. |
| Lifecycle: GH542/W2/Runtime; document and authenticated role retirement → revoked route | Shared model exists. Hold A work across native replacement/role restart; no B effect and one release. |
| Queue/copy: GH540/Core; reserved operation + final enqueue/native post → terminal disposition | Canonical ledger exists, Windows adaptation/writer incomplete. Hold physical write across retirement; count retained charges independently. |
| Realm: GH1184; same main world → direct page authority and readable data | Proposed decision below. Raw page granted-call positive control succeeds; ungranted neighbor fails. |
| Evidence: corpus/Windows operator; actual normal CLI and pinned source → ten XML receipts | Unrun. Removing preload or dialog operation must fail actual startup/workflow oracles. |

Edges are explicit: source qualifies identity; identity plus epoch authenticates; current
window binding selects Guard; both document and role retirement fence publication.
Legacy does not prove any other atom. Queue completion does not prove page consumption.
No performance conclusion is drawn from these source facts.

### Bounded realm and permission amendment

The selected Legacy window uses main-world emulation. Its manifest's exact
`windows.<key>.channels` are direct page authority, even when the preload does not
expose that operation or narrows arguments. All preload arguments, Host-delivered
payloads and local preload bookkeeping are treated as page-readable. A page can forge
its own renderer receipts and damage its own availability; those receipts prove neither
trusted consumption nor an application action. Host identity, grants and native effect
admission never depend on trusting the runtime, preload closure, method name or receipt.

This deliberately supersedes r17's classifier/attestation premise for this explicit
Legacy development slice. It does not assert that the pinned preload is pass-through:
its request-id mutation, callback filtering and argv migration are directly observed.
No `reviewedChannels`, verdict or attestation schema is added. Existing hand-reviewed
literal grants remain the sole carrier; missing grants refuse. Adding a channel later
requires the existing exact-grant review, not inferred wildcard authority.

The currently prepared editor artifact has 13 literals: rendererReq, mainResp,
app-load-finished, args-obj, resize, maximize, unmaximize, isModified,
isModified-result, removeDraft, draftRemoved, saveAndClose, saveAndClose-result (each
with `el:` prefix). These are a reviewed input candidate, not proof that every XML
workflow succeeds or that file watching is granted. Host native operations still use
their separate immutable app-role permissions. Other applications and release builds
remain outside this amendment.

### One source, document and bridge owner

W3 owns retained Windows graph/root reads and the single Guard `file:` component parser;
W1 consumes them. The only eligible top-level Host load is SelectedRendererSource.entry
with its original query. The asset closure does not expand document eligibility.
Page-initiated top-level navigation and popups stay denied. Native ContentLoading must
match the allowed NavigationId and W3's verified top-level served record, with a non-error
commit. URL equality, a same-URI frame, NavigateToString or page-supplied identity cannot
mint the lease. Source validation uses W3's parsed in-root components and committed
scheme on every call; opaque origin strings and raw URL text are not identity. The
Host document identity is fixed at commit; same-document history changes do not mint it.

Extract platform-neutral state/DTO/image accounting from existing
`wkwebview/{macos_document,macos_bridge,macos_resource_source}.rs` into shared Wv owners;
retain Mac imports/aliases and native witness adapters. Preserve the source object
identity edge, revocation witness and checked constructors. Core `compat_router`,
registry and `original_webapp` consume those same shared types. Windows supplies its
actual NavigationId/served-resource witness through the owning adapter, never a dummy
Mac pointer or an unchecked public constructor. W2 retains real COM handlers/controller
and endpoint custody until callback quiescence and native release.

The existing AppPreloadBudget/Image bounds remain 512 KiB per image and 8 MiB aggregate;
additionalArguments remain at most16, 256 bytes each and 2,048 bytes aggregate. Core
selects and rechecks the graph's preload digest and source before native construction;
no page/path reopen can change the admitted bytes. Genuine captured Host versions/facts
use the existing producer. The current app config is not yet an admitted BOOT8 input:
its rendererSource/graphSource preparation is W3's existing grammar and capture task.

### Runtime/carriage adaptation, not a second framework

Extend `packages/@keld/electron/src/preload-runtime.ts` with an explicit native transport
adapter and realm mode. Reuse its four first-proof exposed methods, listener/callback
semantics, bounded copies, frozen intrinsics and the endpoint-value codec. The Windows
adapter uses captured `chrome.webview.postMessage` and a captured message listener;
Mac keeps its actual isolated-world adapter. Do not globally flip the Mac isolation fact.
Execute the exact admitted original source in the existing wrapper; do not rewrite it,
use a per-app JS emulator or hand-edit the generated runtime. Use the existing generator.

Install the one fixed bootstrap with AddScriptToExecuteOnDocumentCreated, await its
registration completion before navigating, and run its top-frame guard before any app
source. First-inline presence is required; IPC remains not-ready until native binding.
Use existing v2 bind/send/consumed and original runtime registration shapes through the
sole `wv_link` decoders. Never forward a raw string to Bun without native admission.
Current source has no shared closed outbound serializer: `macos_bridge.rs::deliver_event`
and `deliver_app_other` build `serde_json` objects inline and embed them in private-world
function calls; `RendererBridgeOutcome` is a Rust enum, not a wire codec. The new Windows
carrier therefore needs one proposed `HostRendererMessage` serializer in the existing
`wv_link` owner and its strict TS counterpart in the existing runtime. This is new carrier
encoding, not an already implemented contract or a second endpoint-value codec/parser.
Its closed JSON union is below; every message has `v:2`, `kind` and `document` (the exact
32-lowercase-hex current epoch), plus only the listed fields. No null/extra fields:

| kind | Additional fields / existing semantic owner |
|---|---|
| bound | `ready:boolean`; existing native current binding and route state |
| event | `delivery:nonzero-u32`, `channel:string`, `payload:u8[]`; existing GH540 event/endpoint bytes |
| send-result | `message:nonzero-u32`, `disposition:WvTerminalDisposition`; existing serialized closed disposition |
| route-state | `ready:boolean`; existing RendererRoleWitness stays Host-only |
| app-registered | none; existing native registration acknowledgement |
| app-refused | `call:nonzero-u32`; existing failed local-call admission |
| app-call | `call:nonzero-u32`, `method`, `args`; existing AppPageCall method/Local grammar and bounds |
| reply | `request:nonzero-u32`, `ok:true`, `payload:u8[]`; existing numeric bridge reply |
| reply | `request:nonzero-u32`, `ok:false`, `code:string`, `detail:string`; existing sanitized numeric error |

Data remains opaque endpoint bytes; Local values reuse AppPageCall, not another value
encoding. Native-only Preflight, EventReleased and ControlCapacity are never serialized:
they select/retire the existing operation; successful preflight may subsequently produce
app-call. `app-call` retains MAX_APP_WV_LINK_ENVELOPE_LEN; event/value reply retains
MAX_WV_LINK_ENVELOPE_LEN and MAX_RENDERER_PAYLOAD_LEN; the other control variants retain
4,096 encoded bytes. Charge encoding/UTF-16 storage through the existing reserved bucket
before allocation. Golden bytes plus missing/extra/null/duplicate/oversize cases bind both
sides. Numeric invokes remain only the already declared Echo channel1. KIPC ids/frames
and all existing page→Host decoder shapes stay unchanged. These new outward tags cannot
be admitted as inbound commands by selecting an arbitrary decoder or role.

Tokenless bind carries no authority. It can cause at most one initial bound post; one
additional bound post at native DOMContentLoaded is allowed if no current registration
receipt arrived. Both target the current qualified document, carry its existing nonce,
and consume the existing CONTROL reservation; neither remints identity nor replenishes
budget. Existing `{document, methods}` registration acts as the bounded startup receipt;
only the exact current nonce and existing four-method shape settle startup. Duplicate,
stale or malformed registration does not settle it. A **new proposed Windows-only five-second
startup receipt bound**, owned by this adapter, begins at the successful native
qualification transition and ends at receipt, not PostWebMessageAsString S_OK. It is
absolute and never extended by duplicate bind/registration. Current Mac source's
`retirement_deadline`/`retirement_drained` five seconds instead bound retirement
quiescence; those are not a startup timer and remain unchanged. Absence of the Windows
receipt closes admission through typed failure. This is availability accounting, not
proof that trusted preload executed. Preload integrity is the separate source atom.

All Host→page control/data uses PostWebMessageAsString, never ExecuteScript/CDP eval.
The UI owner rechecks exact native view, source, lease, role and pending-navigation
state immediately before post. Reuse GH540's separate native and consumer release halves:
S_OK proves native API acceptance only, not delivery/consumption. Page `consumed` is
untrusted and cannot release another key, exceed a charged record or authorize effects.
Hold during pending top-level navigation; cancel-without-commit can release to A;
commit/failure/retirement discards A work.

The native posting cut is one synchronous UI-owner turn: prepare/encode/reserve first;
observe all native getters first; then revalidate the retained native owner, source,
epoch and role under the canonical retirement fence; set the charged operation to
Posting; immediately invoke PostWebMessageAsString on that retained view. Between final
validation and API entry there is no other COM call, user callback, wait, await, nested
message pump or modal dialog. Native navigation callbacks mark Pending/revoke before
any callback-producing native operation. Host navigation is scheduled after this turn
and cannot run inline inside a post. Release Rust interior borrows before the COM call;
a posting sentinel prevents recursively initiating another post or navigation.

The intended native linearization is acceptance of this API call, followed by the
platform's asynchronous navigation-discard behavior. Microsoft documents same-STA,
serial non-reentrant WebView2 callbacks and forbids nested message-loop reentrancy:
[threading model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/threading-model).
That is not a general proof of every COM implementation. If a navigation/commit callback
actually re-enters inside the native post, revoke immediately, retain the in-flight
charge, report outcome-unknown and fail this qualification cell; merely checking again
after S_OK cannot undo a leaked payload. Mandatory native controls order A-post and
B-navigation on both sides of API entry, record callback nesting, and count B's raw
messages. A synthetic callback seam separately forces reentry before API entry and must
suppress the call. No claim that an atomic boolean, STA label or post-return check alone
prevents A→B disclosure is accepted. Unexpected in-call reentry or B receiving A bytes
blocks this adapter pending a supported native targeting/ordering correction.

The adapter remains independent of W3's
single pending app-evaluation slot, allowing a posted event to settle that Promise.

W1's eventual implementation writer owns the shared Core CompatLedger/primary writer
adaptation; W3 consumes it for resource/eval results. Use approved53c95 §4.12 verbatim
for five charged writer positions, one FIFO/physical writer, final authenticated enqueue
then infallible commit within retirement fence. Do not reselect bounds, introduce a
W1/W3 queue, or wait for capacity under publication/current/retirement locks.

## 5. Boundaries

Implement in existing Core app_session/{compat_router,window_router,window_registry,
original_webapp}, shared Wv document/bridge/source representations, webview2 native
adapter, and the existing TS preload runtime/generator/tests. W3 exclusively owns
shared graph/BOOT8/config/resource parser implementation; W2 owns window custody;
W5 owns native dialogs. Coordinate overlapping source files before edits.

No Cargo change is selected here. Existing windows/webview2-com, serde, sha2 and
getrandom facilities suffice; actual platform visibility is checked by the dependency
owner. Any necessary new dependency returns to that owner, not a local workaround.
No original app/source edits or release/source-authentication policy expansion.

## 6. Tasks (ordered vertical slice)

- [ ] Record exact approval of this bounded realm amendment and independent contract review.
- [ ] Add first-proof conformance/negative controls, extract shared owners mechanically,
  then connect real Windows source/document/endpoint/runtime and canonical writer.
- [ ] Compose W3/W2/W4/W5/W6 through normal CLI; run exact-current source/OS tests,
  preserve every failed/unrun atom, then qualify all ten XML workflows. One concern's
  implementation PR; do not publish stub stages or create a PR per mechanical step.

## 7. Test plan

AC1/3: actual first-inline top/iframe/error/changed-source tests; wrong digest, deleted
registration barrier and removed top-frame guard each falsify their own oracle.
AC2: real Bun effect counters for permitted/raw permitted/ungranted/wrong-view/stale
messages. Deterministically hold A's native callback until B commit, then replay it;
removing nonce or literal-grant enforcement must create the forbidden handler effect.
This replaces r17 B7's pre-implementation natural-race gate for this slice. Natural
cross-document delivery frequency remains unknown; no empty at-risk statistic is proof.
AC4: B's raw listener counts payloads, independently of bridge filtering. Hold physical
writer/post and retire in both orders; mutate only final revalidation, charge retention,
or release-key match. Count aggregate live/retired work, not only active windows.
AC5: stale bind ahead of B's listener, late second bound, no/forged/duplicate receipt,
role crash, close during post and healthy follow-up. Use events/barriers, not sleeps.
AC6: unchanged original preload/webapp byte census, actual on-disk XML and native dialog
outcomes. A Ready token or fixture-only success is not product acceptance.
Run actual fmt, warning-denied workspace clippy, workspace tests and relevant generated
artifact checks on the final implementation; no such run is claimed by this proposal.

## 8. Review gates triggered

Public API: yes, Windows facade/realm facts and shared owner exports. Permission model:
yes, direct page use of exact grants and visible preload data. Wire protocol: yes,
Windows carrier/startup receipt semantics, no new KIPC channel. Unsafe: yes, exact COM
registration/post/removal paths require nearest Wv owner allowlist and independent final
diff evidence. Dependency addition: none proposed; target-visibility edits remain the
shared dependency owner's gate. Named independent architecture/security reviewers may
satisfy technical gates under standing delegation; no human-only review loop is added.

## 9. Perf impact

No performance claim. Preserve existing source/codec/ledger/writer limits. Measure actual
startup and retained work only against attributed baselines; native posting versus eval
is a semantic choice, not a claimed speedup.

## 10. Open questions

Resolved by the receipt above: approve main-world emulation for this selected-source explicit
Legacy development slice, with direct page use of every declared literal channel and
page-readable preload arguments/data, instead of the unimplemented general r17
classifier/attestation design. Recommended for the bounded XML MVP. Declining preserves
the current refusal and needs a supported isolated-world engine route or a smaller app
contract before preload implementation. W6 Legacy approval alone does not decide this.

Approval leaves source extraction, Windows document/resource adapter, carrier goldens,
unsafe allowlist, native negative controls and ten-workflow acceptance as technical work.
It does not mark historical r17 B1–B11 globally closed: B1/B2 become this scoped truthful
realm/tracker amendment; B3/B4/B6 use approved W3 selected graph/entry owners; B5 uses
actual AAC owners; B7 uses deterministic delayed-callback falsification; B8 uses
exact-current independent review; B9 uses bounded receipt above; B10/B11 use existing
hand-reviewed source/config and exact channel keys. General migration/release remains
outside this slice. Approval covers this bounded contract; no native qualification is asserted.
