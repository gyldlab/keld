# Spec: Windows no-flag acceptance layout — final candidate
Status: implementing
Linear: KEL-259 · Owner: GYLDLAB · Updated: 2026-09-28

Human approval: KEL259 comment `9250e584-cad5-44e0-bb99-c38981c62047` approved the exact r2 arrangement and root compatibility exception. The immutable approval bundle `kel259-proposal-revision-7fd7754-r2` remains retained: draft SHA256 `c449f51c4143f1f4f343875bed534feb7f3825f4301c44217ecc38733407920a`, manifest SHA256 `f965af0b3da0ada0b98a535720edb15fc34f6bb21ab95c41c7b6e3051804173b`. Its mapped source is `7fd7754c582993677f318b3bea5d79a4360d1a6b`, original no-flag source SHA256 `711ff46536f22a30355673a0d18d986ceebbebb5520846243305b0ae40845a5c`.

KEL271 / PR332 merged as `a7b3db742e374e524a344491de3aee0442e852c2`. Candidate and landed trees both equal `f387967a361f100b6815b25a79ccd8399f760e16`. The fresh landed native inventory `kel259-landed-native-inventory.json`, SHA256 `6b84203f2d652d21eab8465d6661b4ec32b42c7793d8a01866395c0669364f6e`, binds default and profile-test-hooks listings at 51 cases/nine ignored and the 42-case GUI group at concurrency one. KEL259 comment `e3f5e3a8-02a1-4bb2-bbb7-3ce14b8e0a9e` records this admission before structural edits.

This tracked copy records progress only; the accepted arrangement and acceptance criteria are unchanged. The named maps, checks and prerequisite receipts below are retained approval evidence, not implied sibling repository files. The ten structural families are unchanged from code source tip `8c564ecd2c96a48ef8a8579c5d081ac522a0d984`, based on `origin/main` `95994436eb74a31d2cb3a709d744249dc8dfc931`; all subsequent tracked changes are progress documentation, and the Windows test source tree is unchanged. `git range-diff` confirms each family commit and the Windows source files are byte-preserved by the rebase. Current-source signed artifacts are admitted; rows 1–7 passed, and rows 8–9 passed together on one authenticated live U2 request. U1 row 8 selected and passed exactly one test; U2 root row 9 selected and passed exactly one helper. Their shared nonce, distinct nonadmin SID, A/P1 namespace/origin, six five-store reports and both-user cleanup are retained in `rows/row8.result.json`, `rows/row9.result.json` and `source-rebind.json`. Row 8 attempts 1 and 2 failed due, respectively, missing Bun on U2 PATH and an unattended request timeout; both failures remain preserved and neither was counted. The last completed exact local `just ci` passed with 839 Rust tests passed and 27 configured skips; the status-only T3b update is being revalidated. PR #335 remains draft while final hosted checks and handoff are refreshed.

## 1. Goal & non-goals

Make existing Windows product scenarios, fixtures and independent OS observations navigable while preserving all behavior, executable/selector identities, assertions, programs, deadlines and resource lifetimes. One integration target remains `no_flag_windows`.

No production/API/permission/wire/dependency change, pending PR287 integration, shared three-OS harness, new instruction, additional integration executable, scheduling change, cleanup rewrite or performance claim. KEL271 fixes are already part of this candidate and must not be reconstructed from older c37/v75 bodies.

## 2. Spec refs

KEL96 sections 3, 4.5–4.7 and 7 govern no-flag ownership/recovery/cleanup; KEL135 sections 3–4, T2 and 7.1 govern Windows identity/storage/current-user/purge/media evidence. Its macOS-only 7.3 amendment is not Windows authority. Architecture 01 sections 1–2 and 06 section 1 retain production boundaries. Host AGENTS and existing testing/test-layout/workflow owners govern this extraction. KEL255/KEL258 supply a proven mapping method, not Windows approval. `atoms.md` records this candidate's independent decision predicates.

## 3. Acceptance criteria

1. Before/after native Cargo/libtest and nextest registries are bijective in default and `profile-test-hooks` modes, including ignored flags/reasons. Candidate source has 51 tests, nine ignored; the retained reviewer default listing also contains 51, but it does not replace final landed default/hooks inventory. Preserve `binary(no_flag_windows)`, group `windows-no-flag-gui`, `max-threads=1` and existing CI profile.
2. Preserve four exact root entries: `keld_dev_windows_helper`, `kel135_second_user_storage_helper`, `shipping_windows_ctrl_c_preserves_host_output_and_ordered_cleanup`, and `isolated_console_timeout_reaps_ready_descendants`. The fourth self-launches direct/grandchild roles through `run_console_timeout_fixture`; all consumers must remain resolvable. Keep external core identity/purge and wv media selectors unchanged. Missing-selector, no-Job and missing-exit-observation controls remain meaningful; exit zero is not an execution oracle.
3. All 169 top-level entries (152 semantic items, 16 imports, one module), 20 impl methods, four extern declarations and 1,413 literals have one mapped owner. Preserve bodies/attributes/cfg/ignore text, field/Drop order, local scopes, environment capture and exact program bytes except reviewed named imports, minimal visibility and module-relative fixture paths. A changed assertion/literal must fail conservation checks; actual compilation separately proves include resolution.
4. Product oracles precede emergency cleanup. Keep the observer's process-lifetime Job before descendants, normal Ctrl-C assertions before observer exit, retained HANDLE observations before capture join, and saved regression failure before socket release. Preserve ordinary 90-second wait, bounded post-kill wait and control-error rejection. Forced-timeout stage deletion is not claimed. `wait_for_process_signal` stays with the existing process observation owner; the same FFI call serves zero-time and bounded checks.
5. Move one family at a time; compile/list/run its affected native tests before continuing. Final default/hooks nonignored suites and all nine signed/media/second-user ignored entries require actual final-source results. Retain unavailable rows with exact prerequisite/owner/command instead of counting skips as passes. The original prerequisite receipt recorded historical fixtures and an ordinary account, but did not bind current source. The current KEL259 signed manifest binds nine test artifacts to the final Windows code source; rows 1–7 passed. Rows 8 and 9 passed together on the same fresh live request after verifying U2 distinct/nonadmin identity, namespace and origin; U2 isolated storage survived restart and cleanup, U1 state remained unchanged, and both users cleaned up. Two preceding failures are retained as failures. All nine acceptance entries now have a passing final-source execution; the accepted layout and one-test selectors remain unchanged.
6. Preserve shared fixture bytes and all tracked selector consumers; record actual final module sizes and representative scenario/fixture/observer/production read sets. No reasoning-speed claim follows from byte counts. Keep newer main HTTP and terminal-diagnostic regressions when reconciling PR287.
7. Final exact `just ci`, required hosted checks and current-head independent review pass, including named independent evidence for test-only unsafe moves. Keep every failed/native/unrun condition attributed. KEL252 remains open until actual three-OS comparison and parent criteria are reconciled.

## 4. Design

No production boundary change. Reuse every existing fixture/resource owner and native primitive. The KEL271 observer crash-owner correction is already in this candidate; this layout does not change it. No new channels, grants, manifests, public types or protocol are added. No rewrite has a named unmet requirement; none is selected.

Use direct root path-module declarations, populated `profiles/mod.rs` and `support/mod.rs`, explicit imports and narrow cross-module visibility. No extra suite prefix, wildcard maze, forwarding registry or include-based pseudo-extraction. The four root bodies remain at their exact names. Other test renames are completely specified by `test-name-map.json`; all item/method/field destinations and lexical dependencies are in `item-census.json`, `item-map.md` and `dependency-map.json`.

| Contract | Populated modules under `tests/no_flag/windows/` |
|---|---|
| Stage admission/DACL/pinning/junction | `staging.rs`; `support/stage.rs`, `support/product.rs` |
| No-flag ownership and generations | `lifecycle.rs`, `recovery.rs`; `support/product_cycle.rs` |
| CLI/host death and console | `dev_lifecycle.rs`, `console.rs`; `support/handles.rs`, `support/process.rs`, `support/window.rs` |
| Signed startup/concurrency/crash | `profiles/startup.rs`, `profiles/lifecycle.rs`; `support/signed_process.rs` |
| Five-store state/purge | `profiles/storage.rs`, `profiles/storage_contract.rs`, `profiles/purge.rs`; `support/profile_run.rs`, `support/profile_server.rs`, `support/profile_response.rs`, `support/profile_observation.rs`, `support/signed_identity.rs`, `support/signed_purge.rs` |
| Second user/media | `profiles/cross_user.rs`, `support/cross_user.rs`, `media.rs` |
| Control and renderer contracts | `control_contract.rs`, `renderer_http_contract.rs`, `renderer_publication.rs`, `renderer_deadlines.rs`, `renderer_connections.rs`; `support/control.rs`, `support/renderer.rs` |

Together with root and `support/mod.rs`, these retain 35 populated destinations. `console.rs` owns its existing native scenario/launcher, missing-selector regression, `accept_console_timeout_readiness` and `run_console_timeout_fixture`: 313 source-item lines/14,130 bytes. A separate 74-line/3,161-byte timeout fixture file was evaluated and rejected as unnecessary fragmentation of this one boundary. The root timeout scenario retains its acquired vectors, unwind boundary and worker ownership; it calls these narrow functions without moving resource scope.

Keep substantial JS/TS in existing `profile_state.js`, `t1b_harness.ts`, `link.ts` and `transport.ts`; keep the shared wv `windows_renderer_http.rs` owner unchanged. Inline PowerShell/HTML/worker programs remain with their owning item for this first mechanical move. Hash exact literals and compare actual included bytes. Structs move with all impl/Drop blocks: notably process-before-stage in `SignedProfileStateRun`, child/Bun HANDLE guard, ProfileStateServer Stop/wake/join, and RendererBeacon activation/receiver/worker join with no invented Drop.

Root exception: **224 source-item lines/9,524 bytes**, before final declarations/imports/attributes. Owner GYLDLAB/KEL259; reason is retaining four exact-selector bodies and their compatibility/resource scopes. Review at the next root-selector edit or if final size materially exceeds this projection; remove only through a separately justified selector/caller proof, not a wrapper registry. No scenario/support item sum exceeds 400 lines or 16 KiB. Measure final full files after imports/comments.

Compatibility fallback is the unchanged binary/four roots. Revert a family with its name/import map while preserving newer regressions. Compare actual Linux/Mac/Windows implementations only after Windows migration; promote sharing only with equivalent ownership/cleanup/failure semantics and at least two live consumers. No universal harness is preselected.

## 5. Boundaries

Future implementation writes: host `tests/no_flag_windows.rs`, populated `tests/no_flag/windows/**`, the approved tracked spec, and exact identified selector-consumer references. Existing fixture assets remain byte-identical. Production, Cargo/CI/nextest/instructions, other OS suites and PR287 branch are outside scope.

`pr287-coexistence-review.md` supports current-main-only migration while preserving draft PR287 (`9518f3adfd291f2002c1074dec5094bee61487a9`). Do not import its product changes or change its do-not-merge intent. Preserve #300's shared HTTP owner instead of its historical inline parser. Preserve #329's bounded LINK_EOF diagnostics and `startup_terminal_record_preserves_host_failure_output`; future KEL135 rebase must reconcile its immediate-kill arm. Branch-only profile probes and installed-root work are not candidate APIs or migration prerequisites. Whichever branch integrates later must use the map and requalify actual changed consumers; if #287 lands first, refresh the source map rather than silently importing it.

## 6. Tasks

- [x] T0: human approval and root exception recorded; KEL271 landed with identical candidate tree, claim/overlap refreshed and fresh landed source/fixture/native inventory bound before edits (provenance above).
- [x] T1: extract existing fixture-contract/support owners; compare conservation and native registration; run affected cases.
- [x] T2: extract staging/lifecycle/recovery/dev/console families, preserving four roots and all native/resource scopes; run affected cases and selector/timeout controls.
- [x] T3a: extract signed profile/storage/purge/cross-user/media families; bind current-source signed fixtures and complete current-user acceptance rows 1–7 with exact cleanup evidence.
- [x] T3b: complete the authenticated second-user rows 8–9 on one live request, preserving the controller handshake and both-user cleanup proof.
- [x] T4a: final current-tip `just ci` and isolated current-head code/unsafe review passed.
- [ ] T4b: module-size/read-set report and KEL-252 three-OS sharing disposition are complete (`three-os-sharing-review.md`); refreshed hosted checks, PR integration, terminal execution artifact, claim/resource reconciliation and authorized cleanup remain. KEL-252 stays open for its separately owned KEL-273 Linux acceptance and PR #288 design reconciliation.

## 7. Test plan

`acceptance.md` retains all exact commands, ignore reasons, external selectors and prerequisites. AC1 uses native default/hooks Cargo+nextest inventories and the complete name bijection. AC2/4 use actual missing-selector, timeout/no-Job/exit-observation controls plus real Ctrl-C and healthy relaunch; force-zero polling is explicitly not a measured 90-second expiration. AC3/6 use item/literal/fixture conservation, changed-assertion rejection and compiler-resolved paths. AC5 maps to every family and all nine manual rows. AC7 uses current `just ci` and independent exact-final-diff review. Port zero, causal handshakes and unchanged kill-switch bounds remain; no sleep-sync, retries, assertion weakening or OS substitution.

KEL271 predecessor proof is retained: its two regressions and real Ctrl-C pass; no-Job and missing-exit-observation controls fail at their intended oracles. The merged same-tree predecessor and fresh native inventory satisfy admission, not final migration acceptance. All ten moved-code families are separately evidenced and independently reviewed. Current-source signed rows 1–7 passed on Windows. On one third live request, row 8 passed the distinct-user storage-isolation test and row 9 passed the exact helper under U2. Their shared handshake, storage census and cleanup are recorded; earlier failed attempts remain attributed and preserved. The latest full local `just ci` passed on `b1ac349` with 839 Rust tests passed and 27 configured skips, warning-denied clippy, format, docs/render and cargo-deny. Independent code/unsafe review found no surviving issue. PR #335 remains draft/do-not-merge while the latest progress commit receives exact-tip checks. The KEL-252 three-OS sharing disposition is recorded: no shared helper is promoted; a two-OS StageFixture candidate is deferred pending an approved common-helper scope. KEL-252 itself remains open for KEL-273 and PR #288 reconciliation; final KEL259 integration/handoff remain open.

## 8. Review gates triggered

Unsafe: yes, independent exact-diff review of relocated test-only Windows FFI/HANDLE/console observation. Public API: none. Permission model: none. Dependency addition: none. Wire protocol: none.

## 9. Perf impact

None. Source navigation estimates only: stage refusal 22,366 item bytes/four files; CLI-death/lease 56,480/11; signed-state restart 65,546/15, versus the 166,770-byte candidate root. These exclude new import/comment overhead; external production/fixture sizes are separately recorded. No runtime, token, speed or agent-success claim.

## 10. Open questions

None. The human approved the arrangement and root exception. Remaining implementation and acceptance evidence stays open under sections 3, 6 and 7.


Attribute-map correction (r2): outer attributes, including `#[link]` and `#[ignore]`, have explicit attached spans and their literals inherit the owning item and destination. Crate-level `#![...]` remains at root. This corrects evidence ownership only; source bytes, item/name counts and the 35-destination arrangement are unchanged. See `attribute-remediation.json` and `remediation.md`; the prior package remains historical.
