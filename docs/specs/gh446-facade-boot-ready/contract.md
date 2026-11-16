# Contract and acceptance criteria
Part of: [index](index.md)
Owns: Governing specification references and binary acceptance criteria for schema-2 facade boots.

## 2. Spec refs

Governing:

- `docs/specs/kel96-no-flag-host-boot.md`: §4.1 (KEL-96 D-3, KEL-96 D-11), §4.2, §4.4, §4.5, §7, and KEL-96 AC-1, KEL-96 AC-8, KEL-96 AC-11, KEL-96 AC-16.
- `docs/specs/kel139-macos-product-spine.md`: KEL-139 AC-3, KEL-139 AC-5, and "Recovery".
- `docs/specs/kel142-macos-renderer-bridge-api.md`: KEL-142 AC-4 (`whenReady` waits for real host `Ready`).
- `docs/specs/kel75-principalized-bun-child-roles.md`: role owners table, and "`LinkBound(g)` and `Ready(g)` are deliberately separate".
- `docs/specs/gh531-window-registry-close-state-machine.md`: criteria 13–15, criterion 30, §4.c, §4.i GH-531 D-6, the Boundaries adoption line, and §6.
- `docs/specs/gh527-worker-owned-blocking-call-transport.md`: §4.6–§4.7.
- `docs/specs/gh532-first-proof-evidence-rules.md`: rules 4–6.
- `docs/specs/gh566-corpus-manifest-owner.md`: target admission.
- `docs/engineering/keld-error-codes.md`: `KELD-CORE-034`, `KELD-CORE-035`, `KELD-CORE-037`.
- Architecture 01 §5 (cold start). Architecture 02, no-flag primary paragraph. Architecture 05 §3 (window ownership). Architecture 06 §1 (macOS/Windows no-flag primary).
- PANEL-P2 #419 settles close and quit only. Nothing in it concerns `Ready`.

Deviations, applied by the implementing change (T-1) with the exact text in §4.10:

- KEL-96 gains decision row KEL-96 D-12 and amended KEL-96 AC-1, KEL-96 AC-8 and KEL-96 AC-11. It also gets amended §4.4 steps 2 and 7, a §4.5 note, and amended approval metadata and §7.
- KEL-139 "Recovery" and AC-3 gain one scoping sentence each.
- Architecture 02 and architecture 06 §1 change one sentence each.
- `keld-error-codes.md` amends 034 and 035 and registers 044.

This spec PR also amends GH-531 in place. Each change is marked "amended by #657":

- **§4.i GH-531 D-6:** the 2026-10-08 scope paragraph.
- **Criterion 14:** its "Before Ready" rule (D-4 here).
- **§4.c and §4 Migration:** the owner and removal condition of the legacy teardown arm.
- **§6:** the GH-531 D-6 re-slice separates the initial-window and close-state tasks.
- **Criterion 20 and the Boundaries adoption line:** the 2026-10-23 revision makes
  accepted subscription and complete replay a facade-readiness prerequisite, and
  records the new, narrowly scoped KEL-139 recovery amendment (§10).
- **Criterion 46:** independent host/facade identity controls and legitimate multiple
  creates distinguish adoption from an accidental one-window limit.

The historical delegated approval in §10 applies only to the technical
candidate at head `a04e1ed889733a64cc33c1b64535e02b4afb0902` and the GH-531 content
bound by that record. It does not approve this split directory or the amended GH-531
bytes at the current PR head. The final split-head exact-content gate remains pending,
as stated in `provenance.md`. Historical KEL-96, KEL-139 and GH-531 approvals do not
by themselves approve the new semantics. This PR changes only this spec and GH-531;
T-1 applies §4.10 to their owning documents with the production change.

## 3. Acceptance criteria

The names below are proposed.

### AC-1: Schema 2 parses
- **Given:** A valid schema-2 descriptor with `schema`, `name`, `entry` and `permissions`.
- **When:** The host parses it.
- **Then:** The parsed initial window is `InitialWindow::None`.
- **Negative control:** Add a forbidden `renderer` key, including `null`, to the valid descriptor.
  - **Single fault:** Add only `renderer: null`.
  - **At-risk precondition:** The schema-2 parser accepts a closed descriptor.
  - **Expected RED:** The parser accepts the forbidden key instead of returning `KELD-CORE-035`.
- **Additional control:** An unknown key such as `initial_window` also returns `KELD-CORE-035`.

### AC-2: Schema 1 is unchanged
- **Given:** Every existing schema-1 parse and staging fixture, including optional `auth`.
- **When:** The fixtures run on the final diff.
- **Then:** They pass unchanged. The unsupported-version probe in `duplicate_unknown_version_name_and_permissions_fields_fail_closed` moves from schema 2 to schema 3.
- **Negative control:** Make `renderer` optional in the schema-1 parser.
  - **Single fault:** Remove the required `renderer` constraint from the schema-1 struct.
  - **At-risk precondition:** Schema 1 must retain its approved closed shape.
  - **Expected RED:** A schema-1 descriptor without `renderer` parses, and `schema_one_still_requires_renderer` fails.
- **Additional control:** Schema 2 does not inherit schema 1's optional `auth` field.

### AC-3: Compiler declaration
- **Given:** `keld.config.ts` declares `initialWindow: "none"` and no `renderer`.
- **When:** `keld dev` stages the app.
- **Then:** The stage contains a schema-2 descriptor and no renderer file. If both config fields are set, the CLI returns `KELD-CLI-047` in `project config` and names both fields and the fix. Without `initialWindow`, descriptor bytes equal today's schema-1 output.
- **Negative control:** Default `renderer` to `index.html` when `initialWindow` is `"none"`.
  - **Single fault:** Add that default to the schema-2 staging path.
  - **At-risk precondition:** The compiler must not infer a renderer for a windowless boot.
  - **Expected RED:** The stage contains a renderer file.

### AC-4: Ready before any window in generation 1
- **Given:** A schema-2 boot whose `@keld/electron` main prints `READY` inside `app.whenReady()`.
- **When:** The test reads `READY` on macOS.
- **Then:** The role reads exactly one lifecycle `Ready`. It is the first application-lifecycle `EVENT` after the `HELLO` reply. A correlated control `REPLY`, including `Subscribed`, may precede it. `app.isReady()` is true. A successful process-wide native application-window census for the host PID is zero.
- **Negative control:** Restore the host-created initial window and emit `Ready` on its `NavigationReady` event.
  - **Single fault:** Re-enable `CreateInitialWindow` for schema 2.
  - **At-risk precondition:** The census must detect hidden, untitled and differently titled application windows.
  - **Expected RED:** The census reports one application window at `READY`.
- **Census contract:** The census remains zero until this fixture's `Quit`. It separates application windows from system auxiliary windows. Observation failure reports
  unavailable, never zero. The PID selects the observed process and grants no authority. `NativeWindowObserver` remains the positive presentation
  oracle. Its empty result cannot prove absence. Hidden, untitled and differently titled positive controls each make the zero assertion fail. A
  create-then-destroy control requires an allocation-history receipt. A snapshot proves absence only at its observation boundary.

### AC-5: Host initialized, not just authenticated
- **Given:** A schema-2 router with an authenticated and installed generation, but no `HostInitialized` event.
- **When:** The role sends its echo-fence call.
- **Then:** The fence reply arrives and no application-lifecycle `EVENT` has been written. After `HostInitialized`, exactly one `Ready` is written. Correlated control replies remain legal.
- **Negative control:** Write `Ready` when the generation installs on `HELLO` acceptance.
  - **Single fault:** Move the Ready trigger to HELLO acceptance.
  - **At-risk precondition:** Authentication can finish before host services and the UI loop are ready.
  - **Expected RED:** The role reads `Ready` before the fence reply.

### AC-6: Exactly one trigger per boot kind
- **Given:** Schema-1 and schema-2 coordinators with their own declared Ready trigger.
- **When:** A coordinator receives the other boot kind's trigger, or receives a second trigger.
- **Then:** It writes no extra `Ready` and ends the session with `KELD-CORE-044`. Schema 2 rejects `NavigationReady`. Schema 1 rejects `HostInitialized`.
- **Negative control:** Map both events to `signal_ready`.
  - **Single fault:** Remove the boot-kind check before `signal_ready`.
  - **At-risk precondition:** Recovery can be armed after the first Ready.
  - **Expected RED:** The second trigger writes a second `Ready`.

### AC-7: Only Init triggers and the UI loop serves commands
- **Given:** The tao event sequence `Opened`, `NewEvents(StartCause::Init)`, then `NewEvents(StartCause::WaitCancelled)`.
- **When:** The pure milestone handler processes the sequence.
- **Then:** It emits one `HostInitialized` at `Init`. After `Init`, a past first-milestone deadline does not exit the session.
- **Negative control:** Emit on the first closure call.
  - **Single fault:** Treat `Event::Opened` as the first host milestone.
  - **At-risk precondition:** `Opened` can arrive before `Init`.
  - **Expected RED:** The handler emits `HostInitialized` at `Opened`.
- **macOS integration:** On AC-4's boot, `app.quit()` inside the `whenReady` continuation preserves the KEL-96 §4.5 order: correlated reply, revoke and close, terminate and reap, UI exit, exit status zero, and zero Keld descendants.
- **Additional control:** Keeping the navigation deadline active after `Init` must fail because it exits a healthy windowless session.

### AC-8: Recovered generation, host side
- **Given:** AC-4's schema-2 boot after `READY`.
- **When:** Only the Bun role is killed and its successor authenticates on macOS.
- **Then:** The successor reads exactly one `Ready`. The process-wide census remains zero because the host creates no window in any generation.
- **Negative control:** Remove the `window_ready` replay during successor install.
  - **Single fault:** Skip the successor Ready replay.
  - **At-risk precondition:** A recovered role waits for the replay before its lifecycle fence completes.
  - **Expected RED:** The successor's `whenReady` remains pending past its echo fence.

### AC-9: Recovered generation, compatibility side
- **Given:** The `electron-app-v1` cell `app.ready.recovered-generation` on macOS.
- **When:** The corpus records the compatibility result.
- **Then:** The cell remains `unknown` and uncited under GH-532 rule 6. Electron has no
  matching supervised main-process recovery contract. The existing
  `app.ready.api-first-late-listener` today-test in `app-surface.test.ts` covers only
  no-replay behavior for a listener registered after the API observed Host Ready. It
  does not exercise supervised role recovery or change this cell's status.
- **Negative control:** Replay cached Ready to a listener registered after the API observed Host Ready.
  - **Single fault:** Deliver a cached `ready` event to a listener registered after the API has observed Host Ready.
  - **At-risk precondition:** The API observed Host Ready before the late listener was registered.
  - **Expected RED:** The mapped today-test observes `lateListener=1` instead of `lateListener=0`.
- **Scope:** This cell remains unknown for supervised recovery. Eligible-survivor adoption is a separate registry invariant tested by AC-19; it is not a cap on legitimate application-created windows.

### AC-10: Renderer boots and KEL-237 regressions
- **Given:** The existing renderer-boot and recovered-generation regression suite.
- **When:** The final diff runs.
- **Then:** These pass unchanged: `successor_reader_waits_for_retired_fs_drain_before_ready_and_call`;
  `recovered_generation_orders_window_events_once_across_installation`; `retired_generation_eof_cannot_fail_or_replace_the_successor`;
  `failed_initial_ready_write_denies_recovery_before_successor`; `pre_ready_bun_crash_is_startup_failure_not_a_recovered_window`; the macOS, Linux and
  Windows no-flag lifecycle cycles; and `lifecycle_corpus_rust_oracles_execute`, including `app.when-ready.host-ready-gate` and
  `when_ready_does_not_resolve_before_host_ready_event`.
- **Negative control:** Remove the schema-1 Ready replay.
  - **Single fault:** Delete the existing G2 Ready replay.
  - **At-risk precondition:** A recovered schema-1 generation consumes that replay.
  - **Expected RED:** `successor_reader_waits_for_retired_fs_drain_before_ready_and_call` fails.
- **Additional control:** Replacing `whenReady` with `Promise.resolve()` fails the KEL-237 cell.

### AC-11: Other platforms fail closed
- **Given:** A schema-2 descriptor on Linux or Windows.
- **When:** `keld-host` starts without a flag.
- **Then:** It returns `KELD-CORE-034` before any endpoint, child or window exists, and makes one attempt. The detail states that schema 2 runs on macOS only and tells the user to remove `initialWindow` from `keld.config.ts`.
- **Negative control:** Remove the platform check.
  - **Single fault:** Allow schema 2 to enter the renderer path on Linux or Windows.
  - **At-risk precondition:** Those hosts do not implement schema-2 startup.
  - **Expected RED:** The no-resource assertion fails when the host attempts to start without a renderer.

### AC-12: Existing compatibility cells pass unchanged
- **Given:** `app.ready.emitted-once`, `app.when-ready.is-ready-agreement` and `app.when-ready.host-ready-gate`.
- **When:** The final diff runs the corpus.
- **Then:** All three pass unchanged. The first two belong to `electron-app-v1` in #656. #656 already records both as `pass`. Initial facade-window absence is AC-4's native observation and AC-16's host rule. It is not a new corpus cell.
- **Negative control:** Change the expected status of `app.ready.emitted-once` from `pass`.
  - **Single fault:** Change only that recorded cell status.
  - **At-risk precondition:** The corpus runner checks the recorded cell contract.
  - **Expected RED:** The unchanged-cell assertion fails.
- **Evidence limit:** These cells do not prove that production has no host-created window.

### AC-13: Normative documentation and digest
- **Given:** T-1 applies the §4.10 amendment to its owning documents.
- **When:** The production change updates the documents.
- **Then:** The §4.10 text lands in the same change as the Ready emission point. KEL-96's header digest equals the §4.1 recomputation in §7.
- **Negative control:** Omit one required normative amendment from KEL-96.
  - **Single fault:** Remove one approved §4.10 sentence from the owning document.
  - **At-risk precondition:** The digest binds the complete decision text.
  - **Expected RED:** The recomputed digest differs or the required text is absent.

### AC-14: Failed window-module install writes no Ready
- **Given:** A schema-2 coordinator whose engine drops its event sender before `HostInitialized`.
- **When:** The coordinator observes the closed event channel.
- **Then:** It returns `KELD-CORE-037`, the role's echo fence shows no application-lifecycle `EVENT`, and recovery is denied. A concurrent successor request is rejected.
- **Negative control:** Signal `Ready` when the coordinator starts, before the install result.
  - **Single fault:** Move Ready emission before the install result.
  - **At-risk precondition:** Window-module installation can fail after HELLO and before Init.
  - **Expected RED:** The role reads `Ready` before the failed install is reported.

### AC-15: Bounded wait for the Init turn
- **Given:** A schema-2 coordinator whose deadline has passed without `HostInitialized`.
- **When:** The coordinator wait expires.
- **Then:** It creates `KELD-CORE-037` with detail `the UI loop did not deliver its first turn within 15 s`. It passes the error by borrow after bounded cleanup. It writes no Ready and denies recovery. It calls the binary-owned terminal function once. It retains main-thread custody and keeps the dev stage when release is unproved.
- **Negative control:** Replace the bounded wait with an unbounded `recv()`.
  - **Single fault:** Remove the absolute first-milestone timeout.
  - **At-risk precondition:** The UI loop may never deliver Init.
  - **Expected RED:** The bounded terminal-callback observation does not complete.
- **Terminal oracle:** The child test records one `TERMINAL_ENTERED`, a failure exit, and no `COORDINATOR_RETURNED`. Returning `Err(037)` instead of invoking the terminal function fails this oracle. AC-20 exercises actual termination.

### AC-16: Window calls before Ready
- **Given:** A schema-2 boot before its first Ready write, with a link subscribed to `window`.
- **When:** The role sends `Create` or another window call.
- **Then:** The reply is `KELD-CORE-040` and no native window is created. An unsubscribed link receives `KELD-CORE-045` first. `Subscribe` remains legal before Ready.
- **Negative control:** Mark the registry ready when it is constructed.
  - **Single fault:** Move the registry's serviceable state to construction.
  - **At-risk precondition:** A role can send a window call before Ready.
  - **Expected RED:** The pre-Ready `Create` succeeds instead of returning `KELD-CORE-040`.
- **Ownership:** F02-T2 in #449 implements this rule with its first window call. GH-531 criterion 14 carries the corresponding test.

### AC-17: Entry-evaluation ordering is qualified
- **Given:** Separate synchronous CJS and native-ESM entry fixtures, plus a target-derived fixture whose imported dependency invokes `whenReady()` before the entry registers its `ready` listener.
- **When:** Each fixture records dependency start/end, entry start/end, Ready delivery and both public callbacks.
- **Then:** Neither callback runs before synchronous graph evaluation completes, and the listener sees one event. The tests also coalesce HELLO and Ready in one transport delivery and delay listener attachment. Awaited ESM setup remains a separate Electron behavior, not an inference from synchronous tests.
- **Negative control:** Dispatch Ready before listener installation.
  - **Single fault:** Deliver Ready before the entry's listener is installed.
  - **At-risk precondition:** An imported dependency can invoke `whenReady()` during evaluation.
  - **Expected RED:** A callback runs before graph evaluation ends or the listener misses the event.
- **Ownership:** #445 and #446 own lifecycle scheduling. A loader change needs a separately reviewed boundary. It must preserve native `@keld/api` callers that await filesystem and auth readiness.

### AC-18: One usable public facade-ready state
- **Given:** Raw host Ready has arrived, but the correlated window `Subscribe` acceptance is held.
- **When:** The test inspects `app.isReady()`, `whenReady()` and the `ready` event.
- **Then:** `isReady()` remains false, `whenReady()` remains pending and `ready` has not fired. After every replayed `Created` fact is applied and the accepted
  `Subscribed` reply is processed, all three agree and a constructor in either callback succeeds. Repeat with zero survivors, surviving windows and
  successor install where Ready precedes reader startup. A refused subscription fails closed.
- **Negative control:** Resolve public readiness on raw host Ready.
  - **Single fault:** Remove the accepted-subscription and replay barrier.
  - **At-risk precondition:** Raw Ready can precede Subscribe acceptance.
  - **Expected RED:** A public readiness observable changes before the correlated reply.
- **Boundary:** Keep this facade composition out of shared native API readiness policy.

### AC-19: Adoption identity is independent
- **Given:** A live host window reference and successor role generation whose identity cannot equal the facade's first local allocation counter.
- **When:** The facade subscribes, applies replay and constructs a window.
- **Then:** The constructor returns the existing mirror object, host reference and native identity with zero new native allocations. Retain two survivors and consume only one. Verify both native identities survive. Separately create two legitimate windows after adoption eligibility is exhausted.
- **Negative control:** Map a facade counter to a host window ID.
  - **Single fault:** Use the facade's local counter as the host identity.
  - **At-risk precondition:** The host ID is deliberately different from the local allocation counter.
  - **Expected RED:** The returned identity is wrong or a new native window is allocated.
- **Additional control — eligible-survivor adoption:** Delete the registry's successor `Create` adoption arm.
  - **Mutation:** Skip the adoptable-window match and fall through to native allocation (one fault).
  - **Setup:** The successor has an eligible survivor after accepted Subscribe/replay and invokes `Create` through the constructor.
  - **Failure:** The host returns `Created` instead of `Adopted`, native allocation count increases, and returned identity is not the survivor's.
- **Other controls:** Drop an unclaimed survivor, disable adoption, or enforce a global one-window cap. Each mutation must fail its own assertion.

### AC-20: Failure custody survives the new boot mode
- **Given:** The ordinary host runner returns `Result<(), HostRunFailure>` and unresolved AppKit, presentation or profile owners remain on the main thread.
- **When:** The stalled-Init terminal path runs in a child process.
- **Then:** The typed failure occurs once, no Ready or recovery is admitted, and an unproved dev stage is not removed. The ordinary retained-custody return path remains unchanged. The terminal function reports the failure by borrow and does not receive retained owners.
- **Negative control:** Flatten the failure to `HostAppError`, move or drop a retained owner, or remove the stage before custody is proved.
  - **Single fault:** Drop one retained main-thread owner before the terminal call.
  - **At-risk precondition:** Fatal cleanup may run while AppKit or profile owners remain live.
  - **Expected RED:** The child test observes missing custody or premature stage deletion.
- **Additional control:** Treat process exit as a physical-drain receipt. The cleanup oracle must reject that inference.
