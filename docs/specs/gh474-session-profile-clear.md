# Spec: Default Session identity and macOS website-data clearing

- Status: draft
- Tracker: GH-474
- Owner: @0monish (specification coordination)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Bind Electron's default Session to the Host-selected KEL-135 profile. Clear explicitly selected supported data on that profile with an authenticated request and an asynchronous completion barrier.
The first caller is Zettlr's main-window close handler. Keep path-selected stores, multiple partitions, cookies read/write, whole-store purge, cache management and private WebKit interfaces outside this change.

## 2. Spec refs

- FACT: [KEL-135](kel135-persistent-profile-identity.md) owns profile selection, identifier binding, lease and lifecycle. The app cannot choose a profile path or a persistent identifier.
- FACT: [KEL-102](kel102-host-guard-enforcement.md) and [Architecture 03](../architecture/03-security.md) govern privileged operations. `crates/keld-ipc/src/guard_dispatch.rs:1` documents the single guard-before-handler entry point.
- FACT: `crates/keld-wv/src/wkwebview/macos_profile.rs:295` sets the selected WK store. Its existing clean-shutdown path at line 338 releases that store. Selective clear needs a separate live operation and shutdown barrier.
- FACT: [#553](https://github.com/gyldlab/keld/issues/553) owns the shared facade export. [#554](https://github.com/gyldlab/keld/issues/554) owns conformance. #471 owns custom-scheme registration and #1358 owns the Zettlr proof.
- FACT: Electron v44.4.5 [partition and default-session docs][sessions] bind the empty partition to the default. Its [storage method][storage] accepts storages and a single origin. [clearData][data] uses separate names and filtering rules.
- FACT: Electron clearData documents registrable-domain deletion for cookies when filtering by origins. Other storage matching can depend on top-level site, origin and third-party context.
- FACT: The [public WK record][record] groups website data by domain using the public suffix list. The [store API][store] removes selected types for records or after a date.
- FACT: [WebKit's domain calculation][wkfilter] groups non-file origins via the top privately controlled domain. A record's displayName is a presentation string.
- FACT: [Zettlr v4.9.0][zet] requests cookies, localstorage and shadercache during close. It attaches a rejection handler. It does not await completion before returning from its close listener.
- FACT: Primary sources above were retrieved on 2026-11-06. Electron is the API oracle. Zettlr is the demand pin. Neither is a KELD execution receipt.

[sessions]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/session.md#L26-L70
[storage]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/session.md#L734-L746
[data]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/session.md#L1753-L1789
[record]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/Cocoa/WKWebsiteDataRecord.h#L44-L86
[store]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/Cocoa/WKWebsiteDataStore.h#L62-L83
[wkfilter]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/WebsiteData/WebsiteDataRecord.cpp#L45-L79
[zet]: https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/windows/create-main-window.ts#L117-L130

## 3. Acceptance criteria

### AC-1: One wrapper per admitted app generation

- **Given:** Two live default webContents backed by one Host profile and a current Ready link.
- **When:** Read both session properties, session.defaultSession and session.fromPartition("").
- **Then:** All four references compare strictly equal. isPersistent() returns the verified backend state. Dev returns false.
- **Negative control:**
  - **Single fault:** Allocate a wrapper for each webContents getter.
  - **At-risk precondition:** All getters return objects that target the same store.
  - **Expected RED:** Identity comparison fails despite equivalent data access.

### AC-2: Refuse alternate stores and paths

- **Given:** One admitted default session.
- **When:** Request a nonempty partition, a window-specific partition/session selector or a storage-path getter.
- **Then:** Return the existing typed unsupported diagnostic at the proper synchronous boundary. No additional store, path disclosure or partial window occurs.
- **Negative control:**
  - **Single fault:** Silently map a nonempty persistent partition to the default.
  - **At-risk precondition:** That partition name is valid in Electron.
  - **Expected RED:** The request succeeds and aliases data which the app expected to isolate.

### AC-3: Authorize once before native mutation

- **Given:** Two apps have distinct authenticated profile bindings and seeded cookies/local storage.
- **When:** The first app requests a permitted clear. Repeat with the grant removed, a forged role and a stale link.
- **Then:** Only the admitted request reaches the first store. For each refused request, verify zero native deletion and preserved data.
- **Negative control:**
  - **Single fault:** Bypass dispatch_privileged in the backend call path.
  - **At-risk precondition:** A well-shaped clear request reaches the real Host with no grant.
  - **Expected RED:** A native deletion starts or a sentinel disappears before authorization.

### AC-4: Clear precisely the selected types

- **Given:** A quiescent admitted store contains cookies, local storage, unselected IndexedDB data and a custom-scheme origin where supported.
- **When:** Run Zettlr's three-name clear, then await its promise in the test harness.
- **Then:** Selected cookies and local storage disappear. Unselected IndexedDB survives. shadercache has an explicit partial-compatibility row.
- **Negative control:**
  - **Single fault:** Implement selective clear with allWebsiteDataTypes.
  - **At-risk precondition:** Seed and verify the IndexedDB canary in the same store before clearing.
  - **Expected RED:** The unrelated IndexedDB record disappears.

### AC-5: Reject an unrepresentable filter atomically

- **Given:** Two sibling subdomains share a registrable domain. Seed same-host different-port and scheme-distinct storage too.
- **When:** Request a selective origin, excluded origin or third-party matching behavior against these fixtures.
- **Then:** The first version rejects the entire filtered request before invoking a destructive API. All target and sibling sentinels survive.
- **Negative control:**
  - **Single fault:** Match a WK displayName and remove its data record instead.
  - **At-risk precondition:** Two distinguishable origins occupy one WK record for the selected type.
  - **Expected RED:** A sibling disappears or the deletion callback fires for the refused request.

### AC-6: Await native completion through close and restart

- **Given:** A clear has reached its WK store while the caller closes the last window or loses its link.
- **When:** Hold completion, request clean shutdown, then complete the original callback. Repeat with the inverse order.
- **Then:** No early success or lease release. Complete at most once on the initiating live link. A replacement generation receives no stale reply.
- **Negative control:**
  - **Single fault:** Release the retained store and lease on window close before the native callback.
  - **At-risk precondition:** The callback is outstanding and a competing operation requests the same profile.
  - **Expected RED:** A second owner proceeds or the old result reaches a new generation before the operation retires.

### AC-7: Establish persistence independently

- **Given:** A qualified persistent identified store, new unique markers, an unchanged canary and an equivalent no-clear control.
- **When:** Verify seeded values, run the accepted clear, close cleanly and relaunch using the identical validated app identity and store.
- **Then:** Cleared markers stay absent, the canary persists, and matching markers survive in the no-clear control.
- **Negative control:**
  - **Single fault:** Relaunch with a new empty ephemeral store.
  - **At-risk precondition:** Both runs would otherwise show the selected data as absent.
  - **Expected RED:** The store identity or surviving canary fails. Dev relaunch cannot earn this result.

## 4. Design

### D-1: Resolve identity once

Use the #553 facade object and configured link. Each Bun generation owns its own JS wrapper over the authenticated Host profile. Multiple windows share that wrapper.
Expose the admitted metadata only after #446 Ready under the #554 conformance contract. Refuse premature use rather than selecting a fallback store.
Once the default exists, the empty selector returns that existing instance. Check the pinned oracle before accepting options on an existing empty partition.
Reject nonempty partitions, fromPath and unsupported per-window selectors before construction. Do not return a false path, leak an on-disk UUID or rely on release mode as a persistence test.
The caller has no field for store identifier, raw path, platform handle or another app principal. Both clear methods return Promise<void>.

### D-2: Define the v1 supported request shapes

| Method and arguments | Host/backend behavior |
|---|---|
| clearStorageData with explicit nonempty storages containing cookies or localstorage | Select the canonical WK cookies/localStorage type set. Accept duplicates once. No origin option is allowed in v1. |
| That set also includes shadercache | Accept the documented partial case. Report shadercache as unsupported on WK. Delete only selected real types. No cache substitution. |
| shadercache alone, unknown type, security-sensitive extra field, omitted list, empty list or omitted options | Reject with a typed subset explanation before any native mutation. Omitted options request a wider Electron contract. |
| clearData with explicit nonempty dataTypes containing only cookies and/or localStorage | Map its correctly cased names to the same authorized type set. The default unfiltered behavior applies to these explicit types only. |
| clearData origins, excludeOrigins, originMatchingMode or avoidClosingConnections option | Reject before mutation in this tranche, including unsupported combinations with otherwise supported types. Record the unsupported option. |
| Unselected WK data, unrelated persistent app, private data-store handle and physical paths | Never touch them through either method. |

Treat any type outside each method's allowlist as unsupported, except the explicitly documented shadercache combination. Reject the complete request. Never process a supported prefix before validating all entries.
Reject all unknown fields and malformed shapes. Accept only a bounded, flat sequence of literal type names in the wire request. Normalize only canonical spellings. Do not convert potentially hostile arrays into selectors in the privileged backend.
Proposed wire input reduces validated types to a nonempty bounded set of two known bits. Host validates the set and rejects unknown bits. Keep method-specific validation at the API boundary. Allocate channel and grant keys with their respective owners.

### D-3: Keep origin and domain meanings distinct

An origin consists of scheme, host and effective port. A WK record name can cover multiple origins. Matching one visible record cannot prove exclusive access to a site's entire local storage.
Electron clearData deliberately handles cookies at registrable-domain scope with origin filters. Its other storage and third-party modes have different rules. Do not extend that cookie exception to local storage.
In v1 reject even cookie-only filtered calls. A future filtered implementation must prove cookie domain semantics, excluded domains, partitioned contexts and per-type storage precision under separately approved grants.
Exercise custom-scheme origin discovery and include custom-scheme stored data in unfiltered removal tests. Opaque or unobservable origin handling remains a recorded gap. Never guess a domain from a URL string.
Use no private WebKit APIs, same-origin JavaScript injection, cookie enumeration exposed to Bun, directory deletion or global WK default store.

### D-4: Own the destructive operation and its completion

Guard validates the current principal, operations and verified manifest before handler entry. Bind the permitted normalized set to this live KEL-135 store and lease. Reject stale generation, busy/stopping state and missing profile.
The UI-thread backend uses one public store call with exactly the selected types and the all-history lower time bound. Never invoke whole-store identifier removal for a selective data clear.
A native completion can prove an attempt finished. It does not authenticate a caller or prove durable absence after relaunch. AC-4 and AC-7 provide independent observations.
Keep one outstanding clear per profile initially. Refuse a concurrent request with the owner-approved busy error. Never coalesce requests across generations or capability sets.
Serialize admission against the stopping transition. If stop wins, refuse before submission. If clear wins, keep the profile/store alive until completion. Do not block the UI thread on the callback.
A closed window is independent of the profile lifetime. The close handler returns without awaiting Zettlr's promise. Continue service for the admitted operation while retiring the corresponding caller if needed.
After callback, release operation ownership once. Deliver only to the matching active link. Late or duplicate completion cannot unlock another operation.
Before submission, failure proves deletion did not start. After submission, link loss, crash or deadline may leave the outcome unknown. Keep that diagnostic distinct. Never automatically retry an uncertain deletion.
Bound completion waits by an approved Host operation deadline. If it expires, quarantine/retain the in-flight ownership until an actual callback or verified process teardown. Never hand the same store to a successor on a timeout alone.
Use KEL-135's recovery policy if the Host dies. A client timeout cannot mark the store idle, certify successful clearing or start a filesystem repair.
Do not log cookie values, origins, configuration paths or website data. Log bounded operation kind, requested type set, owner-approved correlation and result class only.

## 5. Boundaries

#474 owns the API subset and facade delegation. #553 owns one exported Session object. KEL-135 owns the live selected store and async lease barrier. KEL-102 and schema owners govern the destructive grant.
Do not modify the frozen permission snapshot, allocate an implementation-local wire constant, or turn a role into a privileged data-store reader. Windows and Linux remain separate unqualified lanes.
No rewrite of profile admission, no partition creation and no silent widening of a clear. The architecture of Host custody is retained.

## 6. Tasks

### T-1: Freeze the oracle and owner amendments

- **Blocked by:** none
- After exact-content approval, record #554 references and red cases. Confirm #553 object identity. Obtain KEL-135 selective-operation/stop-barrier and generated grant/channel review.
- Establish independent selected and unselected seeded records, including the custom scheme and persistent same-profile relaunch.

### T-2: Deliver identity and authenticated selective clear

- **Blocked by:** T-1, B-1
- Implement AC-1 through AC-6 as one vertical sequence. Prove direct forged Host calls fail after bypassing JavaScript validation.
- Report supported and refused argument shapes per method. Keep a late callback from satisfying a successor request.

### T-3: Qualify Zettlr and installed persistence

- **Blocked by:** T-2, B-2
- Invoke the v4.9.0 close path. Exercise normal close and cancellation; check competing close handlers still run. Verify AC-4, AC-6 and AC-7 on the named macOS build.
- Publish the partial shadercache row, refused origin cases and per-platform evidence. Keep dev and persistent receipts separate.

## 7. Test plan & evidence

Run identity and negative-intake tests with multiple windows, invalid partition options and a reused numeric link identifier. Verify denial counters at the real native boundary.
Use separate real profiles for control and target runs. Seed each marker before clear. Suspend page writers while observing removal. A concurrent writer may legitimately recreate data after completion.
Verify cookies and local storage independently on the same live store. Verify custom-scheme storage if that backend supports it. Check canaries in an unrelated data type and application.
Seed sibling subdomains, distinct ports, different schemes and third-party data. Positive or negative tests must assert both the requested target and protected neighbor. Seeded controls must survive when clearing is disabled.
Exercise stop-before-admission, admission-before-stop, disconnect-while-pending, duplicate callbacks, deadline and callback after a new generation begins. A simulated callback qualifies ordering alone.
[Zettlr #1354](https://github.com/gyldlab/keld/issues/1354) D-Z8/D-Z9 use a fresh profile for the product workflow. Record a non-vetoing close independently from verified deletion.
A new empty profile can pass that workflow even if a clear implementation is a no-op. Require positive seeded before/after evidence for AC-4 and same-store persistence for AC-7.
No profile path or cookie contents may cross the wire. Observe physical persistence through a real controlled page or trusted native test after relaunch.

### E-1: Source and demand verification

- **Run:** Compare the immutable Electron API, WebKit grouping and store API, KEL-135 lifecycle and Zettlr main-window factory cited above.
- **n:** One Electron oracle, one pinned Zettlr caller and one WebKit API source revision.
- **Environment:** GitHub-only documentation review.
- **Limits:** Actual website-data clearing, grant enforcement, custom-scheme support and relaunch persistence on macOS are unrun.

## 8. Review gates triggered

Public API and permission model. Generated wire changes and exact native unsafe operations require independent review where introduced. No new dependency is requested.

## 9. Perf impact

No new steady-state polling. Bound in-flight work and the accepted bit set. Record clear latency and shutdown overlap with realistic storage. No performance target or measured improvement is claimed.

## 10. Open questions, blockers & unknowns

### B-1: Profile operation and guard registration

- **Status:** open
- **Owner:** KEL-135, KEL-102, #553/#554 and the generated schema/channel owners.
- **Impact:** No destructive implementation may run until exact privilege, busy semantics, bounded deadline, active-store lifetime and completion ownership are accepted.
- **Check:** Attach approved owner artifacts and one guard-denied pre-handler test. Verify no second Session facade or store selector exists.

### B-2: Real data-store qualification

- **Status:** open
- **Owner:** #474 macOS operator with #471 custom-scheme and KEL-135 profile owners.
- **Impact:** Persistent, custom-scheme and Zettlr workflow rows await independent native evidence. Filtered deletion remains deliberately unsupported in v1.
- **Check:** Qualify the named identified store, selected types, surviving canaries, active callback barrier and unchanged app identity on relaunch.
