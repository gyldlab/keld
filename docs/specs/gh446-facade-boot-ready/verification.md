# Verification and performance
Part of: [index](index.md)
Owns: Test mapping, regression situations, platform acceptance and performance impact.

## 7. Test plan & evidence

### Test mapping

- **AC-1, AC-2:** `schema_two_declares_no_initial_window`, `schema_two_rejects_renderer_and_unknown_fields`, `schema_one_still_requires_renderer`; unit (keld-core)
  Anti-flake: pure parse
- **AC-3:** `initial_window_none_stages_schema_two_without_renderer`, `initial_window_none_with_renderer_is_a_staging_error`; unit (keld-cli);
  `stage_is_random_owner_private_new_inode_and_byte_consistent` unchanged Anti-flake: temp dirs
- **AC-4, AC-7:** `schema_two_ready_precedes_every_window_and_quit_is_ordered`; real macOS no-flag integration with the dedicated process-wide application-window
  observer and its hidden/untitled/different-title controls. `init_arm_alone_emits_host_initialized` (including pre-Init Opened) and
  `init_disarms_the_milestone_deadline`; keld-wv unit Anti-flake: native observation fenced at READY; unavailable is not zero; past Instant and
  constructed events; allocation-history control for any never-allocated claim
- **AC-5, AC-6:** `schema_two_ready_waits_for_host_initialized`, `foreign_or_repeated_ready_trigger_is_fatal`; keld-core router unit Anti-flake: echo-fence
  ordering, bounded by frames, not time
- **AC-8:** `schema_two_successor_reads_ready_once_and_host_creates_no_window`; macOS recovery integration Anti-flake: existing recovery support; fence after
  `READY`
- **AC-9:** cell `app.ready.recovered-generation` remains `unknown` and uncited under
  GH-532 rule 6; no current today-test proves supervised main-process recovery. The
  separate `app.ready.api-first-late-listener` today-test in `app-surface.test.ts`
  protects only no-replay behavior for a listener registered after Host Ready; its NC
  replays Ready to that late listener and expects `lateListener=0`. Eligible-survivor
  adoption is tested separately by AC-19 / GH-531 criterion 46. Anti-flake: gh566
  admission; observer child process, explicit listener-order markers, no timing
- **AC-10:** the named existing tests; `lifecycle_corpus_rust_oracles_execute` Anti-flake: unchanged
- **AC-11:** `schema_two_descriptor_is_refused_before_any_application_resource`; Linux and Windows no-flag integration (CI) Anti-flake: resource census like
  `windows_pre_ready_crash_denies_successor_before_provisioning`
- **AC-12:** `app.ready.emitted-once` and `app.when-ready.is-ready-agreement` (`electron-app-v1`, #656) and `app.when-ready.host-ready-gate` run unchanged;
  "before any window" is AC-4's keld-host test Anti-flake: unchanged cells
- **AC-13:** review T-1 against §4.10. Recompute the KEL-96 header with
  `awk '/KEL96_DECISIONS_V1_START/{f=1;next}/KEL96_DECISIONS_V1_END/{f=0}f' docs/specs/kel96-no-flag-host-boot.md | shasum -a 256`;
  preserve the separately recorded original D1–D11 approval digest. Anti-flake:
  deterministic exact bytes; removing D12 or one required amendment must fail.
- **AC-14:** `closed_event_channel_before_host_initialized_is_startup_failure`; keld-core router unit Anti-flake: echo fence; dropped sender, no timing
- **AC-15:** `init_wait_expiry_is_typed_startup_failure`; bounded terminal-callback observation plus AC-20 child fixture Anti-flake: past deadline; typed
  error received once; no ordinary-return marker
- **AC-16:** `create_before_ready_is_refused_with_040` in F02-T2 (#449) Anti-flake: registry unit, no GUI
- **AC-17:** synchronous CJS, synchronous native ESM, early dependency lifecycle call, coalesced HELLO/Ready and delayed-listener fixtures; a separate
  awaited-ESM comparison probe Anti-flake: explicit evaluation and delivery markers, controlled fences; no generic ESM PASS from a CJS test
- **AC-18:** `public_ready_waits_for_subscribed_replay` in #449, both Ready/Subscribe arrival orders, empty/nonempty replay and refusal Anti-flake:
  hold/release the correlated reply and inspect all three public observables
- **AC-19:** GH-531 criterion 46 and `host_identity_does_not_alias_facade_counter`; #449 Anti-flake: host ids advanced independently, native identity
  observation and exact allocation receipts, two legitimate creates
- **AC-20:** `pre_init_failure_preserves_main_custody_and_stage`; child-process terminal test plus existing retained-custody regressions Anti-flake: typed
  error/exit receipt, explicit release markers; no successful-drain inference from death

**Current-pin source reconciliation.** The original first-RED statement belongs to
historical approval base `3503a8aa`; it is not a valid RED at the current Pin
`ec29b7ce`. At this Pin, the strict schema-2 parser, CLI stage producer,
`HostInitialized` coordinator, first-Init deadline and terminal callback already
exist. Linux refuses during staging; Windows product builds refuse with
`KELD-CORE-034`. Windows test admission is debug-only and is compiled out of product
builds. T-1 must reuse these owners and must not expect a valid schema-2 parse to fail.
Source presence does not prove that tests ran or that native acceptance is complete.

**Failure-first receipts for remaining gaps.** For each remaining AC atom, retain the
exact source head, command, intended failing assertion, observed RED, GREEN and
one-at-a-time mutation result. A stub-host PASS cannot substitute for AC-4's real
macOS native observation. Each control must exercise its own causal fault; one broad
reverted implementation is not evidence that all controls work.

| Regression/recovery situation | Required preserved behavior | Owner/test boundary |
|---|---|---|
| Schema-1 boot, including existing auth | Same strict descriptor, host-created renderer window and navigation-triggered Ready | AC-2/10; existing KEL-96/KEL-139 fixtures |
| Bad, missing or stale HELLO | No Ready or privileged service admission; existing one-attempt startup result | Existing authenticated-link negatives plus AC-5 |
| Bun dies before first successful Ready | Startup failure, no successor provisioning | Existing pre-ready crash test; AC-14/15 |
| Init never arrives or a wrong trigger arrives | Typed failure, no Ready/recovery; existing custody retained | AC-6/15/20 |
| Bun dies after facade Ready with zero windows | Fresh authenticated successor, one replayed Ready, no automatic window | AC-8 |
| Bun dies with surviving facade windows | All native identities retained; transfer/replay once; eligible Create adopts | GH-531 D-6/c30/c46, AC-18/19; ClosePending remains #450 |
| Retired generation still has filesystem work | Quiescence and authority retirement precede successor Ready and privileged work | Unchanged G2 filesystem-drain test; AC-10 |
| Delayed old-generation EOF or stale window reference | Cannot fail/replace successor or operate its windows | AC-10 and GH-531 generation/admission tests |
| Quit with zero windows / renderer close | Existing correlated reply, revoke, child reap, UI-exit ordering; schema-1 LastWindowClosed unchanged | AC-7/10; no unrelated shutdown change |
| Windowless session after Init | No first-window timer and no synthetic last-window event | AC-7 and D-8 |

Required OS acceptance: macOS on a physical Mac; Linux and Windows AC-11 in their CI
lanes. This design revision runs no native acceptance. Every unrun lane stays unrun
until an actual receipt is attached.

## 9. Perf impact

- **Renderer boots:** none.
- **Schema-2 boots:** the host removes navigation from its Ready prerequisites; the
  first requested window follows the qualified app/facade path and a Create round trip.
  Module evaluation and host startup overlap, so source order alone does not quantify
  either latency delta.
  The architecture 01 §5 "cold start → first paint" row can move for facade boots; it
  is measured under X05-T1's registered metric. This spec makes no performance claim.
- **Other consumers:** none on current platforms. INFERENCE: the KEL-270 updater
  health gate, which requires that the app "reached application `Ready`", would mean
  "role authenticated and host loop live" for a schema-2 boot. That gate is
  Windows-only today, and Windows refuses schema 2.
