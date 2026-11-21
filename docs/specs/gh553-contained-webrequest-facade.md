# Spec: Containment-qualified WebRequest registration facade
Status: draft
Tracker: GH-553 (#517) · Owner: @0monish (exact approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future documentation; do not merge or implement from this draft.

## 1. Goal & non-goals

Expose the shared default Session and the two draw.io WebRequest registration methods only within the exact
Host/package/native-policy scope already adopted by the owner. Accepted calls return undefined, retain/invoke no listener, and
leave real resource containment/CSP with the Host. All other methods remain typed refusals.

Non-goals: No general request interception, callback emulation, listener registry, arbitrary pattern matching, policy parser,
copied CSP header, new grant, provider resolver, additional Session identity store or broad enablement on other engines. Existing
scoped local-test approval remains scoped; publication does not approve production.

## 2. Spec refs

- [Owning issue GH-553](https://github.com/gyldlab/keld/issues/553).
- [Current facade exports](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts).
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md).
- [Architecture 04](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md).
- [Default Session draft](https://github.com/gyldlab/keld/blob/ffdbb828d36b40718f99d7b823830b924c120123/docs/specs/gh474-session-profile-clear.md).
- [Containment resource draft](https://github.com/gyldlab/keld/blob/8e8ec354f03e01b15a043d74a361e263967ef496/docs/specs/gh473-macos-resource-origin.md).
- [Initial D1 adoption](https://github.com/gyldlab/keld/issues/553#issuecomment-6561125416).
- [Scoped selector and pairing adoption](https://github.com/gyldlab/keld/issues/553#issuecomment-6561790580).
- [Reported native positive and local integration limit](https://github.com/gyldlab/keld/issues/553#issuecomment-6563380170).
- [Original draw.io registrations](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js).
- [Electron WebRequest oracle](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-request.md).
- [Electron Session oracle](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/session.md).

Verified source facts and limits: Pinned main exports app and BrowserWindow, not session. Original draw.io registers
onHeadersReceived without a filter and onBeforeRequest with a file URL filter before later ipcMain registrations. The later D1
comments adopted original-editor-offline, the H2 policy, trusted Core window-key binding and an exact copied Host/three-provider
pairing. A subsequent comment reports a real native positive and allows local test integration; final independent mutation and
complete original-app/workspace acceptance remain open. These reported receipts are not executions performed by this drafting
session.

The design below is proposed where it goes beyond the cited owner's adopted contract. Existing approved scopes stay approved;
draft publication does not transfer their authority or prove runtime behavior.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given one admitted Ready generation, when session.defaultSession, a default WebContents.session and supported empty-partition
access are read, then they use the same shared Session wrapper and webRequest identity. Premature/unbound access refuses instead
of selecting a fallback profile.

Independent negative control: Create a new Session object per getter or expose it before the authenticated Ready source exists.

### AC2

Given the exact qualified D1 scope, when original draw.io registers onHeadersReceived(listener) and onBeforeRequest(filter,
listener), then both return undefined and execution reaches the immediately following recorder statement.

Independent negative control: Throw from either accepted method; the next-statement witness must fail independently of later
ipcMain functionality.

### AC3

Given accepted registration and the entire owning resource/network probe set, when allowed/denied probes run, then listener
invocation count stays zero and the facade retains no listener or filter. Actual Host policy still determines resource effects.

Independent negative control: Store or invoke a listener; use reachable side-effect and retained-reference witnesses that do not
depend on garbage collection timing.

### AC4

Given missing/malformed policy, unknown selector, mismatched Host or provider bytes, wrong scope, unready/dead source or an
unqualified engine, when either registration is attempted, then the existing KELD-COMPAT-013 fallback remains and no silent
acceptance occurs.

Independent negative control: Enable on a self-reported metadata flag or ignore one actual provider fingerprint;
qualification-negative and allowed-positive fixtures must discriminate.

### AC5

Given the other six methods, when onBeforeSendHeaders, onSendHeaders, onResponseStarted, onBeforeRedirect, onCompleted or
onErrorOccurred is called with valid Electron-shaped inputs, then each throws the owning typed unsupported diagnostic before
effect.

Independent negative control: Silently accept onCompleted; the direct synchronous outcome test must fail.

### AC6

Given the approved call shapes, when a callable listener or null is supplied with/without a supported filter, then the no-op shape
follows the pinned reference/divergence table. Invalid arity/listener/filter inputs refuse atomically. No filter pattern becomes a
Host grant.

Independent negative control: Treat an arbitrary object as a listener or compile file://* into a permissive authorization rule.

### AC7

Given keld dev and an accepted non-null registration, when the call returns, then exactly one bounded diagnostic names the
explicit compatibility divergence and replacing Host policy. Null unsubscribe produces no retained state and follows its
separately pinned diagnostic rule.

Independent negative control: Emit one message per request or omit the registration message; count messages for two registrations
and repeated real resource probes.

### AC8

Given a qualified registration followed by source death, document retirement or policy/pair mismatch, when a new registration or
resource request occurs, then no cached facade acceptance bypasses the current owner checks and no stale listener/callback runs.

Independent negative control: Cache one lifetime-wide true boolean or treat prior no-op registration as authorization for a
changed Host/package.

### AC9

Given the exact original application and qualified native pair, when the no-op facade is composed into its actual
startup/workflow, then the original registrations advance, allowed local resources and wasm work, and forbidden resource effects
stay absent under independently load-bearing controls.

Independent negative control: Remove only the real native containment/CSP enforcement while preserving facade acceptance; actual
effect witnesses must change. A toy next-statement marker alone cannot pass this product criterion.

## 4. Design

### 4.1 Shared object, separate owners

GH553 owns the export and webRequest facade; GH474 owns profile/partition/storage semantics and consumes this one object. Use a
stable wrapper per authenticated generation and its live Ready source. Do not create a second native store, select a profile from
app arguments, or implement other Session APIs as success-shaped stubs. Unsupported members identify their own owner.

### 4.2 Preserve the adopted D1 contract

The initial unresolved selector note is superseded by the later scoped adoption: original-editor-offline, H2 and exact
Host/provider pairing remain with their original owners. Consume the typed verified policy/qualification result through the
trusted Core window binding. Do not reopen that choice, duplicate its parser/header, or treat this new facade document as approval
to widen it. A copied version string, file name or JS boolean cannot establish physical qualification.

### 4.3 Two accepted methods, six refusals

Only onHeadersReceived and onBeforeRequest can be accepted in the qualified lane. They perform no listener retain/invoke or
interception and return undefined. The other six always refuse in this tranche. Argument validation is a bounded facade concern
and is not a security URL-pattern interpreter. Use the original call shapes as mandatory positives; uncommon filter grammar waits
for the explicit oracle/disposition table rather than permissive coercion.

### 4.4 Registration and diagnostics

Accept an optional filter plus callable listener or null only after full shape validation. Null is the unsubscribe shape and
removes no state because no state is stored. Propose no new diagnostic for null; GH470's reference/divergence record must approve
that detail. Each accepted non-null dev registration emits one bounded event naming the method, divergence row and policy
identity. Do not print raw URLs, user paths, request bodies or callback text. The diagnostic path uses existing bounded logging
and cannot block native policy.

### 4.5 Admission and continued enforcement

The facade consumes the existing qualified-lane receipt; it does not mint or infer one. If unavailable, keep COMPAT-013.
Host/package/native policy admission precedes actual native response bytes, independent of whether JS registration has occurred.
Every subsequent operation still passes its real owner. A no-op registration creates no lease over future policy/pair state and
cannot preserve a stale Ready source.

### 4.6 Original caller and compatibility boundary

Retain draw.io's unmodified registration order and two argument forms. The next-statement witness belongs here; later ipcMain
registration and whole launch/open/edit/save/close remain with their product owners. The offline H2 scope is an explicit
divergence from generic Electron networking. Google Fonts, remote media, other document policies and Windows/Linux do not inherit
acceptance from the local test integration.

### 4.7 Retirement and reuse

Store no listener/filter closures, so there is no subscription replay across Bun generations. The shared Session's source end
invalidates its admission state; a fresh generation receives a new authenticated wrapper over the Host-selected profile only
through existing owners. Reuse central error taxonomy, bounded diagnostics and qualification source. No native callback, transport
parser, policy table or extra thread is introduced by the no-op methods themselves.

## 5. Boundaries

Only the new facade specification and related PR/issue metadata change. The existing gh553-verified-csp-admission-d1 native policy
work, original Host/provider qualification, GH473 adapter and active source owners remain intact. Do not modify their
implementation or declare unobserved native results. This publication does not change runtime boundaries.

## 6. Tasks (ordered vertical slices)

### T1

- [ ] Reconcile the exact D1 scope and current native qualification artifacts with GH470/GH474; pin accepted/refused input shapes
and null-diagnostic behavior before broad activation.

### T2

- [ ] Implement the single Session export and two qualified no-op registration methods with zero retained callbacks, six explicit
refusals and COMPAT-013 on every unqualified lane.

### T3

- [ ] Run admission invalidation, malformed input, provider/Host mismatch and listener-retention controls; verify each tested
failure is reachable independently.

### T4

- [ ] Compose the exact packed candidate into original draw.io, repeat native policy/containment controls and full owner workflow,
and report local test versus production acceptance separately.

## 7. Test plan

AC1–AC2 compare direct facade identity/return/throw and an immediately following original-shape marker. AC3 uses listener side
effects plus an independent reference census; a GC timeout alone is not a release oracle. AC4/AC8 alter one qualification input or
source lifetime at a time. AC5 tests all six valid-call refusal cells individually. AC6–AC7 pin malformed shapes and diagnostic
counts. AC9 uses actual native effects with allowed counterweights so an earlier unrelated guard cannot mask a missing
CSP/containment check. Report actual Host, package/provider digests, original source and OS/engine identities.

Every negative control changes one named contract fault while a legitimate positive remains reachable. Native and original-app
tests above are future acceptance, not runs performed here. Documentation validation checks source pins, complete template
sections, independently falsifiable criteria, bounds and exact published readback. Hosted CI is reported at the actual PR head.
This GitHub-only session ran no local hooks, just ci, native test or machine closeout. Official upstream GitHub sources were
retrieved on 2026-11-06. Context7 and other machine evidence are not claimed; the user's source restriction governs this pass.

## 8. Review gates triggered

Public API. Permission-model review applies to the consumed qualification/containment boundary and any future change to it; this
facade introduces no grant. Unsafe, dependency and wire changes are none in this prose publication. Native prerequisite approvals
remain independently required.

## 9. Perf impact

No performance gain is claimed. The facade retains no listeners and adds only bounded registration diagnostics. Native resource
costs and Host/provider qualification remain attributable to their existing owners.

## 10. Open questions and implementation gates

1. GH470/GH553: approve the bounded argument/disposition table and null-unsubscribe diagnostic rule without claiming generic
Electron interception.

2. Existing D1/native owners: publish or link the exact current qualified Host/provider/policy artifacts and independently
load-bearing mutation results; preserve prior local-test-only scope.

3. GH474/Ready source owner: confirm one Session identity and synchronous pre-Ready/dead-source refusal without constructing a
fallback store.

4. Product owner: execute actual original-app acceptance at the packed candidate; other engines/scopes retain typed refusal until
separately qualified.

Independent review and fixes must bind the actual publication revision. Unavailable native proof remains an acceptance gap, not a
completed feature or an implicit owner decision.
