# Spec: GH-1404 original application recovery startup

- Status: implementing
- Tracker: GH-1404
- Owner: GYLDLAB / prime
- Updated: 2026-11-07
- Pin: 151776c72f3615f6c87d8a03ccbab00a0c08e955

This is the bounded Core/native, SDK and original-main recovery implementation. Prime owns builds, exact final
review and native acceptance. A passing component test is not a recovery pass; v6 evidence remains frozen.

## 1. Goal & non-goals

After genuine Bun replacement, retain the same unsaved original draw.io 31.7.0 page and complete a real
successor request and Save through unchanged authority.
Suppress only the automatic startup load of an actually adopted, current, finished original document; explicit
later loads keep their normal behavior.
No renderer/preload change, URL authentication, fabricated navigation/Ready event, new permission, new file
blessing, browser persistence claim, or cross-platform acceptance.

## 2. Spec refs

[Migration reconciliation](gh1404-existing-migration-reconciliation.md), [installed runtime
facts](gh1404-macos-installed-original-runtime-facts.md), [GH531 window
ownership](gh531-window-registry-close-state-machine.md).
Architectures [02 §1](../architecture/02-ipc.md), [03](../architecture/03-security.md), [04
§3](../architecture/04-electron-compat.md) govern generation fencing, Guard and facade ownership.
The existing Core/native owners supply separately qualified physical observations and fresh successor
bindings; missing facts never authorize retention.

## 3. Acceptance criteria

### AC-1: Fresh Created window loads exactly once

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Fresh Created window loads exactly once; the exact original requirement below is normative, not
  substituted.
- **Negative control:**
  - **Single fault:** Omit the fresh initial LoadUrl.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Fresh window never receives its initial navigation.

Original criterion 1 (normative source text):

A fresh Created window performs its automatic initial LoadUrl exactly once.

### AC-2: Qualified Adopted retains the finished document

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Qualified Adopted retains the finished document; the exact original requirement below is
  normative, not substituted.
- **Negative control:**
  - **Single fault:** Skip load on an unfinished source.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Unfinished or unqualified document avoids loading.

Original criterion 2 (normative source text):

Actual Adopted of the same branded wrapper and exact current, finished, qualified original document skips only
that startup LoadUrl.

### AC-3: Stale or incomplete observations cannot skip load

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Stale or incomplete observations cannot skip load; the exact original requirement below is
  normative, not substituted.
- **Negative control:**
  - **Single fault:** Treat replay enumeration as actual Adopted.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Replay-only state bypasses actual source admission.

Original criterion 3 (normative source text):

Enumeration replay alone, absent/unfinished/retired document, changed incarnation/generation/source/session,
dead admission or unqualified launch cannot skip.

### AC-4: A failed first startup decision cannot rearm

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** A failed first startup decision cannot rearm; the exact original requirement below is normative,
  not substituted.
- **Negative control:**
  - **Single fault:** Rearm after a failed startup decision.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Second adoption incorrectly skips load.

Original criterion 4 (normative source text):

A rejected first decision consumes the opportunity. Repeated adoption, repeated helper calls and later
explicit loadURL cannot rearm it.

### AC-5: Explicit later LoadUrl retires the one-shot exemption

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Explicit later LoadUrl retires the one-shot exemption; the exact original requirement below is
  normative, not substituted.
- **Negative control:**
  - **Single fault:** Permit explicit LoadUrl to keep the startup slot.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** A later explicit load incorrectly reuses exemption.

Original criterion 5 (normative source text):

An explicit loadURL retires the opportunity before any await, even while the prior finished mirror remains
visible or the load later fails.

### AC-6: Retained recovery preserves ordinary listeners and startup join

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Retained recovery preserves ordinary listeners and startup join; the exact original requirement
  below is normative, not substituted.
- **Negative control:**
  - **Single fault:** Suppress registered listeners on adopted recovery.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Native/IPC event handlers or loadFinished proof are missing.

Original criterion 6 (normative source text):

All ordinary window/close/IPC listeners are installed. An adopted startup sets firstWinLoaded from verified
retained state and does not replay fresh args-obj or synthesize either half of loadFinished.

### AC-7: Genuine G2 crash recovery preserves unsaved 270

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Genuine G2 crash recovery preserves unsaved 270; the exact original requirement below is
  normative, not substituted.
- **Negative control:**
  - **Single fault:** Replace retained unsaved 270 with disk 260.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Real G2 inspection reports disk 260 or new Untitled page.

Original criterion 7 (normative source text):

Real installed crash: disk geometry 260, unsaved 270; after genuine G2 Ready and a correlated page request,
the same native document still contains 270.

### AC-8: Recovered file authority saves and reloads 280

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Recovered file authority saves and reloads 280; the exact original requirement below is normative,
  not substituted.
- **Negative control:**
  - **Single fault:** Forge a writable renderer path for Save 280.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Independent file readback does not show saved 280.

Original criterion 8 (normative source text):

Edit that retained page to 280, Save through its existing authorized fileObject/origStat path, independently
read 280 from disk, then fresh-open and read 280.

### AC-9: Unblessed and stale-role filesystem requests still refuse

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Unblessed and stale-role filesystem requests still refuse; the exact original requirement below is
  normative, not substituted.
- **Negative control:**
  - **Single fault:** Accept stale G1 role file permission.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Unblessed or old-role request succeeds.

Original criterion 9 (normative source text):

An unblessed path remains refused; renderer path strings never restore permission. Old-role requests and stale
document continuations remain ineffective.

### AC-10: Removal mutants falsify document retention and consumption

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Removal mutants falsify document retention and consumption; the exact original requirement below
  is normative, not substituted.
- **Negative control:**
  - **Single fault:** Remove the one-shot recovery exemption or consumption.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** Mutation does not break the pinned regression.

Original criterion 10 (normative source text):

Removing the startup exemption must reproduce the loss (Untitled or disk 260); removing consumption must fail
a later-load control.

### AC-11: Exact native Host exits with verified zero status and no residue

- **Given:** The pinned original application/source and exact authorizations stated in this criterion.
- **When:** The corresponding source or native event is exercised under its declared owner.
- **Then:** Exact native Host exits with verified zero status and no residue; the exact original requirement
  below is normative, not substituted.
- **Negative control:**
  - **Single fault:** Infer clean close solely from launcher disappearance.
  - **At-risk precondition:** The qualified original source, document and ownership have been established.
  - **Expected RED:** False-zero close passes despite native fatal or surviving processes.

Original criterion 11 (normative source text):

After recovered Save and native close, the exact Host must exit with kernel status zero, no native-retirement
fatal diagnostic, and no surviving owned process. Process disappearance or the launcher exit code alone is
insufficient.

## 4. Design

Existing Core Entry and WINDOW payload owners carry the recovery transition; no new handle, principal,
transport, permission, registry or crash owner is introduced.

**Atom / owner: Native retention / existing Core + WebView**
- **Boundary and contract:** Same Entry retains qualified physical lease/source/credited URL/finish; G1 role
  binding is revoked, G2 binds explicitly
- **Independent falsifier:** Created-only or late native registration grants routing; native test must still
  prove G2 IPC

**Atom / owner: Qualification / API app singleton**
- **Boundary and contract:** Existing currentOriginalWindowSession plus the same live WindowSource, window
  incarnation and finished document generation
- **Independent falsifier:** Different source/session, retirement or generation makes captured predicate false

**Atom / owner: Startup consumption / BrowserWindow**
- **Boundary and contract:** Actual Adopted arms one private slot on the same wrapper; one decision consumes
  it
- **Independent falsifier:** Replay-only or second decision skips; explicit LoadUrl is omitted

**Atom / owner: Application initialization / original main**
- **Boundary and contract:** Reattach ordinary handlers; fresh bootstrap args only for fresh startup
- **Independent falsifier:** Retained 270 replaced by startup disk contents; fake load/Ready is emitted

**Atom / owner: File authority / existing original main**
- **Boundary and contract:** Persisted trusted blessedPaths and unchanged assertWritablePath remain
  authoritative
- **Independent falsifier:** Unblessed renderer-supplied path writes

**Atom / owner: Evidence / root native driver**
- **Boundary and contract:** Observe the real page after G2 exchange and read saved XML independently
- **Independent falsifier:** Early temporal retention alone is reported as acceptance


Core stores the actual committed MacNativeDocumentLease, registered/finished observations and credited
MacNavigationUrl in the same Entry, separately from bound_owner: Option<WindowOwner>. The authenticated
pending load binds the initial owner, regardless of registration arriving before or after commit.
role_lost qualifies retention before cancellation or pending-call erasure: exact source allocation, native
stamp/view, Open phase, actual registration/finish, current lease and no pending load/destroy/close. Every old
role binding is removed; incomplete observations are discarded and late old callbacks cannot restore them.
Window Subscribe transfers the existing owner and emits Created then DocumentRecovered before Subscribed. This
is a physical snapshot only; it grants neither routing nor completion. COMPAT Subscribe under current/shutdown
and completed Window Subscribe rechecks the same source/lease and binds the successor; ordered DocumentBound,
Subscribed and RouteState use its new role witness.
An early COMPAT Subscribe cannot bind before Window Subscribe. A later explicit repeat publishes availability
if it newly binds a retained document. Native Registered is always physical metadata and cannot bind a
successor. All route access checks bound_owner; lease revocation independently invalidates every subsequent
route/native effect.
The SDK API adds captureOriginalWindowRecoveryDocument(window): (() => boolean) | undefined beside
currentOriginalWindowSession. It captures the existing source, exact WindowRef and native document generation;
every call rechecks qualified session, live/current/finished snapshot and the real COMPAT DocumentBound
association. It returns no URL/path or new authority.
BrowserWindow adds one private false/undefined/predicate slot. Only an authenticated Adopted response can arm
it, including the existing early return of a replay wrapper. Replay enumeration alone cannot arm it; repeated
Adopted cannot reset a consumed slot.
The existing internal original-ipc seam exports consumeOriginalRecoveryStartup(candidate: unknown): boolean.
Private-field branding and registry equality reject structural copies/proxies; the helper consumes before
returning its one result.
Public loadURL always retires the startup slot before its existing native call. Its real Loaded/Finished
completion and did-finish-load dispatch remain unchanged.
Original createWindow calls the helper only when the initial caller supplies its local startup-result object.
Other callers always take the ordinary load; false preserves that load too. Ordinary listeners remain
unconditional.
Only the initial caller branches on that result: retained sets firstWinLoaded; fresh installs the unchanged
two-event loadFinished join. A retained page receives no replayed argv/file-open bootstrap.
The immutable preload retains request IDs/callbacks and original renderer state. New main imports reinstall
handlers; a fresh request/response must prove the surviving bridge, independently of the SDK decision.
Original main persists native-picker blessings synchronously through real electron-store/Conf and reloads them
on startup; keep that existing authority path. In-flight-at-crash replies and lost file watchers are separate,
unclaimed behavior.
Rejected alternatives: weakening loadURL globally, trusting getURL/caller flags, fake
NavigationFinished/Ready, renderer-path blessing, or replaying args-obj over unsaved content.
Recovered close additionally retains native retirement custody. A resource-reader proof becomes permanent only
after its exact successful join wake is observed before the original deadline; independent renderer retirement
cannot invalidate that proof later. Missing or late first proof remains a failure. Real renderer-completion
events and the existing renderer deadline drive the existing pending-release owner; no clock is extended.
Terminal bridge revocation retains the bridge record until the existing release owner has verified both reader
and renderer quiescence. No-reader views publish no reader deadline.
Wire: append WINDOW event tag 10, DocumentRecovered { window, document_generation, native_url }, in
crates/keld-ipc/src/window.rs. Literal vectors and the existing canonical generator own both codecs. This
snapshot does not emit NavigationFinished/did-finish-load/Ready. The borrowed encoder keeps the same
source-wide URL credit attached; framing/protocol version, manifest, dependencies and native permissions are
unchanged. Platform acceptance: macOS only, pending.

## 5. Boundaries

Production owners: Core app_session/window_registry.rs, window_router.rs and compat_router.rs; IPC window.rs;
API app/WindowSource/COMPAT binding seam; Electron browser-window.ts and internal original-ipc.ts. Root owns
SDK integration and canonical codec generation.
Separate declared migration patch: src/main/electron.js, based on v6 SHA-256
ef561d45036d167640651ec3e09c884dff458d2c1320bb416ef7120d23307f48.
Worker-19 owns this spec and bounded Core/wire implementation/tests, frozen under recovery-v7/native-source.
Debug-only profile-test-hooks fixture uses the real NativeDocumentOwner/lease and existing admitted source;
callback identity/URL are explicitly modeled, never native acceptance evidence.
Native shipping lease implementation, source identity, Guardian, Guard, original renderer/preload and settled
dialog/CSP behavior remain their existing owners. No new unsafe or native OS API is introduced.

## 6. Tasks

### T-1: Review physical recovery and role binding

- **Blocked by:** none

Original owner task:

- [ ] T-1: independent architecture/security review of the final physical-state/role-binding transition,
      appended snapshot payload and exact SDK/main candidate; prime records exact content.

### T-2: Preserve RED/GREEN regressions and socket controls

- **Blocked by:** T-1

Original owner task:

- [ ] T-2: preserve root behavioral RED def07e74e0704a6f90746c938dd51db6 and retention GREEN
      5b2c331ed355497c8b91ae5323dd512f; run remaining registry/socket/wire controls and removal mutants
      against the frozen final source.

### T-3: Rebuild pinned original and run genuine recovered Save

- **Blocked by:** T-2

Original owner task:

- [ ] T-3: root rebuilds/seals the integrated original migration and runs real G2 IPC plus 270→280
      Save/readback and the removal control.

## 7. Test plan & evidence

Core tests use actual admitted graph/preload/create/load owners, native lease state machine and socket writer
with modeled callbacks. They distinguish retained physical state from role authority; they do not authenticate
Runtime or prove WebKit behavior.
API component tests cover qualified session/source/window/generation and current/finished, including the
actual Created-only replay gap.
Facade tests exercise real BrowserWindow private brand/Adopted/LoadUrl decisions and assert exact
native-request counts, replay identity, no synthetic events and non-rearming.
Original-main component tests execute its actual createWindow and initial startup join: fresh has one load and
requires both events; retained has no load, no args-obj and ordinary listeners.
Root must retain failing baseline output, passing candidate output, removed-exemption and removed-consumption
failures, plus genuine native observations.
Registry controls cover both registration orders, unfinished/unregistered/pending-load/destroy/close, stale
callbacks, revoked lease, substituted source and old-role cleanup. Existing
original_document_destroy::role_loss_clears_registration_and_subscription_cannot_renew_it remains unchanged
and must pass.
Socket controls assert snapshot-before-subscription, no route from Window Subscribe alone, explicit COMPAT
binding/reply/availability order with new role witness, early COMPAT ordering and revocation between snapshot
and binding. Native effects retain their existing final lease/role checks.
Native waits observe process identities, authentic G2 exchange, document geometry and disk XML; timeouts are
kill switches, not readiness evidence.
Clean Quit uses an external pre-registered kernel exit-status observer for that exact Host birth, plus its
bounded final stderr and independent family census. Controls include real exit zero, nonzero and signal exits,
forged birth, and rejection of the actual v7 fatal log even with an otherwise zero status. The
held-native-evaluation fixture proves terminal revocation cannot drop its bridge early; restoring the old map
clear must fail that same real WebKit case.
Retain native-final-recovery-v6/recovery-verdict.json and its early positive observation; v6 remains FAIL.

### E-1: Recorded recovery-v6 baseline and candidate source tests

- **Run:** retention-v6 and source/harness receipts as described in the preserved plan.
- **n:** 1 recorded source-level recovery sequence.
- **Environment:** macOS G1/G2 app, original renderer/preload; source test fixture.
- **Limits:** Native installed G2 Save and exact Host kernel exit remain unqualified.

## 8. Review gates triggered

Internal API/lifecycle semantics, Core successor binding, appended public payload and exact original-main
adaptation require independent architecture/security/wire review.
unsafe: none added; permission model: unchanged (negative control required); dependency: none; WINDOW payload:
one appended observation, framing unchanged.
Review must confirm revocation before transfer, no delayed G1 callback binding G2, actual finish
qualification, charged URL retention, explicit two-stage subscription and failure-path cleanup before
installed acceptance.

## 9. Perf impact

One bounded private slot per existing wrapper, one existing native lease/role binding and one credited native
URL per existing Entry. Snapshot encoding borrows the existing URL under its source-wide byte budget; no
per-frame work, new queue, polling or service.
No performance improvement claim. Existing startup and IPC budgets remain.

## 10. Open questions, blockers & unknowns

The confirmed baseline defect discarded the finished physical observation at role_lost. The implementation now
retains only qualified physical state and explicitly binds a fresh authenticated subscriber; no native UI
roundtrip is needed to read the existing atomic lease observation.
An Acquire liveness read is not a reservation. Snapshot publication, DocumentBound/RouteState and every later
native effect retain their existing current lease/role checks; revocation cannot create a usable stale
binding.
Pending acceptance: final independent review, frozen source gates and installed genuine G2 request followed by
stable unsaved 270, Save 280 and independent/fresh-open readback. In-flight replies and lost file watchers
remain outside this bounded claim.
AC-7–8 remain unproved until the root's real installed run passes. v6 remains FAIL, including its separately
preserved early positive temporal observation.

### B-1: Native G2 recovered Save and clean Host retirement are unqualified

- **Status:** open
- **Owner:** GH-1404 original recovery owner
- **Impact:** No source-level or early temporal result can close installed recovery.
- **Check:** Run real 270 unsaved retention, Save/readback 280, native exit-zero/census and removal mutants.