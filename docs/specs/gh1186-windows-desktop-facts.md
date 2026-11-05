# Spec: Windows desktop facts through the shared mirror
Status: draft
Linear: GH-1186 (#517 tracker exception) · Owner: 0monish / GH481 and GH446 shared owners · Updated: 2026-11-05

## 1. Goal & non-goals

Qualify native Windows display identity, bounds, work area, scale and effective application appearance for the existing desktop snapshot mirror. A selected application must receive its first qualified snapshot before its accepted window Subscribe permits facade Ready. Later observations and a recovered primary role must use the same shared snapshot, writer and generation fences. Synchronous screen/theme reads must be local. The executable fixture in this change supplies native feasibility and contract evidence; it does not activate the production Windows facade.

This is a Windows-specific amendment proposal to the canonical GH481 contract, not a replacement desktop specification. Shared rules below are references to the existing owner decisions, not new authority. The [bounded S1 decision](https://github.com/gyldlab/keld/issues/481#issuecomment-6556306946) defines the shared snapshot and pending-subscription behavior. Its [selection follow-up](https://github.com/gyldlab/keld/issues/481#issuecomment-6557264649) approves only a macOS development path. Windows selection, identity interpretation, projection and post-Init admission still require owner approval. Status remains draft until those exact decisions receive review.

Explicit non-goals:

- No second desktop mirror, RPC per getter, new transport channel or new Ready/security rule.
- No concurrent edit to the W2 lead's native loop, Core coordinator, Host lifecycle or release paths.
- No change to global display, DPI, text-size, theme or accessibility settings for testing.
- No claim of mixed-DPI multi-monitor layout, forced-colors conversion, live display/theme notification delivery, stable cross-reboot IDs or Electron display-ID equality.
- No main merge, auto-merge, capability enablement, renderer source carrier or managed BrowserWindow integration.

## 2. Spec refs

| Authority | Governing contract |
| --- | --- |
| [Architecture overview](../architecture/01-overview.md), [IPC](../architecture/02-ipc.md), [security](../architecture/03-security.md) | Existing process ownership, authenticated channels, default-deny admission and bounded messages. |
| [Electron compatibility](../architecture/04-electron-compat.md), [native webview](../architecture/05-webview-and-native.md) | Compatibility requires measured native behavior and one native owner. |
| [GH531 window registry](gh531-window-registry-close-state-machine.md) | One window subscription, ordered writer, current role/generation and survivor replay. |
| [GH1188 Windows adapter](gh1188-windows-window-adapter.md) | Lead-owned real Windows Init and lifecycle. This proposal does not supersede its prerequisites. |
| [GH481 S1 approval](https://github.com/gyldlab/keld/issues/481#issuecomment-6556306946) | Tag 7 complete optional desktop snapshot, bounded cache, pending Subscribe, typed unavailable, fixed deadlines. |
| [GH481 follow-up](https://github.com/gyldlab/keld/issues/481#issuecomment-6557264649) | At most one preceding desktop event on the existing Init deadline; macOS-only development selection. |
| [GH446 Ready approval](https://github.com/gyldlab/keld/issues/446#issuecomment-6371650353) | Facade Ready requires raw Ready, applied replay and accepted correlated Subscribe. |

Inspected source bases are main `df99602ef10abd2cd6f3b5c633ba6d1cffee97bc`, immutable S1 `e09b520eaee1e4bd69bd0bde2137ec52f0fed18a`, and tested W2 PR1403 `fe9a4ab8d69823c8ead58281ab1715a811a75835`. Earlier PR1397/PR1400 applicability checks are historical. Current-head equivalence is recorded below and never implies complete integration or compilation.

S1 exists in the Mac composition and is absent from current main and PR1397. The exact eleven shared fixture inputs are pinned by Git blob in [shared-pin.json](../../tools/windows_desktop_facts/shared-pin.json). They are exported without edits for testing, not committed as a second implementation. The shared owner must publish a portable dependency. Do not cherry-pick the 50-file S1 composition wholesale.

## 3. Acceptance criteria (binary, each becomes a test)

The table separates executable feasibility from production acceptance that this draft cannot authorize.

| ID | Given / when / then | Evidence boundary |
| --- | --- | --- |
| F1 | Given Ramani's unchanged desktop, when the isolated native probe runs with exact per-monitor-v2 process awareness, then it returns the actual DisplayConfig target set, physical monitor/work rectangles, effective DPI, high-contrast state and application/system theme observations; every reported query failure stays visible and cannot become usable complete DesktopFacts. | Actual Windows APIs; process owns no window or persistent native handle. |
| F2 | Given one primary display at origin and no forced colors, when the actual WinRT text-scale observation is combined with native DPI, then the adapter produces enclosing logical rectangles, a positive effective scale and the system application dark/light choice. Independent physical mode/SPI and an Electron oracle must agree on the current measured geometry/theme. | Actual native observation plus a separately identified Electron binary; qualified configuration only. |
| F3 | Given invalid monitor, invalid DPI enum or malformed high-contrast query size, when the same native wrappers execute, then a real API failure is retained with a nonzero exit and no sample. Given an unaware process at non-unit DPI, physical-unit validation refuses it. | Actual native negative controls; no fabricated HRESULT or setting mutation. |
| F4 | Given distinct complete native target sets, when records reorder, disconnect or a Bun role recovers, then a retained Host-session identity maps to the same nonzero u32, another identity maps differently, and the 33rd lifetime identity is refused without eviction or partial allocation. | Deterministic identity controls, explicitly synthetic; raw live target tuple is separately retained. |
| F5 | Given the real S1 LifecycleLink/WorkerLink on a Windows named pipe, when a valid Subscribe is observed before any native sample, then it remains pending; desktop replay is applied before correlated Subscribed permits resolution. The native-selected test invokes the first sampler only after the pending assertion. | Real client/parser/mirror and pipe; scripted test Host. Does not establish Windows Core admission. |
| F6 | Given an accepted subscription, when newer complete snapshots or invalidation arrive, then the original shared source applies them in order, local reads add no Host CALL frames, equal/equal snapshots are idempotent and contradictory/regressing epochs fail closed. | Exact shared source plus real client pipe; no maintained replacement mirror. |
| F7 | Given role retirement and a new primary, when the survivor/latest replay is sent to the new link, then the new role sees the latest desktop; the retired source refuses further application. Core stale-generation completion refusal is a separate production gate. | Retired client mirror and fresh link; Core generation fencing remains P2/P4. |
| F8 | Given no desktop completion, when the existing 5,000-ms Subscribe caller deadline elapses, then the actual client rejects and retains its rejected state through retirement. A failure-first mutation must make this test fail on its semantic assertion. | Existing client timeout. Does not establish the Core 15-second watchdog. |
| P1 | Given an approved selected Windows Host, when genuine first Tao Init runs, then one native capture and DesktopObserved precede HostInitialized; unselected paths perform no capture. Native failure enters the shared unavailable path. | Requires approved selector and owner-applied seam; not implemented by this fixture. |
| P2 | Given the authenticated current primary in the real Windows Core, when Subscribe arrives before the first sample, then at most one pending correlation is retained, no subscriber/replay exists while pending, and completion writes desktop → survivor windows → correlated Subscribed atomically under the existing writer. Failure returns KELD-CORE-049 with zero replay. | Requires portable S1 router/coordinator composition and actual Core tests. |
| P3 | Given accepted Init, when a genuine subsequent desktop refresh arrives, then the existing router advances or invalidates the snapshot without republishing Ready or resetting its epoch/deadlines. | Current S1 coordinator rejects this path; an explicit reviewed amendment is required. |
| P4 | Given real coordinator timing/retirement, when desktop observations duplicate or arrive after expiry, then they cannot postpone the 15-second Init watchdog, reauthorize a stale role or publish Ready. | Owner-controlled integration, including duplicate-correlation and late-native completion tests. |
| P5 | Given the production facade after recovered replay, when screen/theme getters run and both real window-only consumers receive a desktop event, then reads stay local and consumers do not dereference a nonexistent window. | Owner composition of existing S1 consumer fixes and Windows facade selection. |

F1–F8 are the fixture acceptance scope. P1–P5 are mandatory production gates, not unchecked details that this PR may waive. Executed counts, failures and hashes belong in the evidence report and PR, not inferred from these criteria.

## 4. Design

### Ownership, trust and reuse

The Host/UI owner obtains native facts. Core owns the admitted primary role, latest desktop epoch/snapshot, window registry and one ordered writer. Bun owns the existing S1 WindowSource view. Native IDs are references to observations, not identities for authorization. A test Host may exercise the real client but cannot prove Core's role admission.

| Option | Decision and unmet requirement |
| --- | --- |
| Reuse S1 WindowSource, codec, LifecycleLink and WorkerLink | Selected. These already define snapshot validation, local reads, ordering and retirement. Exact original bytes are required by the fixture. |
| Copy S1 into a new W4 mirror or add screen/theme getter calls | Rejected. This would introduce another state owner or change the synchronous local-read contract. |
| Import the complete Mac composition | Rejected. It would overwrite unrelated Host, Ready, renderer and lifecycle work owned by other agents. |
| Use HMONITOR, GDI name, array index or a constant singleton ID | Rejected. None proves identity across a reordered/recovered observation. |
| Truncate/hash the native identity to u32 without history | Rejected. It cannot refuse collisions with disconnected identities. A keyed hash with historical full keys has more machinery than the bounded map. |
| Use default DPI 1, zero rectangles, registry text-scale absence or default light mode on API failure | Rejected. These would manufacture facts from unavailable observations. |
| Independent per-monitor division for arbitrary mixed-DPI layouts | Deferred. Chromium's logical arrangement must be qualified before claiming multi-display parity. |

There is no production compatibility fallback. Existing unselected Windows behavior remains default-deny. The fixture has no performance claim and never runs in the shipping process.

### Native acquisition and units

The isolated Rust executable is a standalone workspace under `tools/windows_desktop_facts/native`. It uses the repository's existing dependency versions without changing root manifests or lockfiles. It calls Windows APIs through documented unsafe boundaries, checks their results and bounds topology races. The raw record identifies the process, OS build, requested/effective DPI context, API inputs, source identity and cleanup.

1. Select and verify the exact per-monitor-v2 DPI awareness context. No per-monitor-v1 fallback is accepted. Awareness is process-local; no global display preference changes.
2. Enumerate monitor rectangles and work areas. Match complete active DisplayConfig source/target paths, retaining clone targets and both signed/unsigned adapter LUID words. A LUID is Windows' locally unique adapter identifier; it is not a permanent hardware serial.
3. Read physical display-mode dimensions independently with EnumDisplaySettingsW and the primary work area through SystemParametersInfoW. These cross-check unit assumptions and the monitor query.
4. Read effective monitor DPI (dots per inch) and preserve its source. Device scale is DPI / 96.
5. Read actual WinRT UISettings.TextScaleFactor in Windows PowerShell 5.1. Registry absence is not substituted for this native value.
6. Read application and system theme registry results, high-contrast state and system colors. A registry failure remains unavailable. The qualified effective-dark projection uses application appearance only when forced colors are absent.

The current adapter admits one primary logical display whose physical origin is (0,0). It refuses mixed-display layouts and forced colors while retaining their raw evidence. For qualified input, effective scale is device scale × native text scale. A DIP is a device-independent pixel. Logical rectangle edges are floor(left / scale), floor(top / scale), ceil(right / scale), ceil(bottom / scale); width/height are differences of those enclosing edges. The conversion is tested against the actual oracle and negative/fractional deterministic cases.

The conservative single-primary proposal requires the physical work area to remain within its monitor before identity allocation. GH481 must approve or amend this bounded qualification rule; it is not asserted as a universal Windows guarantee.

Native failure, malformed identity, missing/contradicted appearance, nonphysical coordinates, invalid scale, impossible rectangle or unsupported projection produces unavailable. There is no synthetic desktop fallback.

### Proposed Host-session display IDs

The map key is the explicit lexicographically sorted set of serialized target tuples:

`[adapterLuid.highPart, adapterLuid.lowPart, targetId, completeTargetDevicePath.toLowerCase()]`

Each high word is a signed 32-bit integer; low word and target ID are unsigned 32-bit integers. There are 1–32 unique target tuples per logical identity. The full path is retained, not truncated to fit the u32 output. Missing, duplicated or overlong native paths are refused.

The bounded map retains at most 32 distinct identities for one Host session. After all input/rectangle validation succeeds, a new complete identity gets the next nonzero u32. Existing keys keep their IDs across removal, enumeration reorder and Bun-role recovery. No eviction or reuse is permitted. The 33rd distinct identity yields unavailable without changing the map; a later known identity can still resolve. A Host restart may start a new map. The map stores only native-key/opaque-ID associations; Core and WindowSource remain the only desktop snapshot owners.

On a single-display machine, live ID 1 is an expected allocation result, not sufficient evidence by itself. Distinct-target and history controls must detect a constant-ID implementation. Case normalization, clone-group meaning and the 32-lifetime bound are explicit review decisions.

### Existing types, channel and ordering

Reuse the existing S1 data:

```rust
DesktopSnapshot { epoch: u64, snapshot: Option<DesktopFacts> }
```

The existing nonzero u64 epoch is checked and cannot wrap. Snapshot absence invalidates the desktop. Retain tag 7, at most 32 active displays, unique nonzero u32 display IDs, exactly one primary, representable positive rectangles, finite positive scales and a complete event no larger than 32 KiB. Equal epoch/equal facts is idempotent; regression or equal-epoch contradiction is a protocol failure.

No channel, frame version, manifest capability, OS grant, public callback or permission rule is added. Production uses the same authenticated current-primary window subscription and writer. The caller deadline remains 5,000 ms and the native Init watchdog remains 15 seconds. Pending is not subscribed. Role retirement removes pending state and must make late capture inert in Core. Correlation counters restart per WorkerLink; correlation matching alone cannot distinguish an old completion sent to a successor link. The fixture does not claim that Core fence.

The first selected replay is one existing-writer batch: desktop snapshot, all surviving windows, then the correlated Subscribed acknowledgement. Facade Ready remains the already-approved conjunction of raw Ready, replay application and accepted Subscribe.

### Runtime seam and post-Init gap

The [unapplied patch](../../tools/windows_desktop_facts/integration.patch) proposes only a Host-supplied `Option<Box<dyn FnOnce() -> Option<crate::AppDesktopFacts>>>` at the existing W2 native entry. Outer None means unselected. A selected capture returning None means native unavailable. The capture runs once on genuine first Init. After capture completes, it records the timestamp before sending DesktopObserved and then HostInitialized with that recorded time. Time before capture would backdate slow work; time after the first enqueue could make timely work late. No app-supplied environment toggle selects it.

This is not a standalone compiling patch. Its prerequisite definitions and caller changes are enumerated in [integration-notes.md](../../tools/windows_desktop_facts/integration-notes.md). The lead applies it only after exact-hunk agreement and approval. It must compose with the lead's current release/lifecycle changes.

The exact S1 router accepts updates, but its coordinator rejects DesktopObserved after Init or a previous desktop observation. Its native producer emits only at Init. Production refresh therefore needs an owner-reviewed distinction between the unique pre-Init requirement and later ordinary selected observations. Later updates must not rearm the watchdog, reset epochs or republish Ready. This fixture does not install a notification observer or claim delivery of global changes.

### Migration unit and platforms

The portable dependency comprises the existing desktop types/reexports, wire codec and error code, router/pending state, ordered transport application, WindowSource and both window-only consumer guards. Selection must be captured once by the Host and carried by the canonical preload; native capture, Core requirement and facade selection must agree. W4 does not fabricate Mac launch facts or widen selectors by OS string.

The shared owner retains the permanent compatibility facade and removes any temporary composition patch after a reviewed integrated commit. Native probe/fixture stays test-only. Mac and Linux behavior is unchanged by this PR; they are not exercised as production targets here.

## 5. Boundaries

Implement the authorized executable proposal only in:

- `docs/specs/gh1186-windows-desktop-facts.md`.
- `docs/engineering/keld-error-codes.md`, only the exact already approved S1 `KELD-CORE-049` entry, under the [narrow catalog-key claim](https://github.com/gyldlab/keld/issues/1186#issuecomment-6646655855). Its original macOS S1 classification and error meaning are unchanged; registration is not Windows activation.
- `llms-full.txt`, regenerated from that catalog entry with the current repository generator under the [generated-corpus claim](https://github.com/gyldlab/keld/issues/1186#issuecomment-6646939854). The only generated delta is the same seven-line entry; `llms.txt` is unchanged.
- `tools/windows_desktop_facts/`: standalone native probe, native-to-existing-schema adapter, real-client test Host/roles, immutable shared-input manifest/export helper, receipts/report, and the unapplied integration patch.
- Owned session evidence under the common Git closeout directory.

Do not edit the lead's worktree or product `crates/keld-wv/src/webview2/mod.rs`, `crates/keld-core/src/app_session.rs`, Host lifecycle/main/release paths, root Cargo manifests/lockfile, shared mirror/transport, generated public contracts or BrowserWindow implementation. No product file imports the fixture. Read-only exact-source inspection and patch applicability checks are allowed.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 — This draft: deliver the isolated native probe, real shared-client tests, failure-first controls, independent native review, precise proposal and measured evidence. Keep it draft/open.
- [ ] T2 — Shared owners review the exact Windows identity, units, appearance, selection and post-Init decisions; publish the minimal portable S1 dependency. Record approval on GH481/GH446 and the resulting immutable source.
- [ ] T3 — W2/Core owner composes the reviewed first-Init seam and real Windows router/coordinator/consumer gates, proves P1/P2/P4/P5 and preserves release semantics on its current approved baseline.
- [ ] T4 — Shared/native owner implements and qualifies the approved refresh/invalidation source, mixed-DPI layout and forced-colors behavior before widening those supported cases. Prove P3 with actual observations without manufacturing global settings changes.

T2–T4 are dependencies, not authorization for this W4 agent to implement unapproved shared semantics.

## 7. Test plan

| Criteria | Executable / method | Required control |
| --- | --- | --- |
| F1/F3 | Standalone Rust probe `--mode pmv2`, `--mode unaware`, `--control invalid-monitor|failed-dpi|high-contrast-size`. | Actual invalid Windows API inputs must exit nonzero and omit sample. Wrong process units must be distinguishable. |
| F2 | `text_scale.ps1`, `native_adapter.ts`, a fresh independently pinned Electron oracle run. | Physical-as-logical geometry is rejected at non-unit scale; query failures never become defaults. |
| F4/F6 | `native_adapter.test.ts` and original S1 `window-desktop-source.test.ts`. | Distinct identity, changed history, 33rd lifetime identity, invalid units/theme, stale/equal contradictory epoch and retired-source mutation. |
| F5/F7/F8 | `subscribe_contract.test.ts` with real S1 LifecycleLink/WorkerLink on Windows pipes. Native case gated by `W4_RUN_NATIVE=1`. | Early acceptance, missing replay, unavailable, retired-source refusal, latest-role replay, deadline, semantic source mutations. |
| P1–P5 | Exact owner-integrated Core/native/facade tests. | Authenticated early Subscribe, unselected Windows, duplicate pre-Init observations, post-expiry capture, same-correlation duplicate, windowless consumers and generation fences. Not replaced by the scripted Host. |

All synthetic records are labelled. Native receipts retain current observations; historical machine dimensions are never expected constants. Native runs are serialized through the GH1188 queue, with bounded owned processes, a private oracle profile, before/after census and cleanup release. Code and non-GUI tests may run in parallel. The fixture never kills another agent's process.

The original shared files are hash-verified before testing. Semantic mutants use isolated owned copies and must fail assertions/protocol expectations, not fail to compile or select a missing test. Original shared hashes are checked afterward. A source/readback failure is not counted as a contract falsifier.

Record exact source HEAD, tool versions, command, exit status, test count, logs, fixture/oracle hashes and cleanup. Production-wide format/Clippy/test gates are reported separately from the isolated probe. Unrun or failed gates remain visible.

## 8. Review gates triggered

- **Unsafe:** direct Windows FFI in the standalone probe needs an independent API, lifetime and cleanup review. Product import of this code would require a new review of its UI-thread/observer lifetime.
- **Public API:** proposed Windows interpretation of existing display ID, logical units, effective appearance and unsupported cases needs GH481 review. This PR exports no production API.
- **Permission model:** no change. Existing authenticated role/default-deny behavior is retained; the fixture cannot approve Windows activation.
- **Dependency addition:** standalone fixture manifest/lock only, reusing existing pinned versions; independent review must confirm no root dependency or shipping closure change.
- **Wire protocol:** no new schema invented; use exact S1 tag 7/049. Porting that unlanded dependency into Windows requires the shared owner gate and exact codec/conformance tests.

Independent fixture review cannot substitute for the named shared owners' approval of P1–P5.

## 9. Perf impact

None on production: this PR is unimported feasibility code and documentation. Local reads are checked by counting Host CALL frames, not by claiming an unmeasured speedup. Native probe duration is evidence about the test run only. Production sampling cadence/notification coalescing needs measurement in the integrated owner work; no refresh budget is approved here.

## 10. Open questions

1. GH481: approve or amend the full target-set identity, case normalization, clone grouping and 32 retained lifetime IDs. Confirm the one-display/non-forced qualification boundary.
2. GH481: approve effective scale/text-scale and enclosing logical geometry for Windows; define mixed-DPI layout and forced-colors conversion before widening support.
3. GH446/GH1188: approve one immutable Host-owned Windows selection across native capture, Core pending Subscribe and facade Ready. Specify its canonical carrier without app-controlled environment selection.
4. GH481/GH446: approve the separate post-Init refresh/invalidation admission path without any Ready, watchdog, epoch or role-retirement change.
5. Shared owner: publish the portable S1 subset and both window-only consumer fixes at an immutable public review head. The current exact Mac-local object is a test dependency, not a consumable merged baseline.
6. W2 lead: apply/rebase the small first-Init patch only after the preceding decisions and current release baseline are qualified; run the actual Core/native P1–P5 tests.

Current-head binding at 2026-11-05 11:36:57Z: W2 PR1403 `f7b5848556154aa1c356a214270e342369267c0e` has the same `webview2/mod.rs` blob `ec493b0bc16250a19dd4cc0d8615d7870a364748` as the actual tested fe9a input. The draft v3 helper results carry forward only for that unchanged source; portable composition and P1-P5 approval remain pending.
