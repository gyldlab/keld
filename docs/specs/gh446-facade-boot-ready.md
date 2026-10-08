# Spec: facade-boot Ready — host initialized and window module available before any app window (F01-T2)
Status: draft
Linear: GH-446 (#517) · Owner: @0monish · Updated: 2026-10-08

## 1. Goal & non-goals

In an Electron app, `ready` fires once Electron has initialized and before any
`BrowserWindow` can exist, and the app creates every window itself. In Keld, the
host emits lifecycle `Ready` only after it has created its own window and finished
that window's first navigation:

- `coordinate_window_events` maps `AppWindowEvent::NavigationReady` to
  `PrimaryRouterHandle::signal_ready` (`crates/keld-core/src/app_session.rs:3540`).
- That event fires only for the host-created initial window
  (`crates/keld-wv/src/wkwebview/mod.rs:723`).

So an `@keld/electron` main that calls `new BrowserWindow()` inside `whenReady` would
get a second window next to a host window it never asked for. This spec decides who
creates window #1 and what `Ready` means for each generation.

The observable outcome is a new boot kind. When the compiled descriptor declares that
the app creates every window, the host on macOS:

1. authenticates the role's `HELLO`;
2. enters its UI loop with no window;
3. writes `Ready` once, on the loop's `Init` turn.

The app then sees `ready`/`whenReady` with zero native windows, and the host refuses
every window call made before that `Ready`. Renderer-declared boots behave exactly as
today.

Non-goals:

- The window `Create` call and the `BrowserWindow` constructor (F02-T2 #449, gh531).
- `Create` adoption for recovered generations. The gh531 registry owns it (gh531 D6
  scope, amended by #657), and it lands with #449 (gh531 T1).
- Two-phase quit (F01-T3).
- Launch-time `open-url`/`open-file` delivery (#524, F01-T8).
- Windows and Linux emission points. Those hosts refuse the new boot kind (D9).
- `keld migrate` writing the new config field, and `compat: { electron: true }` (architecture 04 §2 sketch).
- The cold-start delta. It is measured under X05-T1's registered metric and is not
  an acceptance criterion here.

## 2. Spec refs

Governing:

- `docs/specs/kel96-no-flag-host-boot.md`: §4.1 (D3, D11), §4.2, §4.4, §4.5, §7, and AC1, AC8, AC11, AC16.
- `docs/specs/kel139-macos-product-spine.md`: AC3, AC5, and "Recovery".
- `docs/specs/kel142-macos-renderer-bridge-api.md`: AC4 (`whenReady` waits for real host `Ready`).
- `docs/specs/kel75-principalized-bun-child-roles.md`: role owners table, and "`LinkBound(g)` and `Ready(g)` are deliberately separate".
- `docs/specs/gh531-window-registry-close-state-machine.md`: criteria 13–15, criterion 30, §4.c, §4.i D6, the Boundaries adoption line, and §6.
- `docs/specs/gh527-worker-owned-blocking-call-transport.md`: §4.6–§4.7.
- `docs/specs/gh532-first-proof-evidence-rules.md`: rules 4–6.
- `docs/specs/gh566-corpus-manifest-owner.md`: target admission.
- `docs/engineering/keld-error-codes.md`: `KELD-CORE-034`, `KELD-CORE-035`, `KELD-CORE-037`.
- Architecture 01 §5 (cold start). Architecture 02, no-flag primary paragraph. Architecture 05 §3 (window ownership). Architecture 06 §1 (macOS/Windows no-flag primary).
- PANEL-P2 #419 settles close and quit only. Nothing in it concerns `Ready`.

Deviations, applied by the implementing change (T1) with the exact text in §4.10:

- KEL-96 gains decision row D12 and amended AC1, AC8 and AC11. It also gets amended §4.4 steps 2 and 7, a §4.5 note, and amended approval metadata and §7.
- KEL-139 "Recovery" and AC3 gain one scoping sentence each.
- Architecture 02 and architecture 06 §1 change one sentence each.
- `keld-error-codes.md` amends 034 and 035 and registers 044.

This spec PR also amends gh531 in place. Each change is marked "amended by #657":

- **§4.i D6:** the 2026-10-08 scope paragraph.
- **Criterion 14:** its "Before Ready" rule (D4 here).
- **§4.c and §4 Migration:** the owner and removal condition of the legacy teardown arm.
- **§6 T1/T2:** the D6 re-slice.
- **The Boundaries adoption line:** this sentence is marked PROPOSED. It awaits the
  repository owner's exact-content approval (§10).

Approving this PR approves every one of these amendments except the PROPOSED
sentence. The PR changes no other file.

## 3. Acceptance criteria (binary, each becomes a test)

The names below are proposed.

1. **Schema 2 parses.** Given `{"schema":2,"name":"A","entry":"src/main.ts","permissions":{…}}`, when the host parses it, then the selection's initial window is `InitialWindow::None`. *NC:* a schema-2 document with a `renderer` key (even `null`), or with any unknown field (including `initial_window`), is rejected with `KELD-CORE-035`.
2. **Schema 1 is unchanged.** Given every existing schema-1 parse and staging test, when it runs, then it passes unchanged. The one exception: in `duplicate_unknown_version_name_and_permissions_fields_fail_closed` (`app_session.rs:7019`), the unsupported-version probe moves from `schema:2` to `schema:3`. *NC:* making `renderer` optional in the schema-1 struct makes a schema-1 document without `renderer` parse, and the new `schema_one_still_requires_renderer` fails.
3. **Compiler declaration.** Given `keld.config.ts` with `initialWindow: "none"`, when `keld dev` stages, then the stage holds a schema-2 descriptor and no renderer file.
   - The same config with `renderer:` also set fails with `KELD-CLI-047` (phase `project config`), naming both fields and the fix.
   - Without `initialWindow`, the descriptor bytes equal today's schema-1 output.
   - *NC:* defaulting `renderer` to `index.html` when `initialWindow` is `"none"` stages a renderer and fails the test.
4. **Ready before any window, generation 1 (macOS, real host).** Given a schema-2 boot whose `@keld/electron` main prints `READY` inside `app.whenReady()`, when the test reads `READY`, then:
   - the role has read exactly one lifecycle `Ready`, and it was the first lifecycle frame after its `HELLO` reply;
   - `app.isReady()` is true;
   - the macOS native-window census for the host PID is 0.

   The census stays 0 until Quit. At T1 this is structural: no window call exists on the wire. From F02-T2 on, AC16 keeps it a host fact. *NC (from the issue):* restoring the host-created initial window, with `Ready` on its `NavigationReady`, for a schema-2 boot makes the census 1 at `READY`.
5. **Host initialized, not just authenticated.** Given a schema-2 router with an authenticated, installed generation, when no `HostInitialized` event has been delivered, then the role's echo-fence call gets its reply and no lifecycle frame has been written. After `HostInitialized`, exactly one `Ready` is written. *NC:* writing `Ready` at generation install (on `HELLO` acceptance) makes the role read `Ready` before the fence reply.
6. **Exactly once, one trigger per boot kind.** In both directions, a trigger that belongs to the other boot kind writes no `Ready` and ends the session with `KELD-CORE-044`:
   - a schema-2 coordinator that receives `NavigationReady`;
   - a schema-1 coordinator that receives `HostInitialized`.

   A second trigger event also ends the session with `KELD-CORE-044`. *NC:* mapping both events to `signal_ready` writes a second `Ready` once recovery is armed.
7. **Only the `Init` turn triggers, and the UI loop serves commands at Ready.**
   - **Unit (keld-wv).** The loop's milestone handler is a pure function of the tao event and its state. Given the sequence `Event::Opened`, then `Event::NewEvents(StartCause::Init)`, then `Event::NewEvents(StartCause::WaitCancelled)`, it emits exactly one `HostInitialized`, at `Init`. Given an `Init` already seen and a past deadline, it does not exit. *NC:* emitting on the first closure call emits at `Opened`. *NC:* keeping the navigation deadline after `Init` exits a healthy windowless session.
   - **Integration (macOS).** Given AC4's boot, when the main calls `app.quit()` in the `whenReady` continuation, then the KEL-96 §4.5 order holds: correlated reply, revoke and close, terminate and reap, UI exit, exit status 0, zero Keld descendants.
8. **Recovered generation, host side (macOS).** Given AC4's boot after `READY`, when only the Bun role is killed, then:
   - the successor authenticates and reads exactly one `Ready`;
   - the census stays 0 throughout, because the host creates no window in any generation.

   *NC:* removing the `window_ready` replay in successor install leaves the successor's `whenReady` pending past its echo fence.
9. **Recovered generation, compat side.** The `electron-app-v1` cell `app.ready.recovered-generation` is recorded `unknown` on macOS. It is uncited (gh532 rule 6), because Electron has no main-process recovery to cite. Its mapped today-test asserts that a fresh role process observes exactly one replayed `Ready`. *NC:* names #449's keld-core adoption test `successor_create_adopts_the_oldest_unclaimed_window`: deleting the registry's adoption arm makes a successor `Create` produce a second window. That test enforces the invariant that a recovered generation never produces a second window (D7).
10. **Renderer boots and the KEL-237 gate.** On the final diff, these pass unchanged:
    - `successor_reader_waits_for_retired_fs_drain_before_ready_and_call` (the "G2 Ready replay", `app_session.rs:8633`);
    - `recovered_generation_orders_window_events_once_across_installation` (9599);
    - `retired_generation_eof_cannot_fail_or_replace_the_successor` (9540);
    - `failed_initial_ready_write_denies_recovery_before_successor` (10597);
    - `pre_ready_bun_crash_is_startup_failure_not_a_recovered_window`;
    - the macOS, Linux and Windows no-flag lifecycle cycles;
    - `lifecycle_corpus_rust_oracles_execute`, which runs the cell `app.when-ready.host-ready-gate` → `when_ready_does_not_resolve_before_host_ready_event`.

    *NCs (from the issue):* removing the schema-1 `Ready` replay fails the G2 replay test, and replacing `whenReady` with `Promise.resolve()` fails the KEL-237 cell.
11. **Other platforms fail closed.** Given a schema-2 descriptor on Linux or Windows, when `keld-host` starts with no flag, then it fails with `KELD-CORE-034`. The detail says schema 2 runs on macOS only, and the fix is to remove `initialWindow` from `keld.config.ts`. This happens before any endpoint, child or window exists, with one attempt. *NC:* removing the platform check sends the boot into the renderer path, which has no renderer, and the test fails at its no-resource assertion.
12. **Existing cells pass unchanged (restates the issue's AC1).** `app.ready.emitted-once`, `app.when-ready.is-ready-agreement` and the KEL-237 cell `app.when-ready.host-ready-gate` pass unchanged on the final diff. "Ready before any app window" is a host fact proven by AC4's keld-host macOS no-flag test (`NativeWindowObserver` census 0 at `READY`) and kept by AC16. It is not a corpus cell. The first two cells are `electron-app-v1`'s (#656), and #656 records both as `pass`, so nothing flips.
13. **Docs.** The §4.10 text lands in the same change as the emission point, and KEL-96's header digest equals the §4.1 recomputation (§7).
14. **A failed window-module install writes no Ready.** Given a schema-2 coordinator, when the engine drops its event sender before `HostInitialized` (an install step failed), then:
    - the coordinator returns `KELD-CORE-037`;
    - the role's echo fence shows no lifecycle frame;
    - recovery is denied, so a concurrent successor request is rejected.

    *NC:* signalling `Ready` when the coordinator starts, before the install result, writes `Ready`.
15. **Bounded wait for the `Init` turn.** Given a schema-2 coordinator whose deadline has passed and which has received no `HostInitialized`, when it waits, then:
    - it returns `KELD-CORE-037`, with the detail "the UI loop did not deliver its first turn within 15 s";
    - it writes no `Ready`;
    - it denies recovery and runs the UI-independent cleanup;
    - it calls the host terminal function exactly once (a test double).

    *NC:* an unbounded `recv()` never returns. The test detects this with a bounded join on the coordinator thread's result channel, never a sleep.
16. **Window calls before Ready (an edge F02-T2 must satisfy).** Given a facade boot whose first `Ready` write has not happened, when the role sends `Create` or any other window call, then the reply is `KELD-CORE-040` and no native window is created. *NC:* marking the registry ready when it is constructed serves that `Create`. This spec decides the rule (D4). Its test lands with the first window call, in F02-T2 (gh531 criterion 14 "Before Ready", amended by #657).

## 4. Design

### 4.0 First-principles and reuse decision

This is architecture under root AGENTS.md: it changes who owns the creation of
window #1, which sets crash ownership of the first window. It also changes when
recovery is armed for the new boot kind. The atoms come first. Each one was
falsified or proved before §4.1–§4.9 chose anything.

| # | Atom | Owner | Boundary, input → output | Failure mode | Observable | Evidence |
|---|---|---|---|---|---|---|
| A1 | Who declares the boot kind | the boot compiler (`keld-cli boot.rs`) and parser (`keld-core parse_boot_bytes`) | `keld.config.ts` → `keld.boot.json` → `ParsedBoot` | the kind is inferred from absence, or the host guesses | AC1–AC3 | FACT: schema 1 is closed and has a required `renderer` (`app_session.rs:837`, `deny_unknown_fields`); KEL-96 §4.2: "schema evolution requires a version decision" |
| A2 | Host initialized | `keld-core` startup state machine and the keld-wv macOS loop | `HELLO` + installed generation + `Init` turn → `Ready` trigger | `Ready` before the host can serve, before a startup failure, or on a pre-`Init` event | AC5, AC7 | FACT (tao 0.35.3): `NewEvents(StartCause::Init)` is issued from `applicationDidFinishLaunching:` (`platform_impl/macos/app_state.rs:284-306`); `open_urls` delivers `Event::Opened` through the same callback with no ready check (`:309`); macOS `run_app` awaits `HELLO` before `WkWebViewEngine::new` (`app_session.rs:1777`) |
| A3 | Window module available, and no window call before Ready | the gh531 registry (F02-T2); today the UI wake bridge | installed window-call servers → app `Create` | `Create` refused after `Ready`, or served before it | AC14, AC16, gh531 criterion 14 | FACT: `spawn_app_wake_bridge` runs before `run_return` (`wkwebview/mod.rs:303-330`); the g1 reader starts at install, before the loop; successor install replays `Ready` before it starts the reader (`app_session.rs:5462`) |
| A4 | One `Ready` per generation, ordered after `HELLO` and after the app's first lifecycle call | primary router, `@keld/api` link | trigger → one frame → `onHostReady` | double `Ready`, or `Ready` lost before a listener | AC4–AC6, AC8 | FACT: `signal_ready` writes again once recovery is armed (`app_session.rs:4601`); the link is lazy (`ensureLink`, `packages/@keld/api/src/app.ts`); WorkerLink drops an EVENT with no listener (`transport.ts:2329`). INFERENCE: no loss, because `open` resolves in its message task and `LifecycleLink.connect` registers in that task's microtask checkpoint, while records dispatch in a later `kick` task. AC4 is the proof. |
| A5 | Failure and timeout | KEL-96 §4.4 rules; keld-core coordinator | pre-`Ready` failure → typed startup failure; post-`Ready` → runtime rules | hung boot, or misclassified failure | AC7, AC11, AC14, AC15 | FACT (tao 0.35.3): the waker starts only in `launched` (`app_state.rs:300-301`), and `wakeup` returns while `!HANDLER.is_ready()` (`:319-325`). So proxy commands (`Quit`, `Fatal`) and `WaitUntil` deadlines are inert before `Init` |
| A6 | Renderer boots unchanged | KEL-96, KEL-139 | schema 1 → today's path | a changed descriptor, trigger or replay | AC2, AC10 | FACT: named tests above; gh531 criterion 15 |
| A7 | Recovered schema-2 generation | host: this spec; the no-second-window rule: the gh531 registry (#449) | g1 loss → g2 `HELLO` → `Ready` replay → `Subscribe` → `Created` replay → `Create` adopts | a second native window | AC8, AC9 | FACT: replay comes from the `window_ready` flag at install (`app_session.rs:5462`); gh531 D6 Option B for every boot kind, scope amended by #657 |
| A8 | Wire | keld-ipc | `LifecycleEvent::Ready` `0x00` | a new variant | none | FACT: doc "The host is ready for the app process to create windows / run." (`keld-ipc/src/lifecycle.rs:26`) |
| A9 | Platforms | keld-core parser per OS | schema 2 on Windows/Linux → refusal | hollow backend method; silent fallback | AC11 | FACT: KEL-96 §4.4 forbids a hollow `WebEngine` method; `KELD-CORE-034` is registered as "No-flag application boot is unavailable on this platform" |

Independence and the coupling they expose:

- **A2 ↔ A5 coupling.** Moving the trigger earlier also moves the recovery-arm point,
  because recovery is armed on the first successful `Ready` write (KEL-96 §4.4). This
  edge is explicit: D3 and D4 put every startup-fallible step before the trigger, so
  the arm point never precedes a startup failure.
- **A3 ↔ F02-T2.** This is an edge, not shared state. This spec fixes where the
  registry install sits and when its not-ready state ends. F02-T2 builds the registry
  and its test (AC16).
- **A7 ↔ gh531 D6 and #449.** These are explicit cross-spec edges. D6 owns host crash
  ownership of windows for every boot kind. The registry's `Create` adoption, which
  lands with #449, owns the no-second-window rule, keyed on registry state.

Reuse:

- **Wire.** Kept: lifecycle channel 3 and its replay rule. No new message.
- **Router.** Kept: `PrimaryRouterHandle::signal_ready` and its arm protocol. Only
  the event that calls it changes per boot kind.
- **Replay.** Kept: the `window_ready` replay at successor install. It already means
  "the initial `Ready` was written".
- **macOS UI loop.** One private owner, parameterized by its startup milestone. No
  copied closure.
- **Deadline.** One constant, `INITIAL_NAVIGATION_DEADLINE`, renamed
  `INITIAL_MILESTONE_DEADLINE` in T1 with the same value.
- **Window census.** The existing `NativeWindowObserver`.
- **Error codes.** Reused: `KELD-CORE-034` (platform), `035` (descriptor), `037`
  (session startup), `040` (registry not ready, gh531) and `KELD-CLI-047` (staging).
  One new code, `KELD-CORE-044`, takes the next number after gh531's reserved
  038–043, so F02-T2 cannot collide with it.

No rewrite. Compatibility fallback: `not required`. Schema 1 stays valid and is still
emitted for renderer boots, so no published descriptor changes meaning.

### 4.1 D1 — How a facade boot is declared

**Decision.** `keld.boot.json` schema 2 is a closed document:

```json
{
  "schema": 2,
  "name": "Example",
  "entry": "src/main.ts",
  "permissions": {
    "file": "keld.permissions.jsonc",
    "content_sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  }
}
```

- `schema: 2` is the declaration: the host creates no initial window, and the app
  creates every window.
- `renderer` is not a schema-2 field.
- All other rules are schema 1's: 64 KiB, UTF-8, duplicate keys, `name`, the relative
  `entry`, `permissions`.

The parser dispatches on `schema` with a typed probe (`struct SchemaProbe { schema: u8 }`),
then parses the version's own `deny_unknown_fields` struct. It never goes through
`serde_json::Value`, whose map keeps the last duplicate key. The schema-1 struct and
its checks are byte-unchanged. Both yield one `ParsedBoot`, whose internal
`initial: InitialWindow` is `Renderer(PathBuf)` for schema 1 and `None` for schema 2.

The project declares it with `initialWindow: "none"` in `keld.config.ts`. One keld-core
resolver beside `renderer_from_config_ts` reads it (`crates/keld-core/src/hello.rs:124`).
The boot compiler, `keld doctor`'s renderer check and `load_dev_window_html` all
consume that resolver instead of each defaulting `renderer` on its own. `keld migrate`
will write this field (out of scope). A later `compat: { electron: true }` must not
imply it separately: one fact, one key.

**Rejected:**

- *Optional `renderer` in schema 1, where absence means facade.* Absence would carry
  meaning: a dropped line silently changes window and crash ownership, and it changes
  schema 1's meaning without a version decision (KEL-96 §4.2).
- *A new field added to schema 1.* Same objection: v1 is closed by approval.
- *A single-valued `initial_window: "none"` field in schema 2.* `schema: 2` already
  says it. A one-value field is redundant, and it is a second place to keep in sync.
- *Schema 2 for every boot, with a v1 fallback.* This changes every renderer-boot
  descriptor and fixture (A6) and adds a second accepted renderer shape with no
  shipped v1 artifact to protect.
- *A schema-2 form that carries `renderer`.* It would be accepted but never emitted:
  dead surface (YAGNI). A later version can add it.
- *The host inspects `entry` for `@keld/electron`.* The host never evaluates app
  source (KEL-96-D3).
- *An environment variable or CLI flag.* This is the no-flag boot. A flag would be a
  second declaration channel (KEL-96-D3/D9).

**Falsifier.** A document that the parser accepts but cannot classify. Or a schema-2
document with `renderer` that parses. Or a schema-1 `ParsedBoot` that changes. AC1 and
AC2 cover all three.

### 4.2 D2 — Renderer-declared boots keep CreateInitialWindow before Ready

**Decision.** Yes, decision (b) is "keep". Schema-1 boots keep KEL-96 §4.4 steps 7–8
verbatim: `NavigationReady` triggers `Ready`, the descriptor bytes are unchanged, and
the run path is unchanged.

The gh531 "legacy teardown arm" is therefore the permanent schema-1 close path, not a
temporary adapter. Its owner is KEL-96, whose §4.5 defines schema-1 close and
`LastWindowClosed`. It is removed only by an approved spec that registers the schema-1
initial window in the gh531 registry or retires schema 1 (gh531 §4.c, amended by #657).

**Rejected:** *Move `Ready` for every boot.* It would change:

- KEL-96 AC11 for shipped renderer boots;
- the KEL-139 product fixture, which reads `READY` and then expects exactly one native window (`no_flag/macos/support/product.rs:217-256`);
- the Linux and Windows beacon-then-`READY` cycles;
- the recovery-arm point for a host window that is still navigating, which opens a pre-navigation recovery the KEL-139 AC5 continuity oracle does not cover.

The issue's INFERENCE that fixtures would need a window-specific event is confirmed by
`product.rs`.

**Falsifier.** Any AC10 test changes, or a schema-1 `Ready` is written before
`NavigationReady`.

### 4.3 D3 — What "host initialized" means

**Decision.** In a schema-2 boot, the host is initialized when all of these hold:

1. KEL-96 steps 1–6 succeeded, with `ResolveTargets` validating `entry` only.
2. The authenticated generation is installed in the router, and the router is attached.
3. Every window-call server is installed (D4).
4. The macOS UI loop has delivered the `Event::NewEvents(StartCause::Init)` turn on the
   UI thread. tao 0.35.3 issues that turn from `applicationDidFinishLaunching:`.

The backend emits `AppWindowEvent::HostInitialized` exactly once, from the `Init` match
arm only, never from "the first closure call". The reason is that tao's `open_urls`
delivers `Event::Opened` through the same callback with no ready check
(`app_state.rs:309`). So a launch URL can reach the closure before `Init`. The
coordinator maps `HostInitialized` to `signal_ready`.

**Launch URLs.** keld-wv's macOS loop has no arm for `Event::Opened` today, so a
launch URL delivered before or after `Init` is dropped. Their owner is #524 (F01-T8):
`open-url`/`open-file` delivery, with a pre-ready queue after `will-finish-launching`.
Until #524 lands, they are a recorded unknown. This spec only guarantees that `Opened`
never triggers `Ready` (AC7).

Electron's own documentation matches this point (v44.4.5 `app.md`):

- `will-finish-launching` "represents the `applicationWillFinishLaunching` notification of `NSApplication`";
- `ready` is "Emitted once, when Electron has finished initializing".

INFERENCE: Electron's macOS `ready` follows `applicationDidFinishLaunching:`. This spec
does not verify that against Electron source and does not depend on it.

**Rejected:**

- *`Ready` on `HELLO` acceptance (the literal issue wording).* Engine and loop creation
  would then run after `Ready`. Their failures would become post-`Ready` runtime
  failures, and recovery would be armed while the host cannot serve a window. That
  breaks KEL-96 §4.4's rule that a failure before `Ready` is a startup failure (AC5's NC).
- *`Ready` on the first closure call.* A pre-`Init` `Opened` would trigger it (AC7's NC).
- *`Ready` after the first app-created window navigates.* Deadlock: an Electron app
  creates windows only after `ready` (`browser-window.md`: "cannot be used until the
  `ready` event").
- *KEL-75's role `Ready(g)` acknowledgement.* KEL-75's `Ready(g)` is a role-to-host
  receipt. This spec's `Ready` is the KEL-72 host-to-app event. Conflating them changes
  direction and wire. KEL-75's meaning is untouched, and this spec infers nothing about it.

**Falsifier.** The role reads `Ready` while a startup-fallible host step is still
ahead (AC5, AC14), or on a pre-`Init` event (AC7).

### 4.4 D4 — What "window module available" means

**Decision.**

1. **Install before the loop.** Every host component that serves the app's window calls
   is installed before the schema-2 loop is entered.
2. **No window call before Ready.** The host refuses every window call until the first
   `Ready` write: a call admitted before that write gets `KELD-CORE-040` (registry not
   ready) and creates nothing.
   - The not-ready state ends in the same router transition that writes the first
     `Ready`, under the shutdown transition guard, as the recovery-arm enqueue does
     today. So a call sent in the task in which `whenReady()` resolves is served.
   - A successor generation cannot reach a window call before its `Ready` replay,
     because install replays `Ready` before it starts the reader (`app_session.rs:5462`).
   - The rule is the host's, keyed on the registry's state. The facade's own
     Electron-style refusal ("Cannot create BrowserWindow before app is ready") is not
     the guarantee, because a role can send the frame directly.
3. **A failed install is a startup failure.** It writes no `Ready`, calls
   `deny_recovery`, runs the KEL-96 steps 5–8 cleanup, and ends with `KELD-CORE-037`.
   The coordinator treats a closed event channel before `HostInitialized` as that
   failure, never as success. Today, `while let Ok(event) = events.recv()` ends quietly
   (`app_session.rs:3545`); schema 2 must not inherit that.

At this spec's landing, the set of window-call servers is the UI wake bridge (`Quit`,
`Fatal`), and no window call exists on the wire. So rule 2 has nothing to refuse yet,
and "census 0 at `READY`" holds structurally (AC4). F02-T2 adds the registry to the
pre-loop slot and implements rule 2 with its first window call (AC16; gh531 criterion
14 "Before Ready", amended by #657).

**Rejected:**

- *Facade discipline only.* A role that bypasses the facade could create a window
  before `Ready`, so census 0 would not be a host fact.
- *A registry-ready handshake before `Ready`.* The registry does not exist yet; a
  handshake now would be a hollow seam.
- *A second lifecycle event, "window module ready".* That is a wire change, and
  Electron has one `ready`.

**Falsifiers and NCs:**

- A `Create` before the first `Ready` write that does not get `KELD-CORE-040` (AC16).
  NC: marking the registry ready at construction serves it.
- A failed install that writes `Ready` (AC14). NC: signalling before the install result
  writes it.
- gh531 criterion 14's NC: a registry started after `Ready` refuses the first `Create`
  with `KELD-CORE-040`.

### 4.5 D5 — Ordering against HELLO and the first whenReady

**Decision.** Generation 1 of a schema-2 boot runs in this order:

1. The main evaluates.
2. Its first `app` lifecycle call (`whenReady`, `on('ready')` or `quit`) opens the lazy
   link (`ensureLink`).
3. `HELLO` is authenticated.
4. The generation is installed.
5. The window-call servers are installed, the engine is created, and the loop is entered.
6. The `Init` turn fires `HostInitialized`.
7. One `Ready` write. In the same transition, the registry's not-ready state ends (from
   F02-T2 on). Then the recovery arm runs (unchanged protocol).
8. The link dispatches `Ready` in a later task, and `onHostReady` resolves the waiters.

So `ready` always follows the main's synchronous top level. This matches Electron's
note: "`ready` is only fired after the main process has finished running the first
tick of the event loop".

Generation g≥2 receives `Ready` from the existing install replay, once, before its
reader starts.

Once-ness has one owner, the coordinator. Each boot kind has one `ReadyTrigger`. A
foreign or repeated trigger is a host invariant fault: `KELD-CORE-044`, then `Fatal`.
It is never ignored.

**Rejected:**

- *Rely on backend discipline alone.* After F02-T2, app-created windows navigate.
  If one of them ever emitted `NavigationReady`, `signal_ready` would write a second
  `Ready`.
- *Make `signal_ready` silently idempotent.* That swallows a fault, which root
  AGENTS.md forbids.

**Falsifier.** AC4 sees a lifecycle frame before `Ready`, or `whenReady` pending
after the fence (lost `Ready`). AC6 sees a second `Ready`.

### 4.6 D6 — Failure and timeout behaviour

**Decision.**

- **Before `Ready`.** KEL-96 §4.4's rules are unchanged. Any failure is a typed startup
  failure with one attempt and no successor; a schema-2 boot never has a window to
  close. The `HELLO` deadline is unchanged (`APP_LINK_IO_DEADLINE`, 5 s). So a main
  that never touches the `app` lifecycle within 5 s fails startup with today's typed
  error. Whether `@keld/electron` should connect eagerly is the facade's decision, not
  this spec's.
- **Waiting for `Init` is bounded.**
  - **The wait.** The coordinator waits for `HostInitialized` with `recv_timeout`. The
    deadline is set when `run_app` enters the schema-2 loop, the same moment the
    schema-1 navigation deadline starts today.
  - **The bound.** `INITIAL_MILESTONE_DEADLINE`, 15 s: today's
    `INITIAL_NAVIGATION_DEADLINE`, renamed in T1 with the same value. One constant
    bounds the first host milestone of both boot kinds. Its documented sizing rule
    (`keld-wv/src/lib.rs:30-37`) is at least 3× margin over every observed healthy
    start, "while every test harness that waits for `Ready` allows longer". Schema 2's
    milestone (`Init`) is a strict prefix of the work schema 1's milestone already
    covers (`Init`, then navigation). So 15 s cannot cut short a start that schema 1
    accepts.
  - **On expiry.**
    1. Return `KELD-CORE-037` with the detail "the UI loop did not deliver its first
       turn within 15 s". No `Ready` is written.
    2. Deny recovery, then revoke the generation, close the link and endpoint, and
       terminate and reap Bun. Router and guardian threads own all of these; none needs
       the UI thread.
    3. Call the host terminal function once.
  - **Why the UI thread cannot do it.** tao delivers proxy commands only after
    `launched` sets ready (A5), so `Fatal` would wait for the very `Init` that never
    came. Before `Init`, a schema-2 boot holds no UI-owned resource, so nothing is left
    for the UI thread to tear down.
  - **The terminal function.** keld-host passes its existing failure tail into
    `run_guarded`: print the error, remove the dev stage, `exit(1)`
    (`crates/keld-host/src/main.rs:116-133`). Process exit stays in the binary.
- **Schema-1 exposure before `Init` (not changed here).** UNKNOWN: whether AppKit can
  withhold `applicationDidFinishLaunching:` indefinitely. Per A5, schema 1's in-loop
  navigation deadline is inert before `Init`. The only exception is a non-user event
  such as `Opened`, which runs the closure early. That is a FACT read from tao source,
  and no test exercises it. Recorded with owner KEL-96.
- **A failed install or a failed `Ready` write.** Both are startup failures with
  `deny_recovery` (D4 rule 3; today's write-failure path).
- **After `Ready`.** A session with zero windows is valid and has no timer, as in
  Electron (tray-style apps). It ends by Quit, by host death, or by a role exit under
  the KEL-96/KEL-116 runtime rules. The milestone deadline is disarmed at `Init` (AC7).

**Rejected:**

- *No bound on `Init`.* A boot could hang with no typed outcome.
- *The in-loop deadline as the bound.* It is inert before `Init` (A5).
- *`process::exit` inside keld-core.* Exit authority belongs to the binary, and it
  would skip keld-host's dev-stage cleanup.
- *A "first window within N seconds" timer.* Electron has none, and a timer would kill
  valid windowless apps.

**Falsifiers, each with a test:**

- A schema-2 boot that hangs before `Ready` with no typed outcome (AC15).
- A healthy schema-2 session ended by a host timer after `Init` (AC7 unit).
- A failed install that writes `Ready` (AC14).

### 4.7 D7 — Recovered schema-2 generations

**Decision.**

- **Host side, decided here.** The host rule is the same for every boot kind. Recovery
  is armed after the first successful `Ready` write, and g≥2 gets `Ready` replayed
  after its fresh `HELLO`. In a schema-2 boot the host creates no window in any
  generation on its own (AC8). This is gh531 D6 Option B, which sets the host crash
  ownership of windows for every boot kind and needs a successor for its handover
  (criterion 30). It is consistent with KEL-75, whose `primary` owner stops at session
  stop, not at a window.
- **The invariant (Agent Brief):** a recovered generation must never produce a second
  window for a document the lost generation opened. The gh531 registry owns this rule,
  and the facade only complies (gh531 D6 scope, amended by #657):
  - A recovered generation's `Create` never creates a native window while a
    transferred window is unclaimed. Instead it adopts the oldest unclaimed window and
    returns that window's existing pair and replayed state.
  - The constructor binds to that pair and emits `browser-window-created` once.
  - Bounds and title come from the replay.
  - Renderer-authority options (`webPreferences`, after #455's triage) must match, or
    the `Create` is refused with a typed error.
  - The rule keys on registry state, never on boot kind. Renderer boots never populate
    the registry (gh531 criterion 15).
  - The app owns navigation, so an adopted window's document survives only until the
    recovered main navigates.

  Transfer, the `Created` replay at `Subscribe` (criterion 30 without `pending_close`)
  and `Create` adoption land with #449 (gh531 T1). `pending_close` and closes during
  the recovery gap stay with #450 (gh531 T2).
- **The compat cell.** `app.ready.recovered-generation` is `unknown` (AC9). It is
  uncited, and its NC names #449's keld-core adoption test.
- **Scoring fact, which carries no weight here.** The first proof scores only "install
  + activation + a 4-step primary_workflow" (#432), so no scored path crosses
  generation loss. The invariant holds regardless.

This spec is safe in its own landing window. No `Create` exists until #449, so no
schema-2 generation can have a window.

**Rejected:**

- *A boot-kind-scoped crash rule:* schema-2 boots that do not arm recovery, so role
  loss ends the session. Criterion 30's transfer and replay would then have no
  consumer, and the host would carry two lifecycle policies keyed on the descriptor.
  It would also give up supervised recovery for every facade app. Electron 44.4.5 has
  no main-gone semantics to copy, so "Electron-faithful" would only mean that the app
  dies.
- *A facade-owned no-second-window rule.* It cannot be unit-tested without a GUI host,
  and a host refusal beside it would give one policy two owners.
- *Suppressing a same-URL `loadURL` after adoption.* It changes Electron semantics, and
  it stalls `did-finish-load` waiters.
- *X06-D1(c)'s session-ending default.* It was a research correction proposal that was
  never filed (it is absent from the adopted decisions), so it cannot override the
  owner's later D6. It is superseded.
- *Withhold `Ready(g2)`.* The successor's `whenReady` would hang with no typed outcome.

**Falsifiers:**

- A host-created window in any schema-2 generation (AC8).
- AC8's census exceeds the pre-crash count once #449 lands.
- A keld-core registry test with a fake UI port yields two windows. The NC deletes the
  adoption arm (#449).
- `NativeWindowObserver` shows a new window identity after facade recovery. This one
  backs the PROPOSED gh531 adoption sentence (§10).
- gh531 D6's own falsifier: a successor cannot safely adopt a window it did not create.

### 4.8 D8 — Wire

**Decision.** None. `LifecycleEvent::Ready` stays `0x00` on channel 3, and its doc
already reads "ready for the app process to create windows / run". For schema-2 boots
the host never writes `LastWindowClosed`; window-all-closed belongs to the gh531
registry ("For facade boots the existing `LastWindowClosed` variant is not used",
gh531 §4.i Public surface).

**Rejected:** *A new `HostInitialized` lifecycle variant.* It has no consumer, and
Electron has one `ready`.

**Falsifier.** A design step that needs a new frame.

### 4.9 D9 — Platforms

**Decision.** macOS implements schema 2. On Linux and Windows, the descriptor is valid
but the platform cannot run it, so `ResolveBoot` refuses it with `KELD-CORE-034` ("No-flag
application boot is unavailable on this platform"). That happens before any
application resource exists (KEL-96: "Failure in steps 1–3 leaves no application
resource"). `KELD-CORE-035` remains for invalid descriptors.

No `WebEngine` trait method is added; the new run method exists only on
`WkWebViewEngine`. The one new cell, `app.ready.recovered-generation`, declares macOS.
On Windows and Linux it is an unrun lane, reported `unknown` (gh566 D13).

**Rejected:**

- *Implementing Windows and Linux now.* Out of the first-proof milestone, and their
  emission points are unknown.
- *A trait method with stub backends.* KEL-96 §4.4 forbids hollow methods.
- *`KELD-CORE-035` for the refusal.* The document is valid; 035 means "invalid descriptor".

### 4.10 Amendment text (exact content for owner approval)

**KEL-96 §4.1.** Append inside the decision block. This changes `decision_digest`:

```text
| `KEL-96-D12` | `keld.boot.json` schema 2 declares a boot whose app creates every window: closed fields `schema`, `name`, `entry` and `permissions`, and no `renderer`. Its `Ready` follows authenticated `HELLO` and the UI loop's `Init` turn; the host refuses window calls before that `Ready` and creates no window in any generation on its own. Schema 1 is unchanged and keeps `CreateInitialWindow` before `Ready`. Schema 2 runs on macOS only; Windows and Linux refuse it with `KELD-CORE-034` before any application resource (GH-446). |
```

**KEL-96 approval metadata.** Amend in place:

- **Header `Decision digest:`.** Replace it with the digest T1 recomputes over D1–D12,
  using the §4.1 rule and the command in this spec's §7.
- **§2 provenance.** "The eleven selections below" reads "The eleven original
  selections below (D1–D11)". After the provenance list, append:

  > **Amendment GH-446 (D12).** `KEL-96-D12` was added by GH-446 (spec #657, implementation T1). Its exact-content approval is recorded on that implementation PR's review. The header digest covers D1–D12 under the §4.1 rule; the `decision_digest` in the list above remains the value the original review approved for D1–D11.

- **§7 "Decision completeness".** "Exactly one `KEL-96-D1` through `KEL-96-D11`" reads
  "Exactly one `KEL-96-D1` through `KEL-96-D12`".
- **§10.** "The eleven architecture decisions are frozen" reads "The twelve
  architecture decisions (D1–D11, and D12 added by GH-446) are frozen".

**KEL-96 AC1.** Replace with:

> A valid `keld.boot.json` is a bounded, strict document of schema 1 or schema 2.
> Schema 1's exact closed fields are `schema`, `name`, `entry`, `renderer`, and
> `permissions`; schema 2's are `schema`, `name`, `entry`, and `permissions`.
> `permissions` contains only `file` and `content_sha256`. Duplicate or unknown
> fields, an unknown schema, non-UTF-8 input, or input over 64 KiB is rejected.

**KEL-96 AC8.** Append:

> This criterion is for schema 1. Given a valid schema-2 descriptor on macOS, launching
> `keld-host` with no diagnostic flag starts the Bun `entry` and runs the event loop with
> no native window until the app creates one (GH-446).

**KEL-96 AC11.** Replace with:

> Startup follows §4.4's state machine. Lifecycle `Ready` is emitted only after
> authenticated `HELLO` and, for schema 1, initial-window registration/navigation
> readiness, or, for schema 2, the UI loop's `Init` turn; it is never implied by
> connection or handshake.

**KEL-96 §4.4 step 2.** Read "canonicalize/open `entry` and, for schema 1, `renderer`".

**KEL-96 §4.4 step 7.** Replace with:

> 7. `CreateInitialWindow` (schema 1): create/register the window and finish its
>    initial renderer navigation on the UI thread. `StartLoop` (schema 2): install every
>    host component that serves the app's window calls, enter the UI event loop with no
>    window, and wait, bounded by the initial milestone deadline, for its `Init` turn on
>    the UI thread. Window calls are refused until step 8.

**KEL-96 §4.5.** Append:

> For schema-2 boots the host creates no window, so step 1 is not this coordinator's:
> window counting and `window-all-closed` belong to the gh531 window registry, and the
> host writes no `LastWindowClosed`. Steps 2–5 apply unchanged.

**KEL-139 "Recovery".** Append:

> This spine is a schema-1 (renderer-declared) boot. A schema-2 boot (GH-446) has no
> host-owned window or document; its `Ready` follows authenticated `HELLO` and the UI
> loop's `Init` turn. Its recovered-generation window behaviour is owned by gh531 D6
> (window crash ownership) and the gh531 registry's `Create` adoption (#449); the
> document survives until the recovered main navigates; AC5's document half is not the
> host's claim there.

Falsifier for that sentence: AC8's census exceeds the pre-crash count once #449 lands.

**KEL-139 AC3.** Append:

> The same `Ready` gate holds in a schema-2 boot, where host `Ready` precedes every window.

**Architecture 02, no-flag primary paragraph.** Replace "Initial `Ready` follows native
renderer navigation;" with:

> Initial `Ready` follows native renderer navigation in a schema-1 boot and the UI
> loop's `Init` turn in a schema-2 boot, which has no host-created window (GH-446);

**Architecture 06 §1, macOS no-flag primary.** Append:

> A schema-2 boot (GH-446) creates no native window: the same router writes `Ready` on
> the UI loop's `Init` turn, and a successor generation still receives `Ready` after its
> fresh HELLO.

**`docs/engineering/keld-error-codes.md`.**

- `KELD-CORE-034`: append to its fix, "For a schema-2 descriptor on Windows or Linux,
  remove `initialWindow` from `keld.config.ts` to boot a renderer window."
- `KELD-CORE-035`: the message reads "The private schema-1 or schema-2 boot descriptor
  is invalid", and the fix reads "Remove duplicate or unknown fields and regenerate a
  bounded strict `keld.boot.json` of schema 1 or schema 2."
- New `KELD-CORE-044`. Crate keld-core. Message: "A UI startup milestone arrived that
  this boot kind does not expect, or arrived twice". Fix: "This is a host defect;
  report it with the host log, then relaunch the staged host."

### 4.11 Types, platform notes, runtime seam, migration

Types (sketch):

```rust
// keld-core, private
enum InitialWindow { Renderer(PathBuf), None } // from schema 1 or schema 2
struct ParsedBoot { name: String, entry: PathBuf, initial: InitialWindow, permissions_digest: [u8; 32] }
enum ReadyTrigger { InitialNavigation, HostInitialized } // chosen from InitialWindow
fn coordinate_window_events(
    events: &Receiver<AppWindowEvent>,
    router: &PrimaryRouterHandle,
    commands: &Sender<AppWindowCommand>,
    trigger: ReadyTrigger,
    first_milestone_deadline: Instant,
    terminal: fn(HostAppError) -> !,
) -> Result<(), HostAppError>;

// keld-core, public: keld-host passes its failure tail
pub fn run_guarded(boot: ValidatedBootSelection, terminal: fn(HostAppError) -> !) -> Result<(), HostAppError>;

// keld-wv, public
pub enum AppWindowEvent {
    NavigationReady,
    LastWindowClosed,
    /// The UI loop delivered its `Init` turn on a backend that created no initial
    /// window (macOS: tao `StartCause::Init`, from `applicationDidFinishLaunching:`).
    /// Emitted at most once per process, never for a pre-`Init` event.
    HostInitialized,
}
impl WkWebViewEngine {
    /// Runs the UI loop with no host-created window until Quit or Fatal (GH-446).
    pub fn run_app_until_quit_without_window(
        &mut self,
        commands: Receiver<AppWindowCommand>,
        events: Sender<AppWindowEvent>,
    ) -> Result<(), WvError>;
}
```

`run_app_until_quit` and the new method share one private loop body, parameterized by
the startup milestone: initial navigation with its deadline, or `Init`. Under schema 2,
`run_app` skips the renderer read, the renderer dispatch thread and
`create_app_with_renderer_bridge`. Bridges for app-created windows are F02's.

- **Capabilities and manifest:** none. `keld.permissions.jsonc` and its digest rule are unchanged.
- **Wire:** none (D8).
- **Platforms:** macOS implements. Linux and Windows refuse with 034 (D9).
- **Runtime seam:** the execution owner is unchanged: keld-core owns startup, keld-wv
  owns the UI loop, and keld-host owns process exit. There are no new OS grants. The
  crash domain is unchanged; only the recovery-arm moment moves, and only for
  schema 2 (D3).
  - **Callback lifetime:** the backend emits `HostInitialized` from inside the loop
    closure's `Init` arm, through the existing `events` sender. The terminal function
    is a `fn` pointer with no captured state.
  - **Configuration** is captured once at `ResolveBoot`.
- **Migration unit:**
  - **Callers:** `run_app` (macOS), `parse_boot_bytes`, and `keld-host` `main` (the `run_guarded` terminal argument).
  - **Handlers:** `coordinate_window_events`.
  - **Config resolvers:** keld-cli `boot.rs`, `doctor.rs` and `dev.rs` consume the one
    resolver.
  - **Persisted state:** none.
  - **Temporary adapter:** none. The gh531 legacy teardown arm is the permanent
    schema-1 close path (D2).
  - **Compat facade:** `@keld/electron` is unchanged. Its `ready`/`whenReady`/`isReady`
    simply observe the earlier `Ready`.

## 5. Boundaries

**Implement in:**

- `crates/keld-core/src/app_session.rs`: the parser, `run_app`, `run_guarded`, the coordinator, and tests.
- `crates/keld-core/src/hello.rs`: the config resolver.
- `crates/keld-host/src/main.rs`: the terminal function.
- `crates/keld-cli/src/{boot,doctor,dev}.rs`.
- `crates/keld-wv/src/engine.rs`, `crates/keld-wv/src/lib.rs` (the deadline rename) and `crates/keld-wv/src/wkwebview/mod.rs`.
- `crates/keld-host/tests/no_flag/{macos,linux,windows}/`.
- `crates/keld-compat/fixtures/app-corpus/` (the one `unknown` cell) and `packages/@keld/electron/src/app-surface.test.ts` (its mapped today-test).
- `docs/engineering/keld-error-codes.md` (034, 035, 044).
- The spec and architecture files in §4.10.

**Must not touch:**

- `keld-ipc` wire and channel tables.
- `packages/@keld/api` and `packages/@keld/kipc` behaviour.
- The webview2 and webkitgtk backends.
- The frozen `electron-lifecycle-v0` corpus and its receipts.
- The existing `electron-app-v1` cells (#656).
- gh531 registry code (F02-T2).
- Workspace `Cargo.toml` (no new dependency).

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 (GH-446, after #445 lands): one vertical slice covering AC1–AC15. AC16 is
  F02-T2's test of this spec's rule. T1 contains:
  - the schema-2 parser and resolver;
  - the compiler;
  - the macOS milestone loop and `HostInitialized`;
  - the coordinator trigger, the bounded `Init` wait, the install-failure path and
    `KELD-CORE-044`;
  - the keld-host terminal function;
  - the Linux and Windows refusal with 034;
  - the `app.ready.recovered-generation` cell;
  - the tests;
  - the §4.10 text, the error-code entries and the recomputed KEL-96 digest.

  The descriptor cannot land without its consumer: KEL-96-D6 rejects a standalone
  descriptor as theater.

## 7. Test plan

| AC | Test (proposed name), kind | Anti-flake |
|---|---|---|
| 1, 2 | `schema_two_declares_no_initial_window`, `schema_two_rejects_renderer_and_unknown_fields`, `schema_one_still_requires_renderer`; unit (keld-core) | pure parse |
| 3 | `initial_window_none_stages_schema_two_without_renderer`, `initial_window_none_with_renderer_is_a_staging_error`; unit (keld-cli); `stage_is_random_owner_private_new_inode_and_byte_consistent` unchanged | temp dirs |
| 4, 7 | `schema_two_ready_precedes_every_window_and_quit_is_ordered`; macOS no-flag integration with `NativeWindowObserver`. `init_arm_alone_emits_host_initialized` (with the pre-`Init` `Opened` case) and `init_disarms_the_milestone_deadline`; keld-wv unit | census taken after the `READY` line; no sleep; constructed tao events and a past `Instant` |
| 5, 6 | `schema_two_ready_waits_for_host_initialized`, `foreign_or_repeated_ready_trigger_is_fatal`; keld-core router unit | echo-fence ordering, bounded by frames, not time |
| 8 | `schema_two_successor_reads_ready_once_and_host_creates_no_window`; macOS recovery integration | existing recovery support; fence after `READY` |
| 9 | cell `app.ready.recovered-generation` (`unknown`, uncited) → a Bun today-test in `app-surface.test.ts`; its NC names #449's `successor_create_adopts_the_oldest_unclaimed_window` | gh566 admission; fresh role process, no timing |
| 10 | the named existing tests; `lifecycle_corpus_rust_oracles_execute` | unchanged |
| 11 | `schema_two_descriptor_is_refused_before_any_application_resource`; Linux and Windows no-flag integration (CI) | resource census like `windows_pre_ready_crash_denies_successor_before_provisioning` |
| 12 | `app.ready.emitted-once` and `app.when-ready.is-ready-agreement` (`electron-app-v1`, #656) and `app.when-ready.host-ready-gate` run unchanged; "before any window" is AC4's keld-host test | unchanged cells |
| 13 | review of the T1 diff against §4.10 verbatim. The KEL-96 header digest equals the output of `awk '/KEL96_DECISIONS_V1_START/{f=1;next}/KEL96_DECISIONS_V1_END/{f=0}f' docs/specs/kel96-no-flag-host-boot.md \| shasum -a 256` (this command reproduces today's `053ad0c4…` digest at `630457ec`) | deterministic bytes |
| 14 | `closed_event_channel_before_host_initialized_is_startup_failure`; keld-core router unit | echo fence; dropped sender, no timing |
| 15 | `init_wait_expiry_is_typed_startup_failure`; keld-core router unit with a terminal test double | past deadline; bounded join on the result channel |
| 16 | `create_before_ready_is_refused_with_040` in F02-T2 (#449) | registry unit, no GUI |

OS acceptance: macOS is real (on a physical Mac). Linux and Windows are covered by
AC11 in CI only. No claim is made for unrun lanes.

## 8. Review gates triggered

- **unsafe:** none.
- **public API:** yes.
  - `keld.boot.json` schema 2, which also triggers the manifest-schema gate (KEL-96-D11).
  - The `keld.config.ts` field `initialWindow`.
  - The keld-wv items `AppWindowEvent::HostInitialized` and `WkWebViewEngine::run_app_until_quit_without_window`.
  - The keld-core `run_guarded` terminal parameter.
  - The error-code registry entries.
  - Electron-visible timing of `ready`/`whenReady`/`isReady` in schema-2 boots.
- **permission model:** yes by routing only. KEL-96-D11 sends every `keld.boot.json`
  change here; no capability, policy or guard path changes.
- **dependency:** none.
- **wire protocol:** none.

## 9. Perf impact

- **Renderer boots:** none.
- **Schema-2 boots:** `Ready` moves earlier by the initial-navigation time. The first
  window moves later, by Bun module evaluation plus one `Create` round trip (gh531 §9).
  The architecture 01 §5 "cold start → first paint" row can move for facade boots; it
  is measured under X05-T1's registered metric. This spec makes no performance claim.
- **Other consumers:** none on current platforms. INFERENCE: the KEL-270 updater
  health gate, which requires that the app "reached application `Ready`", would mean
  "role authenticated and host loop live" for a schema-2 boot. That gate is
  Windows-only today, and Windows refuses schema 2.

## 10. Open questions

1. **Approval of the PROPOSED gh531 adoption sentence (repository owner, exact
   content).**
   - **The sentence.** In the gh531 Boundaries adoption line: "Adoption (D6): no
     KEL-139 amendment. AC5's same-window half holds in every boot mode; its
     same-document half holds where the host owns navigation (schema 1) and, in facade
     boots, until the recovered main navigates."
   - **Why the owner must approve it.** The owner chose D6 Option B on the wording
     "AC5 holds for every boot mode". This sentence narrows AC5's same-document half,
     so it needs the owner's explicit approval of the exact content.
   - **The KEL-139 text.** The "Recovery" amendment text in §4.10 states the same
     narrowing, so it needs the same approval.
   - **Falsifier:** `NativeWindowObserver` shows a new window identity after facade
     recovery.
   - **Recommendation:** approve. In facade boots the app owns navigation, so the host
     cannot keep a document the recovered main replaces. The same-window half is kept
     in full.

Decided on 2026-10-08 under the owner's delegation, and recorded here:

- **Compat main-role loss.** gh531 D6 Option B holds for every boot kind.
  - The gh531 registry owns the no-second-window rule, keyed on registry state, and
    the facade complies (§4.7; gh531 D6 scope, amended by #657).
  - The rule lands with #449 (T1), and `pending_close` with #450 (T2).
  - X06-D1(c) is superseded. An earlier KEL-143 attribution was wrong: KEL-143 is
    KEL-139 T3, host continuity.
- **The issue's AC1.** It is restated as AC12, and no `app.ready.no-host-window` cell
  is added.
  - #656 records both cited cells as `pass`, so nothing flips.
  - gh566 admits only keld-compat libtest and `packages/**/*.test.ts` targets, and its
    D8 rejected Rust oracle targets in other crates. That routing edge belongs to
    gh531's AX-click real-Mac cells (gh531 §4.i D3), not to this spec.
