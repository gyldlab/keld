# Spec: Electron-core autoUpdater with platform-specific source oracles
Status: draft
Linear: GH-582 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Specify the ten-member Electron-core autoUpdater facade on @keld/electron as a future consumer of GH-581's Host-owned updater
channel. Establish separate Windows Squirrel, macOS native and MSIX conformance cells before implementing behavior. The issue
records zero resolved corpus call sites; this draft increases specification coverage under the current user request and leaves
implementation parked.

Non-goals: implementing the electron-updater package adapter, a new updater service/channel, caller-controlled feed/signing authority, a
Linux built-in Electron autoUpdater, promising one universal quit sequence, filling unknown getFeedURL behavior with a guessed
URL, enabling unsupported Host installation modes or superseding the inert PR1367 adapter.

## 2. Spec refs

- [GH-582 scope and historic conformance assumptions](https://github.com/gyldlab/keld/issues/582)
- [GH-581 Host channel prerequisite](https://github.com/gyldlab/keld/issues/581)
- [Current runtime/feed-base limits](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md)
- [Current security/update authority boundary](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md)
- [Electron44.4.5 autoUpdater reference](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/auto-updater.md)
- [Electron44.4.5 Windows Squirrel implementation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/auto-updater/auto-updater-win.ts)
- [Electron44.4.5 MSIX implementation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/auto-updater/auto-updater-msix.ts)
- [Electron44.4.5 native facade event/window wrapper](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_auto_updater.cc)
- [Electron44.4.5 macOS Squirrel binding](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/auto_updater_mac.mm)
- [electron-updater6.8.9 BaseUpdater shared core event](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/BaseUpdater.ts)
- [electron-updater6.8.9 MacUpdater native core usage](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/MacUpdater.ts)
- [Reviewed cancelable quit contract](https://github.com/gyldlab/keld/blob/04f65fcfd3cf82fdf1fbf68cee27ab937b4949de/docs/specs/gh451-cancelable-two-phase-quit.md)

Verified source and limits: The pinned Windows Squirrel implementation emits no before-quit-for-update in quitAndInstall: it calls processStart then
app.quit after confirming updateAvailable. MSIX emits that event and requests registerPackage with forceShutdown; it does not
implement the macOS C++ window-close path. The macOS wrapper emits the event before closing windows and its OnWindowAllClosed
calls QuitAndInstall again, so multiplicity/close-veto behavior requires a native oracle. This corrects GH-582's historical
shared macOS/MSIX ordering claim. Windows/MSIX explicitly error for check-before-feed. macOS check uses the native updater
pointer; absent-feed observation remains unknown without the requested run. macOS invalid serverType throws TypeError. These are
source receipts, not executed native results.

All proposed API, schema, ownership and permission changes below are future contracts.
This documentation PR changes no runtime boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

A ledger contains all ten members in section4.1, each with the exact documentation sentence, pinned implementation path,
platform/native receipt or explicit unknown, expected Keld match/▲/unsupported verdict and independent mutation. No facade
implementation begins with missing cells.

Independent negative control: Delete one event, conflate core with electron-updater's similarly named event, or mark a source inference native PASS; the
ledger rejects the claim.

### AC2

Windows Squirrel, macOS native and MSIX have separate quit traces for ready/no-update, zero/multiple windows, veto and repeated
calls. The source-level sequences in section4.2 are preserved, and any Keld difference is explicitly approved.

Independent negative control: Assert before-quit-for-update for Windows Squirrel or assert MSIX's forceShutdown path necessarily emits the macOS window-close
sequence; source comparison fails and native status remains unknown.

### AC3

Check-before-feed is one explicit error with the pinned message on Windows/MSIX under the normal listener fixture. macOS remains
unknown until its exact native run. The fixture also records return value, synchronous throw versus rejection and behavior
without/with throwing error listeners.

Independent negative control: Substitute a universal no-op/check Promise or fabricate a macOS result; the per-platform trace or honesty check fails.

### AC4

setFeedURL preserves approved signature/type validation, including macOS TypeError for invalid serverType, but never changes
Host feed, headers, signing identity, channel, target or downgrade authority. The necessary divergence is recorded per platform.

Independent negative control: Pass a different URL, headers or allowAnyVersion and observe an altered Host request/verification policy; the no-authority
assertion fails.

### AC5

getFeedURL is recorded unknown until architecture06 names the owned feed-base field and approved exposure semantics. No caller
URL echo, template substitution, empty success or deprecated adapter string is used to disguise the missing core contract.

Independent negative control: Return a constructed feed URL or electron-updater's deprecated literal and label the cell conformant; the unknown-cell check
fails.

### AC6

update-downloaded payload cells use actual platform source and event argument positions. Windows Squirrel includes event
placeholder, release notes, version, date, URL and install callback; Keld's path/URL-free projection requires an explicit
approved divergence.

Independent negative control: Use the stale 'only releaseName' documentation summary, drop argument positions unnoticed or leak an internal Host artifact
URL/path through a callback/error; the source/projection oracle fails.

### AC7

Repeated check and quit calls have separately recorded semantics. Core docs' double-download warning, Windows/MSIX source and
native observations are compared with GH-581's single-attempt rules; any coalescing/duplicate-event change is ▲ rather than an
assumed match.

Independent negative control: Reuse electron-updater AppUpdater's coalescing as evidence that Electron-core coalesces, or start two Host activation attempts
from a duplicate call; the ownership/conformance checks fail.

### AC8

The package exposes one stable core emitter object. Where the electron-updater adapter's accepted route emits on core
autoUpdater, listeners registered on that exact exported object observe the approved event once per defined semantic transition;
identity survives normal repeated imports.

Independent negative control: Create a private adapter emitter or emit from both adapter and Host for the same transition without a documented upstream
multiplicity; the listener identity/count test fails.

### AC9

Unqualified or stale roles cannot trigger updater effects, and missing GH-581 admission yields a defined unsupported result
rather than an available/downloaded event. Listener callbacks, local fields and setFeedURL cannot manufacture Host attempt
state.

Independent negative control: Forge a downloaded event or retain a callback from a prior role generation and invoke install; no second or unauthenticated Host
mutation occurs.

### AC10

Each admitted platform uses the approved Host quit/activation/rollback contract and its real native receipts. Core return
values, error multiplicity, callback timing and ordinary-quit differences are explicit API decisions, not normalized silently by
a universal Promise wrapper.

Independent negative control: A Windows no-update call incorrectly quits because MSIX does, or a macOS source-only trace is counted as Keld
activation/rollback proof; the platform comparison fails.

## 4. Design

### 4.1 Ten-member inventory

| Kind | Member | Contract focus |
| --- | --- | --- |
| Method | setFeedURL | Validation versus no caller feed authority. |
| Method | getFeedURL | Unknown until owned feed-base exposure exists. |
| Method | checkForUpdates | Feed precondition, repeat/return/error behavior. |
| Method | quitAndInstall | Platform path, repeat/no-update and Host effect boundary. |
| Event | error | Error object/message, synchronous and listener behavior. |
| Event | checking-for-update | Exact singular spelling and state source. |
| Event | update-available | Selected availability; no install proof. |
| Event | update-not-available | Actual Host check outcome; no fabricated fallback. |
| Event | update-downloaded | Platform payload and staged/verified divergence. |
| Event | before-quit-for-update | Exact emitter, ordering, count and platform scope. |

Keep core and package-adapter APIs distinct. Core setFeedURL/getFeedURL are not the inert electron-updater package's silent
setter/deprecated getter. Core event checking-for-update is not the Host wire label checking. This facade maps an approved wire
projection; it creates neither update authority nor another transport contract.

### 4.2 Source conflict and native ordering ledger

| Platform/source | What is verified in source | What still needs native evidence |
| --- | --- | --- |
| Windows Squirrel | processStart then app.quit; no before-quit-for-update emit. | Actual app/window trace, veto and launch outcome. |
| macOS wrapper | Emits before-quit-for-update, closes windows, re-enters after all close. | Event multiplicity, veto, native install/app quit order. |
| MSIX | Registers restart, emits event, registerPackage(forceShutdown:true). | OS shutdown/window events, cancellation and restart result. |
| Windows no update | Emits error and returns without app.quit. | Exact listener/return timing under the pinned binary. |
| MSIX no update | Emits error then app.quit. | Ordinary quit, listener exceptions and native outcome. |

The GH-582 planning body groups macOS/MSIX into one close sequence. The pinned implementation does not justify that grouping, so
this spec does not ratify it. Native macOS re-entry may emit before-quit-for-update more than once when windows existed; mark
the count unknown until measured rather than normalizing it from prose. Each trace records event sequence and count, open-window
registry, process identity and installed outcome. A source-level processStart/registerPackage call is not proof an update ran
successfully.

Keld must use the GH-451 lifecycle transaction and GH-581/KEL-53 activation seam. Whether Keld intentionally differs from a
source path that starts installation before a cancelable quit, or from MSIX forced shutdown, is an explicit compatibility
decision. This facade must not introduce a force-close bypass to imitate an unqualified platform.

### 4.3 Configuration, payload and error boundary

setFeedURL may validate compatibility arguments and retain only the bounded inert state required by an approved API decision. It
never forwards caller URL/headers/serverType/allowAnyVersion as Host update authority. On macOS, rejecting an invalid serverType
as TypeError is a source-backed behavior; a blanket never-throw facade would need an explicit ▲. Do not store arbitrary
authorization headers for hypothetical future use.

getFeedURL is an implementation-blocking unknown. Once the Host owns a feed-base field, the owner must decide whether any
sanitized value can be exposed or a typed unsupported result remains necessary. Until that exact decision, the ledger stays
unknown and no working core getter is claimed.

The Windows source emits update-downloaded with an initial event object, releaseNotes, version, current Date, updateURL and a
callback invoking quitAndInstall. Native/macOS uses its native payload builder. GH-581 forbids Host paths/URLs in Bun
projections and v0 has no release notes, so exact core payload compatibility cannot be claimed. The public API owner must define
placeholder/omission/error policy and callback behavior as named ▲ cells before implementation. A callback is a local
convenience that performs a fresh authenticated trigger, never a captured install capability.

Use the existing EventEmitter semantics as the oracle, including an absent error listener and a listener that throws. Record
native/core return values per platform; don't turn synchronous errors, void returns and async functions into one Promise API
without a public-API decision.

### 4.4 Stable object and attempt ownership

One core emitter is exported by @keld/electron. The separate electron-updater adapter may address that object on its approved
BaseUpdater-compatible route, while its Mac native route has different event origins. Select one delivery owner for each Keld
semantic transition. Deduplicate transport replay by attempt/generation; do not erase an upstream event multiplicity merely
because a once-only expectation is convenient.

Host owns availability, stage and activation facts. Local listeners cannot authorize install by emitting an event. Old-role
callbacks and events are dropped/denied according to GH-581 correlation, while any existing Host mutation is reconciled from its
authoritative outcome. Dispose listeners without deleting Host stages. Import alone must remain free of feed I/O or
update-channel setup, preserving the existing facade/adapter purity boundary.

## 5. Boundaries

Future code belongs to the @keld/electron facade and an approved mapping over GH-581. This draft edits no module exports,
adapter package, channel table, guard operation registry, KEL-53 journal or native updater. GH-581, GH-451 and PR1367 owners
retain their scopes. Return/type, event payload and unsupported-result decisions are required before implementation. Windows
Squirrel/MSIX are reference-platform cells, not declarations that Keld supports either installer ecosystem.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Record all ten members and separate platform source traces in the conformance ledger. Add zero/multiple-window, no-update,
missing-feed, double-check and invalid-serverType fixtures with explicit unknowns.

### T2

- [ ] Resolve getFeedURL, safe payload/return shapes and compatibility divergences with public-API and Host owners. Approve
shared-emitter identity, event count and quit boundary before enabling code.

### T3

- [ ] Implement the thin facade over the admitted GH-581 channel while preserving import purity and exact object identity. Add
forged-event, stale-callback, wrong-generation and payload-leak controls.

### T4

- [ ] Execute pinned native Electron reference and Keld admitted-mode fixtures, refute source/doc conflicts with receipts, and review
actual process/activation outcomes separately from JS events. Keep unsupported cells parked.

## 7. Test plan

An oracle recorder independently captures timestamp/order, argument types and values, return/throw/rejection, event count,
window/process identities and filesystem-independent selected-version evidence. Run the same scenarios on the pinned Windows
Squirrel, MSIX and macOS Electron binaries; record package/binary digests. A table generated from the facade itself is not an
oracle. Include no-error-listener and throwing-error-listener fixtures, because EventEmitter failure behavior can interrupt
source paths. Pair each native observation with the exact Keld admitted-mode run; source-only or unavailable platforms remain
unknown. The absent-feed macOS run and native MSIX/macOS quit traces were not executed here.

Every acceptance criterion has an independently observable outcome and named negative control.
Record source commit, pinned reference, exact platform/runtime and raw receipt for every claimed result.
Documentation validation covers immutable source/readback, ten sections, controls and file limits.
Runtime, native and original-app tests are planned, not executed by this drafting session.
GitHub documentation CI cannot establish platform acceptance. Local `just ci`, hooks and machine
closeout were not run: the user restricted this work to GitHub. Official upstream GitHub sources
were read within that restriction; no Context7 query or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and compatibility/oracle review are mandatory. Permission, wire and activation gates are inherited from
GH-581/KEL-53; this facade cannot declare them satisfied. Lifecycle owners must approve divergence from Windows
install-before-quit, macOS re-entry and MSIX forced shutdown. Implementation remains parked with zero recorded corpus demand
until the owner explicitly selects it; the current user request authorizes this draft documentation only.

Status remains draft. Implementation requires the section 10 decisions and approval of the exact contract.
Draft publication does not approve an upstream dependency or change a parked implementation schedule.

## 9. Perf impact

One shared emitter and bounded metadata mapping are intended. Import should allocate no transport, Worker or updater network
request. Core repeated-call behavior may differ from upstream due to the Host's single-attempt rule; measure only after that
divergence is approved. No new polling loop or event-history cache is justified. No runtime benchmark was executed.

## 10. Open questions (block implementation)

1. Oracle owner: supply pinned macOS missing-feed and event-multiplicity traces, MSIX forceShutdown trace and Windows Squirrel
ordinary-quit trace, including veto and no-update cases.

2. Host/public-API owner: name the owned feed-base field and decide getFeedURL exposure or typed unsupported behavior; preserve
unknown until then.

3. Public-API owner: approve exact safe update-downloaded argument shape, missing release notes/URL behavior, callback signature
and platform return/error semantics.

4. Lifecycle owners: select Keld quit/install behavior where Windows, macOS and MSIX sources disagree; explicitly approve each
divergence without bypassing the Host close transaction.

5. Adapter owner: define shared core-emitter routing with GH-581 so event replay and upstream native multiplicity are
distinguished. Do not change PR1367 before its real branch is approved.

Independent review must refute or fix concrete findings on the published revision. Platform gaps
remain explicit; no unexecuted acceptance test is reported as passed.
