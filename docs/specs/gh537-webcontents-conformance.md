# Spec: WebContents conformance fixtures and recorded observations
Status: draft
Linear: GH-537 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: future conformance proposal; documentation only, do not merge.

## 1. Goal & non-goals

Give the next WebContents tranche independent, attributable conformance cells and fixtures.
Documented event promises, recorded event sequences and Keld authority decisions have different
oracles. The result must preserve that distinction while exposing missing and divergent behavior.

This slice owns cells and fixtures. It does not implement WebContents navigation, popup policy,
evaluation, navigation-generation replies or render-process-gone. GH459/GH538 own the facade and event vocabulary, while GH460 owns navigation policy and
GH461 owns popup policy under KEL-79. GH462 owns evaluation; GH542 and GH488 retain
separate cells.

## 2. Spec refs

- [GH537 and its Agent Brief](https://github.com/gyldlab/keld/issues/537#issuecomment-6026136530).
- [Pinned WebContents documentation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-contents.md).
- [GH532 evidence rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh532-first-proof-evidence-rules.md).
- [GH566 corpus owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh566-corpus-manifest-owner.md).
- [KEL74](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel74-compat-evidence-schema.md) owns records and scoring.
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md) and [05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md) govern authority and native observations.
- [GH494 packaging/comparison](gh494-two-arm-oracle-harness.md) and [GH495 recorder](gh495-lifecycle-recorder-replay.md) are draft prerequisites, not available execution evidence.
- [GH538](https://github.com/gyldlab/keld/issues/538) owns the event vocabulary;
  [GH459](https://github.com/gyldlab/keld/issues/459) owns the WebContents facade;
  [GH462](https://github.com/gyldlab/keld/issues/462) owns evaluation.
- [GH460](https://github.com/gyldlab/keld/issues/460) owns guard-first navigation policy;
  [GH461](https://github.com/gyldlab/keld/issues/461) owns popup handler policy under KEL-79.
  [GH546](https://github.com/gyldlab/keld/issues/546) instead concerns concurrent IPC invoke
  credits under KEL-80 and does not own a navigation or popup decision.

No production boundary change. Current source/doc inspection supplies design inputs only; no
golden, native execution, conformance record or compatibility percentage is created here.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a documented cell, its manifest carries the exact pinned sentence, snapshot and quote
   digests required by GH532; the mapped test asserts that statement and its declared status.
   A source change, fabricated quotation or unsupported status fails admission.
2. Given an observed-only cell, its fixture, Electron golden and receipt are committed together
   through GH495. Its label is recorded, its evidence verdict is unknown and its provenance
   includes the verified darwin-arm64 archive. Transcript agreement cannot promote it to pass.
3. Given navigation stimuli, separate cells distinguish programmatic loadURL, page/user
   navigation, preventDefault veto and same-document navigation. Deleting or adding the wrong
   event makes the mapped behavioral assertion fail.
4. Given a top-level recorded load sequence, comparison uses observed event names and their
   per-process order, with no timestamp comparison. Removing did-navigate fails the sequence
   oracle even if the other lifecycle events remain.
5. Given popup handler denial or an unrecognized return, creation is prevented; the latter
   also produces its documented console-error observation. A canceled creation does not emit
   did-create-window. Tests independently observe native creation count and emitted events.
6. Given title, load failure and in-page stimuli, cells assert their pinned payload/settlement
   contracts. A test cannot substitute a successful promise alone for an event and its arguments.
7. Given an external-link or non-granted popup attempt, allowing it in an application hook
   cannot override a denied authority decision. The cell consumes the adopted Keld policy and
   proves the relevant host effect did not occur; this spec does not select a new navigation policy.
8. Given Windows or Linux coverage, unqualified cells remain unknown with explicit platform
   rows. A macOS receipt or headless stub cannot manufacture a pass for another native engine.
9. Given a recorded cell with its committed fixture removed, renamed without rebinding, or
   mutated without an updated recording, provenance validation fails. Independent controls
   and a healthy follow-up accompany each cell before any claimed family exit.

## 4. Design

### 4.1 Single owners and source facts

The pinned WebContents page distinguishes user/page-initiated navigation from programmatic
loadURL and from same-document navigation. It documents in-page events for any frame, failure
events and loadURL rejection, popup handler return behavior, cancellation and title arguments.
It does not supply every total event order requested by GH537.

GH532 Rule 4 therefore governs the recorded-only rows below. A recorded ordering is useful
regression evidence while remaining unknown conformance until an approved cited contract exists.
Keep test execution status separate from the evidence verdict.

Reuse the shared GH566 corpus registration and snapshots. Do not add a second parser, duplicate
quote validator or new scorer. GH494/GH495 provide fixture format, comparator and recording;
family code supplies only stimuli and assertions. These family cells remain registered showcase
v1 cells with manifest-byte artifact digests. A GH495 differential recording is cited as supporting
provenance; its fixture-set artifact and denominator cannot substitute for this v1 corpus.
Compatibility fallback: not required for new
cells; retain existing cells and the currently adopted navigation policy.

### 4.2 Documented cell inventory

Each row may expand into several CellKeys when different observables need different oracles.
CellKeys remain the existing operation_id/oracle_id pair; do not invent report-only verdicts.

| Cell family | Fixture stimulus and independent observable | Pin location |
|---|---|---|
| Programmatic navigation | loadURL changes page without will-navigate | Event will-navigate |
| User/page navigation | An actual page action reaches the main-frame navigation hook | Event will-navigate |
| Veto | preventDefault leaves destination unloaded and prior page usable | Event will-navigate |
| Same-document navigation | Fragment/history action has no will-navigate | Event will-navigate |
| In-page event | Main and child-frame cases retain isMainFrame and URL | Event did-navigate-in-page |
| Load failure | Controlled invalid destination produces did-fail-load and rejects loadURL | did-fail-load; loadURL |
| Popup return policy | deny, allow and unrecognized return each have their own observation | setWindowOpenHandler |
| Canceled popup event | No native child and no did-create-window on canceled creation | did-create-window |
| Title | Explicit title and file-URL synthesized title preserve title/explicitSet | page-title-updated |

Citations are copied from the pinned Markdown snapshot during implementation, with exact byte
hashes and review against upstream. This table is a work inventory, not an already admitted corpus.
Do not cite mutable Chromium error tables to invent exact failure messages. Capture only the
pinned statement and stable controlled error behavior that the owning contract promises.

Use a controlled page or local test origin, not a public website whose content or network can
change. The test origin does not grant arbitrary application network authority.

### 4.3 Recorded-only inventory

| Observation | Required fixture and recorded witness | Evidence treatment |
|---|---|---|
| Top-level complete load | did-start-loading, did-start-navigation, did-navigate, dom-ready, did-frame-finish-load, did-finish-load, did-stop-loading | Exact observed sequence; unknown |
| loadURL A then B | Correlated call/resolve/reject rows for each invocation | Record actual settlement; unknown where docs are silent |
| Synchronous evaluation throw | executeJavaScript throw and exact rejection observation | Record the pin; no invented error shape |
| WebContents identity | Same view navigated through controlled documents, observed id each time | Record stability; unknown without a citation |
| window.stop | Controlled provisional load canceled by page action | Record actual provisional-failure behavior; unknown |

The list names required observations, not an asserted universal sequence. GH411 and a real
Electron run determine the admissible per-process ordering and any allowed normalization.
A synthesized Keld event sequence cannot generate its own expected golden.

All three provenance objects are required: committed input fixture, committed golden and
GH495 receipt. The receipt binds each hash and exact Electron/archive identity.
A transcript copied from an issue discussion or hand-written expected list is not a recording.

### 4.4 Keld authority cells

Use the Zettlr-style external link and a non-granted popup as two distinct cells.
Inputs include the actual guard decision, adopted application hook result and requested effect.
Output includes the observable navigation/window/shell effect according to the owning policy.

The negative control changes a hook to allow a guard-denied attempt. Assert the denied effect
is absent using the owner-approved native witness, then prove an authorized follow-up succeeds.
Do not satisfy the test merely by inspecting an error string or the guard function's source.

GH460 navigation and GH461 popup decisions are owned under KEL-79; this spec consumes only
their exact approved revisions and any governing KEL-102 permission-schema amendment. GH546's
IPC credit-window ownership is unrelated and cannot settle either policy. If either policy or
its manifest grant is not approved, the dependent cell remains blocked; no conformance green
is inferred from a proposed guard decision.

### 4.5 Platforms, failures and lifecycle

macOS arm64 is the initial actual recorder/native replay target. Windows/Linux rows remain
unknown until their own fixtures, engine identities and negative controls execute.
BrowserWindow or WebContents wrapper existence is not proof of an engine observation.

Keep child processes and native windows bounded. Fixture teardown waits for actual close/reap
and removes its isolated data. It cannot release a retained native operation by declaring a
promise locally settled; GH462 owns that lifetime contract.

Failed comparison on an observed-only cell fails its regression run while its KEL74 verdict
remains unknown. For a documented pending implementation, GH532's expected-fail form names
its implementing ticket; a passing regression test does not make the evidence pass.
Only a reviewed behavioral implementation updates assertion, expected verdict and evidence.

Runtime seam: fixture actions cross the existing facade/native boundary; no new bridge, principal,
grant or channel is proposed. Value/error/order assertions must come from the cited owner.
Migration: additive registered cells and shared snapshots; preserve old corpus identities.

## 5. Boundaries

Future edits: keld-compat corpus fixtures/registration and family-owned mapped test targets;
shared recorder/comparator only through GH494/GH495 ownership. This draft changes one file.
Do not alter navigation policy, runtime implementations, GH542 generation cells, GH488 crash
cells, permission grants, native callback retirement, historical recordings or product scores.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Obtain exact spec approval and GH494/GH495 evidence prerequisites; resolve dependent
  policy approvals and choose nonoverlapping registered CellKeys with GH566.
- [ ] T2 Add documented navigation/failure/title cells with pinned snapshots, expected statuses
  and independent controls that expose missing behavior without changing runtime code.
- [ ] T3 Build and record the five observed-only fixtures on macOS arm64; bind their raw/golden
  evidence and retain unknown verdicts.
- [ ] T4 Add popup and authority cells with real effect/absence witnesses and healthy follow-ups;
  preserve the adopted guard/application policy.
- [ ] T5 Run shared admission/receipt checks, publish exact platform coverage and hand pending
  behavior to its implementing ticket. Windows/Linux remain unknown until separately qualified.

## 7. Test plan

| AC | Test and independent falsifier |
|---|---|
| 1 | Pinned snapshot/quote admission; corrupt quote, digest or expected status |
| 2 | Real recording provenance; mark an uncited recorded cell pass |
| 3 | Four distinct stimuli; emit will-navigate on loadURL, ignore veto, or conflate in-page |
| 4 | Native observed sequence; delete did-navigate or swap two per-process rows |
| 5 | Native window census plus events; allow invalid handler output or emit canceled child |
| 6 | Title/payload and load-failure assertions; preserve promise result but delete the event |
| 7 | Guard-denied effect absent; force hook allow, then prove authorized healthy follow-up |
| 8 | Exact platform records; copy macOS result onto an unexecuted Windows/Linux lane |
| 9 | Fixture/golden/receipt hash join; remove input, mutate one byte or substitute another cell |

No native, recorder or test execution occurred here. Per-process order uses handshakes and
sequence numbers. Native failure stimuli must be deterministic and independent of internet state.

## 8. Review gates triggered

Documentation publication: none. Future conformance implementation requires human approval,
shared owner/recorder prerequisites and native platform acceptance. Any change to navigation or
permission policy belongs to its existing owner and separate public API/permission review.
No new dependency or wire change is proposed.

## 9. Perf impact

No production cost change or measurement claim. Record fixture process/window counts, cleanup
completion, output size and elapsed time during qualification. A slower oracle does not justify
dropping a required event, using a timestamp tolerance or converting unknown to pass.

## 10. Open questions (block implementation)

- GH411 receipt and approved GH494/GH495 recorder are prerequisites for recorded cells.
- Exact recorded sequences, message details and native macOS evidence are not yet available.
- Confirm each implementing ticket and adopted navigation policy revision before admitting cells.
- Additional engine qualification is future work; Windows/Linux unknown is explicit.
