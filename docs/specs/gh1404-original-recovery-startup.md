# Spec: GH1404 original application recovery startup
Status: implementing
Tracker: GH-1404
Owner: GYLDLAB / prime
Updated: 2026-11-19
Pin: d98675c1a3b2eabb19e3000705e178f84fd169b4

This is the bounded Core/native, SDK and original-main recovery implementation. Prime owns builds, exact final review and native acceptance. A passing component test is not a recovery pass; v6 evidence remains frozen.

## 1. Goal & non-goals
After genuine Bun replacement, retain the same unsaved original draw.io 31.7.0 page and complete a real successor request and Save through unchanged authority.
Suppress only the automatic startup load of an actually adopted, current, finished original document; explicit later loads keep their normal behavior.
No renderer/preload change, URL authentication, fabricated navigation/Ready event, new permission, new file blessing, browser persistence claim, or cross-platform acceptance.

## 2. Spec refs
[Migration reconciliation](gh1404-existing-migration-reconciliation.md), [installed runtime facts](gh1404-macos-installed-original-runtime-facts.md), [GH531 window ownership](gh531-window-registry-close-state-machine.md).
Architectures [02 §1](../architecture/02-ipc.md), [03](../architecture/03-security.md), [04 §3](../architecture/04-electron-compat.md) govern generation fencing, Guard and facade ownership.
The existing Core/native owners supply separately qualified physical observations and fresh successor bindings; missing facts never authorize retention.

## 3. Acceptance criteria

### AC-1: a fresh window performs one initial load

- **Given:** A newly Created BrowserWindow with no retained original document.
- **When:** Its normal startup path runs.
- **Then:** The automatic initial LoadUrl occurs exactly once.
- **Negative control:**
  - **Single fault:** Remove or duplicate the initial LoadUrl.
  - **At-risk precondition:** A fresh window is created.
  - **Expected RED:** The exact native request-count oracle fails.

### AC-2: only an actual Adopted document skips startup loading

- **Given:** The same branded wrapper and an exact current, finished, qualified original document.
- **When:** The real Adopted response is consumed.
- **Then:** Only that startup LoadUrl is skipped.
- **Negative control:**
  - **Single fault:** Treat replay enumeration or a copied wrapper as Adopted.
  - **At-risk precondition:** The candidate is not the exact retained current document.
  - **Expected RED:** The startup load is not skipped.

### AC-3: unqualified or stale observations cannot skip

- **Given:** Replay-only enumeration, absent/unfinished/retired documents, changed
  incarnation/generation/source/session, dead admission or an unqualified launch.
- **When:** The startup decision is evaluated.
- **Then:** None of those conditions can skip the initial load.
- **Negative control:**
  - **Single fault:** Allow an enumeration replay to satisfy the adoption predicate.
  - **At-risk precondition:** No exact current finished document is retained.
  - **Expected RED:** The independent no-skip control fails.

### AC-4: a rejected first decision consumes its opportunity

- **Given:** The first startup decision has been rejected.
- **When:** Adoption, helper calls or a later explicit loadURL occurs.
- **Then:** The opportunity cannot be rearmed.
- **Negative control:**
  - **Single fault:** Reset the private slot after rejection.
  - **At-risk precondition:** A later adoption or helper call occurs.
  - **Expected RED:** The later call incorrectly skips startup loading.

### AC-5: explicit loadURL retires the opportunity before awaiting

- **Given:** A retained finished mirror remains visible.
- **When:** An explicit loadURL is called, even if that load later fails.
- **Then:** The startup opportunity is retired before any await.
- **Negative control:**
  - **Single fault:** Delay retirement until after the native load completes.
  - **At-risk precondition:** A second startup decision occurs during the await.
  - **Expected RED:** The stale retained document incorrectly suppresses loading.

### AC-6: adopted startup preserves ordinary listeners and state

- **Given:** A verified retained document is adopted.
- **When:** Original main initialization completes.
- **Then:** Ordinary window/close/IPC listeners are installed. firstWinLoaded comes
  from verified retained state. Fresh args-obj is not replayed and neither half of
  loadFinished is synthesized.
- **Negative control:**
  - **Single fault:** Skip ordinary listener installation or synthesize a Ready/load event.
  - **At-risk precondition:** The retained page is used after G2 binding.
  - **Expected RED:** The real bridge or event-count oracle fails.

### AC-7: recovery retains unsaved content after a real crash

- **Given:** Disk geometry is 260 and the unsaved original document is 270.
- **When:** A real installed crash is followed by genuine G2 Ready and a correlated
  page request.
- **Then:** The same native document still contains 270.
- **Negative control:**
  - **Single fault:** Replace retained state with startup disk contents.
  - **At-risk precondition:** Recovery occurs after the original process is lost.
  - **Expected RED:** The native document reports 260 or Untitled.

### AC-8: recovered content saves through existing file authority

- **Given:** The retained page contains 270.
- **When:** It is edited to 280 and saved through its authorized fileObject/origStat path.
- **Then:** Independent disk readback is 280, and a fresh open also reads 280.
- **Negative control:**
  - **Single fault:** Bypass the existing file authority or alter the written bytes.
  - **At-risk precondition:** The recovered document performs Save.
  - **Expected RED:** Independent disk readback differs from 280.

### AC-9: renderer strings and stale roles cannot restore file authority

- **Given:** An unblessed path, old-role request or stale document continuation.
- **When:** The renderer or retired role attempts a file operation.
- **Then:** The path remains refused and stale work has no effect.
- **Negative control:**
  - **Single fault:** Trust a renderer-supplied path or stale role request.
  - **At-risk precondition:** The path is not in the existing blessed set.
  - **Expected RED:** The write or stale effect counter detects unauthorized access.

### AC-10: removal mutants prove both sides of the startup exemption

- **Given:** The real startup exemption and its consumption logic.
- **When:** Each is removed independently.
- **Then:** Removing the exemption reproduces the loss (Untitled or disk 260);
  removing consumption fails a later-load control.
- **Negative control:**
  - **Single fault:** Remove either the exemption or its one-shot consumption.
  - **At-risk precondition:** Run the corresponding recovered or later-load scenario.
  - **Expected RED:** The matching loss or later-load control fails.

### AC-11: the exact Host exits cleanly after recovered Save and close

- **Given:** Recovered Save has completed and the native window is closed.
- **When:** The exact Host process exits.
- **Then:** Kernel exit status is zero, no native-retirement fatal diagnostic exists,
  and no owned process survives.
- **Negative control:**
  - **Single fault:** Leave an owned process alive or emit the fatal diagnostic.
  - **At-risk precondition:** The recovery workflow reaches native close.
  - **Expected RED:** The external birth-bound exit observer or process-family census fails.

## 4. Design
Existing Core Entry and WINDOW payload owners carry the recovery transition; no new handle, principal, transport, permission, registry or crash owner is introduced.
| Atom / owner | Boundary and contract | Independent falsifier |
|---|---|---|
| Native retention / existing Core + WebView | Same Entry retains qualified physical lease/source/credited URL/finish; G1 role binding is revoked, G2 binds explicitly | Created-only or late native registration grants routing; native test must still prove G2 IPC |
| Qualification / API app singleton | Existing currentOriginalWindowSession plus the same live WindowSource, window incarnation and finished document generation | Different source/session, retirement or generation makes captured predicate false |
| Startup consumption / BrowserWindow | Actual Adopted arms one private slot on the same wrapper; one decision consumes it | Replay-only or second decision skips; explicit LoadUrl is omitted |
| Application initialization / original main | Reattach ordinary handlers; fresh bootstrap args only for fresh startup | Retained 270 replaced by startup disk contents; fake load/Ready is emitted |
| File authority / existing original main | Persisted trusted blessedPaths and unchanged assertWritablePath remain authoritative | Unblessed renderer-supplied path writes |
| Evidence / root native driver | Observe the real page after G2 exchange and read saved XML independently | Early temporal retention alone is reported as acceptance |

Core stores the actual committed MacNativeDocumentLease, registered/finished observations and credited
MacNavigationUrl in the same Entry, separately from bound_owner: Option<WindowOwner>. The authenticated
pending load binds the initial owner, regardless of registration arriving before or after commit. role_lost
qualifies retention before cancellation or pending-call erasure: exact source allocation, native stamp/view,
Open phase, actual registration/finish, current lease and no pending load/destroy/close. Every old role
binding is removed; incomplete observations are discarded and late old callbacks cannot restore them. Window
Subscribe transfers the existing owner and emits Created then DocumentRecovered before Subscribed. This is a
physical snapshot only; it grants neither routing nor completion. COMPAT Subscribe under current/shutdown and
completed Window Subscribe rechecks the same source/lease and binds the successor; ordered DocumentBound,
Subscribed and RouteState use its new role witness. An early COMPAT Subscribe cannot bind before Window
Subscribe. A later explicit repeat publishes availability if it newly binds a retained document. Native
Registered is always physical metadata and cannot bind a successor. All route access checks bound_owner; lease
revocation independently invalidates every subsequent route/native effect. The SDK API adds
captureOriginalWindowRecoveryDocument(window): (() => boolean) | undefined beside
currentOriginalWindowSession. It captures the existing source, exact WindowRef and native document generation;
every call rechecks qualified session, live/current/finished snapshot and the real COMPAT DocumentBound
association. It returns no URL/path or new authority. BrowserWindow adds one private false/undefined/predicate
slot. Only an authenticated Adopted response can arm it, including the existing early return of a replay
wrapper. Replay enumeration alone cannot arm it; repeated Adopted cannot reset a consumed slot. The existing
internal original-ipc seam exports consumeOriginalRecoveryStartup(candidate: unknown): boolean. Private-field
branding and registry equality reject structural copies/proxies; the helper consumes before returning its one
result. Public loadURL always retires the startup slot before its existing native call. Its real
Loaded/Finished completion and did-finish-load dispatch remain unchanged. Original createWindow calls the
helper only when the initial caller supplies its local startup-result object. Other callers always take the
ordinary load; false preserves that load too. Ordinary listeners remain unconditional. Only the initial caller
branches on that result: retained sets firstWinLoaded; fresh installs the unchanged two-event loadFinished
join. A retained page receives no replayed argv/file-open bootstrap. The immutable preload retains request
IDs/callbacks and original renderer state. New main imports reinstall handlers; a fresh request/response must
prove the surviving bridge, independently of the SDK decision. Original main persists native-picker blessings
synchronously through real electron-store/Conf and reloads them on startup; keep that existing authority path.
In-flight-at-crash replies and lost file watchers are separate, unclaimed behavior. Rejected alternatives:
weakening loadURL globally, trusting getURL/caller flags, fake NavigationFinished/Ready, renderer-path
blessing, or replaying args-obj over unsaved content. Recovered close additionally retains native retirement
custody. A resource-reader proof becomes permanent only after its exact successful join wake is observed
before the original deadline; independent renderer retirement cannot invalidate that proof later. Missing or
late first proof remains a failure. Real renderer-completion events and the existing renderer deadline drive
the existing pending-release owner; no clock is extended. Terminal bridge revocation retains the bridge record
until the existing release owner has verified both reader and renderer quiescence. No-reader views publish no
reader deadline. Wire: append WINDOW event tag 10, DocumentRecovered { window, document_generation, native_url
}, in crates/keld-ipc/src/window.rs. Literal vectors and the existing canonical generator own both codecs.
This snapshot does not emit NavigationFinished/did-finish-load/Ready. The borrowed encoder keeps the same
source-wide URL credit attached; framing/protocol version, manifest, dependencies and native permissions are
unchanged. Platform acceptance: macOS only, pending.

## 5. Boundaries
Production owners: Core app_session/window_registry.rs, window_router.rs and compat_router.rs; IPC window.rs; API app/WindowSource/COMPAT binding seam; Electron browser-window.ts and internal original-ipc.ts. Root owns SDK integration and canonical codec generation.
Separate declared migration patch: src/main/electron.js, based on v6 SHA-256 ef561d45036d167640651ec3e09c884dff458d2c1320bb416ef7120d23307f48.
Worker-19 owns this spec and bounded Core/wire implementation/tests, frozen under recovery-v7/native-source. Debug-only profile-test-hooks fixture uses the real NativeDocumentOwner/lease and existing admitted source; callback identity/URL are explicitly modeled, never native acceptance evidence.
Native shipping lease implementation, source identity, Guardian, Guard, original renderer/preload and settled dialog/CSP behavior remain their existing owners. No new unsafe or native OS API is introduced.

## 6. Tasks

### T-1: independently review physical-state and role-binding ownership

- **Blocked by:** none
- [ ] Review the physical-state/role-binding transition, appended snapshot payload
  and exact SDK/main candidate.
- [ ] Prime records exact-content approval.

### T-2: complete registry, socket and wire controls

- **Blocked by:** T-1
- [ ] Preserve root behavioral RED def07e74e0704a6f90746c938dd51db6 and retention
  GREEN 5b2c331ed355497c8b91ae5323dd512f.
- [ ] Run remaining registry/socket/wire controls and removal mutants against frozen source.

### T-3: rebuild and qualify the integrated original migration

- **Blocked by:** T-2
- [ ] Root rebuilds and seals the integrated original migration.
- [ ] Run real G2 IPC, 270→280 Save/readback and the removal control.

## 7. Test plan & evidence
Core tests use actual admitted graph/preload/create/load owners, native lease state machine and socket writer with modeled callbacks. They distinguish retained physical state from role authority; they do not authenticate Runtime or prove WebKit behavior.
API component tests cover qualified session/source/window/generation and current/finished, including the actual Created-only replay gap.
Facade tests exercise real BrowserWindow private brand/Adopted/LoadUrl decisions and assert exact native-request counts, replay identity, no synthetic events and non-rearming.
Original-main component tests execute its actual createWindow and initial startup join: fresh has one load and requires both events; retained has no load, no args-obj and ordinary listeners.
Root must retain failing baseline output, passing candidate output, removed-exemption and removed-consumption failures, plus genuine native observations.
Registry controls cover both registration orders, unfinished/unregistered/pending-load/destroy/close, stale callbacks, revoked lease, substituted source and old-role cleanup. Existing original_document_destroy::role_loss_clears_registration_and_subscription_cannot_renew_it remains unchanged and must pass.
Socket controls assert snapshot-before-subscription, no route from Window Subscribe alone, explicit COMPAT binding/reply/availability order with new role witness, early COMPAT ordering and revocation between snapshot and binding. Native effects retain their existing final lease/role checks.
Native waits observe process identities, authentic G2 exchange, document geometry and disk XML; timeouts are kill switches, not readiness evidence.
Clean Quit uses an external pre-registered kernel exit-status observer for that exact
Host birth, plus bounded final stderr and an independent family census. Controls include
real exit zero, nonzero and signal exits, forged birth, and rejection of the actual v7
fatal log even with an otherwise zero status. The held-native-evaluation fixture proves
terminal revocation cannot drop its bridge early; restoring the old map clear must fail
that same real WebKit case.
Retain native-final-recovery-v6/recovery-verdict.json and its early positive observation; v6 remains FAIL.

## 8. Review gates triggered
Internal API/lifecycle semantics, Core successor binding, appended public payload and exact original-main adaptation require independent architecture/security/wire review.
unsafe: none added; permission model: unchanged (negative control required); dependency: none; WINDOW payload: one appended observation, framing unchanged.
Review must confirm revocation before transfer, no delayed G1 callback binding G2, actual finish qualification, charged URL retention, explicit two-stage subscription and failure-path cleanup before installed acceptance.

## 9. Perf impact
One bounded private slot per existing wrapper, one existing native lease/role binding and one credited native URL per existing Entry. Snapshot encoding borrows the existing URL under its source-wide byte budget; no per-frame work, new queue, polling or service.
No performance improvement claim. Existing startup and IPC budgets remain.

## 10. Open questions, blockers & unknowns
The confirmed baseline defect discarded the finished physical observation at role_lost. The implementation now retains only qualified physical state and explicitly binds a fresh authenticated subscriber; no native UI roundtrip is needed to read the existing atomic lease observation.
An Acquire liveness read is not a reservation. Snapshot publication, DocumentBound/RouteState and every later native effect retain their existing current lease/role checks; revocation cannot create a usable stale binding.
### B-1: final installed recovery acceptance is pending

- **Status:** open
- **Owner:** prime / native acceptance owner
- **Impact:** component and modeled callbacks do not prove real installed recovery.
- **Check:** final independent review, frozen source gates and installed genuine G2
  request followed by stable unsaved 270, Save 280 and independent/fresh-open readback.

### B-2: clean Host exit and v6 failure remain explicit

- **Status:** open
- **Owner:** root / native process acceptance owner
- **Impact:** AC-11 and final recovery acceptance are not complete.
- **Check:** verify exact Host kernel status zero, no fatal diagnostic and no owned
  process. v6 remains FAIL, including its separately preserved early positive
  temporal observation. In-flight replies and lost file watchers remain outside scope.
