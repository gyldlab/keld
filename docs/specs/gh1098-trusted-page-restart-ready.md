# Spec: Trusted-page notification after Bun restart readiness
Status: draft
Linear: GH-1098 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Tell an eligible application page once that a replacement Bun generation has reached communication readiness, without
exposing internal identities, reloading the page or implying success of an earlier request.

No automatic request replay, success acknowledgement, hung-handler liveness, reload, fake load/ready event or new
trusted-origin definition. The public notification has no arguments or data. Host routing metadata stays private.

## 2. Spec refs

- [Owning issue GH-1098](https://github.com/gyldlab/keld/issues/1098); current public owner decisions remain authoritative.
- [Owning requirements and direction](https://github.com/gyldlab/keld/issues/1098).
- [Request classification and resend rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh1095-request-error-codes.md).
- [Native/page bridge and document lifetime](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/macos_bridge.rs).
- [Host app-session generation/Ready owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session.rs).
- [Generation filesystem recovery](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel143-macos-generation-filesystem-recovery.md).
- [Renderer bridge invariant](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel142-macos-renderer-bridge-api.md).
- [Trusted navigation owner](https://github.com/gyldlab/keld/pull/1192).
- [Secure app origin owner](https://github.com/gyldlab/keld/issues/1090).

Verified source baseline: The current native bridge owns webview, navigation generation, document nonce and request state.
Those identifiers are internal authority checks, not a public restart payload. GH1098 records the direction but still
requires a defined trusted-page predicate and an approved spec. GH1095 r2's resend rules remain a draft dependency at this
source baseline.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given an eligible trusted document surviving a Bun replacement, when the new generation reaches the approved
communication-Ready barrier, then exactly one native notification is delivered and each currently registered listener is
invoked once with zero arguments.

Independent negative control: Trigger on process spawn or HELLO alone while the existing readiness barrier is held; any early
callback fails.

### AC2

Given one trusted and one untrusted/external document, when the same replacement becomes Ready, then only the trusted
document receives the notice.

Independent negative control: Authorize by page URL string, a page-provided flag, or the mere presence of window.keld; the
external negative must observe zero native delivery.

### AC3

Given the published public notification, when inspected at the page boundary and its transport serializer, then the argument
list/data is empty and contains no id, token, key, generation, counter or outcome field.

Independent negative control: Add the current generation number or original request id; literal schema/byte inspection must
fail.

### AC4

Given a page nonce and load-event count recorded before restart, when the notice arrives, then the nonce is unchanged and no
new load or initial-ready event is synthesized.

Independent negative control: Reload the page or dispatch a fake page-load event to signal recovery.

### AC5

Given pending requests with unknown or definitely-not-dispatched outcomes, when restart readiness is announced, then each
retains its own result and handler count; the framework performs zero automatic resends.

Independent negative control: Resolve an old Promise as success or enqueue its payload on the new generation.

### AC6

Given duplicate Ready observations for one generation/document, when processed, then only one notice crosses that native
document boundary.

Independent negative control: Deduplicate by event queue position only or reset the latch on each Subscribe callback.

### AC7

Given navigation/destruction or another restart before native delivery, when an old notice is dequeued, then the
authoritative lifetime/readiness recheck drops it and records that internal disposition without notifying a successor
document.

Independent negative control: Match only the native window id or deliver an old queued notice into a new page with the same
URL.

### AC8

Given initial boot, a failed restart that never reaches Ready, or a document admitted after the relevant Ready edge, when
callbacks are observed, then no historical restart event is replayed.

Independent negative control: Emit on every initial Ready, retain an unbounded historical notification list, or replay it to
a late subscriber.

### AC9

Given registered, removed, repeated and throwing listeners, when one valid notice is delivered, then existing listener
semantics are deterministic, removal works and one throwing listener cannot corrupt native ownership or another listener.

Independent negative control: Keep removed listeners alive across navigation or abort native delivery cleanup on a user
exception.

### AC10

Given each engine whose bridge and trust predicate are qualified, when results are published, then at least ten real hosted
runs per arm include both positive and independent negative evidence; other platforms remain unqualified.

Independent negative control: Promote a macOS result to Windows/Linux or replace native delivery with a fabricated DOM event.

## 4. Design

### 4.1 Trust is inherited from one owner

The shared navigation/preload owner supplies a native, current-document eligibility decision tied to the selected app source
and declared profile. This spec consumes that predicate only. A matching origin string, data attribute, DOM message or page
listener does not authenticate trust. If the prerequisite has no approved decision for an engine or document, deliver nothing
and report the lane unqualified.

### 4.2 Ready is communication readiness

Use the same existing Host barrier that permits new authenticated application calls after recovery, including required
generation binding and facade subscription. Process existence, token possession, HELLO or an observer log line alone is
insufficient. Record whether this is initial admission or a replacement using private Host state. The notification is an edge
for a currently ready replacement, not a permanent promise that the generation will remain healthy.

### 4.3 Proposed API and empty payload

Proposed public shape is window.keld.onRestartReady(listener: () => void): () => void. The returned function removes that
listener. Naming and exact export ownership require public API approval. The callback has zero arguments; no CustomEvent
detail or generation object is exposed. Existing generated event/transport owners allocate the event kind only after wire
review; public serialized data is empty. Internal native routing may retain generation/document identity but never copies it
into page-visible data.

### 4.4 Exactly-once eligible delivery

At the Ready edge, enumerate current eligible documents from the native owner and enqueue at most one private notice per
document/generation. Recheck generation readiness and document identity at delivery. A generation that dies or a document
that navigates before delivery invalidates that notice; record suppressed/stale internally rather than reporting successful
delivery. The exactly-once promise applies to a Ready edge whose recipient remains eligible through delivery. A page admitted
afterward or a late subscriber receives no replay.

### 4.5 Bounded lifetime and listener semantics

Attach the delivery latch to the existing document record and current replacement generation. Coalesce duplicate observations
of the same edge; retire state with the generation/document. Do not keep a session-long list of all restarts. Page listeners
are owned by the current document, use the existing event-listener facility where applicable and are released with it. Freeze
function identity/descriptor behavior through conformance before introducing a second callback registry.

### 4.6 Request outcomes stay independent

The callback cannot rewrite a pending request, certify rollback or initiate a framework resend. Applications may use it only
as an availability trigger allowed by the exact approved GH1095 contract; unknown outcomes remain unknown. A separate user
action or application idempotency policy is outside this signal. GH1097 deadlines remain independent.

### 4.7 Migration and platform seams

Extend the existing native-to-page event path and page facade together after approval. Preserve original document nonce and
webview ownership, and make the first-boot exclusion explicit in the same state owner. Any generated wire extension follows
the common schema/version owner. Do not create a raw page-native endpoint or reclassify untrusted content to make an engine
pass.

## 5. Boundaries

Future work belongs to Core's recovery Ready owner, the existing native document event route and public page facade.
Trusted-origin policy, process restart, renderer load lifecycle and request-outcome schema remain separately owned. This
publication changes no runtime or policy.

This publication changes only `docs/specs/gh1098-trusted-page-restart-ready.md` and related grouped-PR/issue handoffs. Shared
architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze the exact trusted-page predicate, communication-Ready barrier, empty public API shape and eligible/superseded delivery rule with navigation, GH1095 and transport owners.

### T2

- [ ] Add native generation/document latches and one generated empty event route with duplicate, stale, initial-boot and failed-recovery controls.

### T3

- [ ] Implement listener lifecycle through the existing facade owner and prove original page continuity and unchanged request outcomes.

### T4

- [ ] Run ten-or-more native positive/negative repetitions per qualified engine and publish per-platform results without promoting unavailable bridge lanes.

## 7. Test plan

AC1 uses an explicit Host Ready hold; delivery before release is failure. AC2 pairs a real approved-source page with an
external page and directly observes native delivery. AC3 inspects exact public serialized payload and callback arguments. AC4
preserves an independently recorded page nonce and load count. AC5 separately counts actual handler entries. AC6/7 force
duplicate Ready and navigation/restart ordering with barriers, not sleeps. AC8/9 cover absence/release and listener
exceptions. No real engine was exercised in this drafting session.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Public API, permission/trusted-page and wire/event-owner review. Unsafe/dependency changes are not proposed; any platform
adapter requiring them must declare and qualify its own gate.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

One bounded private pending notice/latch per eligible document, without retaining restart history or request payloads. No new
polling loop, log channel or performance claim.

## 10. Open questions (block implementation)

1. Approve the exact trust predicate for each selected origin/profile/engine and preserve any earlier owner's adopted source
decision.

2. Approve the public name/signature, generated event kind, zero-data encoding and version rule.

3. Approve the eligible-through-delivery rule, late-subscriber behavior and stale-notice suppression so exactly-once does not
imply stale readiness.

4. Complete the prerequisite bridge/readiness implementation and native ten-run arms before enabling or claiming this
notification.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.

