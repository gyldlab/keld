# Spec: Full macOS navigation event vocabulary
Status: draft
Linear: GH-538 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Synthesize Electron's full navigation vocabulary from correlated engine-neutral signals while keeping one host
navigation-authority mint and one load-completion owner.

Non-goals: Navigation policy, new-window admission, other platforms and another IPC or navigation-generation system are excluded.

## 2. Spec refs

- [Owning issue GH-538](https://github.com/gyldlab/keld/issues/538) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Engine and native events](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/engine.rs).
- [macOS navigation adapter](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/mod.rs).
- [Engine method rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/AGENTS.md).
- [Electron webContents events](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-contents.md).
- [Navigation owner issue](https://github.com/gyldlab/keld/issues/538).
- [Authority ordering correction](https://github.com/gyldlab/keld/issues/459#issuecomment-6371834142).

Verified baseline: Current native engine seam exposes navigation admission and limited events, not this complete facade
vocabulary. The issue assigns DOMContentLoaded and same-document reporting to the app world; those reports are informational and
cannot establish trusted navigation authority.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given the pinned macOS top-level navigation oracle, when a document loads, then start/provisional/commit/DOM/finish/stop events
match its recorded order and frame scope.

Independent negative control: Derive did-navigate from finish or reorder commit after DOM readiness.

### AC2

Given pushState or hash change proven same-document, when reporting occurs, then did-navigate-in-page is emitted while the
existing host navigation identity stays unchanged.

Independent negative control: Mint a new generation for a same-document reporter event.

### AC3

Given a failed matching load, when the native failure arrives, then did-fail-load carries the approved error mapping and the same
loadURL Promise rejects once.

Independent negative control: Emit an error but leave the request pending, or reject a newer unrelated attempt.

### AC4

Given a cancelled/stopped/superseded attempt, when its late native signals arrive, then the current attempt's loading state, URL
and Promise remain correct.

Independent negative control: Use a single uncorrelated global isLoading flag.

### AC5

Given page-title updates, when a current qualified report arrives, then getTitle and page-title-updated reflect it; a stale report
cannot overwrite the new document title.

Independent negative control: Accept old-document title callbacks after navigation identity changes.

### AC6

Given forged app-world DOM/same-document data, when received, then it may be rejected but cannot rebind privileged source
identity, restore revoked authority or choose a window.

Independent negative control: Use a payload generation to authorize the sender.

### AC7

Given loadFile, reload and stop, when invoked, then they reuse the existing guarded navigation/attempt owner and terminate prior
pending work according to the approved contract.

Independent negative control: Create a separate unguarded reload path or leave cancelled work retained.

### AC8

Given a new engine hook, when it lands, then a live macOS backend and consumer are present in the same change; unsupported engines
remain explicit.

Independent negative control: Add a default empty hook that allows tests to pass without native delivery.

## 4. Design

### 4.1 One signal model

Proposed logical signal set is provisional-start, commit, same-document, DOM-content-loaded, finish, fail and title. Carry
existing view, native-attempt and navigation identity types. Keep native error codes factual at the engine seam; the compatibility
layer maps Electron names and signatures.

### 4.2 Authority separation

Only GH542's trusted host classifier/state machine may revoke, advance or bind navigation authority. An app-world reporter
provides content observations and can be forged by application code. Validate its existing source binding and current lifetime;
never let it trigger commit or restore an old lease. No bridge-world exposure or second generation counter.

### 4.3 Shared completion state

GH459 owns each load admission and completion record. This child extends its event and failure vocabulary, including explicit
stop/supersession dispositions. isLoading/getURL/getTitle derive from the current qualified attempt and native observation, not
optimistic command arguments. Late failure of an earlier attempt cannot reject a later Promise.

### 4.4 Frame and event semantics

Pin GH537's event-order oracle before implementing names such as did-frame-finish-load, did-frame-navigate and
did-fail-provisional-load. Main-frame and subframe behavior need distinct cells. DOM readiness is not finish; commit is not
finish; same-document reports do not imply a loadURL completion.

### 4.5 Runtime seam and migration

Use the existing UI callback/Core queue and generated contracts. Any added engine-neutral hook must have its live macOS producer
and facade consumer in the same implementing PR. Reporter registration and teardown follow the current document owner;
navigate/destroy/role retirement release state. Existing narrow GH459 consumers migrate to the extended signal owner without
duplicate dispatch.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation stays in the macOS engine adapter, current host navigation owner and compatibility WebContents facade. Do
not change navigation guard semantics, invent a second transport or enable unsupported platform no-ops.

This publication changes only `docs/specs/gh538-macos-navigation-vocabulary.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Pin GH537 oracle signatures/order and GH542 trusted classification contract; record per-event pending cells.

### T2

- [ ] Implement correlated live native signal production and the shared completion/state consumer with old-attempt negative
controls.

### T3

- [ ] Add constrained app-world DOM/same-document reporting and loadFile/reload/stop facade routing through existing guards.

### T4

- [ ] Run real native success/failure/redirect/hash/subframe/title/destruction fixtures and update only evidenced vocabulary
cells.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-538;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use a controlled multi-frame local corpus with explicit native attempt IDs and barriers. Inject forged/stale reporter messages
separately from genuine native commit callbacks. Assert event ordering and Promise settlement independently; do not compare two
implementations driven by the same synthetic signal list alone.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and WebEngine design review; wire and permission owners review any actual boundary amendment. GH537/GH542 qualification
remains prerequisite.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Retain only bounded per-view current/pending attempt state and existing queue accounting. Measure callback churn under repeated
navigation; no first-paint or throughput claim.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve complete GH537 order/error/frame mappings and stop/supersession behavior.
2. Qualify trusted native classification and app-world reporter provenance/lifetime.
3. Approve any new live-engine hook and shared schema with current owners; other engines remain unknown.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

