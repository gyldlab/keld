# Spec: Session permission handlers that only narrow Host authority
Status: draft
Tracker: GH-472 (#517) · Owner: @0monish (exact approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future documentation; do not merge or implement from this draft.

## 1. Goal & non-goals

Plan one Session request/check facade over the existing Host permission evaluator. An application handler can deny a permitted
engine operation, but cannot create authority or turn an unknown engine hook into a guarded permission. Implementation remains
parked.

Non-goals: No new permission mapping table, schema key, permission cache, device/HID/USB/serial/Bluetooth/WebAuthn support, origin
inference, screen-capture implementation, OS consent bypass or activation of the parked issue.

## 2. Spec refs

- [Owning issue GH-472](https://github.com/gyldlab/keld/issues/472).
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md).
- [Engine media boundary](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/media.rs).
- [Engine rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/AGENTS.md).
- [Mapping owner GH505](https://github.com/gyldlab/keld/issues/505).
- [Window grants draft](https://github.com/gyldlab/keld/blob/66390503b01560ac96b1f4db9aea60116ecae792/docs/specs/gh594-window-web-grants.md).
- [Default Session owner](https://github.com/gyldlab/keld/blob/ffdbb828d36b40718f99d7b823830b924c120123/docs/specs/gh474-session-profile-clear.md).
- [Electron Session oracle](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/session.md).

Verified source facts and limits: Main's guarded media builder installs an explicit permission handler using a minted WebviewId
and the shared guard. Its comments distinguish new permission requests from saved browser permission behavior. GH594 is a draft
for future per-window grants and explicitly keeps clipboard/origin extraction separate. GH505 owns the single permission-name and
per-engine hook mapping. This document consumes that mapping and cannot classify an unobserved engine hook as covered.

The design below is proposed where it goes beyond the cited owner's adopted contract. Existing approved scopes stay approved;
draft publication does not transfer their authority or prove runtime behavior.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a host-surfaced permission request and no matching app handler, when the native decision is due, then the result denies and
the diagnostic names the missing policy/handler requirement. Clearing a handler with null restores this state.

Independent negative control: Treat a missing or removed handler as allow; verify a real denied media effect, not only a mock
boolean.

### AC2

Given a current view with guard Allow, when a request callback returns false or a check handler returns false, then the native
decision denies. When all independent authority and handler conditions allow, an available-device positive can proceed to the OS's
own consent decision.

Independent negative control: Ignore the app's false; the denied effect and positive control must distinguish guard policy from
missing hardware.

### AC3

Given guard Deny, when a forged, late or incorrectly accepted handler response says true, then the native decision still denies
and no effect starts. Normal dispatch does not invoke a narrowing handler after guard Deny.

Independent negative control: Replace guard AND app with guard OR app; arrange a reachable guard-denied request and assert zero
native effect.

### AC4

Given a camera-only window grant, when media requests ask for audio plus video, then the combined request denies; video-only may
pass the remaining checks. Request evaluation covers every requested mediaType; check evaluation covers its one mediaType.

Independent negative control: Use only the first requested media kind or map microphone to camera; a controlled combined request
must fail.

### AC5

Given a permitted request paused before native completion, when navigation, view destruction or role retirement invalidates its
identity, then the later app callback denies/no-ops according to the native API and cannot authorize a successor document.

Independent negative control: Omit the final Host identity check or use numeric view id without incarnation; observe a forbidden
native effect independently.

### AC6

Given a still-pending handler never responds, throws or supplies a non-boolean before terminal settlement, when that failure or
the approved deadline/check boundary is reached, then permission denies once and admission/retained callback budgets remain
bounded until real release. A later failure cannot retroactively deny an already consumed native Allow.

Independent negative control: Free a native reservation on local timeout while the callback is held; repeated requests must expose
the retained-owner accounting error.

### AC7

Given request and check calls for the same mapped permission/principal, when both are evaluated, then they use one guard mapping
and immutable policy snapshot with API-specific callback/return shapes. A check success alone does not fabricate a completed
request or cache a broader grant.

Independent negative control: Maintain a separate permissive check table or substitute Promise<boolean> for the synchronous check
result.

### AC8

Given an engine-decided or unexposed permission, or unavailable requestingUrl/isMainFrame provenance, when publishing a
conformance result, then the row stays a named divergence/refusal/UNKNOWN and exposes no fabricated origin identity.

Independent negative control: Copy the top-level URL into subframe provenance or label an engine fallback as a passed Host guard
test.

### AC9

Given the default Session has admitted request/check handlers, when another Session selector, display-media handler or device API
is requested without its own supported owner, then it refuses with the owning typed category and creates no native capability.

Independent negative control: Route setDisplayMediaRequestHandler into camera Allow or accept a nonempty partition as the default.

### AC10

Given guard Allow and a current request identity, when the first Host-admitted native decision is consumed and the app then
responds again, then true followed by false preserves that first Allow, while false followed by true preserves that first Deny.
Each sequence invokes the native decision closure exactly once. A throw or deadline after consumption cannot change that outcome.

Independent negative control: Apply the second boolean, revoke a consumed Allow merely because the handler threw later, or invoke
the native closure twice; independent decision/effect counters must fail each incorrect transition.

## 4. Design

### 4.1 One decision, separate observations

GH505 owns the permission-name/engine-hook table; GH594/KEL-102 own per-window grants. Reuse their typed mapping and verified
policy. Authentication, guard authorization, app narrowing, OS consent and native effect are separate atoms. A facade callback
cannot mint a principal, grant, permission name or origin. Request and check share the evaluator, not one cached answer across
distinct operations.

### 4.2 Handler registration

Use the GH553/GH474 default Session object and one request-handler plus one check-handler slot for its admitted generation. A
function replaces the prior slot for future requests; null removes it. Preserve the registration incarnation captured by an
existing request; replacement cannot redirect its callback. Reject malformed registrations without altering the old slot. No
arbitrary partitions or per-wrapper registries.

### 4.3 API shapes and conservative timing

The request API supplies a boolean completion callback; the check API returns a boolean synchronously. Preserve that distinction.
Missing handlers deny. While a request remains pending, a throw, non-boolean completion or deadline selects Deny. Once its native
decision is consumed, a later throw, duplicate or timeout cannot change that result or claim to revoke an effect. Do not block the
UI thread waiting on a Bun callback
that can reenter the same Host owner. Native synchronous checks need an approved owner path that can satisfy this contract; until
qualified they remain unsupported/denied rather than returning a fake Promise or optimistic true.

### 4.4 Guard first and final admission

Consult app handlers only after a permitted Host evaluation. Final native completion rechecks current document/request incarnation
under the shared owner before granting. Forged/late true cannot override a prior Deny. If app code invokes another broker during
its callback, that broker independently enforces its own grants. A camera allow does not authorize screen capture, clipboard,
filesystem or shell actions.

### 4.5 Media and provenance

Use the mapping owner's all-kinds rule for combined media requests; unknown or empty/unrepresentable kind sets deny. Host-minted
WebviewId/current document selects authority, never AppProcess fallback. requestingUrl/isMainFrame and subframe attribution
require their native provenance owner. Missing fields stay explicitly unavailable; do not synthesize them from a page string.
Saved permission behavior belongs to KEL-135 and requires separate qualification.

### 4.6 Capacity and lifetime

Propose 16 in-flight native request decisions per view lifetime and 64 per Host session, with an owner-approved 30-second decision
deadline. Deny overflow before allocating another retained callback. Local timeout settles the app-facing operation and arranges
the native deny; capacity remains charged until the native completion is consumed or teardown is proved quiescent. Role restart
does not reset retained work. One Host-owned terminal latch arbitrates valid completion, pre-settlement failure, deadline and
lifecycle loss. The first admitted terminal decision wins after the final identity/guard check; each native decision closure is
invoked at most once on its owning UI thread. Subsequent app completions are inert for both true-then-false and false-then-true.
A consumed Allow is not retroactively Deny, and a consumed Deny cannot be widened by a later true. Any actual revocation belongs
to its separate native authority/lifetime owner.

### 4.7 Unsupported surface and parked status

setDisplayMediaRequestHandler and device-specific handlers/events remain explicit typed refusals until their own capture/device
owners supply approved contracts. Main-process clipboard/Notification are not silently gated by this facade. Publicly document
per-engine coverage from GH505's receipts, not a copied permission matrix. Planning is authorized here, while parked
implementation requires its actual reopen decision.

### 4.8 Migration unit

The future vertical slice includes Session registration, generated request/completion messages, guard mapping consumption, native
callback lifetime and conformance rows together. Central error/schema owners allocate wire/error identities. No second native
permission handler replaces the already guarded builder; compose narrowing at the existing decision seam. No new dependency is
proposed.

## 5. Boundaries

This publication changes only the proposed facade spec and PR/issue metadata. GH505 retains mapping ownership and its existing
unpublished documentation work. GH594/KEL-102 retain grant/schema authority; KEL-135 retains saved-permission/profile lifetime.
Future native changes require the current platform owner's claim and exact unsafe/design review.

## 6. Tasks (ordered vertical slices)

### T1

- [ ] Obtain the parked-feature reopen and exact spec approval, consume the GH505 mapping artifact, and record the Electron
request/check reference shapes with distinct negative controls.

### T2

- [ ] Implement one genuinely host-surfaced media permission through existing guard, Session narrowing and native completion,
including camera-only versus combined-media tests.

### T3

- [ ] Add handler replacement/removal, malformed responses, both duplicate-boolean orders, throw-after-completion, native
held-completion/deadline and role/document retirement tests with independent native-decision and retained-owner counts.

### T4

- [ ] Qualify the actual macOS permission hooks and saved-profile interactions; record engine-decided/unknown cells without
activating unqualified Windows/Linux or device APIs.

## 7. Test plan

AC1–AC4 require an available real device/controlled OS consent positive as a counterweight to denied effects. Unit truth-table
tests prove only guard/app composition. AC5–AC7 fence native request, callback, role loss and deadline events separately; inspect
real closure retirement and the next healthy request. AC8 checks provenance against a native subframe fixture. AC9 exercises
direct API intake and verifies no native call. AC10 holds the native-consumption boundary, then injects the opposite duplicate,
a later throw and a later deadline; observe one immutable consumed decision and its actual effect independently of callback logs.
Never use a missing device or unrelated default denial as proof that the tested policy branch enforced the outcome.

Every negative control changes one named contract fault while a legitimate positive remains reachable. Native and original-app
tests above are future acceptance, not runs performed here. Documentation validation checks source pins, complete template
sections, independently falsifiable criteria, bounds and exact published readback. Hosted CI is reported at the actual PR head.
This GitHub-only session ran no local hooks, just ci, native test or machine closeout. Official upstream GitHub sources were
retrieved on 2026-11-06. Context7 and other machine evidence are not claimed; the user's source restriction governs this pass.

## 8. Review gates triggered

Permission model and public API. Any generated wire change and new native unsafe operation needs its own independent gate. No
dependency addition. GH505/GH594 approvals and platform facts remain separate prerequisites.

## 9. Perf impact

No performance claim. The 16/64 request and 30-second proposals need owner approval and actual callback-retention evidence. No new
polling thread; existing native completion and Host owner drive progress.

## 10. Open questions and implementation gates

1. GH505 owner: publish/reconcile the single permission mapping and per-engine receipts; preserve the existing unpublished GH505
work rather than replace it.

2. Native/transport owners: qualify synchronous check and asynchronous request routing without UI-thread deadlock or reentrant
authority widening.

3. Approve the proposed request/retained-callback caps and deadline, then demonstrate actual native deny/quiescence on timeout and
teardown.

4. Resolve parked implementation priority and exact native/media prerequisites; document all unsupported engine, subframe,
saved-permission and device cases.

Independent review and fixes must bind the actual publication revision. Unavailable native proof remains an acceptance gap, not a
completed feature or an implicit owner decision.
