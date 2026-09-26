# Spec: macOS no-flag test ownership migration
Status: implementing
Linear: KEL-258 (KEL-252 macOS slice) · Owner: Keld maintainers · Updated: 2026-09-26

Human approval: KEL-258 comment `09995d8d-d4b0-4796-8122-041ce68f1f3c` approved the original draft SHA256 `18dd9ea7cf36643664d39b0d6ff2ff1135ce4678531a15dbcfa88835b1c9a668`. That immutable approval artifact is retained; this tracked copy records implementation status without changing the accepted design. Baseline: `699e27097454cbe52e4a0810d6d4daa8713326aa`. Native implementation claim: `c0f5cb9f-5991-4b7e-8d2c-f315216760f7`, refreshed for T1 by `586fb9fc-d388-4fbb-b5a0-6b34c2ec4325`. KEL-252 at `cf60141f938d24df093e7b55488ed5f77ed9d33e` remains draft. KEL-255 supplies the landed Linux migration method; the separate approval above owns this Mac scope.

## 1. Goal & non-goals

Make each existing macOS no-flag, profile and media contract navigable through its scenario and narrow fixture/observer owners while preserving the existing integration executable, native behavior, assertions, fixture bytes, exact helper selection, feature/ignore conditions and resource lifetime. This is one source-organization concern, executed in reviewable native-qualified families.

No production/API/permission/wire/dependency change, universal cross-OS harness, additional integration executable, runner/concurrency change, media-policy correction, new product claim, cleanup rewrite, or removal of ignored/manual acceptance. Keep substantial existing fixture assets shared at their existing paths. Do not extract tiny literals or replace resource-owning types to meet a size counter. Existing timing, parsing or cleanup concerns discovered while moving are separately recorded; this migration does not silently fix them.

## 2. Spec refs

KEL-96 sections 3, 4.5–4.7 and 7 govern no-flag process/window ownership, recovery and cleanup. KEL-135 sections 3–4 and 7, including the adopted 7.3 media evidence predicate, govern profile identity, isolation, purge, second-user, crash/reboot and media observations. Architecture 01 sections 1–4 and architecture 06 sections 1–2 retain production ownership. Host AGENTS owns native acceptance/unsafe limits; `.agents/test-layout.md` owns placement/migration and `.agents/testing.md` owns proof. KEL-252 draft supplies design background; KEL-255 approved/landed Linux slice supplies tested migration method only.

## 3. Acceptance criteria

1. Native macOS Cargo/libtest and nextest listings before/after each identity-changing family retain a bijection for all baseline entries, cfg and ignored reasons. The source census predicts default31 and profile-hook44 total (33 nonignored,11 ignored), including two root helpers. Native listings are authoritative; reconcile any discrepancy before editing. Preserve `no_flag_macos`, existing effective scheduling and `CENSUS_ISOLATION`; there is no Mac one-thread group in the read-pin nextest config.
2. Root `keld_dev_helper_process` and `unix_descriptor_census_fixture_process` stay exactly named. Live shipping HELLO/renderer/READY/ECHO and descriptor-fixture owned-socket/read-release effects execute after the move. A wrong exact selector must fail its caller instead of allowing a successful zero-test subprocess to count as execution. Retain the observed counterfactual and unmutated pass; keep the mutation out of canonical source.
3. Every inventoried top-level item and every impl method/associated constant has exactly one destination. Bodies, assertions, raw fixture bytes, scope/drop order, environment capture, signal/exit expectations, finite bounds and deadlines remain identical except reviewed imports, module-relative visibility and fixture paths. Verify 27 InvalidBoot and5 InvalidPolicy variants and their ALL sets/codes. A controlled assertion change must be rejected by the mechanical conservation check.
4. Each mechanical family compiles and passes its affected real macOS cases before the next extraction. Final default and profile-hook nonignored suites and the applicable manual cases retain actual-result evidence. Ignored signing, second-user, device/prompt and reboot rows remain named acceptance cells, never inferred passed from compilation or regular CI. A unavailable applicable native cell stays open with owner/command/prerequisite; it cannot be silently omitted to finish the migration.
5. Preserve product cleanup observation before fallback cleanup and keep each resource-owning struct, acquisition/finish methods and Drop together. Native CoreGraphics history remains armed before spawn, bound to authenticated host PID at its original check phase, and compared with still-live window IDs. Process/descriptor/lease evidence remains independent of the implementation under observation.
6. Update all exact scenario-name consumers in the same change: KEL-135 reproduction commands and both invocations in `tests/fixtures/run_macos_second_user_acceptance.sh`. Preserve script account/credential/cleanup behavior. Report exact file/import closures for one boot refusal, one recovered CLI lease and one media restart change. No speed/token/agent-success claim follows from source length.
7. Final format, warning-denied workspace clippy, full workspace tests, exact `just ci`, applicable generated docs checks and current-head independent review pass. The test-only unsafe move requires independent exact-final-diff evidence. Keep unrun OS/device claims explicitly unverified. Publish the Windows handoff only from actual Mac results; cross-OS sharing remains later work.

## 4. Design

No boundary change. Reuse `ProductFixture`, `LiveCycle`, `RecoveryCycle`, `RecoveryGeneration`, `ShippingDevCycle`, `ShippingLaunchCleanup`, `NativeWindowObserver`, `Beacon`, profile cleanup guards and `ProfileOrigin`; do not create a replacement supervisor or test framework. No channels/types/capabilities/manifest/wire/runtime seam changes. Compatibility fallback is the unchanged target and two root helpers; revert individual moves with their exact name/import mapping while preserving later regressions and evidence.

Atomic decision model established before choosing the layout:

| Atom/owner | Boundary and inputs → outputs | Failure / falsifier | Independence edge |
|---|---|---|---|
| Registration / root target | cfg/features + modules → native libtest/nextest names | Missing/extra/ignored/renamed helper; compiled listing diff | Module paths explicitly affect names; counts are not behavior |
| Semantic conservation / item owners | immutable source items/literals → moved items | Changed assertion/fixture/scope; token+literal comparison and injected assertion mismatch | Separate from compilation and native execution |
| Helper execution / caller scenarios | exact selector → actual handshake or owned socket | Zero selected tests; wrong-selector caller failure | Root-name preservation alone does not prove execution |
| OS observation / process, FD and window owners | real PID/group/descriptor/window → independent observations | Attribution leak, dead-table false-empty, transient window lost; existing negative/native controls | No model or aggregate count substitutes for native identity |
| Resource lifetime / each owning type | acquisition/fields/finish/Drop → cleanup and healthy follow-up | Cleanup moved before oracle or fallback erases fault; source scopes plus child/status/absence assertions | Keep struct and impl/Drop together; observation precedes fallback |
| Profile identity/authorization / signed-fixture and profile scenarios | valid signature/user/store/origin/run identity → bounded evidence | Aliased identity, wrong user/store, uncontrolled grant; existing counterexamples and real runs | Signed identity distinct from sandbox/permission authorization |
| Media provenance / media evidence modules | actual seed/restart/callback/TCC/nonce/device reports → adopted evidence predicate | No seed or retained page accepted; existing exact negative reports rejected | Query state does not prove durable grant; device class not inferred |
| Lifecycle/revocation / scenario owner | crash/quit/purge/reboot facts → release/quarantine/recovery | Fake reboot, early purge, stale generation; real observable successor/absence | Physical reboot cell remains separate from synthetic hook test |

Memory/I/O ownership is unchanged: native helpers own their own process pipes; test owners retain their sockets/listeners/threads/receivers/temp roots; the shipping host retains actual window, app link and supervised tree. No new authority or principal is minted by the layout. Failures keep existing panics/typed product codes rather than converting missing prerequisites into success.

Proposed directories follow the landed `tests/no_flag/<os>/` convention. The root declares contract modules directly using explicit path attributes and retains both helper bodies; root cfg/lint documentation remains. It does not add an extra `suite::` prefix. `profiles/mod.rs`, `media/mod.rs` and `support/mod.rs` contain declarations; the latter also owns existing common constants with their current cfg. No broad re-export or wildcard import maze.

| Domain | Proposed populated modules under `tests/no_flag/macos/` | Contract |
|---|---|---|
| Core scenarios | boot_admission, startup_rollback, lifecycle, recovery, dev_lifecycle | refusal before resources; startup rollback; ordered lifecycle; generations; CLI lease/ownership |
| Census scenarios | descriptor_attribution, descriptor_liveness | inherited/copy/anonymous/rebound ownership; descriptor-table truthfulness |
| Profile scenarios | profiles/{identity,storage,purge,cross_user,lifecycle,reboot} | validated signature; five stores/ephemeral; durable purge; second user; exclusivity/fatal/crash; actual reboot |
| Media scenarios/observations | media/{restart,query,ephemeral,restart_evidence,dev_evidence,source_evidence} | live signed restart, query-only diagnostic, fresh ephemeral behavior, pure evidence predicates/device qualification |
| Ordinary resource/fixture owners | support/{product,recovery_cycle,dev_cycle,invalid_stage,admission,renderer,control,process,unix_descriptors,lease_descriptors,native_window} | exact existing resource types/observers, no universal harness |
| Profile resource/fixture owners | support/{signed_app,cross_user,profile_evidence,profile_origin,profile_renderer} | signed construction/cleanup, account operations/cleanup, report observations, bounded HTTP/session owner, exact renderer programs |

The complete 210-row item map, 76 methods and2 associated constants are in adjacent `item-map.md`/`item-census.json`; every test rename is in `test-name-map.json`. Those are temporary migration evidence, not a permanent duplicated test registry. Whole impl blocks move with their type; no method is split to hit a limit. Explicit support-to-support edges include product→native_window/control/renderer/process, recovery_cycle/dev_cycle→product/control/process/lease observers, and profile_origin→signed_app/profile_evidence/profile_renderer. Scenario-only assertions do not move into setup builders. Cleanup helpers and signed observation may reference each other through narrow named functions; retain and review those real edges rather than introducing forwarding abstractions.

Retain existing substantial embedded C read-fault and Swift absence observers with their compile-owning types in `support/admission.rs` for the first move; preserve exact raw literals. Keep profile JS/HTML-generating functions together in `support/profile_renderer.rs`, with formatting interpolation unchanged. Existing `t1b_harness.ts` and `native_window_census.swift` stay at their original fixture paths; fix includes explicitly to those same blobs.

Size exception proposed for `support/profile_origin.rs`: owner KEL-258/macOS test maintainer, approximately401 current item lines before imports/attrs; one listener/address/pending-report owner with its actual run/HTTP methods, so splitting its impl would obscure lifetime. Retain for this mechanical migration; re-review only when a future HTTP/session behavior change demonstrates an independent resource responsibility, tracked on KEL-258. Other files crossing400lines/16KiB after formatting get a similarly explicit responsibility review; do not silently split or drop an oracle. Entry root over100lines/4KiB is reviewed likewise, with two required helpers retained.

## 5. Boundaries

Implement only `crates/keld-host/tests/no_flag_macos.rs`, populated files under `crates/keld-host/tests/no_flag/macos/`, exact selector strings in the second-user runner, a small Mac navigation/readme if needed, the approved tracked version of this spec and exact-name reproduction-command updates in KEL-135. Generated included-source artifacts are regenerated by their owner/checks, never hand-edited.

Do not change production crates, existing fixture program bytes, Linux/Windows tests, Cargo manifests/lock, nextest/CI scheduling, agent instruction owners, profile/media semantics, or account/reboot automation. Existing retained source references to the root target remain valid; do not rewrite historical acceptance SHA/names as fresh results.

## 6. Tasks

- [x] T0: Mac scope approved; main/open work and winning claim refreshed; native Mac Mini, macOS 26.5.1 (25F80), arm64 and desktop execution qualified. Cargo/libtest and nextest baseline inventories contain 31 default cases and 44 profile-hook cases, including 11 ignored cases. Baseline and evidence are bound below. Availability/authority for all 11 operator cells remains UNKNOWN and their execution remains NOT RUN; setup admission does not satisfy those later acceptance cells.
- [x] T1: Extracted 50 mapped native process/descriptor/window items, including 13 controls, with both helper root names preserved. All 16 affected native cases pass in default and profile-hook modes. Cargo/libtest and nextest inventories preserve the complete 31/44-case bijection, 11 ignore states and runner groups. Both copied missing-selector controls prove zero selected tests can exit 0 while the parent caller rejects the missing live effect. T6 independent unsafe/layout review and final gates remain open; this family acceptance does not complete the migration.
- [x] T2: Extracted 22 mapped boot/invalid-stage/admission items, including seven native tests. Default and profile-hook runs each passed 7/7; all 27 invalid-boot and five invalid-policy cases, embedded C/Swift literals, suspended-shell/read-fault setup and no-transient-resource checks were preserved. Root accepted the exact family in KEL-258 comment `6ded20a2-a659-4529-865b-3c3677e81176`; T6 final gates remain open.
- [x] T3: Extracted 39 mapped product/lifecycle/recovery/CLI items and nine scenarios with unchanged scopes and complete resource owners. All 31 non-profile consumers pass in default and profile-hook modes. Current post-move dev-helper copied caller passes; the absent selector selects zero tests/exit 0 while the mutated caller exits 101 for the missing Bun control connection. Root independently accepted T3 in KEL-258 comment `32e076b3-7ada-417b-b865-e2924510f419`; T6 final gates remain open.
- [ ] T4: Extract profile fixtures and identity/storage/purge/lifecycle/cross-user/reboot families one qualified family at a time. Keep manual rows/cleanup and exact-name consumers paired with their move. Stop at any unqualified applicable native family, recording its owner/action.
- [ ] T5: Extract media scenarios/evidence/rendering owners, preserving adopted predicates and real device/prompt controls; native pass. Record read closures and final conservation/discovery mapping.
- [ ] T6: Final full gate, independent unsafe/layout/current-head reviews, actual native/manual results and Windows handoff. No merge while required acceptance is unrun.


### Recorded implementation evidence and remaining acceptance

The native discovery archive SHA256 is `09d842db723901728868ee8811797e0dd690bd72a14d6a2759be8862b7629620`. The T1 archive SHA256 is `ab1cd8a88da43382661990b4171c8ec68c243a4015d4e23ba6bafa7aee3609e4`; KEL-258 handoff comment `71d2e2f9-a88f-4316-ba8b-82e1e8f66ca3` binds its retained command/output, source and fixture hashes. All 210 source items, including the 50 moved items, and four fixture inputs reconcile. An injected assertion change is rejected by the conservation check. The unchanged copied descriptor and dev-helper callers pass; each deliberately missing selector returns zero tests/exit 0 alone, while its caller exits 101 for the absent listener or Bun control connection. Canonical source was not mutated for those controls.

The accepted T2 archive SHA256 is `52e216135e9dbb74a08b0c35d73d258e1b74cbef28ce254d78045faad9e423b4`, bound by KEL-258 handoff `d899dfc0-cadb-445d-86a3-0a94457af6d0` and independent acceptance `6ded20a2-a659-4529-865b-3c3677e81176`. All 210 source items (72 moved through T2), four fixture inputs and four T2 raw/byte-raw literals reconcile; the pre-Ready attempt-count mutation is rejected. Both native seven-case runs and the 31/44-case name/group inventories pass.

T3 native family execution and root's independent review are complete: 39 mapped items/nine scenarios moved with their complete resource owners; all 210 item bodies (111 moved cumulatively) and four fixture inputs reconcile. The 31 non-profile consumers pass in both default and profile-hook modes, including all helper callers; the 13 profile/media cases were filtered out in the hook run. Managed native receipt: `run-e036afee6e294c14b9fef858caa9107c`.

T3 cohesion exception: `support/dev_cycle.rs` is 403 formatted lines / 15,942 bytes, including 38 import lines. Owner: `KEL-258/macOS test maintainer`; tracking issue: KEL-258. Keep `prepare_keld_dev_helper`, `ShippingDevCycle`, and complete `ShippingLaunchCleanup` acquisition/release/Drop owners together. Re-review on the next dev-cycle/lifetime behavior change. This retains the existing ownership boundary without a counter-driven split; other T3 modules remain below the review thresholds.

The accepted T3 archive SHA256 is `a00708ad3c0b0b9cca3bc986aba3fba3b899902685036b87b05de62af35c279e` (167 payload hashes verified), including the post-move helper falsifier receipt `run-0c46290e344e4bcbb173622d21b6c782`. Canonical source stayed unchanged during the disposable-copy control. T4a identity/storage structural work is accepted by root in KEL-258 comment `29aeaa17-da4a-4ff6-b2a5-4584bab681c0`: 38 mapped rows/three scenarios and the complete signed-app/profile-origin/renderer/evidence owners moved. All 210 item bodies (149 moved cumulatively) and four fixture inputs reconcile; the fresh-launch service-worker false-to-true assertion mutation is rejected. Cargo/libtest and nextest retain 31 default / 44 profile-hook cases, 11 ignored states and runner groups. The three KEL-135 reproduction selectors follow the mapped names; the second-user script remains byte-identical.

Only `profiles::storage::kel135_macos_dev_profiles_are_ephemeral_across_launches` was executed in T4a: 1/1 passed under profile hooks, with seeded localStorage/cookie/IndexedDB/CacheStorage/service-worker state followed by an empty fresh launch, `store_identifier=none` and `persistent=false`. Managed receipt: `run-2ed5f9873a7a49688dd28961eb4c59bb`. Signed identity/storage and every other manual cell remain UNKNOWN / NOT RUN. No signing, account/sudo, device, permission or reboot action occurred; T4 remains unchecked and incomplete.

The approved `support/profile_origin.rs` cohesion exception now measures 431 formatted lines / 16,473 bytes. Owner: `KEL-258/macOS test maintainer`; tracking issue: KEL-258. Its listener/address/pending-report resource and complete HTTP/session impl remain together. Re-review when a future HTTP/session behavior change demonstrates an independent resource responsibility; no counter-driven split. The existing `run_as_local_user_command` dependency remains at its owning root location until the cross-user family moves.

T4b purge/profile-lifecycle structural work is accepted by root in KEL-258 comment `1532fb6e-09dd-42cc-825c-d924ea9e5e83`: five mapped rows comprise four ignored tests and the existing purge-phase helper. All 210 bodies (154 moved cumulatively), fixture bytes, ignore reasons and scopes are preserved; an injected binding-interruption exit-code change is rejected. Default/profile Cargo/libtest and nextest discovery preserve 31/44 cases, all 11 ignored states, helper/binary identities and runner groups. The two existing KEL-135 reproduction commands for moved concurrency/binding cases use the mapped lifecycle names; no purge/fatal command was invented.

All four T4b tests remain NOT RUN. No nonignored oracle directly depends on the five moved rows, so no runtime test was selected in this compile/list-only slice. Compilation and discovery do not establish purge/lifecycle acceptance. The initial pre-edit consumer hypothesis was corrected by census: the private purge-phase helper is used only by its purge test, not by signed storage; no storage import or owner was changed.

T4c cross-user/reboot source work is structurally accepted by root in KEL-258 comment `3b7040f1-847c-4d97-8b97-415467814b3b`: 15 mapped rows/two ignored cases and their complete account cleanup owners moved; all 210 item bodies (169 moved cumulatively), fixture bytes, ignore reasons and lifecycle scopes are preserved. Only the two test-name arguments in the second-user runner and three KEL-135 reproduction references changed; account identity, authentication, credentials, cleanup and reboot-phase inputs remain identical. Neither test body nor the script/account/sudo/reboot actions ran.

T4c default/profile Cargo/libtest and nextest discovery now preserve 31/44 cases, all 11 ignored states, helper/binary identities and runner groups. The original capacity guard stopped with exit 77 (`run-75b340318d5c457ab437a403b768dc76`); after user-authorized coordinator cleanup of only KEL-260/KEL-264 generated caches, the exact remaining compile/list commands passed (`run-12c40754baa64d28a41a348efd7fe9d8`). The active target, source, scratch and earlier evidence were preserved; no test body or manual action was selected on resume.

T5 structural extraction and pure-oracle execution await independent review: 32 media items plus seven mapped common constants moved, completing 208 relocated items and the two unchanged root helpers. All 210 bodies and four fixture inputs reconcile; the clean-restart predicate mutation is rejected. Each common constant has one `support/mod.rs` owner with unchanged cfg/value tokens and direct imports. The root is 69 lines / 2,856 bytes. Cargo/libtest and nextest retain 31/44 cases, 11 ignored states, helpers/binary and runner groups under four intended media renames.

Only `media::restart_evidence::kel135_macos_media_restart_oracle_rejects_missing_boundaries` executed in T5: 1/1 passed, 43 filtered, with no device/signing/capture/permission operation. Receipt: `run-eca58db16a4b437a88702c5b32649d00`. The adopted `public-grant-restart-v1` predicate, query-only/missing-seed/wrong-store/retained-page/Allow-response counterexamples and TCC/device provenance requirements remain unchanged. The three ignored media cases and all 11 manual cells remain UNKNOWN / NOT RUN; this pure pass does not establish real-media acceptance.

T5 cohesion exception: `media/restart.rs` is 389 formatted lines / 16,416 bytes. Owner: `KEL-258/macOS test maintainer`; tracking issue: KEL-258. Retain the single complete signed-restart scenario so seed/restart/deny/allow/counterexample ordering stays visible. Re-review at the next media scenario or evidence-predicate change. No counter-driven split. The existing dev-cycle/ProfileOrigin exception sizes above include their direct common-constant import paths; owners/reasons/review conditions are unchanged.

T5 independent structural review, T4–T5 native/manual acceptance and final gates remain pending. Overall T4 stays unchecked; all operator cells remain UNKNOWN / NOT RUN. T6 independent exact-diff unsafe/layout/current-head review, full workspace clippy/tests, exact `just ci`, applicable generated-doc checks, final native/manual results and the actual-result Windows handoff remain open. No full migration or merge readiness is claimed.

All 11 operator cells retain their existing prerequisites and assertions. Their current qualification and execution status is:

| Operator acceptance cell | Availability/authority | Execution |
|---|---|---|
| Signed package identity | UNKNOWN | NOT RUN |
| Signed cross-launch storage | UNKNOWN | NOT RUN |
| Fsynced purge recovery | UNKNOWN | NOT RUN |
| Second-user isolation | UNKNOWN | NOT RUN |
| Fatal-command cleanup | UNKNOWN | NOT RUN |
| Concurrent-owner refusal | UNKNOWN | NOT RUN |
| Persistent-media restart | UNKNOWN | NOT RUN |
| Query-only media | UNKNOWN | NOT RUN |
| Dev-media fresh launch | UNKNOWN | NOT RUN |
| Binding crash recovery | UNKNOWN | NOT RUN |
| Real two-phase reboot | UNKNOWN | NOT RUN |

The native owner must establish the applicable signing/device/user/reboot prerequisites and existing operator authority before those actions. Design approval itself does not authorize account creation, permission prompts or a physical reboot. An unavailable applicable cell remains open under the accepted family gates.

## 7. Test plan

AC1: On native Mac record `cargo test -p keld-host --test no_flag_macos -- --list --format terse` and the `--features profile-test-hooks` variant; capture nextest JSON and effective configuration in both modes with the installed runner. Compare exact test sets/ignore reasons, not totals alone. Keep target architecture explicit when used by the existing operator commands. Source census is31/44, not a claim of native listings.

AC2: Run the existing three dev-lifecycle scenario contracts (including recovered lease and lease-byte behavior) and all four spawned-descriptor caller cases. Record real HELLO/renderer/READY/ECHO and listener/read-release effects. Mutate one root-selector use per helper family in a disposable source-equivalent checkout, prove its caller fails, restore and re-run the caller. Observe the absent-selector child's zero-test status explicitly. No change to canonical guard logic is proposed merely to copy Linux's prevention code.

AC3/5: Compare every moved item/literal/field list and nested methods; verify exact hashes of both existing include fixtures and canonical TS inputs. Preserve EVENT_DEADLINE15s, PROCESS_DEADLINE5s and feature-gated MEDIA_PROMPT_DEADLINE2min, plus method-local read/HTTP bounds. Preserve CENSUS_ISOLATION mutex (libtest process-local) and existing nextest behavior. Run process-group safety, observer unwind/EOF, descriptor attribution/dead-table/malformed-lsof and media-oracle negative controls. Existing polling behavior is conserved and not offered as a new recommended pattern.

AC4/6: Run each affected native family, then default and feature-hook nonignored suites. Explicitly selected ignored cells are: signed identity; signed cross-launch storage; fsynced purge recovery; second-user isolation; fatal-command cleanup; concurrent-owner refusal; persistent-media restart; query-only media; dev-media fresh launch; binding crash recovery; real two-phase reboot. Preserve exact ignore reasons in the census. Signing/media/user/reboot prerequisites are live owner qualification, not fabricated by this proposal. Reboot uses prepare/resume with retained `KELD_KEL135_REBOOT_ROOT`, rejects synthetic `KELD_PROFILE_TEST_BOOT_UUID`, and verifies independent boot facts. Second-user uses `KELD_KEL135_SECOND_USER`, actual ordinary user and authenticated operator flow. No password enters reports.

KEL-135 7.3 remains `public-grant-restart-v1`: preserve live seed/repeat, clean same-signed-run identity/store/origin/nonce continuity, authorized requesting-host TCC fact, guarded callback denial without track/sheet and live Allow control; retain actual omitted/changed/retained-page counterexamples. Query-only `prompt` does not mean persistent permission absent. Camera class/qualification (including reviewed Camo version) and microphone remain observed independently; no physical-camera or durable grant-revocation claim is added.

AC7: Execute exact final `just ci` and relevant generated-doc gates; report actual outputs. Independently review test-only unsafe `kill_process`/group cleanup and moved scopes at exact final diff. CI/Windows builds cannot replace macOS window/signing/media/user/reboot acceptance. Existing accepted historical product evidence retains original provenance; any proposed reuse must explicitly show unchanged applicable closure and does not waive newly affected family checks.

## 8. Review gates triggered

Test-only unsafe move: named independent scope/caller/SAFETY evidence required. Public API, permission model, dependency addition, wire protocol: none. No agent-instruction change proposed. Consumer docs may require generated-doc freshness; no diagrams proposed.

## 9. Perf impact

None claimed. Same integration executable and scheduling. Source/read locality is reported as actual file closures, never as runtime speed or model-performance evidence.

## 10. Open questions

None.
