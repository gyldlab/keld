# D3 exact channel guard: scoped implementation decision

Status: approved for bounded local MVP integration under direct owner delegation; runtime acceptance remains unproved.
Linear: GH552 / GH467 (#517 tracker of record) · Owner: @0monish · Updated: 2026-11-01

Recorded before code on 2026-11-01. Root explicitly assigned implementation under
the user's in-scope delegated approvals. No approval wait or new capability
vocabulary is introduced. Canonical writes, Cargo/native execution and final
integration remain root-owned.

Sources: saved worker18 guard implementation/tests; exact local GH552 contract
`voice-resume-01a197d9/ipc-main-20261101/authority-evidence/gh552-exact-channel-grants-guard.md`;
GH541 companion in that same directory; root's present strict-channel instruction.
The GH552 historical draft's optional opt-in marker is not adopted: the present
scoped choice is unconditional strict `windows.<key>.channels`, sharing D1's one
typed WindowPolicy parser and immutable verified manifest. No new marker/grant.

## 1. Goal & non-goals

Implement exact per-window channel selection and the existing final Host Guard callback for the original-application MVP. Preserve the one codec, subscribed WorkerLink, native document owner and Strict profile. No new principal, reusable preflight permit, transport or broad grant.

## 2. Spec refs

Architecture03 default-deny; existing GH552 exact grants, GH540 bounded Send/release, GH542 native document and GH467 preload owner contracts. The direct human D3 decision selects their existing route and delegates bounded owner amendments to root.

## 3. Acceptance criteria

Malformed window/channel data refuses the whole manifest. Exact decoded literals match only the selected trusted window in the same verified snapshot. Preflight returns unit; the final callback rechecks permission and current native/role identity before effects. Denied, retired and cross-snapshot requests have zero successor effects. Physical native/original-app controls remain required; component tests cannot qualify those boundaries.

## 4. Design and decision-bearing atoms

1. Policy admission — Guard owns the existing unique-key bounded JSONC reader
   and WindowPolicy deserializer. Decoded exact `el:` strings enter one immutable
   table; null/wrong shape/wildcard/control/unpaired Unicode/overlong suffix
   reject the whole policy as GUARD005. Observable: literal parser tests with
   exact boundary byte counts. Independent of native authentication.
2. Host window selection — Core owns verified source selection and registry
   association. Guard provides a borrowed selection into the same manifest;
   missing/wrong window or another snapshot cannot match. Observable: wrong-key
   and cross-snapshot denials. Native identity is an explicit dependency, never
   inferred from numeric principal fields or this pure-policy test.
3. Identity/authentication/lifecycle — Core and the native document owner supply
   the actual Webview principal only after authentication and current lease,
   plus admitted current Bun role. This worker changes no mint or counter.
   Observable: Core's final native/role lease controls, pending root integration.
4. Authorization — one GuardQuery owner separates native and renderer inputs;
   the existing Decision becomes generic with native ScopePermit as its default.
   RendererChannelPermit is disjoint, opaque and has no native scope index.
   Exact decoded unprefixed channel comparison allocates nothing on Allow.
   Observable: exact equality controls, permit type compile-fail and allocation
   counter. This cannot prove authentication or OS containment.
5. Final effect/preflight — guard_dispatch owns one common checked decision and
   callback dispatch. Existing native dispatch is a compatibility wrapper.
   Preflight returns unit only; final dispatch repeats the same policy decision
   after Core's final currentness fence. Observable: granted preflight followed
   by denied final query has zero callback effects, granted final calls once.
   Real app-link zero-write/native effects remain Core/native acceptance.
6. OS containment/evidence — existing profile owner remains independent. No
   pure guard result is sandbox, app, native or original-draw.io evidence.
   Candidate formatting/applicability is reported separately from root Cargo
   and actual native execution. Exact candidate hashes bind the packet.

Explicit edges: policy -> borrowed selection -> pure decision -> callback;
Core auth/current lease -> both preflight and final dispatch, with a fresh final
fence. Preflight cannot be stored as a permit or replace the final fence/check.
Changing a grant does not change native identity or profile containment. Changing
the current role/document invalidates Core's admission independently of grants.

Reuse: D1's WindowPolicy owner, existing unique_json/parse_manifest loader,
existing DenyReason::ChannelForbidden text, existing ScopePermit and
guard_dispatch entry. No second parser, grant registry, IPC codec or native state.
Boundary change: adds only the approved renderer channel permission query and
disjoint callback permit; no handle/crash ownership or principal mint change.


## 5. Boundaries

Guard owns one parser, typed selection and effect-free query. Core owns authenticated role/window/source association and final lifecycle fence. Native owns document mint/revocation and physical callback lifetime. The existing ordered WorkerLink owns delivery. No data from a page or Bun supplies a trusted key or native principal.

## 6. Tasks

- [x] Implement one typed parser/borrowed selection and disjoint final permit.
- [x] Add semantic parser regression before the correction and execute focused Guard tests.
- [ ] Complete Core/native integration and execute final-deny and retired-document controls.
- [ ] Prove the original EditorUi dynamic request/reply callback through the actual route.

## 7. Test plan

Execute the malformed-channel parser before implementation (semantic RED), then renderer_channel_admission, renderer_channel_grants, renderer_channel_dispatch and the existing allocation test. Run compile-fail permit privacy docs and prefix/cross-snapshot/wildcard mutations. Native tests must independently deny after successful preflight, retire the original document during asynchronous work, and observe zero successor writes/callbacks. Actual original EditorUi.init -> isFullscreen -> rendererReq -> mainResp -> page callback must preserve its real request ID/source.

## 8. Review gates triggered

Public API and permission model: yes, exact named queries/opaque permits. Unsafe and dependencies: none in this Guard slice. Wire: no frame/version change in this slice; the selected route's WindowRef/table amendments remain separately owned and reviewed. Final exact-diff independent evidence remains required.

## 9. Perf impact

No speed claim. Existing allocation counter independently covers warmed preflight and final callback decisions. Parsing and verified selection stay cold. The exact check adds no steady-state allocation on Allow.

## 10. Open questions

No remaining source-design approval question under the direct delegation. Actual WK identity/origin qualification, physical retirement, final native Guard/effect ordering and original application proof are execution obligations and remain unverified until recorded. They do not authorize a fallback or readiness claim.

## 11. Inbound-only route availability — delegated clarification

Approved local implementation decision: GH552 issuecomment-6562596123. Pure original
listener registration consumes no preflight/Bun credit, so availability cannot depend
on an earlier outbound request. Forward the actual identity-only native Registered
observation on the existing RendererBridge endpoint, with its current lease/source,
window and world. This adds no page operation, numeric allocation, grant or JS epoch.
The existing Registry Entry retains registration for that exact current document and
clears it at the existing navigation/retirement cuts; no second document map is added.

Core emits existing RouteState(true) only after both actual native registration and
the current authenticated COMPAT-control WorkerLink subscription exist. Handle either
arrival order. If Subscribe is second, publish only after its genuine Subscribed
response writes successfully. Retirement emits false for all affected registered
documents, including idle ones without charged callbacks. Native rechecks the exact
bound document when applying the outcome. Availability never caches authorization:
final Host Guard and living-document checks remain mandatory per event.

Unavailable delivery must return an explicit refusal plus actual WK completion and
quiescence, not silently return or falsely emit Consumed. A disclosed native fixture
availability port is not a real Bun subscription. Required independent falsifiers
cover both arrival orders, forged/retired registration, failed subscription reply,
idle-document retirement, final Guard denial despite availability, refused versus
consumed accounting, and genuine physical quiescence. Actual authenticated Host and
original EditorUi acceptance remain separate from component/native fixture results.

Preserve the existing N1 role-loss contract: Core clears the old registration and
routing association and publishes unavailable. Native physical document custody may
survive, but successor subscription alone cannot renew document routing. The successor
must explicitly load and observe its genuine matching Started/Committed and new
Registered before availability. No implicit document adoption or registration replay
is added for this MVP. An early Registered may record bounded pending metadata in the
existing Entry; matching named delivery/availability waits for the actual Core
Committed fact to publish, without fabricating that fact or a navigation callback.
