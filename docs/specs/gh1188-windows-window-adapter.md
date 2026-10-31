# Spec: Windows window adapter — Host Ready, BrowserWindow and vetoable Win32 close (W2)
Status: draft
Linear: GH-1188 (parent #1182, program #517) · Owner: GYLDLAB · Updated: 2026-10-31 (r18: `WindowEvent::Destroyed` only stages a release, and the answer is published on a later closure run after `IsWindow(hwnd)` reads FALSE (D6, AC24); the per-view host obligations are one owner over every view: `NotifyParentWindowPositionChanged` on `Moved`, `SetIsVisible` across minimize, focus return on `Focused(true)` (D10, D11, A26, AC26); events buffered during another window's controller pump get exact-head cells (AC25); the release proof gets a negative on every boot and release kind, and its scope is limited to `run_app_until_quit` (D7, AC27); AC21 NC 4 (a) splits by D8 (2)'s order and (j) gains its no-entry mirror; prior evidence is marked as such (§2); test helpers and `first_host_init_turn` move to the first task that consumes them (§6); a failed publish send takes the Fatal path (D6); two new debug hooks await the owner (U18); r17: product admission of a Windows schema-2 boot lands in T2 with A21 and D7, and T1 runs only behind a debug test admission (D2, AC23); T3's derived `Some(true)` is blocked until the native fault cells of the now-approved U7 fixture pass on real Windows at its exact head; `poll` keeps draining abnormal receipts after the current instance's normal credit (AC21 NC 4 (l)), so the abnormal-exit latch covers every receipt drained before the loop's last exit check; r16: the release proof lands in T2, before any managed window, and `Some(true)` is derived from the engine's support and the injected release witness; AC16 NC 4 keeps only an admitted `Quit`; a browser `ProcessFailed`, or an abnormal `BrowserProcessExited` of any of the session's browsers, latches `AbnormalExit`, and no rebind follows the latch (W1 #1307 Q13 a); every negative control and task re-checked against the whole spec, pre-empted controls re-targeted; r15: every created controller binds its browser instance, failed builds included; checked exit-wake retirement; the loop exits only with an empty `releasing` map)

**Standing.** This is a Windows amendment to the approved shared contract
`docs/specs/gh531-window-registry-close-state-machine.md` (gh531). It changes none of
gh531's shared rules (identity, wire, mirror, close state machine, triage, guard). It
states only the Windows adapter deltas, each naming the gh531 section it plugs into
(§4.a). It builds on #1181 (macOS facade), merged 2026-10-28 as `41b81ecb` (final head
`d264cc51`), and on the #1073 Windows Quit-tail fixes (#1279, #1282; #1073 closed
2026-10-28). Every code and spec citation below is at `main`
`457a9c3b37f71179f167bd1f8c706c461cc4c479`: r6 re-mapped all 227 by content match, 225 by
script and 2 by hand (the r6 reconciliation bundle, §2). No source change and no production claim
accompany this draft. Claim: issue comment
<https://github.com/gyldlab/keld/issues/1188#issuecomment-6459160769>.

Evidence vocabulary: **FACT** = read from the cited source at the cited head;
**INFERRED** = derived, not yet observed; **UNKNOWN** = decided only by the named probe.

## 1. Goal & non-goals

On Windows today a schema-2 (rendererless, `initialWindow: "none"`) boot is refused
with `KELD-CORE-034` (FACT, `crates/keld-core/src/app_session.rs:1340-1351`, called
from `validate_windows_boot_files` at `:1447`; `run_app_direct` repeats the same
refusal inline at `:3431-3438`); the session is built with `window_native_support:
None` (FACT, `app_session.rs:7613-7615`), so a window `Subscribe` answers
`SubscribeRefused { NotSubscribable }` (FACT, `main`
`crates/keld-core/src/app_session/window_router.rs:387-395`) and a window `CALL`
answers `KELD-CORE-045` (FACT, `window_router.rs:451-458`; `KELD-CORE-042` is reached
only with `Some(false)`, `:475-478`, which nothing sets today); the WebView2 loop treats
any registry command as an invariant fault (FACT, `main`
`crates/keld-wv/src/webview2/mod.rs:3330-3343`, `:3388-3394`); and the live Windows
loop drops the view on every `CloseRequested` and emits `LastWindowClosed` at once
(FACT, main `webview2/mod.rs:3355-3370`, `:2816-2834`). A close cannot be cancelled.

Outcome: a verified schema-2 Windows boot publishes Ready only after the real Tao
loop's first Init turn with zero native windows; `new BrowserWindow(<draw.io options>)`
builds one Win32 window plus WebView2 controller whose `id` is the host WindowId and
whose native outer frame matches the request; a `WM_CLOSE` becomes `'close'` for the
correct `(WindowId, WindowGeneration, close_seq)`; a `preventDefault()` keeps the HWND,
the controller's child HWND, the registry entry and the profile lease live for as long
as the app wants; the window is released exactly once on `Allow` or `destroy()`; after
the last window the pinned app's own `app.quit()` ends the process, every WebView2
descendant is gone, the profile lease is released, and a second fresh launch is healthy.
Shared registry behaviour is proved by gh531's tests; this spec proves only the Windows
seams, on a real Windows 11 device, with the issue's two negative controls.

Non-goals (owners in parentheses):

- Native dialogs and the Save/Discard/Cancel prompt (W5 #1189). draw.io's dirty close
  calls `dialog.showMessageBoxSync` with Windows buttons Save=0, Discard=1, Cancel=2
  (FACT, drawio-desktop `src/main/electron.js:869-876` at `2edf9fb9`); the standalone
  fixture here vetoes/allows without any dialog.
- Preload, context isolation, bridge (W1 #1184). File-origin navigation, `loadURL`,
  `executeJavaScript` results (W3 #1185): registry windows here load an empty document.
- Screen work-area and theme facts (W4 #1186): integration input only. draw.io reads
  `nativeTheme.shouldUseDarkColors` (`electron.js:717`) and display work areas
  (`electron.js:740-741`) before its first window; the ordering contract is #481/W4's.
  This spec fixes only where those facts join the readiness barrier (A22).
- Strict/Legacy profile admission, descendants custody, LPAC (W6 #1187; KEL-210 has only
  T1 qualification approved, `docs/specs/kel210-windows-lpac-primary.md:3-9`). The
  primary's process profile (ordinary token, no LPAC) is unchanged; only its argv gains
  the transport preload (D13).
- Menus, Windows chrome parity, `BrowserView`, multiple views per window, macOS or
  Linux behaviour changes, installed-mode profile handover with registry windows (U8).
- WebView2 host surfaces outside W2's parity, each with its owner (the full walk is A26):
  `add_AcceleratorKeyPressed` (W2 has no menu; the macOS menu is #480, no Windows menu
  issue was found, program owner #1182); `add_MoveFocusRequested` and `put_ParentWindow`
  (multiple views per window, above); page `window.close()` (denied until KEL-102, F04-A1,
  gh531 `:1245-1246`); `backgroundColor` (F02-T5, gh531 `:1218`); `put_ZoomFactor` and
  `TrySuspend` (no W2 surface; owner: the spec of any later zoom or suspend API).
- Any change to gh531's shared rules, wire types, channel table or `@keld/*` facade
  logic. A Windows-specific TypeScript change is a blocker, not a delta (§5).

## 2. Spec refs

- gh531 §1 non-goals (`:28-31`), §4.b error table (`:701-708`), §4.c window #1
  (`:754-772`; close path is per boot kind, `:764-765`), §4.e transition table and T11
  (`:976-1091`), teardown order and run-loop rules (`:1155-1164`), public keld-wv surface
  (`:1421-1467`), platform notes (`:1479-1486`), boundaries (`:1533-1535`). #1181's
  amendments, now in gh531 on `main`: "No unbounded rendererless Init" (the paragraph at
  `:268`: 15 s bound, `KELD-CORE-037` terminal, unproved dev stage not deleted) and the Bun
  boot-mode and sidecar preload rule (`:821-831`; rewritten by 9e00c273 as "a bounded bug fix
  to the approved boot-selection invariant").
- `docs/architecture/01-overview.md` crate table; `02-ipc.md` (#1181 diff: window channel
  id 5, no frame or `HELLO` change); `03-security.md`; `04-electron-compat.md` (#1181
  sentence "Linux and Windows currently refuse facade-window subscription");
  `05-webview-and-native.md` §3 "Window ownership" (`:302-304`).
- `docs/specs/kel96-no-flag-host-boot.md`; `kel259-windows-test-layout.md` (test
  families; `support/window.rs` owns HWND observation); `kel135-persistent-profile-identity.md`
  (profile lifecycle record owner; A21 reads its "store/context attached"); `kel254-windows-installed-root-boot.md`
  (installed/persistent harness); KEL-53 §4 handover (`crates/keld-wv/src/engine.rs:93-99`).
- Landed dependency: #1073 (closed 2026-10-28). The named-pipe close that keeps unread
  answers (#1279) and the Windows post-Quit drain (#1282) are on `main`, so AC14's Quit
  tail rests on them; tests still report, never retry.
- Published evidence below is pinned at `0monish/keld-research@f3f21cf` (PR #35, open), except the
  r18 probe, which names its own later pin. Directories
  with more than 32 files are re-sharded under `part-NN/` there, so a bundle's
  `original-SHA256SUMS` lists original paths; `manifest.json` in the run lists every published
  path with its sha256 and `original_path`.
- Native probe evidence ("the U1/U6 bundle"): published in `0monish/keld-research` PR #35
  (note 265 E1), run `artifacts/runs/gh1188-webview2-release-custody-2026-10-29/`, bundle `u1u6/`, whose own `SHA256SUMS` is kept there as
  `original-SHA256SUMS` (`results.json` sha256 `0032b352…fb45c18`, `SHA256SUMS` sha256
  `8e966958…bb001f91`, every listed file verified `OK` before publication; per-run JSON under
  `runs/`, probe source under `source/`; only the probe binary `bin/u1u6.exe` was not
  published).
  The probe ran on Windows 11 10.0.26300.9711, WebView2 Runtime 156.0.4314.50, tao
  0.35.3, webview2-com 0.38.2, windows 0.61.3: 55 runs, all exit 0, no lingering
  `msedgewebview2` or probe process after any run, using Keld's `webview2/mod.rs` as it
  stood at `2458836f` (sha256 `1247c842…b614a3`) with one declared deviation
  (`with_focused(false)` on the builder). Facts from it are marked FACT (probe).
- Native atom-E evidence ("the U10 bundle", 2026-10-29, Windows 11 26300.9711, WebView2
  156.0.4314.50, `webview2-com` 0.38.2): the same PR and run (note 265 E2), bundle `u10-atom-e/`
  (`SHA256SUMS`, kept as `original-SHA256SUMS`, sha256 `81bbcc09…f5bb9bb5`; probe source `main.rs`, runs `run1-3.txt`,
  `run-long.txt`; `v2/` repeats the probe in Keld's order, `SHA256SUMS` sha256
  `4662432b…5293f2f`).
- Release-proof research (2026-10-29), published in `0monish/keld-research` PR #35 (note 265,
  runs `gh1188-webview2-release-custody-2026-10-29` and `gh1188-job-census-probe-2026-10-29`):
  every `msedgewebview2.exe` process inherited a Keld-shaped no-breakaway Job (284/284; then 430
  processes of 11 kinds across print, PDF, download, GPU/renderer crash, media: 0 outside);
  the Job emptied 0.2-0.4 ms after the last `BrowserProcessExited` (19/19) and 263-698 ms after
  the last Close (16/16); 245,430 exclusive opens of the user-data folder after it emptied hit
  0 sharing violations; `GetProcessInfos` and the Chromium lockfile both went empty while
  processes lived (19/19). Labels: membership is FACT (documented descendant inheritance,
  <https://learn.microsoft.com/windows/win32/procthread/job-objects>, and nested-job
  accounting, <https://learn.microsoft.com/windows/win32/procthread/nested-jobs>, plus
  284/284 and 430/430 observed); "Job empty ⇒ folder released" is MEASURED on one
  runtime, not documented. Residual: a process a Windows service starts on the runtime's
  behalf would be outside the Job (INFERRED; not observed among 430 processes of 11 kinds, which were matched by the user-data folder on their command line, so a holder without that argument would not have been seen; a runtime update, the system print dialog and a real permission prompt were not exercised), so the census makes no claim about it
  (D7).
- r6 reconciliation bundle: the same PR and run (note 265 E4), bundle `spec-reconcile/`
  (checker `reconcile.py`, `mapping.tsv` with every citation's old pin, old and new range and
  match confidence, `deltas.md`; its `SHA256SUMS`, kept as `original-SHA256SUMS`, sha256
  `bb83a754…20aa2b4`).
- r18 host-window probe: published at its own pin,
  `0monish/keld-research@c6fc94c292ea51937088d55f112d014bdbc7d294` (PR #35, open, unmerged; note
  265 E6), run `artifacts/runs/gh1188-r18-native-window-probe-2026-10-31/`, bundle `r18/` (source
  `main.rs.txt`, its `SHA256SUMS` kept as `original-SHA256SUMS`, every listed file verified `OK`).
  Windows 11 10.0.26300.9711, tao 0.35.3, webview2-com 0.38.2, windows 0.61.3. 7 runs, all exit
  0: 4 in the notify arm (`defaults-1`, `notify-hold-1..3`), 3 in the mutant arm (`mutant-1..3`,
  none of the three calls below), `Close()` before the drop, as `View::drop`. Results by the
  log counts:
  **Origin:** `window.screenX/Y` was stale just before `NotifyParentWindowPositionChanged` on
  16/16 moves and followed each within 400 ms of the call (16/16); in the mutant arm it followed
  0/12 moves, and stayed stale after a 3 s settle (3/3). The UIA document `BoundingRectangle`
  followed every move with no call (28/28 moves, 7/7 runs), so UIA does not discriminate. In the
  mutant arm the origin did change once with no call, between the last move read and the first
  later read (3/3; Q2's minimize and restore and Q3's activations lie between, and the log does
  not attribute it), and a later maximize, minimize, restore and borderless full screen then left
  it stale (3/3). **Visibility:** a minimized window kept `document.visibilityState` `visible`
  (7/7) until `SetIsVisible(false)` made it `hidden` (4/4), and Tao reported `Moved(-32000, -32000)`
  then `Resized(0, 0)`. **Focus:** after reactivation keyboard focus sat on the top-level HWND and
  `document.hasFocus()` was false (7/7) until `MoveFocus` put it back on the WebView2 child and
  made it true (4/4). **Tao events:** 13 `Moved` and 10 `Resized` per run (7/7). **Teardown:**
  `IsWindow` read TRUE inside `Destroyed` with 0 child HWNDs (7/7) and at the end of the hold (3/3);
  the first posted wake read FALSE (7/7), 127.9–221.6 µs after `Destroyed` in the 4 runs without a
  hold and 300.4–300.8 ms after it in the 3 runs with one; no logged closure run after `Destroyed`
  read TRUE outside the hold. **Defaults:** `ShouldDetectMonitorScaleChanges` read true,
  `BoundsMode` raw pixels and `RasterizationScale` 1.25 on a 125% display (1/1).
  **Limits.** The two arms differ in three calls at once (the notification from the `Moved`
  handler, `SetIsVisible(false)`, `MoveFocus`), so they are confounded: each effect is attributed
  by its own observable and its before/after timing inside one run, never by a single-call A/B,
  and AC26's exact-head mutants are the single-fault proof. The runtime version
  156.0.4314.50 is not printed in the logs. The hold of about 300 ms is inferred from timestamps,
  because `hold_ms` is not logged. A second `Window(Destroyed)` turn at turn+5 (7/7) is
  unattributed, because the turn log prints no window id (INFERRED: the auxiliary window the probe
  drops at `Done`). Q1–Q3 ran inside Tao's `Init` callback, so their Tao events were buffered and
  Q1 made the call directly; only the later steps called from the `Moved` handler. Activation was
  programmatic (`SetForegroundWindow` to an auxiliary window), focus after a restore was not read,
  and a maximized window was never minimized. One desktop session, one 125% display, one runtime;
  prior evidence for T3, never a substitute.
- **Evidence status (one rule).** All probe and research evidence in this section ran on an
  older code path or in a standalone probe, so it is prior evidence: it chose the design and qualifies no head. Each
  load-bearing use has an exact-head cell: U1's pump and buffered delivery (AC6, AC25; U1 built
  visible windows with `with_focused(false)`, D4 builds `with_visible(false)`); U6's regimes
  (AC21 reads each run's regime from D16 lines); U10's environment fact (AC23's
  zero-controller control); the r18 results (AC24, AC26). With no exact-head cell, and therefore
  prior evidence only: the Job-membership counts (284/284, 430/430; AC23 NC 3 and AC27 prove the
  census sees a Job member, not that every WebView2 process is one), the 0.2–0.4 ms and
  263–698 ms Job-empty timings, the 245,430 exclusive opens, and the 22–61 ms `Destroyed`
  latency (used only as a margin, A20, D6).
- Doc drift found, not fixed here: `crates/keld-wv/src/webview2/mod.rs:39-41` says async
  completions are delivered by `webview2_com::wait_with_pump`, but production uses
  Keld's own loop (`wait_for_launch_with_message_pump_until` `:597-617` over
  `pump_message_or_handle` `:554-589`); `wait_with_pump` is imported only under
  `media-acceptance` tests (`:98-99`). T3 corrects the comment in its keld-wv diff.
- Crate rules: `crates/keld-wv/AGENTS.md` (UI-thread-only handles; one exit-wake
  registration; Windows guard handlers before `GuardInstalled`), `crates/keld-core/AGENTS.md`
  (`app_session.rs` is the one lifecycle/router owner; recovery gates need real platform
  tests plus a mutation control).
- Deviation: none from architecture. gh531 §1, §4.b and "Platform notes" and the 04
  sentence are amended in the implementing PR (Windows out of the parked list; Linux stays).

## 3. Acceptance criteria (binary, each becomes a test)

Each names its negative control (NC). "Real" means a real Windows 11 desktop session.
Test names are PROPOSED (§7); none exists today. "Census" is the per-PID top-level
`EnumWindows` oracle (A15); "witness" is a test-side `GetWindowRect` read (A19).

1. **Zero windows at Ready.** Given a verified schema-2 stage (`keld.boot.json`
   `schema == 2`, no `renderer`), when the fixture reads `READY`, then the census is 0,
   and the host is still alive `INITIAL_MILESTONE_DEADLINE + 1 s` later with census 0.
   *NC:* keeping `create_app` for schema-2 boots yields census 1 at Ready.
2. **Ready is Init-bound; the terminal is complete.** Given a debug `profile-test-hooks`
   child parked after loop setup and before the first Init turn (the macOS
   `KELD_T1B_STALL_BEFORE_INIT` control, `wkwebview/mod.rs:411-418`), when 15 s
   elapse, then no `READY` is written; the Windows pre-Init terminal runs exactly once
   with `KELD-CORE-037` and the detail "the UI loop did not deliver its first turn within
   15 s"; the Bun child is retired (pid gone); the host exit status is a failure. Stage
   disposition is AC19's (the Windows host never deletes its own stage; 
   `keld-host/src/main.rs:111-134` is macOS/Linux-only). *NC:* writing Ready from
   `HELLO` publishes `READY` before Init.
3. **Admission is the verified boot only.** Given `initialWindow: "none"`, when
   `keld_cli::boot::stage_dev_boot` runs on Windows, then it stages a schema-2
   descriptor (no `index.html`) and the host admits it (in every build from T2; at T1 only
   under the D2 test admission, while a product build still refuses it `KELD-CORE-034`,
   AC23); renderer boots pass the existing
   Windows tests unchanged and write no channel-5 frame (a renderer boot whose stage
   carries the sidecar additionally launches with the preload, AC20). *NC:* a descriptor
   with `schema == 2` and a `renderer` field is refused `KELD-CORE-035`.
4. **Host-owned facade hint.** Given the test launcher exports
   `KELD_NATIVE_WINDOW_FACADE=1`, when a renderer boot starts, then the Bun child
   observes `"0"` (already true on `main`: `windows_primary_config` forces `"0"`,
   `app_session.rs:3368-3370`, from #1181's 53bc9e6e); a schema-2 boot observes `"1"`,
   which is the new behaviour (at T1 under the D2 test admission, AC23). The fixture prints the live environment value on a control
   line, read as `Reflect.get(process.env, "KELD_NATIVE_WINDOW_FACADE")` (the macOS
   shape, `facade_window.rs:549`): from T1 the launch define (AC5, D13) is substituted
   "while parsing" (<https://bun.com/docs/cli/run>, `--define`) wherever the dotted
   expression `process.env.KELD_NATIVE_WINDOW_FACADE` appears, so a dotted read would
   print the define and hide both mutants below. T1 therefore turns the Windows product
   fixture's `hint` read (`support/product.rs:112`, dotted today) into the same live
   read, which keeps the existing inherited-hint test
   (`tests/no_flag/windows/lifecycle.rs:39`, assertion `:105`) discriminating. *NC 1:*
   reverting the existing `"0"` line makes the renderer-boot child observe `"1"`.
   *NC 2:* forcing `"0"` for every boot kind makes the schema-2 child observe `"0"`.
5. **Boot mode and sidecar fixed at launch (one rule, one owner, macOS's rule).** When
   the Windows primary launches, Bun receives the literal define
   `--define=process.env.KELD_NATIVE_WINDOW_FACADE:"0|1"` from the validated boot kind,
   inherited `BUN_OPTIONS` is removed, and a `bunfig.toml`/`.bunfig.toml` in the stage
   root is refused on every launch, sidecar or direct entry. This is the rule macOS runs
   on `main` (`keld-host/src/main.rs:478-520`; gh531 `:821-831`). Given a stage that
   carries `src/kipc-transport.ts` (staged for every boot kind whenever the project has
   it, `crates/keld-cli/src/boot.rs:37`, `:483-493`), Bun also runs with
   `--preload=<stage>/src/kipc-transport.ts`. Fixture
   shape (decisive for the mutant): the *first* imported module of the entry deletes
   `KELD_NATIVE_WINDOW_FACADE` from the live environment with `Reflect.deleteProperty`
   (a dotted `delete` is rewritten by the define, AC4) before any import that can reach the
   transport, and `@keld/electron` is loaded only by a later dynamic `import()`; the
   pre-import `app.quit()` then still rejects `KELD-COMPAT-012`, because the transport's
   mode expression (`transport.ts:32-33`) is the literal define, not the live
   environment (macOS shape `facade_window.rs:548-557`). Given a `bunfig.toml` or `.bunfig.toml` in the stage
   root, the launch is refused with the `KELD-CORE-037` "staged runtime config … can
   override canonical boot selection or preload" text (`keld-host/src/main.rs:480-487`),
   with or without a sidecar. Given the
   staged sidecar replaced by a symlink, a junction or a directory, the launch is refused with
   `KELD-CORE-037` by the admission owner before Bun starts (strictest rule, D13), so the
   oracle does not depend on how Bun would treat a directory preload. *NC 1a:* on a direct-entry stage with no sidecar (so no preload), launching without
   the define makes the transport, evaluated after the deletion, read the deleted
   environment, so `nativeFacadeBootSelected` is false and the observation flips. With a
   sidecar the preload evaluates the transport before the deletion, so this NC would not
   discriminate there.
   *NC 1b:* the preload's job is now capture order, not the hint. Fixture shape: the
   bundled `renderer_bridge` layout, where the app imports the same staged
   `./kipc-transport.ts` instance that was preloaded (`renderer_bridge.rs:254-275`). The
   fixture's first module replaces `Reflect.apply`, which the transport captures at
   module load (`transport.ts:38`; gh531 `:816-818`), with a recorder that counts only
   calls whose stack includes `kipc-transport.ts`. Expected (INFERRED until T1 runs it):
   with `--preload` that count is 0 during a window call; without `--preload` it is
   above 0. *NC 2:* a
   stage with `bunfig.toml` launches (sidecar and direct-entry cases). *NC 3:* a junction
   sidecar passes admission (no `KELD-CORE-037`; Bun starts). *NC 4:* an inherited `BUN_OPTIONS=--preload=<attacker>` runs before
   the transport.
6. **One host-minted window with a true outer frame.** After Ready,
   `new BrowserWindow({width:640,height:480,x:37,y:91})` returns `id === 1`; census 1;
   `getAllWindows().length === 1`; control line `PUBLIC_CREATED <bun pid> 1 640x480
   contents,window` (`facade_window.rs:659-669`); the witness `GetWindowRect` of that
   HWND equals `Created.state.bounds` converted at the window's scale factor, and its
   size equals 640×480 logical within one device pixel per edge. *NC 1 (identity, r14):*
   the public id is minted only by the shared registry (`window_registry.rs:284-350`);
   keld-wv never receives or returns it (`CreateWindow` carries only the `build` token,
   `engine.rs:104-106`; `WindowBuilt` answers `{ build, view, snapshot }`). In this fixture
   `WebviewId` is also 1 (keld-wv's `next_id` starts at 1, `webview2/mod.rs:2990`), so
   `id === 1` cannot tell the two apart and is not the identity control. The
   discriminating control already exists in the shared registry, which W2 reuses unchanged:
   `native_view_numbers_do_not_mint_logical_window_ids` (`window_registry/tests.rs:150-159`)
   answers views 7 and 8 and asserts `WindowId(1)` and `WindowId(2)`, and
   `native_snapshot_is_the_only_created_state_and_reply_follows_its_fact` (`:126-148`)
   asserts the `Created` fact and the reply carry the registry window for `WebviewId(7)`;
   the mutant that forms the public id from `view` fails both → RED. W2 adds no test there. The Windows half is routing by `view`: given a second
   `new BrowserWindow()` (id 2), posting `WM_CLOSE` to the second HWND fires `close` on
   id 2 only, and window 1 stays in the census; the mutant that does not advance
   `next_id` (both builds answer `WebviewId(1)`) either replaces window 1's view or
   routes that close to the first entry holding the view (`native_close`,
   `window_registry.rs:589-597`) → RED (INFERRED until T3 runs it). *NC 2:* passing the requested outer
   size as `with_inner_size` without the frame correction makes the witness larger than
   640×480 by the frame.
7. **No v0 deadline on registry windows.** Given the window from (6) held open for
   `INITIAL_MILESTONE_DEADLINE + 1 s` (duration oracle), then the census still finds it.
   *NC (concrete mutant):* keeping managed views in the legacy `views` map instead of a
   separate map. Then `views.is_empty()` is false, so the `Wait` short-circuit (main
   `webview2/mod.rs:3293-3294`) no longer applies; `navigation_ready` is never set for a
   registry view; `navigation_deadline_expired` (`:3295-3301`) fires at 15 s and
   `views.clear()` (`:3308`) drops the window: census 0 and a `Navigate` error.
8. **Pre-Ready construction refused.** Given a facade boot whose `app.ready` has not
   resolved, when `new BrowserWindow()` runs, then it throws `KELD-CORE-040` ("native
   window access requires Host Ready and accepted replay", 
   `packages/@keld/api/src/app.ts:293-299`), no channel-5 `Create` frame is written and
   the census is 0. *NC:* the single fault, publishing `app.ready` before the window
   `Subscribe` replay completes, lives in the shared readiness adapter (`maybePublishReady`
   needs Host Ready and the completed replay, `packages/@keld/api/src/link.ts:177-184`),
   which W2 must not touch (§5). A construction before Host Ready throws `KELD-CORE-040`
   under that mutant too, so this Windows run cannot kill it and is a positive control of
   the refusal only. The rule is gh531's (A22); no shared test that kills that mutant was
   located at `457a9c3b` (UNKNOWN), a residual for the gh531 owner.
9. **Cancel retains everything.** Given a `'close'` listener that calls
   `preventDefault()`, when the test posts `WM_CLOSE`, then `CLOSE_EVENT 1`; after
   `VERIFY_VETO` the reply is `VETO_VERIFIED ReplyStale false 1 0 true 0` (stale `Allow`
   refused); the top-level HWND handle is unchanged and `IsWindow` is true; the set of
   child HWNDs under it (`EnumChildWindows`) is unchanged (the controller's hosting
   windows: `Chrome_WidgetWin_0` in the host process, `Chrome_WidgetWin_1` and
   `Chrome_RenderWidgetHostHWND` in the browser process, `Intermediate D3D Window` in a third
   process, FACT (r18 probe, §2), 7/7); the WebView2
   browser pid is unchanged; and a share-none open of the profile lease file
   from the test still fails with `ERROR_SHARING_VIOLATION` (the engine holds it with
   share mode 0, main `webview2/mod.rs:781`). *NC (issue mutant):* an isolated mutant
   that drops the `View` on `CloseRequested` fails the HWND-equality assertion, not a
   later one. The `Stopping`-commit mutant is AC18's NC: ephemeral dev stages carry no
   lifecycle record (FACT, `:1054-1055`), so it is vacuous here.
10. **Accepted close releases once; close path is per boot kind.** Given (9), when the
    test posts a second `WM_CLOSE` while the first is pending, then no second
    `CLOSE_EVENT` (gh531 Q3 merge); when the listener allows the next attempt, then
    exactly one `CLOSED 1`; the host observes Tao `WindowEvent::Destroyed` for that HWND
    and confirms native teardown (D6, AC24) before it writes `WindowReleased`; census 0; `'closed'` precedes
    `'window-all-closed'`; with a `'window-all-closed'` listener that does not quit, the
    session stays live and a follow-up create/destroy succeeds (
    `facade_window.rs:574`); keld-core drops a `LastWindowClosed` in a registry session
    (`app_session.rs:4641-4642`), so that survival does not show the loop wrote none (D7's
    rule has no device observable). Under `profile-test-hooks` with the hang guard
    shortened to a few seconds (D16), the session outlives that guard while a browser
    lives (the vetoed window's, then the follow-up window's), so no barrier was armed
    while a browser lived; an armed barrier whose browser then exits normally never
    faults, so the guard decides only that case (NC 2). *NC 1:* writing `WindowReleased`
    from the `View` drop. It is decided by order, not by the race between the `IsWindow`
    read at `CLOSED` and the 22–61 ms posted destroy (A20): the D16 UI-thread lines
    `DESTROYED <hwnd>` (the matching `Destroyed` staged the entry) and `RELEASED <view>`
    (written inside the one function that sends every `WindowReleased`) must appear in
    that order, and the mutant writes `RELEASED` first → RED. The `IsWindow` read at
    `CLOSED` stays as defence in depth: a run that reads it true fails, one that reads it
    false proves nothing.
    *NC 2:* routing a facade session's `CloseRequested` through the legacy arm arms the
    barrier at the first `WM_CLOSE` (`:3367`; the legacy map is empty in a facade session,
    so `close_requested_view` reports the last window closed, `:2833`) without dropping
    the managed view, so the browser lives on and the shortened guard faults the host
    (`HangGuardExpired`, `:2699-2701`) while the fixture still holds the session; AC9's
    `CLOSE_EVENT 1` fails too, because no `WindowCloseRequested` is sent.
11. **Forged pairs refused; successor untouched.** Given window A destroyed and window B
    live, when raw `invokeWindow` sends `Destroy` with `(B.id, A.generation)` and then
    `SetMaximized` with `(B.id, B.generation + 1)` (the macOS `STALE` shape, 
    `facade_window.rs:78-89`, asserted at `:869`), then each answers `KELD-CORE-038`,
    B's HWND handle and witness are unchanged, and census stays 1. *NC:* a matcher on
    `WindowId` alone destroys B on the first call.
12. **Tombstone, no resurrection.** After `CLOSED 1`, `win.isDestroyed()` is true,
    `getSize()` throws `KELD-COMPAT-011` ("Object has been destroyed": `getSize` 
    `browser-window.ts:516-519` through `#state()` `:494-501`; `:489` is the
    `webContents` getter), a raw call with A's retired pair answers `KELD-CORE-038`,
    and the next `Create` mints `(2, w')` with `w' > w`. *NC:* reusing a retired pair
    addresses the new window.
13. **Focus fact is activation, with no transient blur.** After `Created`, with the
    window active, `getFocusedWindow()` is that window, and across the build's
    `MoveFocus` the host writes **no** `WindowFocus { focused: false }` (the fixture
    counts focus facts: exactly one, `true`); after the test activates another
    process's window, the mirror shows unfocused; after re-activation, focused again, and
    keyboard focus returns to the controller (D11, AC26 (c)). *NC:* forwarding Tao's
    raw `Focused(false)` emitted on the top-level `WM_KILLFOCUS` during `MoveFocus`
    writes a second fact (`false`) for an active window (A14, D11). A run is at risk
    only when the window is active at the build's `MoveFocus`, which D4 runs after
    `set_visible(true)` and which the `Created` snapshot's focused field records; an
    initial window that Windows does not activate (foreground rules may deny it) is the
    other initial case below and is not counted for this NC.
    The first `Created` snapshot must already report focused when the top-level is
    active and keyboard focus is in its WebView child HWND, before any later focus
    callback repairs the mirror; an inactive initial window reports unfocused.
    *Initial-snapshot NC:* replacing D4's D11 sampler with initial Tao `is_focused()`
    reports false in the active-child case. Both initial cases and this single-fault
    control are required Windows acceptance, not established by a later focus event.
14. **Last window, quit, relaunch, no descendants.** With no `'window-all-closed'`
    listener (default `Quit`) and, separately, with draw.io's listener (`app.quit()` on
    non-mac, `electron.js:2112-2125`), when the last window is destroyed, then the host
    exits 0; every pid in the host's pre-Quit process tree snapshot
    (`support/process.rs:151` `process_tree_report`), including all `msedgewebview2.exe`
    descendants, is gone; the ephemeral profile root is deleted (stage disposition is
    AC19's); and a second `run_product_cycle`-style launch yields new host/Bun pids and
    fresh link authority (`tests/no_flag/windows/lifecycle.rs:18-36`). A lingering
    process or a failed relaunch cannot discriminate a missing `Quit` arm: an unarmed
    barrier still drains a normal receipt (main `webview2/mod.rs:2662-2670`, before the
    armed check at `:2685`; the loop polls every turn, `:3271`), and the `View` drop
    closes the controller by itself (`:2836-2843`). So the two controls are independent
    single faults, each recording the assertion that turns red. *NC 1 (arm provenance):*
    the D16 barrier readback at loop exit must report the barrier armed by the terminal
    command (`armed_by=Quit`, after every managed `View` was dropped); the mutant that
    removes only the `Quit` arm call fails that assertion even though the browser exits
    normally and the relaunch succeeds. *NC 2 (armed, receipt missing):* with the barrier
    armed and every receipt withheld (the landed h9 hook, D16, 1 s grace), the host exits
    `KELD-WV-009` (`GraceExpired`, host site) within hang guard + grace, never hangs, and
    the engine releases nothing (the fault returns at `:3381-3386`, before
    `finish_profile_release`, `:3395`); the positive control without the hook exits 0
    and the ephemeral profile root is deleted. The persistent counterpart (record
    `stopping`) is AC21 NC 3. The `QUIT_REPLY`/`LINK_EOF` tail rests on the landed #1073
    fixes; a failure there is attributed, never retried.
15. **Two windows, one lease, one environment.** Two `Create`s yield two top-level
    HWNDs, two controllers, one `BrowserProcessId`, and exactly one `webview2` user-data
    directory under the profile control dir (`plan.user_data_dir =
    control_dir.join("webview2")`, main `:635`, `:648`); destroying window 1 leaves
    window 2 live with the same browser pid. *NC:* a mutant that mints a per-window
    user-data folder creates a second `webview2*` directory and a second browser
    process (distinct user-data folders never share a browser process, WebView2 process
    model <https://learn.microsoft.com/microsoft-edge/webview2/concepts/process-model>).
    Scope of this oracle, stated honestly: a same-folder second environment shares the
    process collection and adds no directory, so it is externally invisible; it is
    excluded by construction (the engine has one `environment` field, main `:2855`, and
    the in-loop build borrows it) and checked by the D16 `profile-test-hooks` readback
    of the environment-creation count, which must read 1.
16. **Build failure after minting leaves no HWND.** Given the U7 fixture's controller
    creation fault injected immediately after the HWND is minted (so no `View` exists,
    D12 case a), when `Create` is admitted, then the caller gets `ERR KELD-CORE-041`
    (stage `Webview`) only after D6 confirmed the half-built window's native teardown: the
    U7 fixture's stderr line names the failed HWND, and the test reads
    `IsWindow(hwnd)` immediately after it reads the error and finds it FALSE, because the
    answer is published only after the UI thread read FALSE (a recycled value could only
    make the read TRUE and fail the test, never pass it); the D16
    loop-exit readback
    reports an empty `releasing` map; and the next `Create` succeeds and mints `(2, w')`
    with a fresh HWND. The visible-window census cannot decide this case, because D4
    builds `with_visible(false)` and the failed window is never shown. *NC 1:* the mutant
    that answers `WindowBuildFailed` in the failing callback is decided by order, not by
    the race with the 22–61 ms destroy (A20): the U7 fixture's UI-thread stderr lines
    (`FAILED_BUILD_DESTROYED <hwnd>` when the entry is staged, `FAILED_BUILD_ANSWERED
    <build>` written inside the one function that sends every `WindowBuildFailed`, so
    no answer path can skip it) must appear in that order, and the mutant reverses them. *NC 2:* the mutant that drops the standalone `Window` without a
    `FailedBuild` entry (the pre-`View` path today has no owner for it) leaves no entry to
    answer from, so the caller never gets `KELD-CORE-041` before `PRODUCT_DEADLINE`.
    *NC 3 (missing `Destroyed`):* the U7 fixture's `swallow` mode swallows the matching
    `Destroyed`; the releasing bound (D6) expires, the loop latches the barrier's one fault
    (`ReleasingExpired`) and takes the D6 fault reaction, and the host exits `KELD-WV-009` with one diagnostic naming
    the HWND once that bound expires (after `RELEASING_DESTROYED_BOUND` from the park, before that bound plus `PRODUCT_DEADLINE`), never answering `WindowBuilt` and never hanging. The record
    half is a keld-wv unit test of the release composition: an expired entry latches the
    fault, so `finish_profile_release` never runs and a persistent record stays
    `stopping`; the mutant that drops the expired entry without latching writes `idle`. The same unit
    runs the D6 fault reaction over every `ProfileReleaseFault` variant, because only some have a
    device cell.
    *NC 4 (the terminal waits for `releasing`, D6 exit gate):* the U7 fixture's `defer`
    mode holds the matching `Destroyed` in Keld's dispatch and writes `DEFERRED_DESTROYED
    <hwnd>`. The hold becomes eligible at the end of the first loop turn in which a
    terminal command has run and the barrier reports ready or faulted (after that turn's
    exit check); the mode then posts one wake through its own `EventLoopProxy`, and at the
    start of the next turn, before the releasing-expiry check, it consumes the held event
    through the same function a real `Destroyed` reaches and writes `DEFERRED_CONSUMED
    <hwnd>`; that function stages the entry, and the D6 publish step then reads `IsWindow`
    FALSE and answers. The terminal is a real admitted `Quit`: the fixture sends a raw
    `invokeWindow` `Create`, not awaited, that fails at the U7 `hwnd` point, then calls
    `app.quit()`. The facade admits that `Quit` because the failed build never published a
    window, so no snapshot is live (`packages/@keld/electron/src/app.ts:139-168`, the
    live-window refusal at `:157`; INFERRED until T3 runs it). keld-core routed the
    `Create` to the UI loop when it arrived (`window_router.rs:586`) and sends `Quit`
    there only at the end of its quit tail (`app_session.rs:7417-7419`), after ending
    the window session, which reports the in-flight `Create` interrupted
    (`app_session.rs:6882`, `window_router.rs:918-924`), so the `CreateWindow` precedes
    the `Quit` in the one UI queue. Receipt, from the UI
    thread's stderr: U7's failed-HWND line (the `CreateWindow` ran and parked its entry),
    then the D16 line `TERMINAL Quit releasing=1` (written when the loop dequeues the
    terminal command), `DEFERRED_DESTROYED` (before or after `TERMINAL`),
    `DEFERRED_CONSUMED`, `FAILED_BUILD_ANSWERED`, the loop-exit readback with an empty
    `releasing` map, then exit 0. A run whose `Quit` is refused or never enqueued (no
    `TERMINAL` line), or whose `TERMINAL` line reports `releasing=0`, is INVALID: reported,
    and counted neither as a pass nor as a mutant kill. The mutant that exits on
    `exit_requested && exit_ready` alone (today's predicate, main
    `webview2/mod.rs:3275-3277`, `:3373-3377`) ends the loop before the hold is eligible:
    no browser was launched, so the barrier is ready in the `Quit` turn (pid 0,
    `:2728-2732`) and the mutant exits at that turn's bottom check (`:3374`), leaving
    `TERMINAL Quit releasing=1`, no `DEFERRED_CONSUMED` and one pending entry in the
    readback → RED. r15's variant (b), an allowed close whose listener calls `app.quit()`,
    is withdrawn: the facade refuses that `Quit` with `KELD-COMPAT-012` while any window is
    not host-destroyed (`app.ts:157`), so it never enqueues a terminal. A pending `Destroy`
    entry can still meet a terminal. keld-core's own accepted `Quit` ends the window
    session first, whose teardown sends `DestroyWindow` for every open window
    (`window_registry.rs:855-866`, `window_router.rs:913-917`), and sends `Quit` to the
    UI loop only at the end of its tail (`app_session.rs:7417-7419`); `@keld/api`'s own
    `app.quit()` reaches that path with a live window, because only the Electron facade
    refuses one (`packages/@keld/api/src/app.ts:248-250`). Host terminals do too (the
    wake bridge's `Fatal` on a lost command channel, `webview2/mod.rs:3504-3508`; the
    `Fatal` after a failed quit tail, `app_session.rs:7401-7405`; the handover `Quit`,
    `:6078-6080`). Without a hold, the teardown's `Destroyed` (22–61 ms, A20) is INFERRED
    to land before that late `Quit`, so a device run would read `releasing=0` and be
    INVALID. The gate is one predicate over both entry kinds and every
    terminal kind, so the `Destroy` kind, `Fatal`, a latched fault and the navigation terminals are
    decided by the keld-wv unit test of the exit predicate, and no further hook is added.
    *Not exercised on a device:* D12's `Window` stage, because U7 has no point before the HWND
    exists and no stimulus makes Tao's build fail; its answer is one arm with no custody.
17. **Recovery keeps the HWND.** Given a facade window and the primary killed, when the
    successor subscribes and receives the `Created` replay, then the HWND handle equals
    the pre-crash handle (`tests/no_flag/windows/recovery.rs:251`). *NC:* destroying
    windows on role loss changes the handle.
18. **Persistent profile stop record (installed harness).** Given a persistent profile
    (installed identity, `app_session.rs:3105-3108`) and a facade boot, when (a) one
    window is built, closed with an allowed close and `Quit`, and separately (b) `Quit`
    is issued with zero windows ever built, then in each case the host exits 0, the
    on-disk lifecycle record parsed by the test-side reader
    (`read_lifecycle_record`, `profiles/sessions.rs:394`) equals
    `LifecycleObservation::idle()` (phase `idle`, no owner) before any successor runs,
    and a successor host on the same profile then reaches Ready. The record check comes
    first because a Windows successor that finds a non-idle record quarantines and
    recovers inline in the same launch (`:1023-1028`), so a successor's
    Ready alone cannot tell a clean stop from quarantine.
    *NC 1:* leaving the `Running` write at the first build (today's
    `mark_profile_running` in `create_app`) makes (b) attempt `Starting → Stopping`,
    which `advance` refuses (`crates/keld-wv/src/profile/lifecycle.rs:153-166`), so the
    host fails `KELD-WV-009`. *NC 2:* committing `Stopping` on `CloseRequested` leaves a
    vetoed window with a `Stopping` record. *NC 3 (Bun family, D7):* (a) removing the exclusion of
    the Bun generation Jobs' members makes every persistent `Quit` exit `KELD-WV-009` with
    the record `stopping`, because Bun is still alive at release (A10); a `Quit` after one
    recovery still exits 0, because the recovered generation has its own Job; the mutant
    with one session Job for all generations fails that case. (b) Real Windows, today's
    unconfined primary: the Bun app spawns a child that opens a file in the user-data
    folder and holds it through `Quit`; the test records that the release proof does not
    detect it, as the disclosed limit (D7), never as a pass. The Strict half (the same
    open must fail with access denied) belongs to #1187 under U17. *NC 4 (census
    witness, D7):* (a) keld-runtime unit test of the shared census in its observe
    policy: a listed non-family member keeps every snapshot unclean (listed ⇒ unclean,
    because list removal lags exit, kel53 `:2335-2338`) until it leaves the list, the
    deadline yields the refusal, and the member is never terminated; the single-fault
    mutant that runs the terminate policy kills it. (a') Pacing: with one held-open
    non-host member and a 5 s deadline, the census takes at most 3 snapshots plus one per
    25 ms unsettled retake, and its CPU time stays under 50 ms; the mutant that retakes
    without waiting exceeds the snapshot bound. (b) keld-wv unit test of the
    release composition with an injected witness: an unclean witness keeps `stopping`
    even after the barrier's normal receipt was credited (today's flag at T2; D8's
    current-instance credit from T3, which re-runs it); the mutant that
    admits `Idle` on the receipt alone (today's barrier) writes `idle`. Its mirror, a
    clean witness with no current-instance receipt, is AC21 NC 3/NC 4. *NC 5 (failed build after controller creation, D8 (2), D12 b):* the U7 fixture's
    `view` point fails the build at the first fallible step after the `View` exists, so its
    controller launched and bound a browser (`REBIND gen=1 pid=<p>`) and then closed it.
    With every receipt withheld by the h9 hook (D16), `Quit` must exit `KELD-WV-009`
    (`GraceExpired`, host site) with the record `stopping`; the positive control without
    the hook writes `idle`. The mutant that keeps r14's "a failed build never rebinds"
    leaves the binding at generation 0, so `exit_ready` admits the census alone (main
    `:2728-2732`) and writes `idle` → RED. The U7 `webview` point (`create_controller`
    succeeds, `CoreWebView2` fails, so no browser identity is readable) must end the
    session `KELD-WV-009` (`UnidentifiedController`) with the record `stopping` (D6 fault
    reaction), even with every receipt delivered; the mutant without the D8 (2) latch writes `idle`. Both are INFERRED until
    T3 runs them with the approved U7 fixture (§10); both points are required real-Windows
    cells at T3's exact head with nonzero at-risk counts (§6 T3 native fault gate), and
    AC21 NC 4 (h) is their unit control, never a substitute.
19. **Stage disposition (`keld dev` lane only).** On Windows the host never deletes its
    stage; the `keld dev` private cleanup process does (`keld-host/src/main.rs:279-292`,
    `WindowsDevStageCleanup::prepare` `app_session.rs:1526-1562`) once the attempt
    Job reports zero processes. Given a facade boot launched through `keld dev`, when
    the host exits by `Quit` or by the AC2 terminal, then the stage count returns to 0
    (`no_flag_windows.rs:362-374` `wait_for_dev_stage_count`) only after every pid of
    the host's tree is gone. *NC:* rule not exercised by this spec's fixtures. Its single
    fault, deleting before the attempt Job empties, needs a member alive at the gate, and
    no W2 fixture leaves one: a zero-window facade boot launches no browser (A21), a
    `Quit` with windows releases every WebView2 process before the host exits (D7), and
    the host-death Job kills the rest when the host exits (`dev_lifecycle.rs:216-219`
    finds the Bun descendant already signaled). The gate is existing code that W2 does
    not change (`gracefully_wait_and_reap_attached`, then
    `wait_and_delete_after_family_exit`, `keld-host/src/main.rs:303-323`,
    `app_session.rs:1581-1617`), and no test on `main` discriminates its refusal
    (`:1603-1610`), so AC19 is a positive control of the facade boot kind only; a test
    of that refusal is the gate owner's (residual).
20. **Renderer boot with a staged sidecar (same rule).** Given a renderer boot whose
    project has `src/kipc-transport.ts`, when the Windows primary launches, then Bun
    runs with the same `--preload`, the legacy close path is unchanged (AC3; its host obligations are AC26 (d)), and
    the child observes `KELD_NATIVE_WINDOW_FACADE == "0"`. *NC:* scoping the preload to
    facade boots lets a renderer app evaluate the transport after its own imports.
21. **Release proof across browser generations (three gap regimes, D7/D8), installed
    harness.** On `profiles/` with `InstalledApp` (persistent; dev stages carry no
    lifecycle record), given one window destroyed and then `new BrowserWindow()` issued
    (i) in the same control command (same-pid reuse, 18–86 ms), (ii) after a 150 ms
    fixture delay (new pid while the old is alive, 125–237 ms), and (iii) after a 400 ms
    delay (clean, ≥274 ms; these ranges are U6's prior evidence, so each run counts under the
    regime its D16 lines show: no `REBIND gen=2` for (i), `REBIND gen=2` with a new pid for (ii)
    and (iii), and (i) and (iii) each need a nonzero count at T3's exact head), then in each case the new window builds and, after the fixture
    closes it with an allowed close (the facade refuses a `Quit` while a window is live,
    `KELD-COMPAT-012`, `app.ts:157`), a following
    `Quit` writes `idle` only after the current browser instance's normal receipt (D8:
    the reused instance in (i), the new one in (ii) and (iii)) and a clean census, with an
    empty post-exit process tree (AC14's oracle) and a successor exclusive environment
    that succeeds. The receipt is bound to the current instance only; older generations'
    normal exits are never credited (an abnormal one latches, D8 (3)) and are covered by the census as one kernel-kept set, so no
    retired-generation list is kept. *NC 1:* AC18 NC 4 (a) and (b) decide
    regime (ii)'s census half (a listed older browser keeps the census unclean, and an
    unclean census keeps `stopping` despite the current receipt), whose overlap is
    timing-dependent on a device. *NC 2:*
    dropping the environment before `Quit` suppresses the event (U6, 0/10), so the
    barrier fails `GraceExpired` and the record stays `stopping`; the environment must
    stay referenced until the event. *NC 3 (stale receipt, fail-first, device):* window A
    is destroyed with a `'window-all-closed'` listener that does not quit; the fixture
    waits until every `msedgewebview2.exe` in the host's tree is gone
    (`process_tree_report`), so A's normal receipt is credited to generation 1; it then
    creates window B (a new browser pid, U6 8/8), closes B with an allowed close (the
    facade refuses a `Quit` while B is live, `KELD-COMPAT-012`) and issues `Quit`, with
    B's receipt withheld by the D16 later-generation hook while B itself exits. Expected: the host
    exits `KELD-WV-009` (`GraceExpired`, host site) within hang guard + the hook's 1 s
    grace and the record reads `stopping`, not `idle`, although B exited and the census
    is clean, so receipt provenance is decided independently of Job emptiness. At-risk
    set: a run counts only when the D16 lines show `RECEIPT_CREDITED gen=1 pid=<A>`
    before `REBIND gen=2 pid=<B>` with B ≠ A; any other order (same-pid reuse, or A's
    receipt not yet credited at the rebind) makes the run void, reported, and counted
    neither as a pass nor as a mutant kill. Positive control: the same at-risk run
    without the hook writes `idle`. Mutant: today's session-wide
    sticky `observed` (`:2668`, accepted by `exit_ready` `:2728-2732`), kept across the
    rebind, writes `idle`. *NC 4 (receipt binding, keld-wv unit; each case asserts the named fault variant,
    because every barrier fault surfaces as `KELD-WV-009` `LifecycleUnproven`,
    `:3381-3386`):* the binding table of
    D8 driven with stamped receipts and the existing live/exited handle helpers
    (`webview2/mod.rs:4123-4142`): (a1) overlap, in D8 (2)'s order: A's normal receipt queued
    during B's build pump is drained by the poll at B's controller completion and credited to
    generation 1 (`RECEIPT_CREDITED gen=1`), the rebind that follows (`REBIND gen=2`) clears that
    credit, and with B's receipt withheld the armed barrier faults `GraceExpired`, so the record
    stays `stopping` even with a clean census; (a2) a stale generation-1 receipt injected after
    B is bound is discarded and never credited to B; (b) the old pid's normal event fired after the rebind is foreign and
    dropped; (c) same-pid reuse with the bound handle unsignaled keeps generation 1 and
    credits the one receipt at the final close; (d) the same pid with the bound handle
    signaled marks the binding ambiguous, credits nothing, and the armed barrier faults
    at the grace; (e) an abnormal receipt of an older generation, drained after the
    rebind, still latches `AbnormalExit`; (f) a rebind after arming latches
    `RebindUnproven` at once; (g) a rebind whose handle cannot be opened (pid 0, or a pid
    that no longer exists) latches `RebindUnproven` at the rebind, before any grace; (h) a build that fails after
    `create_controller` and `CoreWebView2` succeeded binds a new pid exactly as a successful
    build does, and a controller whose `BrowserProcessId` cannot be read latches
    `UnidentifiedController`; (i) a rebind whose old-wake retirement fails (a test double of the
    wake operations that fails `UnregisterWaitEx` and counts registrations) registers no
    second wait, forgets the old wake (never unregistered twice), keeps the binding,
    latches `WakeRetireFailed` and never admits `idle`,
    while the positive control leaves exactly one live registration after a successful
    retirement; (j) a `ProcessFailed` of kind `BrowserProcessExited` from a live view with a
    `releasing` entry pending (so the D6 exit gate holds the loop open), then
    a `CreateWindow`, then the old pid's late abnormal `BrowserProcessExited`: `AbnormalExit`
    is latched once, at the first poll after the `ProcessFailed`, the `CreateWindow` is
    answered `WindowBuildFailed { Webview }` without a build, no rebind occurs, the late
    receipt changes nothing, and the loop exits through the D6 fault reaction; its mirror with
    no entry pending: the poll at the top of the closure run latches, that run's exit check
    ends the loop, and the queued `CreateWindow` is never dispatched, built or answered (D8
    (3)); (k) an
    abnormal `BrowserProcessExited` for a pid other than the bound one (an older instance
    whose event fires after the rebind, with no live view of it left, so no
    `ProcessFailed` came first) latches `AbnormalExit`, while a normal one for such a pid
    is foreign and dropped; (l) the current instance B's normal receipt credited, then an
    older instance A's abnormal `BrowserProcessExited` queued, then a clean census (injected
    witness): the session ends `KELD-WV-009` (`AbnormalExit`) with the record `stopping`,
    never `idle`, and B's credit stays recorded; (m) a `ProcessFailed` of any kind other than
    `BrowserProcessExited` latches nothing; (n) a failed `add_ProcessFailed` registration fails the
    build through D12 (a) custody with the bind kept, so that browser's receipt is still required.
    Case (l) has no device count: its order
    needs an older browser still alive after the current instance's normal exit was
    credited and failing before the loop's last exit check, while a credited current
    receipt at a terminal ends the loop in that same turn (`exit_ready`, `:2728-2732`); no
    landed hook or device stimulus forces that order, so its real-Windows at-risk count is
    0 by construction and this unit case alone decides the rule. Each case
    has its single-fault
    mutant, which must turn its case red: a rebind that does not clear the credit, which
    with a clean census and B's receipt withheld writes `idle` (a1); no generation check, or
    pid-only matching without stamps (a2); no foreign-pid filter, so the late event is stamped with B's
    generation (b); a rebind on every completion, which fails the asserted generation 1
    (c); no ambiguity rule (d); discarding an abnormal receipt by generation (e); a
    rebind allowed after arming (f); a grace-only fallback without a handle (g); r14's "a failed build never rebinds" (h); retiring the old wake by `Drop` and then registering, which counts two registrations after a failed retirement (i); latching only on a `BrowserProcessExited` receipt, which builds the window and rebinds to the new pid before the late receipt latches (j); the pid filter applied to abnormal receipts too (today's handler, `:1576`), which drops the late one as foreign and leaves the session running (k); today's early return once a receipt is credited (`:2649-2651`) kept, so A's receipt is never drained and the clean census writes `idle` (l); latching on every kind, which ends the session on a renderer exit (m); dropping the bind on a failed registration, which leaves generation 0 so the census alone writes `idle` (n). *NC 5 (browser failure, real Windows, installed, positive control):* with window A open, the test terminates the bound browser (the pid of the D16 `REBIND gen=1` line), reads the first D16 `ABNORMAL` line, and only then has the fixture call `new BrowserWindow()`, so the latch precedes the `CreateWindow` in every counted run (a run with no `ABNORMAL` line within `PRODUCT_DEADLINE` is reported, not counted). Expected: the host exits `KELD-WV-009` with the record `stopping`, no `REBIND gen=2` line appears, no `msedgewebview2.exe` started after the kill is in the host's process tree, and the first `ABNORMAL` line names its source. A terminated browser is INFERRED to raise `ProcessFailed` kind `BrowserProcessExited` (reason `Terminated`: "The process was terminated. For example, from Task Manager.", <https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/webview2-idl>) and an abnormal `BrowserProcessExited`; their order is the runtime's, the docs do not guarantee it, and no hook can force it. Without that wait the outcome would race: a `CreateWindow` whose controller completed before the runtime delivered either signal would rebind once before the latch. So this cell is a positive control of the end state, not a mutant decider (the `ProcessFailed`-less mutant passes it, because the receipt latches first); AC21 NC 4 (j) and (k) decide the rules. INFERRED until T3 runs it. The `keld dev` lane's stage is AC19's,
    behind a separate, existing gate (the attempt Job reports no process,
    `app_session.rs:1597-1610`), not this census.
22. **No managed window without the release proof (staging gate, D1).** keld-core derives
    a Windows facade session's `window_native_support`; it is never a literal. It is
    `Some(true)` only when the keld-wv Windows engine reports managed-window support
    (added by T3 in the same diff as D4–D12 and D8) **and** the host lent the D7 release
    witness (U14, landed by T2); otherwise it is `Some(false)`. Given a facade boot on the
    exact T2 head (D7 and A21 landed, D4–D12 and D8 not), when `new BrowserWindow()` runs
    after Ready, then it rejects
    `KELD-CORE-042` (`window_router.rs:475-478`), no `CreateWindow` reaches the UI loop, the
    census is 0, the host stays live and a following `Quit` exits 0. A head whose host lends
    no witness answers the same, but no device build lacks one (the host installs the
    host-death Job on every Windows launch, `keld-host/src/main.rs:96-104`), so that row
    is NC 1's. *NC 1 (keld-core unit,
    fail-first at T2):* of the four input rows (engine support, witness), only (yes, yes)
    answers `Some(true)`; the mutant that ignores the witness (`Some(true)` without D7) and
    the mutant with r15's literal T2 flip each turn a row RED. *NC 2 (device, exact T2
    head):* the literal-flip mutant admits the `Create`, which the T2 loop still treats as
    an invariant fault (`webview2/mod.rs:3330-3343`), so the 042 rejection and the live host
    turn RED. Positive control at T3's head: with both inputs, AC6 builds the window, and
    AC21 (ii) writes `idle` only after the current instance's normal receipt and a clean
    census, so a retired browser that is still alive keeps the record `stopping`. T3
    retires NC 2's device test, whose refusal T3 removes on purpose (§6).
23. **No product facade boot before the release proof (staging gate, D2).** Product
    admission of a Windows schema-2 boot (the path a build takes without D2's T1 test
    admission) lands only with A21's `Running` write and D7's release witness (T2). One
    cell runs unchanged at every head from T1 on: an `InstalledApp` persistent project with
    `initialWindow: "none"`, launched without the test-admission env, that never builds a
    window. Either the launch is refused `KELD-CORE-034` before any `READY`
    (`app_session.rs:1340-1351` via `:1447`; `:3431-3438`), which is T1's required
    outcome, or it reaches `READY`, the fixture calls `app.quit()`, the host exits 0 and
    the lifecycle record (`read_lifecycle_record`) reads `idle` before any successor runs,
    which is T2's required outcome. Two controls qualify the T2 outcome: zero-controller
    (the D16 loop-exit readback reports environment-creation count 1 and
    controller-creation count 0, so the census alone decided, D7) and descendant-release
    (every pid of the host's pre-Quit `process_tree_report` snapshot is gone before the
    record is read, AC14's oracle). Any other outcome is RED. *NC 1 (fail-first at T1's
    exact head):* the test-admission check dropped, so a product build admits the boot at
    T1; it reaches `READY`, and `Quit` exits `KELD-WV-009` with the record `starting`,
    because `Running` is still written only by the first build (`mark_profile_running`,
    `:3149-3162`) and `advance` refuses `Starting → Stopping`
    (`crates/keld-wv/src/profile/lifecycle.rs:153-166`) → RED. *NC 2 (fail-first at T2's
    exact head):* reverting only A21's `Running` write turns the T2 outcome into the same
    `KELD-WV-009` with `starting` → RED (AC18 NC 1 on this fixture). *NC 3
    (environment-started descendant, ephemeral, T2's head, debug fixture):* a dev-stage
    facade boot with zero windows and the D16 descendant hook, which stands in for a
    runtime that starts a process at environment creation (none does on the U10 runtime).
    A run counts only when the helper's pid is in the pre-Quit `process_tree_report`
    snapshot and the D16 loop-exit readback (controller-creation count 0) appears while
    the helper lives. Expected: after that readback the host stays alive and the
    ephemeral profile root exists for 2 s (duration oracle) while the helper lives; the
    test then terminates the helper; the host exits 0, and the root is deleted only after
    the helper's pid is gone. Mutant: the zero-controller exemption without the census
    (today's `exit_ready` and `finish_profile_release` accept an unbound pid 0 at once,
    main `webview2/mod.rs:2728-2732`, `:3124-3128`) deletes the root while the helper
    lives → RED. The persistent counterpart (record `stopping` while a listed member
    lives) is AC27 (d), with AC18 NC 4 (a) and (b) as its units.
24. **A release is published only after native teardown (D6, A8).** Uses the D16 `destroyed`
    hold (U18): last in the `Destroyed` arm, after the stage step, the hook writes
    `DESTROY_HELD <hwnd>` and blocks the UI thread on the `WM_DESTROY` call stack until its
    release file exists, then writes `DESTROY_HOLD_END <hwnd>`. Three cells at T3's exact head:
    (a) an allowed close (`CLOSED 1`); (b) `destroy()` called through raw `invokeWindow`, whose
    reply the fixture prints (`DESTROY_REPLY 1`); (c) the U7 `hwnd` point in its default mode
    on an awaited raw `Create` (`ERR KELD-CORE-041`). In each, after `DESTROY_HELD` the test reads
    `IsWindow(hwnd)` TRUE, the at-risk proof that teardown is incomplete (a run that reads FALSE
    is reported and counted neither as a pass nor as a mutant kill), holds for 2 s (duration
    oracle) and asserts that no `RELEASED`, `FAILED_BUILD_ANSWERED`, `CLOSED`, `DESTROY_REPLY` or
    `ERR` line has appeared, then creates the file. Expected UI-thread order: `DESTROYED <hwnd>`
    (or `FAILED_BUILD_DESTROYED <hwnd>`), `DESTROY_HELD`, `DESTROY_HOLD_END`, `TEARDOWN_CONFIRMED
    <hwnd>`, `RELEASED <view>` (or `FAILED_BUILD_ANSWERED <build>`); then the control line, on
    which the test reads `IsWindow` FALSE for the HWND and for every child HWND captured before
    the stimulus. *NC 1 (publish at `Destroyed`, r17's rule):* the mutant writes `RELEASED` (or
    `FAILED_BUILD_ANSWERED`) before `DESTROY_HELD`, and the reply reaches the test while its
    `IsWindow` read is TRUE → RED, in (a), (b) and (c), because all three answers leave through
    the one publish function. *NC 2 (the `IsWindow` read, keld-wv unit; rule not exercised on a
    device):* with the read injected: a staged entry that reads TRUE publishes nothing and re-arms
    the recheck, then FALSE publishes exactly once; a failed wake post still publishes from the
    recheck; TRUE until the bound latches `ReleasingExpired`; the mutant that publishes on the
    first closure run after the stage, with no read, turns the first case red. The device at-risk
    count is 0 by construction: the hold blocks the thread without pumping, so the wake waits in
    the queue until it ends, and no closure run inside `DestroyWindow` after the callback was
    observed (0/7, §2). *NC 3 (value
    reissue, keld-wv unit, A20):* a build whose id equals a parked or a staged key publishes that
    entry once and continues, the later `Destroyed` for that id finds no entry and is ignored
    with one diagnostic, and an id equal to a live managed key answers `WindowBuildFailed {
    Window }`; the mutant that keeps r17's refusal for a releasing key fails the first case. That path has no
    `Destroyed` before its answer, so it writes `KEY_REISSUED <hwnd>` (D16) in place of `DESTROYED`
    and `TEARDOWN_CONFIRMED`, and a device run with that line is reported and void for the order
    oracles of AC10, AC16 and AC24. *NC 4
    (exit gate, keld-wv unit):* a staged entry keeps the exit predicate false; the mutant that
    counts only parked entries exits with an answer owed. *NC 5 (failed send, keld-wv unit):* the
    publish step with a dropped receiver records the owner failure and takes the `Fatal` path; the
    mutant `let _ = send` leaves the loop running with the answer lost.
25. **Events buffered during another window's controller pump (A7, exact head).** Uses the D16
    `build-pump` hold (U18): once `CreateCoreWebView2Controller` has returned, the build writes
    `BUILD_PUMP_HELD <build>` and keeps pumping in the same `pump_message_or_handle` loop until
    the release file for that build exists and the queue is then empty, and writes
    `BUILD_PUMP_EXIT <build>` before the callback returns. The stimulus therefore lands inside the
    pump by construction, not by timing against U1's 48–85 ms. (a) `CloseRequested`: window A has
    a `preventDefault()` listener; the fixture sends a raw `Create` for B, not awaited; on
    `BUILD_PUMP_HELD` the test posts `WM_CLOSE` to A and waits for `PUMP_SAW_CLOSE <hwnd>`, written
    by the hook when the pump removes that message, which is the at-risk proof (a run without it
    is void); it then creates the release file. Expected: `BUILD_PUMP_EXIT`, then one
    `CLOSE_DISPATCHED <hwnd>` (D16, written in the `CloseRequested` arm) and exactly one
    `CLOSE_EVENT 1`; the veto keeps A's exact HWND and child set; a later `WM_CLOSE` with `ALLOW`
    gives one `CLOSED 1` and A's `IsWindow` FALSE, and B stays live. (b) `Destroyed`: a carrier
    build Z is held; while it is held the fixture sends a raw `Destroy` for A and a raw `Create`
    for B, and the test waits for both `BRIDGE_POSTED` lines (D16, written by
    `spawn_app_wake_bridge`) before releasing Z. The pump buffers both commands (Tao buffers while
    the callback runs, `runner.rs:216-222`) and Tao then dispatches them back to back with no
    message loop between them (`runner.rs:259-271`), so A's posted destroy is first dispatched
    inside B's pump, which holds on `BUILD_PUMP_HELD <B>`. At-risk proof: the test reads
    `IsWindow(A)` FALSE while no `DESTROYED <A>` line has appeared (a run that shows `DESTROYED`
    first is void); it then releases B. Expected: `BUILD_PUMP_EXIT <B>`, one `DESTROYED <A>`,
    `TEARDOWN_CONFIRMED <A>`, `RELEASED`, one `CLOSED`, and B builds. *NC 1:* the local mutant that
    drops the first `CloseRequested` delivered after a completed build leaves no
    `CLOSE_DISPATCHED` and no `CLOSE_EVENT` before `PRODUCT_DEADLINE` → RED. *NC 2:* the mutant that
    drops the first `Destroyed` delivered after a completed build leaves the entry parked, so no
    `CLOSED` arrives before `PRODUCT_DEADLINE` and the host then exits `KELD-WV-009`
    (`ReleasingExpired`) → RED. U1's delivery counts are prior evidence (§2); these cells qualify
    the T3 code path. Both INFERRED until T3 runs them.
26. **Host-window obligations reach every controller (D10, D11, A26).** Uses the D16 page
    readback (U18): `PAGE_READBACK <view> <json>` carries `screenX`, `screenY`,
    `document.visibilityState` and `document.hasFocus()`. On a facade window, in this order, because the origin was once refreshed with no call by a step after
    the moves (§2: unattributed, between the moves and the later reads, and a later minimize and
    restore did not do it), so the move is read before any activation or restore:
    (a) *move without resize:* the test calls `SetWindowPos` with `SWP_NOSIZE | SWP_NOZORDER |
    SWP_NOACTIVATE`, the probe's flags; the witness moves by `(dx, dy)` and
    its size is unchanged, a D16 `PARENT_MOVED <view>` line appears, and within 5 s the readback's
    origin moved by `(dx, dy)` at the window's scale, within one logical point. (b) *minimize:*
    the test calls `ShowWindow(SW_MINIMIZE)`: the readback reaches `hidden`, and `getPosition()`,
    `getSize()` and `isMaximized()` keep their last values, while `getFocusedWindow()` reads null,
    because focus facts are not suppressed (a run where the minimize left the window active is void
    for this read); after `SW_RESTORE` it reaches `visible`, the witness equals the pre-minimize
    rect, and, when `GetForegroundWindow()` is the window (otherwise this read is void), `GetGUIThreadInfo(0)`
    names a child focus HWND and `hasFocus` is true. The same from a maximized window: `isMaximized()`
    stays true throughout, the fixture counts no maximized fact across the minimize and the restore,
    and the restore returns to the maximized rect with the readback `visible`, because the rule
    branches on the state restored to. (c) *focus return:* with the window active and
    focus in the page, the test activates a helper window and then re-activates this one, and
    `GetGUIThreadInfo(0)` names a focus HWND with `IsChild(hwnd, focus)` true (a run where
    `GetForegroundWindow()` is not the window after either step is void, because foreground rules
    can deny it), and the readback's `hasFocus` is true. (d) The controller half of the same three
    (no public facts exist there) on a renderer boot's legacy window, with AC3's other assertions
    unchanged. *NC 1 (issue mutant):* suppressing only
    the notification keeps the readback origin stale past the 5 s bound → RED; FACT (r18 probe,
    §2): 0/12 moves followed without the call (stale after 3 s, 3/3), 16/16 within 400 ms of it;
    stale past 3 s is INFERRED. *NC 2:* suppressing only
    `SetIsVisible(false)` keeps `visible` → RED (r18: `visible` 7/7 before the call, `hidden` 4/4 after it). *NC 3:* forwarding facts
    while minimized moves `getPosition()` to `-32000` at the window's scale and clears `isMaximized()`
    → RED. *NC 4:* removing the focus return
    leaves the focus HWND on the top-level and `hasFocus` false → RED (r18: top-level 7/7 before the
    call, child 4/4 after it). The r18 numbers are prior evidence from confounded arms (§2); these
    NCs are the single-fault proof, each suppressing one call.
    *NC 5:* the single fault that iterates only the managed map turns (d) RED and leaves (a)–(c)
    green. *NC 6:* also suppressing focus facts while minimized leaves `getFocusedWindow()` on the
    window → RED. *NC 7:* reading `WindowMaximized` on `Moved` writes a transient `false` at the
    restore to a maximized window, before the `Resized` that sets Tao's flag → RED (INFERRED from
    source, §2: never run). *NC 8:* removing only the restore's focus return (D10) flips only if
    the restore's `Focused(true)` precedes its `Resized`, which is unmeasured; if it follows, the
    receipt records at-risk 0 and the call is defence only. `run_until_closed`'s loop (`keld hello`)
    calls the same method, and no cell covers that call site (rule not exercised there).
    **What stays unproven:** Microsoft names accessibility and "certain dialogs"
    (<https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#notifyparentwindowpositionchanged>),
    and this cell observes neither. The UIA document rectangle followed the move with no call
    (7/7, §2), so it cannot discriminate, and no WebView2 dialog position was measured (UNKNOWN);
    the page's `screenX`/`screenY` is the one discriminating observable found, on one 125% display.
27. **Release proof on every boot and release kind (D7, A10).** Each cell uses the D16
    descendant hook (a host-death Job member outside every Bun generation Job) and counts only
    when the helper's pid is in the pre-terminal `process_tree_report` snapshot and the helper is
    alive at the D16 loop-exit readback. Expected in each: after that readback the release does
    not complete for 2 s (duration oracle) while the helper lives; the test then terminates the
    helper and the release completes only after its pid is gone. (a) **persistent, renderer boot**
    (`InstalledApp`, T2): the test posts `WM_CLOSE` to the last legacy window; the record reads
    `stopping` during the hold, `idle` after, and the host exits 0. (b) **ephemeral, renderer
    boot** (dev stage, T2): the profile root exists during the hold and is deleted only after the
    helper's pid is gone. (c) **in-host handover, renderer boot** (`profiles/handover.rs`, the
    operator fixture of h9–h12, T2): no `LastWindowClosed` and no second engine construction
    during the hold (`complete_profile_handover` writes it after the loop returned,
    `app_session.rs:3304-3305`); the second session follows the helper's exit. (d) **persistent,
    zero-window facade boot** (T2): `stopping` during the hold, `idle` after. (e) **persistent,
    facade boot with a closed window** (T3): the same after the current instance's receipt.
    The ephemeral zero-window facade cell is AC23 NC 3; handover with registry windows is a
    non-goal (U8). The bounded refusal (`KELD-WV-009`, record `stopping`) when the member outlives
    the deadline is AC18 NC 4 (a); a device cell would wait the 85 s post-exit budget. *NC (a)–(e):*
    each is the single fault that bypasses the witness on that kind only, which writes `idle`,
    deletes the root or starts the second session while the helper lives → RED, leaving every
    other kind's cell green. INFERRED until T2 (T3 for (e)) runs them.

## 4. Design

### 4.0 First-principles and reuse decision

Boundary change: yes. Handle ownership does not move (keld-wv UI thread keeps HWND and
controller; keld-core keeps identity and close policy, gh531 §4.0). Crash ownership is
unchanged (host process; Bun loss takes gh531 T9). Principal minting is unchanged. What
changes is *policy on an existing primitive*: Tao converts `WM_CLOSE` into a cancelable
`CloseRequested` and returns `LRESULT(0)` without `DefWindowProc` (FACT, tao 0.35.3
`src/platform_impl/windows/event_loop.rs:1061-1068`); the HWND dies only when the Tao
`Window` is dropped, which posts `DESTROY_MSG_ID` (FACT, `window.rs:1106-1113`) handled
by `DestroyWindow` on the loop thread (FACT, `event_loop.rs:2238-2240`), whose
`WM_DESTROY` emits `WindowEvent::Destroyed` (FACT, `event_loop.rs:1070-1079`), still
inside that `DestroyWindow`, so the event is not completed teardown (A8). So no
second WndProc is needed; Keld chooses *when* to drop.

| # | Atom | Owner / boundary (in → out) | Failure mode | Observable | Edges | First falsifier | Status |
|---|---|---|---|---|---|---|---|
| A1 | Windows Init milestone | keld-wv Windows loop → keld-core coordinator; in: first `NewEvents(StartCause::Init)` of `run_return` (FACT, tao `event_loop/runner.rs:310-312`); out: `HostInitialized { observed_at }`, then Ready | Ready before the loop runs, or never for a zero-window boot | AC1, AC2 | ← A13 | Ready on `HELLO` | FACT: the loop already observes its first turn (`webview2/mod.rs:3262-3270`); emission is the delta; reuse `first_host_init_turn` generalised over the user-event type (`wkwebview/mod.rs:1039-1045`) |
| A2 | Schema-2 admission | keld-core `validate_windows_boot_files` / `run_app_direct`; in: descriptor; out: `ValidatedInitialWindow::None` | unverified descriptor admitted; renderer boot changed | AC3 | → A12, A17, A18 | `schema 2` plus `renderer` admitted | FACT: refused at `:1447` and inline at `:3431-3438`; today `main` matches on `parsed.initial` (`:1450-1451`); its `None` arm exists (`:1458`) but is unreachable because `:1447` refuses first. The `main` test `app_session/tests/windows_profile_and_boot.rs::windows_boot_files_refuse_a_schema_2_facade_boot` (1372fbd3) pins the `KELD-CORE-034` refusal that D2 removes; T2 replaces it with AC3's admission test, in the diff that admits product facade boots (D2, AC23), as a recorded contract change, not a weakened test |
| A3 | Multi-controller, one lease, one environment | keld-wv engine; in: `CreateWindow` N; out: N `View`s on the one `ICoreWebView2Environment` | per-window environment or lease | AC15 | ← W6 for profile kind | second `webview2*` directory | FACT: one environment per engine (`:2851-2855`) |
| A4 | Close-veto custody | loop `CloseRequested` arm → registry; out: `WindowCloseRequested { view }` only | immediate drop (`close_requested_view`) | AC9 | → A8, A9, A20 | HWND changes on Cancel | FACT primitive; delta is policy |
| A5 | Last window / quit / relaunch | registry T11 → facade policy → `Quit` → loop terminal → views dropped, `Stopping` committed, barrier armed → pending `releasing` entries published (D6 exit gate) and the current browser instance's normal exit receipt under the hang guard and grace (A24) → host-death Job census, Bun family excluded → `Idle` (persistent) or deletion (ephemeral) or handover → scavenge | residual descendant or lease blocks relaunch; an unarmed barrier waits without a bound | AC14 | ← A4, A9, A10, A21, A24 | the arm readback (D16) lacks `armed_by=Quit`; a withheld receipt is not bounded | FACT: terminal path exists (`:3317-3329`, `:3380-3397`). FACT: arming does not decide whether the browser exits: `poll` drains a receipt before the armed check (`:2662-2670`, `:2685`) and `View::drop` closes the controller (`:2836-2843`); arming only bounds the wait (hang guard, grace), so AC14's controls observe the arm and the bound, not a lingering process |
| A6 | Stale/forged pair | gh531 admission (`KELD-CORE-038`) before any UI command | forged pair reaches a successor | AC11, AC12 | shared | B destroyed by `(B.id, A.gen)` | FACT: shared registry; proved end-to-end here |
| A7 | In-loop build with a nested pump | loop; in: `CreateWindow` + `&EventLoopWindowTarget`; out: `WindowBuilt`/`WindowBuildFailed` | controller creation pumps messages inside Tao's callback with Keld's own loop (`:597-617`, pump `:554-589`; not `webview2_com::wait_with_pump`); Tao buffers re-entrant events (`runner.rs:208-227`); loss/reorder of a buffered `CloseRequested` or `Destroyed` | AC6, AC15, AC25 | → A4, A20 | a `WM_CLOSE` during the pump is lost | FACT (probe, U1 resolved, prior evidence on an older path, §2; AC25 is the exact-head cell): the pump completes inside `run_return`'s callback for a first controller in 306–394 ms (median 342, n=10, delay 0) and 311–387 ms (n=10, delay 30), for a second controller in 48–85 ms (n=9), with zero windows in 352–477 ms (n=3); a `WM_CLOSE` to another window and a user event posted during the pump were delivered after the callback returned, exactly once, in post order (`close_requested_A_counts` and `second_counts` all 1; 10/10, 10/10, 6/6 with the stimulus inside the pump, 9/9 delivered, 3/3 quiet); `reentrant_total` 0, `panics_total` 0; the pumping thread spends 0–47 ms CPU per build (no busy spin). A build runs from a `UserEvent`, never inside a WebView2 event handler, where Microsoft does not support a nested loop (<https://learn.microsoft.com/microsoft-edge/webview2/concepts/threading-model>, "Reentrancy"). Rejected: asynchronous completion (`WindowsLoopEvent::ControllerReady { build }` posted from the handler), no longer needed for correctness while AC25 passes |
| A8 | Released = native teardown finished | loop; in: `DestroyWindow { view }`, Tao `Destroyed` for the dropped window's id, then a later closure run; out: `WindowReleased` or `WindowBuildFailed` after `IsWindow(hwnd)` reads FALSE (D6) | release reported at `View` drop, or at `Destroyed`, which fires inside `WM_DESTROY` while `DestroyWindow` is still on the stack, so an independent core thread could answer the caller before teardown ended | AC10, AC16, AC24 | ← A4; → A20 | `CLOSED` while `IsWindow` | FACT (tao): `event_loop.rs:1070-1079`, `:2238-2240`, `runner.rs:216-226`. FACT (docs): "This message is sent first to the window being destroyed and then to the child windows (if any) as they are destroyed" (<https://learn.microsoft.com/windows/win32/winmsg/wm-destroy>); "The WM_NCDESTROY message is sent after the child windows have been destroyed" (<https://learn.microsoft.com/windows/win32/winmsg/wm-ncdestroy>). FACT (r18 probe, §2): `IsWindow` TRUE at `Destroyed` and FALSE at the first posted wake, 7/7 |
| A9 | Profile `Stopping` is irreversible | keld-wv profile lifecycle; in: `commit_profile_stop`; out: on-disk record | committing on `CloseRequested` strands a vetoed session | AC18 NC2 | → A5, A21 | `Stopping` after Cancel | FACT: Starting→Running→Stopping→Idle only (`lifecycle.rs:153-176`); committed today at `:2822-2825` |
| A10 | Release witness for a persistent profile | keld-runtime host-death Job (`windows_job.rs:3167`, installed before any WebView2 process, `keld-host/src/main.rs:96-104`, KILL_ON_JOB_CLOSE with no breakaway, `:2464-2500`, unnamed and non-inheritable, `:3068-3071`, so only descendants of members can join); in: settled snapshots of the Job's process list taken after the current instance's receipt (A24), with the host and the members of the session's Bun generation Jobs (the census "family") excluded; out: released when no other member is listed | proof from one watched browser pid misses an overlapping older browser (regime ii) or a launched-but-unbound one; without the Bun family exclusion the census never comes out clean while Bun lives (Bun is a direct member today, `app_session.rs:3359`, `:3706-3737`); a reused listed id can only force a retake, because reuse implies the member died; terminating a member mid-write could corrupt the profile; a clean census proves no listed member is alive, not that the current browser raised its normal exit (A24 owns that half) | AC14, AC18, AC21 | ← A3, A21; ↔ A24 (both required, neither implies the other); → D7 | `idle` with a live WebView2 process, or every `Quit` failing | FACT (research, §2): every observed WebView2 process inherited the Job (284/284; 430 across 11 kinds, 0 outside); an out-of-Job (service-started) process: no claim (D7); the Job emptied 0.2–0.4 ms after the last `BrowserProcessExited`; the folder was quiet after it emptied (0 sharing violations in 245,430 opens); `GetProcessInfos` and the lockfile fail open. FACT (code): `exit_census` (`windows_job/host_death_census.rs`) observes **and terminates** members for the update exit; the profile witness must be observe-only |
| A11 | UI thread is a COM STA | `ComSta` `!Send` (`:489-510`); thread-affine controllers; `View::drop` closes on the engine thread (`:2836-2843`) | handle touched off-thread | every AC | gh531 A2 | off-loop `DestroyWindow` | FACT; no change |
| A12 | Host-owned facade hint | `windows_primary_config` (main `:3359-3372`); out: child env | app or launcher forces the barrier | AC4 | ← A2; → A18 | inherited `"1"` on a renderer boot | FACT: Windows already forces `"0"` for its renderer boot (`:3368-3370`); macOS derives the mode from the validated boot kind once and passes it both as a strict private Guardian argument and in the environment (`configure_guardian_boot_mode`, `app_session.rs:2342-2352`), and the Guardian adds Bun's literal define (`keld-host/src/main.rs:491-495`); the transport's mode expression is `transport.ts:32-33` |
| A13 | 15 s Init watch | keld-core `InitTerminalWatch` (`:4543`, macOS-only `:4606`, armed `:2603`); Windows terminal `fn(&HostAppError) -> !` | unbounded windowless boot | AC2 | → A1 | a stalled child lives past 15 s | FACT: macOS-cfg today; `run_guarded`/`run_unprivileged` take the terminal only on macOS (`:1888-1890`, `:1947-1949`; `keld-host/src/main.rs:147-156`) |
| A14 | Focus fact = activation | loop; in: Tao `Focused`, controller `GotFocus`/`LostFocus`, and a synchronous `GetActiveWindow()` query on the UI thread; out: `WindowFocus` | Tao's fact dies once a child HWND holds focus; a raw forward emits a transient blur | AC13 | ← A11 | raw `Focused(false)` after `MoveFocus` | FACT from source: Tao emits `Focused` only when `is_active && is_focused` changes (`window_state.rs:191-193`, `:196-207`); `WM_KILLFOCUS` on the top-level clears `is_focused` (`event_loop.rs:1800-1806`); activation alone (`WM_NCACTIVATE`, `:1779-1781`) then never re-emits, and Tao exposes no activation event. On reactivation of a window that is "not minimized", `DefWindowProc` "sets the keyboard focus to the window" (<https://learn.microsoft.com/windows/win32/inputdev/wm-activate>; Tao has no `WM_ACTIVATE` arm; a restore is D10's case), so `WM_SETFOCUS` emits `Focused(true)` (FACT from source; the probe logged no `Focused` at that step) while the page has lost the keyboard (FACT, r18 probe, 7/7; A26). `GetActiveWindow` returns the active window of the calling thread's queue (<https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getactivewindow>), which is this UI thread. INFERRED: the controller hosts a child HWND that takes keyboard focus after `MoveFocus` (`:3637-3641`); U2 confirms. Rejected: deferring Tao's blur until a controller `GotFocus` cancels it (the COM event is asynchronous, INFERRED, so a transient blur cannot be excluded); a WndProc subclass (forbidden) |
| A15 | Zero-window census oracle | test `support/window.rs`; `EnumWindows` + `GetWindowThreadProcessId` + `IsWindowVisible` | `MainWindowHandle == 0` cannot prove zero | AC1, AC6, AC10 | test-only; → A23 | hidden window counted as zero | INFERRED; macOS states the same limit (`facade_ready.rs:19-24`) |
| A16 | Close stimulus | test; `PostMessageW(hwnd, WM_CLOSE, 0, 0)` | not the user path | AC9, AC10 | test-only; → A23 | no `CLOSE_EVENT` | INFERRED; `WM_CLOSE` is what the caption close produces (<https://learn.microsoft.com/windows/win32/winmsg/wm-close>); U5 |
| A17 | Legacy path unchanged | loop legacy arm; `create_app` single slot (`:3176-3184`); renderer boots keep their close path, window slot and navigation path; what changes for them is the launcher (a staged sidecar is preloaded for them too, A18/AC20), the per-view host obligations, whose one owner covers every view (A26), and the release: the census (D7, AC27 (a)–(c)), the `Running` moment (A21), the fault reaction (D6) and an abnormal exit reported before the loop-entry bind (D8 (3)) | renderer boots routed through the veto path; or a renderer boot exempted from the preload rule | AC3, AC20 | compat fallback; ← A18 | existing lifecycle test fails; a renderer app evaluates the transport late | FACT: gh531 criterion 15; the macOS guardian preloads for every primary it spawns (`main.rs:392`) |
| A18 | Transport sidecar: one admission owner, one preload rule | one helper used by the Linux strict launcher, the macOS guardian and the Windows `PrimaryRoleConfig` (`crates/keld-runtime/src/role.rs:390`); in: stage root; out: admitted sidecar path (then `--preload=`), `None` when absent, or `KELD-CORE-037` | three copies today with different strictness: Linux `admit_kipc_transport_sidecar` uses `symlink_metadata` and refuses symlinks (`app_session.rs:3899-3915`, called `:3929`); macOS uses `std::fs::metadata`, which follows symlinks (`keld-host/src/main.rs:497`); Windows file admission refuses reparse points (`open_relative_file_windows`, `:1731-1783`). Also: `bunfig` overrides define and preload, and inherited `BUN_OPTIONS` runs first; macOS on `main` refuses the first on every launch and removes the second (`keld-host/src/main.rs:478-520`) | AC5, AC20 | ← A12 (removes the env hazard at its root) | a junction sidecar is preloaded on one platform | FACT (three copies read); decision: fold into one owner with the strictest rule (refuse symlinks and reparse points everywhere; `bunfig` refusal on every launch; the boot-mode define; `BUN_OPTIONS` removal), applied to every boot kind whose stage carries the sidecar (the compiler stages it regardless of boot kind, `boot.rs:483-493`; the macOS guardian preloads for every guardian-spawned primary, `main.rs:392`). Rejected: a Windows exemption; keeping three copies |
| A19 | Outer-frame size | loop build; in: `CreateWindow` outer logical `width`/`height` (`engine.rs:104-115`); out: window whose `GetWindowRect` matches | Tao has only `with_inner_size` (tao `src/window.rs:337`) and no outer setter; passing outer as inner inflates the frame | AC6 | ← W4 units; → A23 | witness larger by the frame | INFERRED method (D14): build `with_visible(false)`, read the frame delta from `outer_size() - inner_size()` at the window's scale (`outer_size` is `GetWindowRect`, tao `platform_impl/windows/window.rs:264-270`), `set_inner_size(outer − delta)`, then `set_visible(true)`; no new FFI in Keld. Rejected: calling `AdjustWindowRectExForDpi` from Keld, only because it adds a new FFI entry point and unsafe gate; Tao already applies it inside `set_inner_size` (tao `platform_impl/windows/util.rs:91-101`, `:143-150`). Whether that adjustment and `GetWindowRect` agree on Windows 11 frames is U12 |
| A20 | HWND-value reuse in `releasing` under asynchronous destroy | loop release map keyed by Tao `WindowId`, which is the HWND value (`event_loop.rs:1064`) | a late `Destroyed` releases a successor that reused the value; an entry assumed to resolve within the dropping callback | AC10, AC16, AC24 NC 3; unit test | ← A7, A8, A25 | reused key releases the wrong view; an entry never published | Corrected premise: Keld never calls `DestroyWindow`; dropping a Tao `Window` only posts `DESTROY_MSG_ID` (tao `window.rs:1104-1113`), which Tao's WndProc turns into `DestroyWindow` in a later message-loop turn (`event_loop.rs:2238-2240`). FACT (probe): `Destroyed` arrived 22–61 ms after the callback that dropped the views (median 31 ms; 70 window drops over 38 runs, derived from the `runs\u1-*.json` timelines as `WindowEvent(X, Destroyed)` minus the `UserEvent(Drain)` dispatch), so a `releasing` entry outlives its callback and any commands dispatched in between. Reuse argument under asynchronous destroy: the HWND value cannot be reissued while the window exists, and it exists until that posted message runs on this same UI thread; inside that `DestroyWindow` call `WM_DESTROY` is delivered synchronously (<https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-destroywindow>) and on the unbuffered path Tao dispatches it before the loop takes another message (`runner.rs:225-226`), so there the old `Destroyed` precedes any build that could receive the value. Buffered correction (r18): that order holds only on the unbuffered path. A `Destroyed` raised inside another build's pump stays in Tao's buffer behind a `CreateWindow` buffered earlier (`runner.rs:216-222`, `:259-271`), so that build runs after the native destruction and before the event, and could receive the value while the old entry is still parked. Teardown is keyed once: the `releasing` map's key is the Tao `WindowId`, which is the HWND value, and a stage or wake identity would add nothing, because the sweep reads each entry's own HWND and two entries can never share a key. `IsWindow` cannot be fooled toward a false release: FALSE means no window has the value, so the old one is gone. A recycled value can only turn FALSE into TRUE ("because window handles are recycled the handle could even point to a different window", <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindow>): a successor from any process delays publication, and if it outlives the bound the session ends `ReleasingExpired`, fail closed. No exact-value reissue was observed, and how often one can occur is UNKNOWN. Rule: an entry is staged once and published once; intervening events for a releasing HWND are ignored (D5); a `Destroyed` with no entry is ignored with one diagnostic, including one whose id is a live managed key, which can only be the late event of a reissued value: "A thread cannot use **DestroyWindow** to destroy a window created by a different thread" (<https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-destroywindow>) and this thread destroys only through Tao's drop, so the out-of-band close that macOS fails closed on (`wkwebview/mod.rs:590-617`, `NSWindow.close()`) has no Windows analogue; a build whose id equals a parked or staged key proves the old HWND is gone, because a live window's value is not reissued (INFERRED from the handle contract), so it publishes that entry first and continues (AC24 NC 3); a build whose id equals a live managed key is `WindowBuildFailed { Window }` (fail closed; unreachable unless the map is stale); every entry, `Destroy` or `FailedBuild`, is bounded by the releasing bound (D6), whose expiry latches the barrier's one fault (`ReleasingExpired`, `KELD-WV-009`) and takes the D6 fault reaction, so a lost `Destroyed` ends the session observably instead of leaving a pending answer; and the loop never exits while an entry is pending (D6 exit gate), so a terminal command cannot overtake an owed answer |
| A21 | Persistent `Running` moment | `from_selected_environment` (the one constructor `WebView2Engine::new` and the media-acceptance fixture share, `media_acceptance.rs:1157`); in: the environment bound to the verified user-data folder (`create_environment_for_profile_with_options_until`, `:1455-1470`: exclusive-UDF option, `environment_user_data_folder` equals the leased handle, ACL validated) and the `BrowserProcessExited` observer registered (`:3000-3018`); out: record `Running` written as that constructor's last step, so the loop-entry `PersistentStopTransition` (`:3250` `run_app_until_quit`, `:3039` `run_until_closed`; `:433-464`) snapshots `Running` for every boot kind | today `Running` is written only by the first build (`mark_profile_running`, `:3149-3162`, called `:3560`), so a facade boot that quits with zero windows holds `Starting` and `Quit` fails `KELD-WV-009`; with the write moved earlier, a launched-but-unbound browser must not be missed, which the census (A10) covers by Job membership rather than by pid | AC18 | ← A9, A10 | `KELD-WV-009` on zero-window facade `Quit` | FACT (device, U10 bundle, WebView2 156.0.4314.50 on Windows 11 26300.9711 only): creating the environment with the exclusive-UDF option, then reading its user-data folder back and registering `BrowserProcessExited` in Keld's order, launches no browser process (0 `msedgewebview2.exe` descendants: 5 s settle 8/8, 15 s settle 2/2; no exit event); the first controller then launches 6 (10/10, positive control). Keld's own environment doc already states the same (`:1432-1437`). FACT (code): macOS writes `Running` as soon as its store is bound at boot (`wkwebview/macos_profile.rs:457-461`); Windows recovery treats `Starting` and `Running` alike (`lifecycle.rs:469-479`). Decision D7 |
| A22 | Readiness barrier order | `@keld/api` readiness adapter; public `app.ready` only after Host Ready + accepted window `Subscribe` + replay (`app.ts:293-299`); W4 desktop facts join the same barrier later (#481) | construction before replay | AC8 | → W4 | `Create` before `Subscribe` | FACT (shared); Windows adds nothing |
| A23 | Test FFI feature | test crate; `EnumWindows`, `EnumChildWindows`, `GetWindowThreadProcessId`, `IsWindow`, `IsWindowVisible`, `GetWindowRect`, `PostMessageW`, `SetWindowPos`, `ShowWindow`, `GetGUIThreadInfo`, `GetForegroundWindow`, `SetForegroundWindow`, `IsChild` need `windows-sys` feature `Win32_UI_WindowsAndMessaging`, enabled neither in the workspace line (`Cargo.toml:142`) nor on keld-host's dev-dependency (`crates/keld-host/Cargo.toml:36`) | silent build failure of the proposed tests | §7 | test-only | the helper does not compile | FACT; the feature is added on the keld-host dev-dependency line only (`windows-sys = { workspace = true, features = [...] }`), never on the workspace line, so production crates gain nothing (dependency gate, §8). D6's production `IsWindow` is a separate entry through keld-wv's `windows` crate (§8) |
| A24 | Exit receipt bound to the current browser instance | keld-wv `ProfileReleaseWait`, Host site only (the one barrier owner); in: the binding `(generation, pid)` the loop writes at each controller completion of every build, failed or not (D8 (2)), the handler's reported pid and exit kind (`:1559-1593`; an abnormal exit is queued whatever its pid and drained until the loop's last exit check, even after the current instance's normal credit, D8 (3)) and each managed view's browser `ProcessFailed` (D8 (3)); out: a normal receipt credited to the current generation, or a fault | a session-wide sticky `observed` (`:2668`, accepted by `exit_ready` `:2728-2732` and `finish_profile_release` `:3124-3128`) lets an earlier browser's receipt satisfy a later browser's barrier; the receipt is drained even before arming (`:2662-2670`) and the loop polls every turn (`:3271`); the handler matches by pid only (`:1568`, `:1576`), and a queued receipt carries no pid, so one queued for A and drained after B's bind would be credited to B; and `poll` stops draining once a receipt is credited (`:2649-2651`) and reads one receipt per call (`:2662`), so an abnormal receipt queued behind the current normal one is never read | AC21 NC 3, NC 4; AC14 NC 2 | ← A7 (completion order), A3; ↔ A10; → A5 | stale receipt plus clean census writes `idle` | FACT (code, above). FACT (probe, U6): a new controller after the last close gets a new pid (8/8); the old pid's event can arrive during the new creation pump (12/12). FACT (docs): a process id "is valid from the time the process is created until all handles to the process are closed and the process object is freed" (<https://learn.microsoft.com/windows/win32/api/processthreadsapi/ns-processthreadsapi-process_information>), and Keld holds the bound instance's handle until the rebind, so a new browser cannot reuse the bound pid; D8 treats a same-pid report with a signaled handle as inconsistent and fails closed. Decision D8 |
| A25 | Failed-build HWND custody | keld-wv in-loop build; in: a failure at any step after the Tao `Window` exists; out: `WindowBuildFailed { Webview }` at D6's publish step; a controller that exists has bound its browser instance or latched `LifecycleUnproven` (D8 (2)) | before `View` construction there is no `View` to drop: main `create` builds the `Window` (`:3527-3541`), then `create_controller` (`:3544`) or `CoreWebView2` (`:3545-3547`) can fail, and only then is the `View` built (`:3551`); a controller created before a `CoreWebView2` failure is only closed implicitly at its final release (<https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#close>); D12 closes it explicitly | AC16, AC18 NC 5, AC24 (c) | ← A7, A8; → A20, A24 | `IsWindow(hwnd)` true when `KELD-CORE-041` is read; no answer at all | FACT (code, above); decision D12 |
| A26 | Per-view host obligations | loop arms → one `View` method over both maps; in: Tao `Moved`, `Resized`, `Focused`; out: controller calls (D10, D11) | a child WebView2 gets no ancestor-move or minimize message, so its screen origin goes stale, a minimized page keeps running as visible, and the page loses the keyboard after every reactivation; a public bounds fact reads `-32000` | AC26 | ← A11, A14; → A17 | `screenX` stale after a move | FACT (docs): "This is needed for accessibility and certain dialogs in WebView to work correctly"; "WebView2 as a child window does not get window messages when the top window is minimized or restored. For performance reasons, developers should set the `IsVisible` property of the WebView to `FALSE` when the app window is minimized and back to `TRUE` when the app window is restored" (<https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller>). FACT (code): no production call of the first at `457a9c3b`, `Resized` only re-fits (`:3346-3351`), `SetIsVisible` runs only at create (`:3604`). FACT (source): wry 0.56.1's parent subclass (wry `src/webview2/mod.rs:1296-1332`) does three things Keld lost when it left wry on Windows: no re-fit on `SIZE_MINIMIZED`, `MoveFocus` on `WM_SETFOCUS`, the notification on `WM_MOVE`/`WM_MOVING`; it has no `SetIsVisible` on minimize, so that call follows Microsoft's text, not wry parity. FACT (r18 probe, §2): the discriminating observables below, with confounded arms, so each effect is attributed by its own observable and timing, not by a single-call A/B; AC26's mutants are the single-fault proof. FACT (tao): `Moved` comes from `WM_WINDOWPOSCHANGED` unless `SWP_NOMOVE` (`event_loop.rs:1211-1225`), before `DefWindowProc` raises `WM_MOVE`, so it covers each real position change, and `WM_MOVING` only precedes one; FACT (r18 probe): `Moved` fired for `SetWindowPos`, `set_outer_position`, maximize, restore, minimize and borderless full screen in both directions. FACT (tao): `Resized` is `WM_SIZE` and is `(0, 0)` on minimize (`:1227-1235`). Walk of the rest: `put_RasterizationScale`/`BoundsMode`: no call, because the WebView tracks monitor scale itself while `ShouldDetectMonitorScaleChanges` is true and Keld passes physical pixels (true and raw pixels are not stated as defaults by the docs; read back in the r18 probe, §2; U3 stays open for mixed-DPI); `Close` before the HWND: already `View::drop`'s order (`:2836-2843`) and D12's; hide/show: no W2 command exists (`engine.rs:93-139`), so minimize is the only hidden state; `put_ParentWindow`, `add_MoveFocusRequested`, `add_AcceleratorKeyPressed`, `put_ZoomFactor`, `DefaultBackgroundColor`, `add_WindowCloseRequested`, `TrySuspend`: not W2 obligations, owners in §1 |

Promoted edges: A4→A8→A5; A9 and A10 and A21 are hidden couplings inside today's
`CloseRequested`/loop-entry code and are each their own atom; A7→A4/A20 (ordering of
buffered events during a build); A13→A1; A18→A12 (the launch define fixes the hint before
any app code); W4→A19/A22; A24↔A10 (the receipt and the census are separate halves of
one release proof, each with its own falsifier); A7→A24 (the rebind happens at
controller completion, so an old receipt fired inside the nested pump carries the old
generation); A25→A20 (a failed build parks its HWND in the same `releasing` map); A25→A24 (a failed
build whose controller exists binds that browser like a successful build, so the
zero-bound exemption cannot hide it); A7→A8 (a `Destroyed` buffered in a pump arrives after
`DestroyWindow` returned, an unbuffered one before); A26→A17 (the one per-view owner also
serves legacy views).

Ownership, process, memory, I/O, lifecycle, trust and failure facts:

- Handles: HWND, controller and webview stay in keld-wv's `View` (`:2337-2341`) on the
  UI thread; the registry maps `WindowId → WebviewId` (gh531 §4.a). No handle crosses kipc.
- Process: one browser process per user-data folder (A3), possibly overlapping a
  retiring one (U6). Release is proven by two separate halves, both required: the
  current browser instance's normal `BrowserProcessExited` receipt through
  `ProfileReleaseWait` (`:2529-2749`), bound to that instance (A24, D8), and the
  observe-only host-death Job census (A10, D7), which covers every older generation and
  any browser a failed `create_controller` may have launched (whether one can is UNKNOWN,
  D7).
- Queues: core → UI through `AppWindowCommand` and `spawn_app_wake_bridge` (`:3480-3513`);
  UI → core through the one FIFO `AppWindowEvent` sender. Core owns last-sent state and
  changed-only filtering (`wkwebview/managed_windows.rs:418-427`); keld-wv keeps no
  mirror. No new thread, socket or steady-state allocation.
- Lifecycle: Starting→Running→Stopping→Idle (A9); `Running` is marked once the
  environment is bound to the verified user-data folder at engine construction (A21),
  for every persistent boot, as macOS marks it at store bind. The first build's
  `mark_profile_running` (`:3149-3162`) stays and is then a no-op.
- Trust: the facade hint selects a barrier, never authority (gh531 `:828-829`);
  the launch define fixes it before app code (A18); every pair and `close_seq`
  is validated by the shared registry.
- Failure: a build failure is `WindowBuildFailed { stage }` → `ERR KELD-CORE-041`
  (gh531 O2), after the half-built HWND is gone (A8, A25, D12); a loop invariant fault stays
  `Fatal`; no retry of a deterministic failure.

Reuse evaluated (reuse before rewrite):

| Option | Verdict |
|---|---|
| gh531 registry, router, codec, `WindowSource`, facade `BrowserWindow` | reused unchanged. Windows flips `native_supported` and feeds the same `AppWindowEvent`s. |
| Tao `CloseRequested` veto primitive | reused (A4). **Rejected:** a second Win32 WndProc/subclass (duplicates Tao's; issue #1188 forbids it). |
| Existing WebView2 engine: `create_controller`, `install_guarded_media_permissions`, `fit_controller`, `View` drop, `ProfileReleaseWait`, `PersistentStopTransition`, `BrowserExitWake` and its `ProfileReleaseWake` (D6's wake, so no new loop event) | reused; `create()` cannot run in-loop (needs `self.event_loop`, `:3522-3526`), so the build body is factored to take the `target` the `run_return` closure already receives (`:3263`); the in-loop nested pump is proven (A7). `ProfileReleaseWait` stays the one barrier owner and its Host site gains the instance binding (A24, D8); the Probe site is untouched. **Rejected:** a second engine; a per-window environment; asynchronous `ControllerReady` completion (unneeded after U1); a second, per-window or per-generation barrier object, or a retired-generation receipt list (older generations are the census's, A10). |
| Host-death Job census (`keld-runtime` `windows_job.rs`, `windows_job/host_death_census.rs`: `exit_census`, `list_job_process_ids`, `open_census_member`, the settled-snapshot rule and the family exclusion; attempt Job + `assign_child`) | reused as the one release witness: `exit_census` gains a member policy (`Terminate` or `Observe`) and a rights parameter, `open_census_member` takes the rights instead of the hard-coded `CENSUS_MEMBER_RIGHTS` (`host_death_census.rs:110-111`), and the session's Bun generation Jobs are the family (A10, D7, U14). **Rejected:** a second observe-only loop (parallel copy of the census); `GetProcessInfos` and the Chromium lockfile (both fail open, §2); per-pid custody in keld-wv (r4–r7); retiring Bun before the census (r8; needs a reorder, keeps handover on the old barrier, and fails every quit on a lingering Bun descendant) |
| macOS helpers `initial_renderer_pending` (`engine.rs:248-253`), `first_host_init_turn` (`wkwebview/mod.rs:1039-1045`), checked `next_view_id` and `checked_logical_bounds` (`managed_windows.rs:334-374`), borderless `SetFullScreen` (`:198-203`) | hoisted into one platform-neutral keld-wv module and reused; macOS keeps its behaviour, only import paths move. **Rejected:** Windows copies. |
| Sidecar admission: Linux `admit_kipc_transport_sidecar` (`app_session.rs:3899-3915`, the strictest existing copy), macOS preload + `bunfig` refusal (`keld-host/src/main.rs:478-512`), Windows `open_relative_file_windows` (`:1731-1783`) | the Linux helper is generalised into the one owner (symlink refusal everywhere, plus the Windows reparse-point check, the `bunfig` refusal on every launch, the boot-mode define and `BUN_OPTIONS` removal that macOS runs today) and consumed by the Linux launcher, the guardian and `windows_primary_config` (A18). **Rejected:** a Windows exemption; a fourth copy in keld-core. |
| Profile lease | reused per session. **Rejected:** per-window lease. |
| Windows host test harness (`ProductFixture`, TCP `KELD_T1B_CONTROL`, `dev_stage_command`, `wait_for_host_window`, `run_product_cycle`, `process_tree_report`, `InstalledApp`) | reused; census/witness helpers added to `support/window.rs` (A15, A19, A23). **Rejected:** a shared three-OS harness (kel259 §4). |
| Facade boot hint | the macOS rule (A12) plus the preload (A18). **Rejected:** faking Ready from the first navigation (gh531 criterion 13; `KELD-CORE-044`). |
| Mirror reads | reused. **Rejected:** per-getter RPC (gh531 criterion 17). |
| Second registry for Windows | rejected (issue non-goal; gh531 A1 sole minter). |

Named unmet requirement: no live Windows component can hold a close pending, build a
window after loop start, report a release only after native teardown, or bind the profile
stop record after Init. Compatibility fallback: renderer-declared boots keep today's close
path (A17). No performance claim (§9).

### 4.a Windows deltas, by gh531 section

| # | Delta | Plugs into | Evidence |
|---|---|---|---|
| D1 | Windows leaves the parked list; Linux stays. T1 builds facade sessions (reachable only through D2's test admission until T2) with `window_native_support: Some(false)`; renderer boots keep `None`, so the non-macOS `NavigationReady` gate (`app_session.rs:4628`) still signals their Ready (Subscribe accepted, replay empty, calls answer `042`, facade Ready published); T3 turns it to `Some(true)`, never by a literal: keld-core derives `Some(true)` only when the keld-wv Windows engine reports managed-window support (added in T3's diff with D4–D12 and D8) **and** the host lent the D7 release witness (U14, landed by T2), and `Some(false)` otherwise (AC22), so no head and no host without both release halves admits a managed window. No interim where the CLI/Host admit a facade app that dies `045` at connect | gh531 §1, §4.b, Platform notes | `app_session.rs:7613-7615`, `:7707-7708`; `window_router.rs:387-395`, `:475-478` |
| D2 | `validate_windows_boot_files` admits `InitialWindow::None`; `run_app_direct` drops its inline refusal; `ensure_initial_window_supported` becomes Linux-only; `create_app` is skipped for facade boots. **Staged admission (AC23):** at T1 both refusals stay for product builds; one predicate at both sites (`:1447`, `:3431-3438`) admits the boot only when `KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT=1` is set on a `cfg(all(feature = "profile-test-hooks", debug_assertions))` build. That is the existing `KELD_T1B_STALL_BEFORE_INIT` class: a child-only env, compiled out of product builds, scrubbed from the Bun env with the h9 hook (`app_session.rs:3378`). Every descriptor rule (`KELD-CORE-035`, digests, file admission) still runs. T2 removes both refusals for every build and deletes the env, in the same diff as A21 and D7, so no head admits a product facade boot without both | §4.c | `:1340-1351`, `:1447`, `:3431-3438`; main `:3667` |
| D3 | `AppReadyTrigger` and `AppWindowEvent::HostInitialized` cfg widened to Windows and re-exported from `webview2`; `run_app_until_quit` gains `ready_trigger` and a loop-entry callback (the macOS `startup` pair, `wkwebview/mod.rs:342-352`); `HostInitialized` emitted once at `first_loop_turn` (`:3262-3270`); `coordinate_window_events` takes `ready_trigger` and `init_watch` on Windows; `run_guarded`/`run_unprivileged` take the pre-Init terminal on Windows and `keld-host/src/main.rs` supplies it | §4.c; gh531 "No unbounded rendererless Init" (`:268`) | `engine.rs:149-153`, `:290-299`; `app_session.rs:4600-4643`, `:2603`, `:1888-1890`, `:1947-1949`; `main.rs:147-171` |
| D4 | `CreateWindow { build, … }` builds in-loop: Tao window via `target` with `with_visible(false)`, D14 frame correction, controller via `create_controller` (A7), then at once, before any later fallible step, the D8 (2) bind of its browser instance and the D8 (3) `ProcessFailed` registration, guard handlers before any navigation (`:3570-3574`), `SetBounds` + `SetIsVisible` (`:3592-3606`; learnings 2026-08-14), empty initial document, `mark_profile_running` (a no-op after A21), `set_visible(true)`, then `MoveFocus` (best effort, as `:3637-3641`), so focus moves into the controller only after the window is shown (AC13's at-risk order); the view id comes from `next_view_id` before the guard install (the guard's principal needs it), so a later failure burns one `WebviewId` and owes nothing for it; snapshot from the built window (`outer_position`, `outer_size`, `scale_factor`, `is_maximized`, `fullscreen()`; tao `window.rs:213`, `:264`, `:508`, `:613`, `:669`) through `checked_logical_bounds`; the initial focused field reuses D11's UI-thread activation sampler (`GetActiveWindow() == hwnd`), not Tao keyboard focus; insert the `View` in `managed`, then answer `WindowBuilt` | §4.a snapshot source, O1 | main `:3527-3645` |
| D5 | Close path by boot kind: in a facade session every `CloseRequested` whose HWND is a managed view sends `WindowCloseRequested { view }` and nothing else; an HWND in `releasing` or unknown is ignored with one diagnostic (gh531 C3/D3). Renderer sessions keep the legacy arm untouched | §4.c `:764-765`, §4.e P3/Q3/C3/D3, criterion 15 | main `:3355-3370`, `:2816-2834`; `wkwebview/mod.rs:555-573` |
| D6 | `DestroyWindow { view }`: remove the `View` (controller `Close()`, then Tao's posted destroy) and park an entry in `releasing` under its Tao `WindowId`, with the HWND value read before the drop. **Two-step release (A8).** `WindowEvent::Destroyed` is not completed teardown: Tao sends it from inside `WM_DESTROY`, inside the `DestroyWindow` of its posted destroy (A8), 22–61 ms after the drop (A20). **(1) Stage.** The matching `Destroyed` marks the entry staged once, posts one `ProfileReleaseWake` through the existing `EventLoopProxy` (the post targets Tao's thread window, not the dying HWND, `event_loop.rs:569-586`) and writes nothing to the core. **(2) Publish.** At the top of every closure run, after `poll` and before the expiry and exit checks, the loop reads `IsWindow(hwnd)` for each staged entry. FALSE is the proof: the one publish function sends the owed answer (`WindowReleased { view }` here, `WindowBuildFailed` in D12) once, and the entry leaves the map only after that send returned. **Failed steps.** The park and the stage are recorded before fallible steps on purpose, because each failure stays visible: Tao ignores a failed post of its destroy (`window.rs:1106-1113`), so the entry stays parked and the bound faults; a failed wake post falls to the recheck. A failed send means the coordinator is gone, and it takes the macOS reaction (`wkwebview/mod.rs:567-571`: owner failure recorded, `Fatal`, terminal), not a running loop with an answer lost; every `AppWindowEvent` send in a managed arm does the same (AC24 NC 5). The read costs nothing in steady state, because it runs only while an entry is staged. **Early runs.** The closure can run while that `DestroyWindow` is still on the stack: Tao dispatches its buffer right after the `Destroyed` handler (`runner.rs:225-226`), and a pump inside `DestroyWindow` after `WM_DESTROY` returned, for example in child teardown, would run it too. A pump inside Keld's own callback cannot deliver the wake, because Tao buffers it (`runner.rs:216-222`); a pump after the callback returned could. In any such run the read is TRUE, so nothing is published: TRUE at `Destroyed` (7/7) and at the end of a hold of about 300 ms (3/3), and no logged closure run with a TRUE read outside the hold (7/7) (FACT, r18 probe, §2); TRUE between `WM_DESTROY`'s return and the handle's free is INFERRED. A `Destroyed` delivered from Tao's buffer after another build's pump arrives after `DestroyWindow` returned (A7), and the same two steps read FALSE at once. **A TRUE read** leaves the entry staged, and the loop schedules `WaitUntil` the earlier of now + `STAGED_RELEASE_RECHECK` (10 ms) and the entry's expiry; a failed wake post takes the same path. There is no re-post: a wake re-posted inside a nested pump would be dispatched at once and spin, while the recheck waits for the loop; the releasing bound below is what ends it. Rejected: `EnumChildWindows` in production, because `DestroyWindow` "first destroys child or owned windows" (<https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-destroywindow>) and AC24 reads the captured children from the test side (that page does not say how children of another thread are treated, and three of the four children live in other processes, AC9; with `Close()` first, 0 remained at `Destroyed`, 7/7, §2, and a failed build before its controller has none); a `WM_NCDESTROY` subclass (issue #1188 forbids a second WndProc, and Tao has no later event). Value reuse: A20. While an entry is pending, parked or staged, events for that HWND (`Resized`, `Focused`, `CloseRequested`, `Moved`) are ignored (D5) and the registry stays in `Closing` (gh531 C-rows). **Releasing bound (one rule for every entry, `Destroy` here and `FailedBuild` in D12):** each entry records the instant it was parked, and the loop schedules `WaitUntil` its expiry at `RELEASING_DESTROYED_BOUND` = `PROFILE_LAUNCH_DEADLINE` + 5 s, derived from that one constant (`:209`), never copied: a `Destroyed` is buffered while another window's in-loop build pumps inside the callback and is delivered only after the callback returns (A7), and that pump may legally last up to `PROFILE_LAUNCH_DEADLINE` (15 s, `create_controller_observed`, `:1710`), so the bound must exceed it; the 5 s margin is far above the 22–61 ms `Destroyed` latency (A20, prior evidence) and the sub-millisecond wake; an entry still pending at its bound, parked or staged, is a loop invariant fault: the loop removes the entry, latches it into the barrier's one fault cell as `ReleasingExpired` (D8 latch owner; `KELD-WV-009`), writes one diagnostic naming the HWND and takes the fault reaction below, so the host exits `KELD-WV-009` with any persistent record `stopping`, instead of holding a pending answer forever or writing `idle` over an unobserved teardown. **Fault reaction (one shape for every latched Host barrier fault: D6's expiry, every D8 latch, and today's `AbnormalExit` and `ObserverDropped` drained before a terminal; renderer boots included, whose pre-terminal fault therefore also leaves `stopping` instead of today's `running`, with the same successor recovery, `lifecycle.rs:469-479`):** at the loop's existing fault check (main `:3272-3274`, which today only sets `exit_requested`) the loop also drops the managed views and commits the stop (`commit_profile_stop`, which advances the record to `stopping` on disk, `:450-463`; D7's order), unless a terminal already did both, and then exits through the exit gate below. `run_app_until_quit` returns the fault at `:3381-3386`, before `synchronize_profile_stop` (`:3387`) and `finish_profile_release` (`:3395`), so the outcome is always `KELD-WV-009` with a persistent record `stopping`, never `idle`. Today a fault drained before a terminal leaves the record `running`; recovery treats `running` and `stopping` alike (dead owner → quarantine, `profile/lifecycle.rs:469-479`; inline recovery, `webview2/mod.rs:1023-1028`), so the change makes the outcome uniform without changing what a successor does. **Exit gate (one predicate for every terminal: `Quit`, `Fatal`, a latched barrier fault and the navigation terminals):** the loop sets `ControlFlow::Exit` only when `releasing` is empty **and** the barrier is ready, and a staged entry is still pending, because `releasing` empties only at the publish step; today's predicate is `exit_requested && exit_ready` alone (main `:3275-3277`, `:3373-3377`), which would end the loop with an owed answer pending. Until then the loop keeps pumping, so each posted destroy runs and every owed `WindowReleased` or `WindowBuildFailed` is written before the loop ends, and it schedules `WaitUntil` the earliest entry expiry or staged recheck even after the barrier observed its receipt or faulted (today's `schedule` returns early then, `:2738-2740`). Persistent `Idle`, ephemeral deletion and the in-host handover all run after the loop ends, so the gate orders them too. No `CreateWindow` or `DestroyWindow` runs after the terminal command (D8 (4)), so every entry was parked before it and its bound ends at most `RELEASING_DESTROYED_BOUND` after the terminal; the gate waits concurrently with the barrier, so the UI end's worst case stays `WEBVIEW2_UI_END_BOUND`. The views the terminal or the fault reaction drops owe no answer and are not parked (today's `views.clear()`); a command the loop still dispatches for such a view (`DestroyWindow`, a setter) is ignored with one diagnostic, because the session is already ending | §4.e C9 "Released" | tao facts §4.0; U1/U6 bundle |
| D7 | Facade boots: the loop never writes `LastWindowClosed`; `commit_profile_stop` runs only on `Quit`/`Fatal` or a latched barrier fault (D6 fault reaction), after every managed `View` is dropped (controllers closed first, `:2628-2629`). `from_selected_environment` writes `Running` as its last step, once the environment is bound to the verified user-data folder and the exit observer is registered (A21), so every persistent boot stops through the existing Running→Stopping→Idle chain; `advance` is unchanged. **Release proof (A10), on every Windows `run_app_until_quit` release** (persistent `Idle`, ephemeral deletion, and the in-host handover, on renderer and facade boots, AC27; handover with registry windows is a non-goal, U8). `run_app_until_quit` takes the witness as a required input, so no app path can omit it. `run_until_closed` takes none and keeps today's receipt-only rule: its callers are `keld hello` (`run_hello`, `webview2/mod.rs:3726-3729`, dev-ephemeral, with no keld-core session and no host-death Job) and a `media_acceptance` fixture, neither of which is a product app boot; the fixture that calls `run_app_until_quit` passes an always-clean test witness. `finish_profile_release` first requires the Host barrier's normal receipt **for the exact current browser instance** (D8: bound by generation and pid; a receipt of an earlier, delayed or foreign instance never satisfies it), under today's hang guard and post-exit grace (`HangGuardExpired`, `GraceExpired` and an abnormal exit of the current instance stay faults), then calls a release witness injected by keld-core with the remaining post-exit budget. Release needs **both**, and neither implies the other: the receipt no longer admits `Idle` on its own, and a clean census does not prove that the current browser raised its normal exit. That release decision runs only after the loop ended through the D6 exit gate, so no release starts while a `releasing` entry is pending. Only when no controller was ever created in the session (a zero-window facade `Quit`, AC18 b, or builds that all failed before `create_controller` returned a controller) is there no receipt to require, and the census alone decides. Every controller that was created bound its browser instance at controller completion or latched `LifecycleUnproven` (D8 (2)), failed builds included, so a launched and identified browser can never reach this exemption. Whether a `create_controller` that fails can still leave a launched browser is UNKNOWN (no device cell); such a browser would be a Job member, so the census refuses `Idle` while it lives; an abnormal exit of it latches (D8 (3)), and its normal exit is not credited. `ProfileReleaseWait` stays the one barrier owner; only its Host site gains the instance binding (D8). Its Probe site (`ReleaseSite::Probe`: the hidden exclusive-UDF probe, startup recovery and the post-release scavenge) is built with its one instance, never rebinds, keeps today's semantics, and runs no census. The witness is the **one** census owner, keld-runtime's `exit_census`, parameterised by a member policy (`Terminate` for the update exit as today, `Observe` here) and by the rights it opens members with (`Observe` needs no `PROCESS_TERMINATE`). It takes settled snapshots of the host-death Job, excludes the host and the members of the session's Bun generation Jobs (the existing "family" rule, `host_death_census.rs:153-241`, extended from one attempt Job to the session's list), treats any other listed member as unclean, retakes until clean, and never terminates. A clean census writes `Idle` (persistent) or permits deletion (ephemeral) or handover; a deadline, snapshot, open or query error yields `LifecycleUnproven` (`KELD-WV-009`) and the record stays `stopping`. **Bun family:** keld-core gives every Bun primary generation its own attempt Job nested in the host-death Job (the existing single-process attempt-Job type and `assign_child`, `windows_job.rs:1289`, which refuses a second assignment, `:1293-1298`), so a generation started by recovery is family too; whether a generation is created suspended and assigned before its first instruction (the `WindowsSuspendedChild` path the update candidate uses) or assigned right after spawn is U14, and a child Bun starts before assignment is a non-family member, which fails closed. **What the census proves, and what it does not:** it proves only that no member of the host-death Job other than the host and the Bun family is alive. Because every descendant inherits the Job and the Job forbids breakaway (A10), that covers every WebView2 process this host launched and every other host child (FACT: 284/284 and 430/430 observed, 0 outside; prior evidence on a probe Job, §2, with no exact-head membership cell). It proves nothing about a process outside the Job. The residual is a process a Windows service starts on the runtime's behalf (§2; INFERRED, no primary source names one): not observed among 430 processes of 11 kinds, which were matched by the user-data folder on their command line, so a holder without that argument would not have been seen; a runtime update, the system print dialog and a real permission prompt were not exercised, so whether it occurs is UNKNOWN. `idle` therefore claims release by Job members only. A successor launch does not close it either: its exclusive environment refuses only WebView2 creation from another process on the same folder (note 265 D3), not an open file handle, and whether it refuses a folder still held by an out-of-Job runtime process has no device cell (UNKNOWN; AC21's successor exclusive environment and AC18's successor launch are positive controls only). **Rejected:** an exclusive-UDF check at release. Exclusive access governs WebView2 creation, not open handles, so it would not cover a non-WebView2 holder; and since creating an environment launches no browser (A21, U10), the check needs a controller, one more browser launch (about 300 ms or more, A7) at every `Quit`. The probe itself is not the obstacle: the Probe site already runs it under its own barrier (`webview2/mod.rs:1880-1965`). Nor can the census prove that a Bun-family process holds no handle in the folder: a Bun process with the user's authority could equally reach the folder through an out-of-Job broker. Bun's access to the folder is therefore bounded only by OS containment, never by the census (arch 03: the non-breakaway Job is descendant cleanup, not authority, `03-security.md:237-238`). **Today, and for every session W2 ships, no such containment exists:** the Windows primary runs plain `bun` on the user's token (`windows_primary_config`, `app_session.rs:3359-3372`; §1 non-goals), because the Strict LPAC primary (#1187, KEL-210) is not built yet. This is not the opt-in Legacy profile (arch 03 forbids auto-downgrading into it, `03-security.md:246-248`); it is the unbuilt Strict target. Until #1187 lands, the release proof covers WebView2 processes and other host children in the host-death Job only, for every Windows session, and makes no claim about Bun-family file handles. Once the Strict primary exists, its containment of the folder is #1187's claim to prove (U17), not this spec's; retiring Bun before the census would neither close that gap nor survive the KEL-53 handover tail, which needs the live Bun after the UI end (`app_session.rs:3298-3324`). **Pacing (observe policy):** every unclean round waits with the already-sanctioned `WaitForSingleObject` on the first remaining non-host member's handle, up to the remaining deadline, before the next snapshot; an unsettled list is retaken only after the existing `CENSUS_DENIED_MEMBER_WAIT` (25 ms, `host_death_census.rs:114`). A slow member therefore costs waiting, not a spin, and no new FFI operation is added. This removes the per-pid custody of r4–r7 (retired-generation list, attribution of events to older generations, the unresolved-controller flag and the zero-window probe witness); the only attribution kept is the binding of the one receipt to the current instance (D8). Job membership is kept by the kernel, covers every generation and a launched-but-unbound browser, and does not depend on any runtime's start-up order | §4.e T11; gh531 `:1465-1467` | main `:3250`, `:3317-3329`; arm doc `:2628-2629` |
| D8 | **Exit receipt bound to the current browser instance (A24; U6 resolved).** Today the Host barrier's proof is one session-wide sticky flag: `poll` drains a normal receipt even before arming and sets `observed` for good (`:2662-2670`), the loop polls every turn (`:3271`), and `exit_ready` (`:2728-2732`) and `finish_profile_release` (`:3124-3128`) accept it; the handler matches by pid (`:1568`, `:1576`) and queues a receipt without one. Re-registering the wake alone cannot reset that flag, so a receipt of browser A (last window closed, session kept by a `'window-all-closed'` listener) would satisfy browser B's barrier. Rules, Host site only, in the one owner `ProfileReleaseWait`: (1) **Binding.** The `expected_browser_pid` atomic (`:2391-2400`, `:2819-2829`, `:3006-3018`) widens to one binding `(generation: u32, pid: u32)` in an `AtomicU64`, written only by the loop and read by the handler; both run on the UI thread (A11). A facade session starts unbound (generation 0); a generation advances only by `checked_add`, computed before the new handle is opened, and overflow latches `RebindUnproven` with the old wake and binding untouched. The ambiguity flag (rule 2) is a loop-side `Cell<bool>`, not part of the atomic. The legacy path binds once at loop entry (`remember_browser_process_id`, `:2388-2402`) and never rebinds; its second store in `close_requested_view` (`:2828-2830`) is removed, because the loop-entry bind already holds the one legacy view's pid, so renderer boots keep today's behaviour. The Probe site stores `(1, pid)` (today `:1943-1945`) through the same type and the same handler (`observe_profile_browser_exit`, `:1548`). (2) **Rebind at controller completion, for every build.** Controller completion is the moment `create_controller` and `CoreWebView2` have both succeeded (main `:3544-3547`), before the `View` is built (`:3551`) and before every later fallible step (`:3557` onward: saved-permission reconciliation, `mark_profile_running`, guard install, Settings, bounds). At that moment the loop first polls the barrier, and a fault already latched there fails the build through D12 custody with no bind (rule 3); otherwise it reads `BrowserProcessId` (`webview_browser_process_id`, `:1742-1748`), whether the build later succeeds or fails, because the controller has already launched or joined a browser (A21: the first controller launches it). A controller that exists while `CoreWebView2` fails, or whose `BrowserProcessId` read fails or returns 0, names a browser the loop cannot identify, so the build latches `UnidentifiedController` (its HWND still takes the D12 custody path, and the latch takes the D6 fault reaction); no later step and no zero-bound exemption (D7) can clear it. Rejected: deferring that debt to `Quit` as a second unproven flag beside the barrier's fault cell (two owners of one fact, which is already known at the build). The same pid while the bound instance's SYNCHRONIZE handle is unsignaled is the same instance (regime i, same-pid reuse at ≤86 ms, one event at the final close): nothing changes. A different pid is a new instance: the loop opens the new handle, retires the old wake by the checked operation below, registers the wake on the new handle only after that success, and only then writes `(generation + 1, pid)` and clears the credited receipt and the post-exit deadline (all on the UI thread, where the handler also runs, so no receipt can interleave); the first bind of a facade session only registers. **Checked wake retirement.** Today the only unregistration is `BrowserExitWake::drop` (`:2478-2496`): it calls the blocking `UnregisterWaitEx(wait, INVALID_HANDLE_VALUE)` and, on failure, deliberately leaks the callback box and keeps the process handle open, and it returns nothing, so a `Drop` is no witness that the wait is gone, and drop-then-register could leave two live waits. The rebind instead calls `retire(self)` on the same type, which makes the same call on the UI thread (never from the pool callback, which the blocking form forbids; the wait is registered with `WT_EXECUTEONLYONCE` only, never `WT_EXECUTEINPERSISTENTTHREAD`, `:2448-2455`) and reclaims the box and closes the handle only on success. Microsoft documents only that a failed call returns zero, with `GetLastError` for detail (<https://learn.microsoft.com/windows/win32/api/threadpoollegacyapiset/nf-threadpoollegacyapiset-unregisterwaitex>); it does not say whether the wait stays registered after a failure (UNKNOWN). So `retire` consumes the wake and, on failure, forgets it exactly as `Drop` leaves it today (box leaked, handle open, never unregistered a second time; a late callback only posts a harmless `ProfileReleaseWake`); the loop latches `WakeRetireFailed`, keeps the binding unchanged and registers no second wait. The new handle is opened before the retirement, so a failed open leaves the old wake untouched. Keld therefore never has more than one wait it has not confirmed retired, and the crate rule (`keld-wv/AGENTS.md:5`) is reworded in T3 to "at most one live registration, replaced only after a checked `UnregisterWaitEx` success". No native retirement failure is observed or claimed; the unit control is AC21 NC 4 (i). **Latch owner.** Every D8 `LifecycleUnproven` latch, and D6's expiry, sets the barrier's one fault cell (`fault: Cell<Option<ProfileReleaseFault>>`, main `:2539`) through a named variant of the existing enum (`:1503-1514`, today `GraceExpired`, `HangGuardExpired`, `AbnormalExit`, `ObserverDropped`); `context(self, site)` (`:1518-1544`) stays exhaustive over (fault, site), and `:3381-3386` maps each to `KELD-WV-009` as today. The first fault set is kept. `UnidentifiedController` (a controller exists but `CoreWebView2` or its `BrowserProcessId` read failed or returned 0): Host "a host WebView2 controller exists but its browser process could not be identified", Probe "the exclusive-UDF probe controller exists but its browser process could not be identified". `WakeRetireFailed` (the checked retirement failed): Host "the host browser exit wake could not be unregistered before a rebind", Probe "the exclusive-UDF probe browser exit wake could not be unregistered". `RebindUnproven` (the new handle cannot be opened, its wake cannot be registered, the generation overflows, or a rebind is reported after arming): Host "the host browser instance could not be rebound to a watched handle", Probe "the exclusive-UDF probe browser instance could not be bound to a watched handle". `ReleasingExpired` (D6): Host "a host window's native teardown was not confirmed within the releasing bound", Probe "the exclusive-UDF probe has no releasing window, so this fault is never latched there". The Probe site neither builds nor rebinds and polls its handle without a wake (`BrowserProcess::Polled`, `:2502-2507`), so it latches none of the four; their Probe texts exist only because `context` is exhaustive. The D8 ambiguous binding keeps its rule: it credits nothing, and the armed barrier faults `GraceExpired` at the grace. The same pid with the bound handle already signaled cannot name a new process, because a process id stays reserved until all handles to it are closed (FACT, A24) and Keld still holds the old handle; such a report is inconsistent, so the binding is marked ambiguous, no receipt for that pid is credited, and an armed barrier fails closed at the grace. If the new instance's handle cannot be opened (`open_browser_process` returns `None`, `:2373-2385`, which `browser_exited` would read as exited, `:2637-2639`) or its wake cannot be registered, the rebind latches `RebindUnproven`: a rebind never falls back to grace-only waiting. A failed registration after a successful retirement leaves the bound instance with no wake; the latch ends the session through the D6 fault reaction, so nothing relies on that wake afterwards. (3) **Stamp and credit.** The handler stamps a matching normal receipt with the binding's generation at the moment the event fires; a normal receipt for any other pid is foreign and dropped with one diagnostic. `poll` credits a drained normal receipt only if its generation is the current one; an older one is discarded. `poll` drains the channel until it is empty on every call, not one receipt per call (today one `try_recv`, `:2662`), and its only early return is a latched fault (the first fault is kept, so nothing drained later could change the outcome). A credited normal receipt no longer stops draining (today `poll` returns once `observed` is set, `:2649-2651`): an abnormal receipt drained after the current instance's normal credit still latches `AbnormalExit` and blocks `Idle`, in the same fault cell with the same variant, while that credit stays recorded. Each loop turn runs `poll` before its exit check (`:3271`, `:3373`) and the handler sends synchronously on the UI thread (A11), so the decision to end the loop sees every receipt dispatched until then (AC21 NC 4 (l)). Draining before arming stays: a current-instance receipt drained before `Quit` is genuine (its browser exited with the session still open). At the Host site an abnormal receipt is queued whatever its pid and latches `AbnormalExit` whatever its stamp whenever it is drained before the loop's last exit check, including after the current instance's normal credit, and no rebind clears it. Every abnormal receipt there is a crash of one of this session's browsers: the environment raises the event "when the collection of WebView2 Runtime processes for the browser process of this environment terminate" (<https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2environment5>); it is created with exclusive user-data-folder access (`:1455-1470`; "Whether other processes can create WebView2 from WebView2Environment created with the same user data folder", <https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2environmentoptions2>); and no other environment on that folder exists in the host while the loop runs (the Probe site runs at startup recovery before the loop, `:1092-1099`, at purge recovery, `:1196`, and in the scavenge after release, `:2079`, `:2801-2814`) (INFERRED from these facts). So the fault ends the session as today (`:3272-3274`). The pid filter stays for normal receipts only: it keeps an older browser's normal exit from being credited to the current one, while an abnormal exit needs no provenance, because its outcome does not depend on which instance failed. Rejected: filtering abnormal receipts by pid too (today's handler, `:1576`, which r15 kept), because the same crash then latches or vanishes depending on whether its event fires before or after the next rebind (AC21 NC 4 (k)). For a renderer boot, which binds once at loop entry, the only change is an abnormal exit reported before that bind, which today's `expected != 0` check drops (`:1477-1479`). **Browser `ProcessFailed` (W1 #1307 Q13 a).** Microsoft documents, under "The main browser process has exited unexpectedly": "All the WebView2 controls in your application using the same environment configuration will receive the `ProcessFailed` event" with kind `BrowserProcessExited`; "A single `BrowserProcessExited` event will be raised from the `CoreWebview2Environment` too, but the order of these events is not guaranteed"; and "The WebView2 controls need to be recreated" (<https://learn.microsoft.com/microsoft-edge/webview2/concepts/process-related-events>, ms.date 2023-07-13). keld-wv registers no `ProcessFailed` handler at `457a9c3b` (FACT: no match under `crates/`), so the receipt is today's only abnormal signal: a crash whose `ProcessFailed` comes first, followed by a `CreateWindow` that launches a new browser and rebinds, leaves the old receipt foreign and dropped, and the session runs on. Rule: every managed view registers a `ProcessFailed` handler on its `CoreWebView2` at build, right after the rule-2 bind (webview2-com 0.38.2 `ProcessFailedEventHandler`, `callback.rs:254-257`; `ICoreWebView2::add_ProcessFailed`, webview2-com-sys `bindings.rs:1299`); a failed registration fails the build through D12 (a) custody after the bind, so that browser's receipt is still required. A `ProcessFailed` whose `ProcessFailedKind` is `COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED` (`bindings.rs:703`; Microsoft: "Indicates that the browser process ended unexpectedly.", <https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/webview2-idl>) queues an abnormal receipt on the barrier's one receipt channel (a clone of the sender `observe_profile_browser_exit` creates, `:1557`) and posts the existing `ProfileReleaseWake`; `poll` then latches `AbnormalExit` in the one fault cell, the same variant an abnormal `BrowserProcessExited` latches, and no new terminal exists. The view's generation is not consulted, as for an abnormal receipt; every live managed view belongs to an instance this session bound (rule 2), in practice the current one. The handler runs on the UI thread (A11) and sends synchronously, so the receipt is queued before any later loop code runs; the loop polls at the top of every turn (`:3271`) and at controller completion before the bind (rule 2). From the latch on, no rebind happens, a `CreateWindow` the loop still dispatches (the exit gate holds the loop open while a `releasing` entry is pending) is answered `WindowBuildFailed { Webview }` without a build (no HWND, nothing parked), one still queued when the loop exits is never dispatched and gets no build answer, and the loop takes the D6 fault reaction (record `stopping`, `KELD-WV-009`). The latch precedes a rebind only when the runtime delivers its signal first: a `CreateWindow` whose controller completed before that (the order is the runtime's) rebinds once, and the latch then still ends the session. A later `BrowserProcessExited` for that browser, normal or abnormal, changes nothing, because the first fault is kept. Other `ProcessFailed` kinds (renderer, GPU, utility, unresponsive) do not latch: they are not browser exits and stay out of scope. Closing a controller stops its handlers ("After running `Close`, most methods will fail and event handlers stop running", <https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#close>), so a normal `Quit`, a destroy and a failed build, which all close their controller before the browser exits, raise no `ProcessFailed` (FACT, docs). That a retired browser's normal exit in regime (ii) raises no `ProcessFailed` on the new browser's live view is INFERRED (the kind names an unexpected end); AC21 (ii)'s positive control falsifies it. The legacy path registers no handler: it binds once and never rebinds, so its bound pid's abnormal receipt already latches (A17 unchanged). Because a `ProcessFailed` can now set `AbnormalExit`, its context text becomes neutral: Host "the host browser process failed or exited abnormally", Probe "the exclusive-UDF probe browser process failed or exited abnormally" (today `:1532-1537`, which no test matches). Disclosed limit, narrowed: the `ProcessFailed`-first path and the after-rebind path are closed, and a browser a failed `create_controller` may have launched (UNKNOWN whether one can, D7) is covered too if it fails, because its abnormal event is raised on the same environment. What remains unreported as abnormal is an abnormal event that is never raised (the environment stays referenced until the event, U6) or that is raised after the loop's last exit check: nothing is drained after the loop ends, and the census that follows waits on process handles with `WaitForSingleObject` (Pacing, D7), not on the message queue, so an older browser that fails after that check is not reported; the census proves only that such a browser is gone. A receipt queued before that check latches, also after the current instance's normal credit (AC21 NC 4 (l)). A failed build whose controller exists has bound its browser instance or latched `LifecycleUnproven` (rule 2), so either that browser's exit kind is reported or the session fails closed. (4) **Arming.** Arming happens only at the terminal command, after which the loop executes no `CreateWindow` or `DestroyWindow`, so no build completes and no `releasing` entry is parked (D6); a rebind after arming is an invariant fault (`RebindUnproven`). (5) **Bounds unchanged.** The hang guard from arming while the current handle is unsignaled, the post-exit grace once it is signaled, and the faults `HangGuardExpired`, `GraceExpired` and abnormal exit apply to the current instance. Older generations' normal exits are never credited (an abnormal one latches, rule 3) and are covered by the census (D7), so no retired-generation list exists (the 125–237 ms regime raises the old pid's event during the new creation pump, 12/12, with the old generation's stamp: the poll before the rebind credits it to the old generation and the rebind clears that credit (rule 2); drained after the rebind, it is discarded (rule 3); it never credits the new instance, AC21 NC 4 (a1), (a2)). The environment stays referenced until the event, because dropping it suppresses the event (0/10, U6). The landed KEL-53 h9 cell (`profiles/handover.rs:621-633`, its withheld-event hook `KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY`, `mod.rs:2541-2555`) still withholds every receipt and keeps its meaning; the Probe site never rebinds, so the exclusive-UDF probe's `Unproven` on a withheld event (`:1523`) keeps its meaning too. Fail-first proof: AC21 NC 3 (device) and NC 4 (unit) | KEL-53 release barrier | main `:2389-2402`, `:2618-2627`, `:2656-2683`, `:2728-2732`, `:3124-3128`, `:3271`; U1/U6 bundle `U6_summary` |
| D9 | The v0 first-navigation deadline is armed only by the legacy `create_app` window (`initial_renderer_pending`); managed views live in their own map and never arm it | §4.c compat arm | main `:3240`, `:3293-3315`; `engine.rs:248-253` |
| D10 | **Per-view host obligations, one owner (A26).** One `View` method takes Tao's `Moved`, `Resized` and `Focused` for every view of both maps, managed and legacy, and of `run_until_closed`'s loop (`:3061-3066`), so no boot kind keeps its own copy; each call is best effort like `fit_controller`, because the next event re-applies it. `Moved`: `NotifyParentWindowPositionChanged`, in addition to the bounds fact, for the matching view. `Resized` (a call that fails keeps its state, so the next event retries): a minimized window (Tao `is_minimized()`, `IsIconic`, `platform_impl/windows/window.rs:627-629`) gets `SetIsVisible(false)` and no `SetBounds`, as wry skips `SIZE_MINIMIZED`; otherwise `fit_controller`, then `SetIsVisible(true)` once if the minimize hid it, whichever state the window restores to, then D11's focus return, because the restore's `Focused(true)` may arrive while the view is still hidden (order unmeasured, §2). `Focused(true)`: D11's focus return. An HWND in `releasing` gets none of these (D6). **Facts, managed views only:** `Moved`, `Resized` and `ScaleFactorChanged` → read state → forward `WindowBounds`/`WindowFullScreen` observations, and `Resized` alone also `WindowMaximized`, because Tao's `is_maximized()` is a flag (`window.rs:613-616`) that only `WM_SIZE` changes (`:1227-1246`), so a `Moved` before it reads the old value and a restore to a maximized window would write a transient `false` (no keld-wv filtering, B4). `ScaleFactorChanged` is in the list because logical bounds change with the scale even when the physical rect does not (Tao keeps the size of a maximized or full-screen window, `event_loop.rs:1910`, `:1966-1973`, and a `Moved` that precedes `WM_DPICHANGED` reads the old scale). A minimized window forwards no bounds, maximized or full-screen fact, because Tao reports `(-32000, -32000)` and `(0, 0)` then (r18 probe) and clears `MAXIMIZED` (`:1227-1246`, from source), while Electron returns the restored bounds (`GetBounds`, `shell/browser/native_window_views.cc:925-930` at v44.4.5); the facts of the restore follow. This narrows which Tao readings become facts on Windows and changes no gh531 rule: rule 3's change filter and P12–P15 apply unchanged to what is forwarded. Focus facts are D11's and are not suppressed, so a minimize that deactivates the window still writes `WindowFocus { focused: false }` (P13), as macOS forwards focus on every window event (`wkwebview/mod.rs:619-637`). That `IsIconic` already reads TRUE at the minimize's `Moved` is INFERRED (the probe read it later); AC26 (b) decides. `SetMaximized` → `set_maximized`; `SetFullScreen` → borderless (the macOS choice, `managed_windows.rs:198-203`; Tao Windows also implements exclusive mode, `window.rs:699-705`, which Keld does not use) | §4.b ordering rules 2–3, P12–P15 | tao `event_loop.rs:1212`, `:1227`; `window.rs:601`, `:675`; AC26 |
| D11 | Focus fact derived from activation: on every Tao `Focused(_)` and every controller `GotFocus`/`LostFocus`, the loop reads `GetActiveWindow() == hwnd` on the UI thread and forwards `WindowFocus { focused }` from that value only; Tao's `Focused(false)` during `MoveFocus` therefore produces no fact, since the window stays active. **Focus return:** on Tao `Focused(true)`, and only when the same read says this window is active, the `View` method calls `MoveFocus(PROGRAMMATIC)`, best effort as at build (`:3637-3641`), because reactivation leaves keyboard focus on the top-level HWND (A14, A26); wry's parent subclass does the same on `WM_SETFOCUS`, synchronously, while a `Focused(true)` buffered during a build pump can be delivered after another window took activation, and focusing an inactive window's child could pull activation back (INFERRED). The `Focused(false)` it causes produces no fact, by the rule above (AC13, AC26 (c)). The controller events (WebView2 COM handlers registered at build) and the `GetActiveWindow` query are new production `unsafe` (§8). Rejected alternatives in A14 | §4.b P13 | A14 facts |
| D12 | `WindowBuildFailed { stage }` with explicit custody of whatever exists when the step fails (A25). `Window` stage: the Tao build fails, no HWND exists, answer at once. `Webview` stage, (a) **after HWND creation, before `View` construction** (main `create` builds the `Window` at `:3527-3541`; `create_controller` at `:3544` or `CoreWebView2` at `:3545-3547` fails before the `View` exists at `:3551`, so there is no `View` to drop): the build keeps the standalone Tao `Window` and its `WindowId`; if a controller exists (`CoreWebView2` failed, or a step between controller completion and the `View` failed: the D8 (2) bind, a fault already latched there, or the D8 (3) `ProcessFailed` registration), it calls that controller's `Close()` first, as `View::drop` does (`:2836-2843`); it records `releasing[window_id] = Releasing::FailedBuild { build }` and then drops the `Window`, which posts the destroy (A20). (b) **After `View` construction** (guard install, `SetBounds`/visibility, initial document, D14, the snapshot): it drops the `View` (controller `Close()`, then the posted destroy) and records the same entry. In both cases `WindowBuildFailed { Webview }` is written only by D6's publish step, after the matching `WindowEvent::Destroyed` staged the entry and `IsWindow` read FALSE, never in the failing callback; the window id was never published, so no `WindowReleased` is written. The entry obeys the one releasing bound (D6); a lost `Destroyed` latches `ReleasingExpired` and takes the D6 fault reaction, never a silent answer. A failed build whose controller exists has already bound that browser at controller completion (D8 (2)), so the barrier requires its normal receipt like any current instance (a fault already latched at that moment fails the build without a bind and ends the session, D8 (3)); if the controller exists but `CoreWebView2` failed, no identity is readable and the build latches `UnidentifiedController` (D8 (2)). Only a failure of `create_controller` itself binds nothing; whether it can leave a launched browser is UNKNOWN, and such a browser would be the census's (D7). The loop never exits with an entry pending (D6 exit gate), so a terminal command cannot overtake the answer | §4.e O2 | A8, A20, A25 |
| D13 | `windows_primary_config` sets `KELD_NATIVE_WINDOW_FACADE` to `"1"` for a schema-2 boot (it already forces `"0"` for renderer boots, A12) and consumes the one launch owner for every boot kind: the literal define `--define=process.env.KELD_NATIVE_WINDOW_FACADE:"<value>"` with the value above, `BUN_OPTIONS` removed, `KELD-CORE-037` when a `bunfig.toml`/`.bunfig.toml` exists (sidecar or direct entry), `--preload=<root>/src/kipc-transport.ts` when the staged sidecar is a regular non-reparse, non-symlink file, `KELD-CORE-037` when it is a symlink, junction/reparse point or not a regular file, and legacy direct entry when absent (A18). The same owner replaces the macOS and Linux copies in the same change | §4.c Ready declaration; gh531 `:821-831` | main `:3359-3372`; `keld-host/src/main.rs:478-520`, `app_session.rs:3899-3934` |
| D14 | Outer-frame conversion (A19): requested outer logical → physical at the created window's `scale_factor`; frame delta measured on the invisible window; `set_inner_size(outer − delta)`; snapshot read after; `x`/`y` through `with_position` (outer) | §4.a `Created.state` | tao `src/window.rs:337`, `:387`, `:475`, `:766`, `:845` |
| D15 | CLI: `stage_dev_boot` and `keld doctor` refuse `initialWindow: "none"` only on Linux: `stage_dev_boot` from T1 (T1's fixtures stage through it, `tests/no_flag/windows/lifecycle.rs:56`; a product host still refuses the boot, D2), `keld doctor` from T2 with product admission, so doctor never reports a facade boot that the host refuses | consumed by §4.c | `boot.rs:380-390`, `doctor.rs:258-272` |
| D16 | Debug observability, `profile-test-hooks` class only (as `KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY`, `:2548-2555`, scrubbed from the Bun env by keld-core, main `:3378`): `KELD_TEST_WINDOWS_HANG_GUARD_SECS` shortens `BROWSER_SHUTDOWN_HANG_GUARD` so an erroneously armed barrier faults observably (AC10); `KELD_TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT=1` withholds only receipts stamped with generation ≥ 2 (D8) under the h9 hook's 1 s grace (`TEST_BARRIER_GRACE_EXPIRY_GRACE`), so the first browser's receipt is still credited and only a later instance's is lost (AC21 NC 3), while the h9 hook keeps withholding every receipt (AC14 NC 2); setting both hooks is refused at engine construction with a typed error naming both, so no run mixes the two faults; the loop writes `RECEIPT_CREDITED gen=<g> pid=<p>` and `REBIND gen=<g> pid=<p>` on stderr from the UI thread (AC21 NC 3's at-risk set), and `TERMINAL <command> releasing=<n>` (command `Quit` or `Fatal`) when it dequeues a terminal command (AC16 NC 4's validity check), and `ABNORMAL source=<s> gen=<g>` when an abnormal receipt is queued (`s` is `process-failed` or `browser-exited`; AC21 NC 5's wait and report), `DESTROYED <hwnd>` when a `Destroy` entry is staged by its `Destroyed`, `TEARDOWN_CONFIRMED <hwnd>` when the publish step reads `IsWindow` FALSE, and `RELEASED <view>` inside the one function that sends every `WindowReleased` (AC10 NC 1, AC24), `CLOSE_DISPATCHED <hwnd>` in the `CloseRequested` arm and `BRIDGE_POSTED <command>` from `spawn_app_wake_bridge` (AC25), `PARENT_MOVED <view>` when the notification returns (AC26), `KEY_REISSUED <hwnd>` when a build lands on a releasing key (AC24 NC 3); stderr readbacks at loop exit of the environment-creation count (AC15), of the controller-creation count (AC23's zero-controller control), of the barrier's arm record (`armed_by` = the terminal command that first armed it, or `none`; AC14 NC 1) and of the `releasing` map size (AC16). `KELD_TEST_WINDOWS_ENVIRONMENT_DESCENDANT=<path>` (T2, AC23 NC 3 and AC27), compiled only under `cfg(all(feature = "profile-test-hooks", debug_assertions))`, makes keld-core spawn the named test helper once, right after the engine is constructed (A21's `Running` write), without waiting for it: a host child, so a host-death Job member, outside every Bun generation Job. `KELD_TEST_WINDOWS_NATIVE_HOLD` and `KELD_TEST_WINDOWS_PAGE_READBACK` (T3, AC24–AC26) are the two hooks U18 proposes, unapproved until U18 records a decision. Each readback and hook lands with the first task whose AC uses it. Every new hook name is scrubbed from the Bun env with the existing one. Never compiled into release | test evidence for T11/arming/receipt binding | main `:305`, `:2699-2701` |

### 4.b New/changed types & channels

No new wire type, channel or `.k.ts` contract. Rust surface:

```rust
// crates/keld-wv/src/engine.rs — cfg widened (public API gate)
#[cfg(any(target_os = "macos", windows))]
pub enum AppReadyTrigger { NavigationReady, HostInitialized }
#[cfg(any(target_os = "macos", windows))]
HostInitialized { observed_at: Instant },            // AppWindowEvent variant
pub(crate) const fn initial_renderer_pending(registry_managed: bool, closing: bool) -> bool; // cfg + windows

// crates/keld-wv/src/webview2/mod.rs — public signature change (public API gate)
pub fn run_app_until_quit(
    self,
    commands: Receiver<AppWindowCommand>,
    events: Sender<AppWindowEvent>,
    ready_trigger: AppReadyTrigger,
    on_loop_entry: impl FnOnce(),
) -> Result<(), WvError>;

// loop-private; no `last` map — Core owns last-sent state (`managed_windows.rs:418-427`)
struct ManagedViews {
    managed: BTreeMap<u32, View>,                       // registry-managed views
    releasing: BTreeMap<tao::window::WindowId, ReleasingEntry>, // parked, then staged; removed only at D6's publish
}
struct ReleasingEntry { kind: Releasing, hwnd: isize, parked_at: Instant, staged: bool } // hwnd read before the drop
enum Releasing { Destroy(WebviewId), FailedBuild { build: u64 } } // FailedBuild also covers a pre-`View` failure (D12 a)
// D6: Destroyed stages and posts one ProfileReleaseWake; each closure run reads IsWindow for staged
// entries and FALSE publishes; TRUE rechecks at STAGED_RELEASE_RECHECK (10 ms) until the bound.
// D6 exit gate: Exit only when `releasing` is empty and the barrier is ready; an expired entry
// latches the barrier's one fault (ReleasingExpired); every latched fault takes the D6 fault
// reaction: stop committed, managed views dropped, exit through the gate, KELD-WV-009
// existing `fault: Cell<Option<ProfileReleaseFault>>` (:2539) is the one latch; the enum gains
// Host-site variants, each with Host and Probe `context` text (D8 latch owner):
enum ProfileReleaseFault { GraceExpired, HangGuardExpired, AbnormalExit, ObserverDropped,
    UnidentifiedController, WakeRetireFailed, RebindUnproven, ReleasingExpired }

// loop-private (D8): the Host barrier's binding, written by the loop, read by the
// BrowserProcessExited handler; replaces the bare `expected_browser_pid: AtomicU32`
struct BrowserBinding(AtomicU64);                       // (generation: u32, pid: u32); generation 0 = unbound
// generation advances by checked_add (overflow = RebindUnproven); the ambiguity flag is
// a loop-side Cell<bool>; the Probe site stores (1, pid) through the same type and handler
// BrowserExitWake::retire(self) -> Result<(), WvError>: checked blocking UnregisterWaitEx; on
// failure the wake is forgotten as Drop leaves it, never unregistered twice or re-registered (D8)
// every managed view: ICoreWebView2::add_ProcessFailed; kind BROWSER_PROCESS_EXITED queues an
// abnormal receipt on the one receipt channel, so poll latches AbnormalExit (D8 (3)); at
// the Host site an abnormal BrowserProcessExited is queued whatever its pid (D8 (3)); this is
// the one per-view ProcessFailed registration, and W1 (#1307) dispatches RENDER_PROCESS_EXITED
// inside it

// crates/keld-core/src/app_session.rs — Windows takes the terminal too (public API gate)
pub fn run_guarded(boot: ValidatedBootSelection, pre_init_terminal: fn(&HostAppError) -> !) -> …;
pub fn run_unprivileged(boot: …, pre_init_terminal: fn(&HostAppError) -> !) -> …;

// one sidecar admission owner (generalised Linux helper; keld-runtime or keld-core) for the
// Linux strict launcher, the macOS guardian and the Windows primary. Refuses symlinks and
// reparse points on every platform and a staged bunfig on every launch; `None` = absent
// (legacy entry). Its caller also applies the boot-mode define and removes `BUN_OPTIONS`,
// the macOS rule on `main` (`keld-host/src/main.rs:478-520`).
pub fn admit_staged_transport_sidecar(root: &Path) -> Result<Option<PathBuf>, RuntimeError>;
```

`AppWindowCommand`/`AppWindowEvent` variants and `WindowSnapshot`/`BuildStage` are
gh531's (`engine.rs:102-240`); Windows implements them, adds none.

### Capabilities required; manifest changes (spec 03)

none. The window channel stays `Authority::HostInternal`; the Windows guard handlers
(`add_PermissionRequested` deny-by-manifest, `add_NewWindowRequested` deny-all) are
installed per view before any navigation exactly as today (`:3570-3574`).

### Wire/protocol changes (spec 02)

none. Channel 5 and its payloads are gh531's; Windows begins answering them.

### Platform notes

- Windows (this spec): veto primitive per §4.0; release staged at `WM_DESTROY` and
  published after `DestroyWindow` returned (D6);
  outer frame by D14; focus by D11; full screen is borderless (D10); DPI per-monitor-v2
  is set before any HWND (`:512-540`); the UI thread is an STA (A11).
- macOS: behaviour unchanged (gh531 Platform notes; PR #1181); helpers move (§4.0 reuse).
- Linux: stays parked (F02-T12); still `KELD-CORE-034` at boot and `KELD-CLI-047` in the CLI.

### Runtime seam

- Before: Windows loop owns close outright (`close_requested_view`), commits the
  profile `Stopping` at the last close request, arms release there, writes
  `LastWindowClosed`; Ready comes from the first navigation (`NavigationReady →
  signal_ready`, main `app_session.rs:4622-4634`; outside macOS it signals Ready there
  only while no window registry exists, which T1 must keep); Bun starts without the
  transport preload; the stop record is snapshotted at loop entry.
- After: for facade boots the keld-core registry owns identity and close policy; the
  loop executes `CreateWindow`/`DestroyWindow`/setters and reports facts; Ready comes
  from `HostInitialized` under the 15 s watch and the Windows pre-Init terminal;
  `Stopping`/release arming move to the terminal command; the release receipt is bound
  to the current browser instance and rebinds at each controller completion, failed builds included, and a browser `ProcessFailed` or any abnormal browser exit drained before the loop's last exit check, even after the current instance's normal credit, latches `AbnormalExit` (D8); the
  stop record binds `Running` at loop entry (A21); Bun preloads the staged sidecar. OS
  grants: none new. Crash domain: host process. Handle lifetime: `View` from
  `WindowBuilt` to its drop, and the HWND's `releasing` entry from that drop to D6's publish
  step; a failed build's standalone `Window` from its creation to the same step (D12).
  Value/error/order: gh531 §4.b/§4.e. Config capture: `Create` admission (gh531), boot
  kind at descriptor validation (D2), preload at launch (D13).

### Migration unit

- Callers: none in `@keld/*`. Handlers: `coordinate_window_events` (Windows arms), the
  Windows loop arms (D4–D12), `keld-host/src/main.rs` (terminal, launcher helper).
  Generated contracts: none. Persisted state: the profile lifecycle record's `Running`
  moment moves to engine construction (A21) and `Stopping` to `Quit` (A9); the record
  format and phase table are unchanged.
- Temporary adapter: the legacy renderer arm (owner F01-T2, removal per gh531 §4.c).
  Permanent compat facade: `@keld/electron` `BrowserWindow` (gh531).

## 5. Boundaries

- Implement in: `crates/keld-wv/src/webview2/mod.rs` (loop arms, in-loop build,
  release map, focus handlers), `crates/keld-wv/src/engine.rs` (cfg widening), one
  platform-neutral keld-wv module holding the hoisted helpers (import-path edits only in
  `crates/keld-wv/src/wkwebview/`; no macOS behaviour change), `crates/keld-core/src/app_session.rs`
  (D2, D3, D13; Windows direct session only), the shared launcher helper
  (`crates/keld-runtime/src/role.rs` or `app_session.rs`, one owner) and its macOS
  caller `crates/keld-host/src/main.rs` (also the Windows pre-Init terminal),
  `crates/keld-cli/src/boot.rs` and `doctor.rs` (D15), `crates/keld-host/tests/no_flag/windows/`
  (§7), `crates/keld-host/Cargo.toml:36` (the `windows-sys` dev-dependency line gains
  `features = ["Win32_UI_WindowsAndMessaging"]`; A23), the Linux call site of the
  generalised sidecar helper (`app_session.rs:3929`, behaviour unchanged),
  `crates/keld-wv/AGENTS.md` (D6's `IsWindow` query, D10's
  `NotifyParentWindowPositionChanged`, D11's `GetActiveWindow` query and controller focus
  handlers and D8's `ProcessFailed` handler named in the Windows FFI sentence; the exit-wake clause reworded to "at most one
  live registration, replaced only after a checked `UnregisterWaitEx` success", D8, which
  adds no FFI operation), `crates/keld-runtime/src/windows_job.rs` and
  `windows_job/host_death_census.rs` (census policy and rights parameters, D7), the
  Windows primary launch and its per-generation Bun Jobs (`crates/keld-runtime/src/role.rs`,
  `app_session.rs`), `crates/keld-runtime/AGENTS.md` (unsafe scope, U14),
  `docs/architecture/05-webview-and-native.md` (release-proof text), `docs/specs/gh531-…md` (§1, §4.b,
  Platform notes), `docs/architecture/04-electron-compat.md`,
  `docs/engineering/product-status.tsv`.
- Must not touch: `window_router.rs` and the registry (shared state machine),
  `crates/keld-ipc/*`, `packages/@keld/*` (a needed Windows-specific TypeScript change is
  a blocker to raise), the `WebEngine` trait, macOS loop behaviour, Linux backend beyond
  exhaustive-match arms, `keld-guard`, the profile parser/policy (W6), the profile
  lifecycle phase table (`lifecycle.rs:153-176`; A21 moves only the `Running` write
  site; the release witness moves to the host-death Job census, D7), the workspace
  `Cargo.toml` and any other dependency line, the keld-wv production `unsafe` allowlist
  except the two D11 entries (WebView2 controller focus handlers; the `GetActiveWindow`
  query, whose `windows` feature keld-wv already enables, `crates/keld-wv/Cargo.toml:94`),
  D6's `IsWindow` query (same feature), D10's `NotifyParentWindowPositionChanged`,
  and D8's `ProcessFailed` handler registration and exit-wake clause rewording, each with a nested `AGENTS.md` update plus independent unsafe evidence on the exact
  diff (root `AGENTS.md`).

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 **Windows Ready without a window, behind a test admission.** D1 with
  `Some(false)`, D2's test admission (product builds keep refusing, below), D3, D13,
  D15's staging half; the one sidecar admission owner replacing the Linux, macOS and
  Windows copies (with a symlink/junction negative per platform); PROPOSED
  `facade_ready.rs` (AC1–AC5, AC8, AC20) plus the stalled-Init child (AC2); the
  `keld dev` lane (AC19); AC23's cell, whose T1 outcome is the product refusal; the
  Windows product fixture's `hint` turned into a live-environment read (AC4,
  `support/product.rs:112`); `first_host_init_turn` hoisted (D3 consumes it at T1; the other helpers wait for T3); the
  `EnumWindows` census helper in `support/window.rs` and the A23 dev-dependency feature (AC1 and
  AC8 read the census at T1, AC22 at T2); its §7 mutant receipts (AC23 NC 1 included). Because T1
  edits `keld-host/src/main.rs` and the guardian launcher, the macOS `facade_ready`/
  `facade_window` suites run on a real Mac at T1's head before it lands. gh531 §1/notes
  and 04 amendment text. **No product admission at T1.** A product build still refuses a
  Windows schema-2 boot with `KELD-CORE-034` before Ready (`app_session.rs:1340-1351` via
  `:1447`; `:3431-3438`), and the `main` test `windows_boot_files_refuse_a_schema_2_facade_boot`
  stays; only a debug `profile-test-hooks` build launched with
  `KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT=1` admits one (D2), so T1's facade-boot cells are
  `cfg(all(feature = "profile-test-hooks", debug_assertions))`, set that env, and use
  ephemeral stages only. Product admission waits for A21's `Running` write and D7's
  release witness (T2, AC23), because without them a persistent zero-window `Quit` fails
  `KELD-WV-009` with the record `starting` (AC23 NC 1) and an ephemeral release would rest
  on one runtime's environment-start fact (U10). The one user-visible T1 change for a
  schema-2 project: `stage_dev_boot` stages it on Windows (T1's fixtures stage through it),
  so the refusal comes from the host's `KELD-CORE-034` instead of the CLI's staging error;
  `keld doctor` keeps refusing until T2 (D15). Blocks on: nothing outside this spec.
- [ ] T2 **Release proof and product facade admission, before any managed window.** D7: the one census with its
  `Observe` policy and rights parameter, the per-generation Bun Jobs and the keld-wv
  release-witness seam lent by keld-host (U14), applied on every `run_app_until_quit`
  release (renderer boots and zero-window facade boots included); A21's `Running` write site; D2's
  product admission (both refusals removed for every build,
  `KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT` deleted, so T1's facade cells run on product
  admission) and D15's `keld doctor` half, in the same diff as A21 and D7, so no head
  admits a product facade boot without both; `windows_boot_files_refuse_a_schema_2_facade_boot`
  replaced with AC3's admission test as a recorded contract change (A2); D1's
  derived rule, whose engine input (the keld-wv managed-window support query, §8)
  answers no until T3, so facade sessions stay `Some(false)` (AC22); the D16 items its cells use
  (the environment and controller counts, the loop-exit readback and the descendant hook); the
  always-clean test witness of the `run_app_until_quit` fixture (D7); the `post_close` helper and
  a live stderr line reader in the test support, because `process.rs` today reads the pipe only
  after exit (`:400-411`) and AC23 NC 3 and AC27 act on lines while the host lives; the
  `crates/keld-runtime/AGENTS.md` unsafe-scope edit and
  arch 05 text. D7's current-instance wording rests on D8, which lands at T3: at T2 no session can
  start a second browser, because managed windows are refused (AC22) and a renderer boot binds
  once, so today's flag is that instance's receipt (AC18 NC 4 (b) re-runs at T3). Tests: AC23 on T2's exact head (the persistent `Quit` cell with its
  zero-controller and descendant-release controls, NC 2, and the environment-started
  descendant cell, NC 3); AC18 (b) with NC 1, NC 3 and NC 4 in `profiles/facade_lifecycle.rs`
  on `InstalledApp`; AC22 on T2's exact head; AC19's `keld dev` `Quit` variant, now with
  the census before the host's own ephemeral deletion (the stage gate itself is the
  attempt Job's and unchanged, AC19); AC27 (a)–(d), so every release kind has a negative at
  the head that adds the witness; the existing Windows renderer-boot lifecycle and
  profile suites stay green with the census; its §7 mutant receipts (AC18 NC 1, NC 3 (a);
  AC22 NC 2; AC23 NC 2, NC 3; AC27 (a)–(d)).
  Rejected orders: moving D7 into the windows PR puts two unrelated unsafe surfaces
  (keld-runtime Job FFI and WebView2/COM) into one review and one PR; landing the managed
  windows first behind `Some(false)` ships product code that no device test can reach;
  flipping before D7 exposes windows without the release proof (AC22's mutant);
  admitting product facade boots at T1 exposes the persistent zero-window `Quit` failure
  (AC23 NC 1); merging T1 into T2: the only T1 hazard is product admission, which the
  test admission moves into T2 at no cost, while one PR would put the three-platform
  launcher fold (re-run on a real Mac) and the new keld-runtime Job-FFI unsafe scope into
  one review.
  Blocks on: T1 merged (the facade boot that AC18 (b) and AC22 NC 2 launch, and the
  run-entry signature U14 extends); U14 is decided inside T2's own unsafe and
  public-API gates.
- [ ] T3 **BrowserWindow build, veto, release; managed windows enabled.** The engine's
  managed-window support and with it D1's derived `Some(true)` (AC22 positive control),
  blocked by the native fault gate below; D4–D12, D14, D16's remaining lines, the remaining
  hoisted helpers; D6's
  releasing bound, exit gate and fault reaction; D12's pre-`View` custody; D8 in full
  (instance binding, failed-build bind, checked wake retirement, named fault variants, the
  browser `ProcessFailed` and any-pid abnormal latches, drained even after the current
  instance's normal credit), D10's and D11's per-view host obligations (AC26, renderer boots
  included), because T3 is the first change that can build a second
  browser instance in one session, with its unit control (AC21 NC 4); the U7 fixture
  (approved, §10) and the U18 hooks (approved, §10); the `crates/keld-wv/AGENTS.md` edits
  (D6, D8, D10, D11). PROPOSED
  `facade_window.rs` (AC6, AC7, AC9–AC13, AC15, AC16, AC24–AC26), `facade_lifecycle.rs` (AC14, AC17)
  and `profiles/facade_lifecycle.rs` (AC18 (a), NC 2 and NC 5; AC21; AC27 (e)) on `InstalledApp`; the
  `GetWindowRect` witness and the remaining test helpers (A19, §7); the mutant receipts (§7); product-status row; W0 receipt handoff
  (device, commit, WebView2 runtime version). T3 retires AC22's T2-head device test
  (`windows_facade_window_is_refused_until_the_release_proof_lands`), whose refusal it
  removes on purpose, as a recorded contract change (the A2 precedent), replaced by
  AC6; AC22 NC 1 stays. **Native fault gate.** The derived `Some(true)` is BLOCKED until
  AC16 NC 3, AC16 NC 4 and AC18 NC 5 (its `view` and `webview` points) pass on real
  Windows at T3's exact head through the U7 fixture, and AC24 (a)–(c), AC25 (a), (b) and
  AC26 (a)–(d) pass there through the U18 hooks, or an independently approved
  equivalent device oracle, each with a nonzero at-risk count: AC24 counts runs whose
  `DESTROY_HELD` line is followed by a TRUE `IsWindow` read; AC25 counts its at-risk proofs
  (`PUMP_SAW_CLOSE`; `IsWindow(A)` FALSE before `DESTROYED <A>`); AC26 counts runs where the
  witness moved, the window minimized, or both foreground steps held, and each voidable read of
  (b) needs its own nonzero count, except NC 8's declared case; AC16 NC 3 counts runs
  whose U7 failed-HWND line appears (the entry was parked); AC16 NC 4 counts runs with
  `TERMINAL Quit releasing=1` (its validity rule); AC18 NC 5 counts `view`-point runs
  whose `REBIND gen=1` line precedes the U7 failure and `webview`-point runs with U7's
  failed-HWND line. Their receipts must show `TERMINAL Quit releasing=1`, the deferred
  and the missing `Destroyed`, the failed builds after controller creation, the withheld
  current-instance receipt, the bounded `KELD-WV-009` with `stopping` against the
  positive controls' `idle`, and no owed `WindowBuildFailed` lost. Unit results (AC21
  NC 4 (h), the D6 exit-gate and expiry-latch units) are controls of the rules, never
  substitutes, and cannot qualify T3. *Gate NC:* T3's approval evidence names these
  cells with their at-risk counts at the exact head; with the U7 fixture or the U18 hooks
  omitted and no approved equivalent in their place, or any of the cells skipped, compiled out, unrun on real Windows or at zero at-risk runs,
  approval stays RED even when every ordinary Windows test and every unit test passes.
  The U7 approval covers the fixture only; no unsafe-gate evidence is claimed for it or
  for any T3 production path, which keep their own gates (§8). Blocks on: T2 merged (D7,
  A21 and the lent witness, without which D1 stays `Some(false)`); the native fault gate above.

## 7. Test plan

All real-Windows tests live in the existing `no_flag_windows` target
(`crates/keld-host/tests/no_flag_windows.rs:18-58`; kel259 families) and use its TCP
control protocol (`support/product_cycle.rs:28-50`), `dev_stage_command`
(`no_flag_windows.rs:376-383`) and `ProductFixture` (a schema-2 variant writes
`keld.config.ts` with `initialWindow: "none"` and no renderer, mirroring 
`facade_ready.rs:44-52`). The persistent-profile row uses `support/installed.rs`
`InstalledApp` (KEL-254 T3 Part B fixture). At T1 every facade-boot cell is
`cfg(all(feature = "profile-test-hooks", debug_assertions))` and launches with D2's test
admission; from T2 they run on product admission. AC23's cell never sets it.

| AC | PROPOSED test (under `tests/no_flag/windows/`) | Kind |
|---|---|---|
| 1, 3, 4, 5, 8, 20 | `facade_ready.rs::windows_schema_two_host_emits_ready_without_an_initial_native_window`; `::windows_primary_preloads_the_staged_transport_and_refuses_bunfig_and_junctions` (facade and renderer boots; AC5's first-module env deletion and dynamic facade import) | real-Windows integration |
| 19 | `dev_lifecycle.rs::shipping_windows_keld_dev_cleans_a_facade_stage_after_the_attempt_job_empties` (reuses `wait_for_dev_stage_count`, `no_flag_windows.rs:362-374`; Quit and stalled-Init terminal variants) | real-Windows, `keld dev` lane |
| macOS guard (N7) | existing `facade_ready.rs` / `facade_window.rs` suites on a real Mac at T1's head, because T1 edits the shared launcher and `keld-host/src/main.rs` | existing, prerequisite |
| 2 | `facade_ready.rs::windows_schema_two_stalled_init_exits_through_the_binary_terminal_once` (`cfg(all(feature = "profile-test-hooks", debug_assertions))`): asserts terminal once, Bun gone, failed exit status, stage directory present | real-Windows, debug fixture |
| 3 (renderer unchanged) | existing `lifecycle.rs`, `recovery.rs`, `dev_lifecycle.rs` unchanged, plus one assertion that no channel-5 frame is written | existing + one assertion |
| 6, 7, 9, 10, 11, 12 | `facade_window.rs::windows_electron_browser_window_uses_native_host_and_vetoes_wm_close` (vocabulary `PUBLIC_CREATED`, `VETO`, `CLOSE_EVENT`, `VERIFY_VETO`, `VETO_VERIFIED`, `STALE`, `ALLOW`, `CLOSED` as `facade_window.rs:70-95`, `:632-736`; stimulus A16; census A15; witness A19; child-HWND set and lease share-denied probes; AC10's D16 `DESTROYED`/`RELEASED` order and shortened hang guard) | real-Windows integration, debug fixture (AC10) |
| every device NC whose single fault is a code change: 1; 2; 3; 4 NC1/NC2; 5 NC1a/NC1b/NC2/NC3/NC4; 6 NC1 (Windows half)/NC2; 7; 9; 10 NC1/NC2; 11; 12; 13 NC and initial-snapshot NC; 14 NC1; 15; 16 NC1/NC2/NC4; 17; 18 NC1/NC2/NC3 (a)/NC5; 20; 21 NC2/NC3; 22 NC2; 23 NC1/NC2/NC3; 24 NC1; 25 NC1/NC2; 26 NC1–NC8; 27 (a)–(e) | mutation runs: documented local patches, each the single fault its NC names (for example no define; no `--preload`; an inherited `BUN_OPTIONS` preload; immediate `View` drop on `CloseRequested`; `WindowReleased` at drop; the legacy arm in a facade session; managed views kept in the legacy `views` map; `BuildFailed` before `Destroyed`; a pre-`View` failure that drops the `Window` without a `FailedBuild` entry; only the `Quit` arm call removed; today's sticky `observed` kept across the D8 rebind; the loop exit without the `releasing` gate; a failed build that never rebinds; r15's literal T2 flip; the T1 test-admission check dropped; the census skipped for a zero-controller release; a publish at `Destroyed`; the first buffered `CloseRequested` or `Destroyed` after a build dropped; the notification, the minimize visibility or the focus return suppressed; facts forwarded while minimized; focus facts suppressed while minimized; the maximized fact read on `Moved`; the restore's focus return removed; the per-view owner limited to the managed map; the witness bypassed on one release kind), each run in the task that lands its AC (§6), the shared-registry patches of AC11 and AC12 locally only; each is one fault; record the failing assertion and line in the PR evidence; patches are never committed. Not in this row: fault-injection cells (AC14 NC 2, AC16 NC 3, AC18 NC 3 (b)), positive controls (AC21 NC 5), AC8 (the shared adapter's), AC19 (not exercised) and the unit rows below | mutation control |
| 13 | `facade_window.rs::windows_focus_fact_is_activation_without_transient_blur` (counts focus facts across the build's `MoveFocus`; activates a helper process window, then the host window) | real-Windows integration |
| 15 | `facade_window.rs::windows_two_browser_windows_share_one_profile_lease_and_browser_process` (counts `webview2*` directories under the control dir) | real-Windows integration |
| 16 | `facade_window.rs::windows_controller_failure_after_minting_releases_the_hwnd` (U7 fixture, approved; required at T3's exact head, §6 native fault gate: failure right after the HWND is minted; `IsWindow(hwnd)` FALSE when read immediately after the error; empty `releasing` readback; fresh next build) and `::windows_lost_destroyed_after_a_failed_build_ends_in_a_bounded_fatal` (U7 `swallow` mode, NC 3) and `::windows_terminal_waits_for_pending_releases` (U7 `defer` mode, NC 4) | real-Windows, debug fixture |
| 14 | `facade_lifecycle.rs::windows_facade_last_window_quits_releases_profile_and_relaunches` (two cycles; default policy and draw.io's listener; pre-Quit `process_tree_report` snapshot; NC 1 asserts the D16 `armed_by=Quit` readback) and `::windows_facade_quit_with_a_withheld_receipt_fails_bounded` (NC 2: h9 hook, `KELD-WV-009` within the bound, positive control without the hook) | real-Windows integration, debug fixture |
| 17 | `facade_lifecycle.rs::windows_facade_recovery_keeps_the_same_hwnd` | real-Windows integration |
| 21 | `profiles/facade_lifecycle.rs::windows_release_proof_survives_browser_generation_changes` (`InstalledApp`; regimes i and iii on the device; regime ii's census half decided by AC18 NC 4 (a) and (b)) and `::windows_a_previous_browser_receipt_never_releases_a_later_browser` (NC 3: later-generation hook, record `stopping`; positive control `idle`; only runs whose D16 lines show the at-risk order count) and `::windows_a_failed_browser_ends_the_session_before_any_rebind` (NC 5, positive control) | real-Windows, installed, debug fixture |
| 22 | keld-core unit of the derived `window_native_support` (four input rows, two mutants); `facade_ready.rs::windows_facade_window_is_refused_until_the_release_proof_lands` on T2's exact head (NC 2) | unit + real-Windows, staging gate |
| 23 | `profiles/facade_lifecycle.rs::windows_product_facade_admission_waits_for_the_release_proof` (`InstalledApp`, no test-admission env; T1's head: the refusal; from T2's head: `READY`, `Quit`, `idle`, with the zero-controller and descendant-release controls) and `dev_lifecycle.rs::windows_zero_window_release_waits_for_an_environment_started_descendant` (D16 descendant hook, NC 3) | real-Windows, installed, staging gate; NC 3 debug fixture |
| 24 | `facade_window.rs::windows_release_waits_for_native_teardown` (U18 `destroyed` hold; allowed close, `destroy()` reply, U7 `hwnd` point; TRUE `IsWindow` during the hold, FALSE for the HWND and captured children at the reply) | real-Windows, debug fixture |
| 25 | `facade_window.rs::windows_close_buffered_during_a_controller_pump_is_delivered_once` and `::windows_destroyed_buffered_during_a_controller_pump_is_released_once` (U18 `build-pump` hold) | real-Windows, debug fixture |
| 26 | `facade_window.rs::windows_host_window_obligations_reach_the_controller` (U18 page readback; move, minimize, focus return) and `lifecycle.rs::windows_legacy_window_keeps_the_host_window_obligations` (d) | real-Windows, debug fixture |
| 27 | `profiles/lifecycle.rs::windows_renderer_release_waits_for_a_job_member` (a), `dev_lifecycle.rs::windows_renderer_ephemeral_deletion_waits_for_a_job_member` (b), `profiles/handover.rs::windows_handover_waits_for_a_job_member` (c; operator fixture, `#[ignore]` like h9–h12), `profiles/facade_lifecycle.rs::windows_facade_release_waits_for_a_job_member` (d, e) | real-Windows, debug fixture; (a)–(d) at T2, (e) at T3 |
| 18 | `profiles/facade_lifecycle.rs::windows_installed_facade_profile_record_reaches_idle` (`InstalledApp`; record reader U11) and `::windows_failed_build_after_controller_creation_keeps_its_receipt_obligation` (NC 5, required at T3's exact head, §6 native fault gate: U7 `view` point under the h9 hook, positive control `idle`; U7 `webview` point) | real-Windows, installed; NC 5 debug fixture |
| D3 cfg, D14 arithmetic, A20 release map, D6 stage and publish steps (AC24 NC 2–NC 5), releasing bound, exit gate and expiry latch (AC16 NC 3 record half), D10 per-view method table, D8 receipt binding, checked wake retirement, browser `ProcessFailed` latch and any-pid abnormal latch (AC21 NC 4), D11 focus derivation | keld-wv unit tests without `EventLoop::new` (pure functions; release map with a reused key, parked and staged, and an expired entry; the publish step over an injected `IsWindow` read and over a dropped receiver; the exit predicate over `releasing`, staged entries included, and the barrier; the expiry latch with an injected witness; the D8 binding table with stamped receipts and the live/exited handle helpers, including failed-build completions, normal and abnormal receipts of a foreign pid, and an abnormal receipt drained after the current instance's normal credit (AC21 NC 4 (l)); a test double of the wake operations that fails retirement and counts registrations; focus state table, including a `Focused(true)` for an inactive window that makes no `MoveFocus`; a `CloseRequested` for a releasing or unknown HWND ignored with one diagnostic) | unit |

Helpers (PROPOSED, `support/window.rs`; `top_level_windows` lands at T1, `post_close` at T2 with
the live stderr reader, the rest at T3): `top_level_windows(pid)`, `child_windows(hwnd)`,
`window_rect(hwnd)`, `post_close(hwnd)`, `is_window(hwnd)`, `move_window(hwnd, dx, dy)`,
`show_window(hwnd, cmd)`, `focus_owner()` via `windows_sys` with the A23 feature (the test
crate permits test-only `unsafe`, `no_flag_windows.rs:4-6`).

Anti-flake: no sleeps for synchronization (`.agents/testing.md:35-36`); the duration
oracles (AC1, AC7) assert liveness after a bound. Orders come from control-line
sequence. Each test owns its listener port and temp stage; `PRODUCT_DEADLINE` (20 s,
`no_flag_windows.rs:85`) bounds every read. WebView2 first-attempt startup failures
(#814) and Quit-tail faults are reported, not retried. GUI tests run only in the
real Windows desktop lane (`windows-no-flag-gui`, kel259 AC1).

## 8. Review gates triggered

- **unsafe:** yes. (1) Existing sanctioned WebView2/COM blocks are re-sited into the
  loop closure (a new production path: independent unsafe evidence on the exact diff),
  including D8's rebind, which reuses the sanctioned `OpenProcess` SYNCHRONIZE,
  `WaitForSingleObject` and the one `RegisterWaitForSingleObject`/`UnregisterWaitEx`
  exit wake (moved, never doubled: D8's checked retirement re-sites the blocking
  `UnregisterWaitEx` from `Drop` into a fallible operation that must succeed before the
  next registration, and a failed retirement forgets the old wake, latches
  `WakeRetireFailed` and registers nothing; the `crates/keld-wv/AGENTS.md` exit-wake
  clause is reworded to "at most one live registration" in the same diff and needs the
  same independent unsafe evidence), and D12's `Close()` of a controller created before
  a `CoreWebView2` failure (the `View::drop` operation);
  (2) D11 registers `ICoreWebView2Controller` `GotFocus`/`LostFocus` handlers (new
  blocks under the sanctioned WebView2 class) and adds one new Win32 FFI entry point,
  the `GetActiveWindow()` query on the UI thread (no hook, no WndProc); D6 adds a second,
  the `IsWindow(hwnd)` query on the UI thread, for a window this thread created, with its value
  read before the drop and never dereferenced (test-only until now, A23); D10 adds the
  `ICoreWebView2Controller::NotifyParentWindowPositionChanged` call and new sites of the
  sanctioned `SetIsVisible`, and D11 a new site of `MoveFocus`; D8 registers
  `ICoreWebView2::add_ProcessFailed` on every managed view and reads only
  `ProcessFailedKind` from its args, a new WebView2 COM event registration (keld-wv
  registers no `ProcessFailed` handler at `457a9c3b`) under the sanctioned WebView2 class; it is
  the one per-view `ProcessFailed` registration: W1 (#1307 §4.5, Q13) runs its kind dispatch
  inside it (`RENDER_PROCESS_EXITED` → W1's revoke and `onLoss`), W2 owns the registration and
  the `BROWSER_PROCESS_EXITED` latch, and nobody registers a second handler;
  D6's `IsWindow`, D10's notification, D11's two and D8's registration must each be named in
  the `crates/keld-wv/AGENTS.md` Windows FFI sentence, with
  independent unsafe evidence on the exact diff. D14 uses Tao getters
  only. (3) The census observe policy is a new production path in keld-runtime's
  sanctioned Job FFI and needs the `crates/keld-runtime/AGENTS.md` scope edit plus
  independent evidence (U14). Test-only `unsafe` follows the test crate's allowance.
- **public API:** yes. `AppReadyTrigger`/`HostInitialized` on Windows; the
  `WebView2Engine::run_app_until_quit` signature; `run_guarded`/`run_unprivileged`
  taking the terminal on Windows; the shared launcher helper; the keld-wv release-witness
  seam, the keld-wv Windows engine's managed-window support query that D1 derives from (AC22), the `exit_census` policy/rights parameters and the host lending the host-death Job
  capability (U14); Windows stops answering
  `KELD-CORE-034`/`KELD-CLI-047` for schema-2 (the host and `keld doctor` from T2, with product admission;
  `stage_dev_boot` from T1, D15) and `SubscribeRefused`/`KELD-CORE-045` for
  window traffic (documented behaviour change).
- **permission model:** none. No capability, manifest key or guard evaluation changes;
  the window channel stays host-internal (gh531 §4.g); the facade hint selects a
  barrier, not authority (gh531 `:828-829`); guard handlers unchanged
  (`:3570-3574`); the preload adds no authority (it is Keld's own transport).
- **dependency:** yes, feature-only, test-only, at T1. `Win32_UI_WindowsAndMessaging` is added
  on keld-host's `windows-sys` dev-dependency line (`crates/keld-host/Cargo.toml:36`),
  not on the workspace line (`Cargo.toml:142`), so no production crate's feature set
  changes. No new crate; tao 0.35.3, webview2-com 0.38.2, windows 0.61.3 stay the
  audited pins (#1182); keld-wv already enables the `windows` crate's
  `Win32_UI_WindowsAndMessaging` (`crates/keld-wv/Cargo.toml:94`) for D11's query.
- **wire protocol:** none. Channel 5 and payloads unchanged (`02-ipc.md` diff).

## 9. Perf impact

No claim. Architecture 01 §5 "cold start → first paint" moves for facade boots (window
#1 waits for Bun module evaluation, the preload, and one blocking `Create`, gh531 §9);
measure on the device with the keld-benches Windows lane before any number is cited.
Controller creation stays the 313–506 ms floor (learnings 2026-08-15). The release
bounds are unchanged: `WEBVIEW2_UI_END_BOUND` stays hang guard (4 min) + post-exit
budget (85 s) (`webview2/mod.rs:248`, `:305`, `:310-319`). The barrier's bounds are unchanged (D8
binds its receipt to the current instance; it adds no wait), and the census
runs after the event with whatever remains of the 85 s post-exit budget, which keld-wv
passes to the witness (no copied `85` in keld-core). The docs raise the event only "after
all associated processes have exited", and the Job emptied 0.2–0.4 ms after the last
event (19/19), so a healthy census adds no measurable wait; members the event does not
cover (an older generation, a launched-but-unbound browser) are waited for within the
same remaining budget and fail closed at it. The sum, and every consumer of it
(`WEBVIEW2_TEARDOWN_BOUND`, `handover.rs:131-134`, `media_acceptance.rs:55`), is
unchanged. D14 adds one invisible build step per window. D6's publish adds one `IsWindow`
read per closure run while an entry is staged (first wake 127.9–221.6 µs after `Destroyed` in
the 4 r18 runs without a hold, prior evidence), and D10 one COM call per `Moved`; neither allocates, and no number
is claimed for a drag. D6's releasing bound (`PROFILE_LAUNCH_DEADLINE` + 5 s) is a
fault deadline only; a healthy `Destroyed` (22–61 ms, A20) never waits for it. The D6
exit gate waits for pending entries concurrently with the barrier; every entry was parked
before the terminal, so the gate ends at most that bound after the terminal, below
`WEBVIEW2_UI_END_BOUND`, which stays the one UI-end bound; T3 records this in that
constant's doc (`:310-319`), which says a new barrier wait changes the sum.

## 10. Open questions

Each is an UNKNOWN with the probe that decides it, or RESOLVED with the evidence that
decided it. **Blocking for approval: none of the design.** Approval confirms the A21 reading of
KEL-135 recorded under U10 and the census direction in D7 (owner-approved
2026-10-29, gyldlab/keld#1188 comment 6498281882); U14 is decided at T2 under its review
gates.

- **U1 (A7) Nested pump inside the Tao callback — RESOLVED (U1/U6 bundle, §2).**
  Keld's own `wait_for_launch_with_message_pump_until` (`:597-617`, pump `:554-589`)
  completes inside `run_return`'s callback: first controller 306–394 ms (n=10) and
  311–387 ms (n=10), second controller 48–85 ms (n=9), zero-window boot 352–477 ms
  (n=3). A `WM_CLOSE` to another window and a user event posted during the pump were
  delivered after the callback returned, exactly once, in post order, never
  re-entrantly (`results.json` `U1.*`: `delivery_pass` 10/10, 10/10, 3/3, 3/3, 3/3, 3/3,
  3/3; `reentrant_total` 0; `panics_total` 0; in one `second-controller delay=30` run the
  stimulus landed after the pump, still delivered once). No busy spin (process CPU
  0–47 ms per build). The asynchronous `ControllerReady` fallback is recorded only as a
  rejected alternative (A7). Prior evidence on an older code path (§2); the T3 code path's
  exact-head cells are AC6 and AC25.
- **U2 (A14) Child-HWND focus transfer.** Answered from Tao source (A14): Tao cannot
  re-emit focus after a child HWND holds it, until reactivation puts focus back on the
  top-level (A14; r18 probe). *Confirmation probe:* log Tao `Focused` and
  controller `GotFocus`/`LostFocus` on build, `MoveFocus`, Alt+Tab out and in; the D11
  table must reproduce the active-window truth.
- **U3 (A19, W4) Units.** Outer bounds in logical points on mixed-DPI monitors: is the
  created window's `scale_factor()` the oracle W4 declares? *Probe:* W4's coordinate
  contract; until then `WindowBounds` is INFERRED.
- **U4 (D10) Full screen.** Borderless is Keld's cross-platform choice; Tao Windows also
  implements exclusive mode (`window.rs:699-705`). *Probe:* W0's oracle on whether the
  pinned app exercises `setFullScreen` on Windows; if yes, read `fullscreen().is_some()`
  after each `Resized` and record the Electron divergence cell.
- **U5 (A16) Close stimulus.** `PostMessageW(WM_CLOSE)` from the test process is INFERRED
  to reach the host HWND (same integrity level). *Probe:* AC9's `CLOSE_EVENT 1`; if UIPI
  blocks it, use an equal-integrity helper; `taskkill` without `/F` only after its
  semantics are cited.
- **U6 (A10, D8) Browser process between last release and Quit — RESOLVED (U1/U6
  bundle, §2, `U6_summary`).** Closing the last controller ends the browser process in
  236–299 ms (median 283, n=10) even with the environment held, and raises
  `BrowserProcessExited` 244–310 ms after the close (10/10); the next `Create` gets a
  new pid (8/8), so D8's instance rebind (new generation, wake moved) is a live path. A non-last close keeps the shared
  browser (8/8 alive at 10 s, no event). With the environment dropped between windows
  the event is never raised (0/10). The race sweep fixed the three regimes D8 and AC21
  encode: same-pid reuse at 18–86 ms (16/16), overlap with the old pid's event arriving
  during the new creation pump at 125–237 ms (12/12, overlap ≤124 ms), clean at
  ≥274 ms (16/16). The `Close` documentation paraphrase
  (<https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#close>)
  is thereby confirmed for the last controller and refuted for a non-last one.
- **U7 (AC16, AC18 NC 5) Build-failure fixture — APPROVED** (repository owner GYLDLAB,
  2026-10-30, <https://github.com/gyldlab/keld/pull/1280#issuecomment-6533011145>). No debug
  control injects a build failure today. The fixture is one debug-only `profile-test-hooks`
  child-only env (same class as `KELD_T1B_STALL_BEFORE_INIT`), compiled out of product
  builds, that fails the next build once at a chosen
  point and writes the failed HWND on one stderr line, then `FAILED_BUILD_DESTROYED
  <hwnd>` and `FAILED_BUILD_ANSWERED <build>` (written inside the one function that sends
  every `WindowBuildFailed`) from the UI thread in the order the loop does them (AC16
  NC 1). Points: `hwnd` replaces `create_controller` with an error right after the HWND
  is minted (D12 case a; no browser, no bind); `webview` lets `create_controller` succeed
  and makes `CoreWebView2` fail (D12 case a with a controller; no readable identity, so
  the D8 (2) latch); `view` fails the first fallible step after the `View` exists (D12
  case b; the browser is bound, AC18 NC 5). Modes for that HWND's `Destroyed` in Keld's
  dispatch: delivered (default); `swallow`, never consumed, so the releasing bound (D6)
  runs (AC16 NC 3); `defer`, held until the end of the first loop turn in which a
  terminal has run and the barrier reports ready or faulted, then woken by one post on the
  mode's own `EventLoopProxy` and consumed through the same function at the start of the
  next turn, before the releasing-expiry check, with `DEFERRED_DESTROYED` and
  `DEFERRED_CONSUMED` lines (AC16 NC 4). One hook name, scrubbed from the Bun env with the D16 hooks and
  never compiled into release. The approval makes AC16 NC 3/NC 4 and AC18 NC 5 mandatory
  on real Windows at T3's exact head with nonzero at-risk counts, with no unit-only fallback
  (§6 T3 native fault gate); it covers neither unsafe-gate evidence for any production path
  nor any change to product admission, which keep their own gates.
- **U18 (AC24, AC25, AC26) Native-order test hooks — APPROVED by the repository owner
  (https://github.com/gyldlab/keld/pull/1280#issuecomment-6536623057), which also extends the
  `KELD_TEST_WINDOWS_ENVIRONMENT_DESCENDANT` scope to AC27.** No
  earlier hook could do these three jobs. U7's `defer` holds
  the event in Keld's dispatch and returns, so `DestroyWindow` finishes and nothing is held
  inside `WM_DESTROY`; `swallow` never answers; U7's points fail a build; D16's existing hooks
  touch receipts, the guard and a spawned helper. **U7 itself needs no extension:** its approved
  points (`hwnd`, `webview`, `view`) and modes (default, `swallow`, `defer`) stay as they are, and
  AC24 (c) runs the `hwnd` point in default mode with the hold below. Approved under the same
  limits as U7 (debug `profile-test-hooks` class, child-only env, scrubbed from the Bun env,
  compiled out of product builds, no product behaviour, permission or public API; the approval
  covers neither unsafe-gate evidence nor product admission): (1) `KELD_TEST_WINDOWS_NATIVE_HOLD=<point>:<dir>`,
  points `destroyed` (AC24: blocks the UI thread last in the `Destroyed` arm) and `build-pump`
  (AC25: keeps pumping in `pump_message_or_handle`), each released by a file in `<dir>` and bounded
  by `PROFILE_LAUNCH_DEADLINE`, with the lines those ACs name; (2)
  `KELD_TEST_WINDOWS_PAGE_READBACK=<ms>` (AC26): every `<ms>` the loop calls `ExecuteScript` on
  each live view with one fixed expression, because W2 windows have no bridge and no
  `executeJavaScript` (W1, W3), and the only non-call observable found is in the page. The
  cells these hooks serve stay mandatory at T3's exact head (§6).
- **U8 Dev-stage custody and handover.** On Windows the host never deletes its stage
  (the host's own deletion is macOS/Linux, `keld-host/src/main.rs:111-134`); the
  `keld dev` private cleanup process deletes it after the attempt Job empties
  (`run_windows_dev_stage_cleanup`, `main.rs:279-292`; `WindowsDevStageCleanup::prepare`,
  `app_session.rs:1526-1562`), so the macOS "unproved stage not deleted" rule
  (gh531 `:279-282`) and `KELD_HOST_OWNS_FACADE_STAGE_CLEANUP` (`app_session.rs:2403-2414`)
  do not transfer. Stage disposition is asserted only in the `keld dev` lane (AC19).
  *Remaining probe:* AC19's terminal variant (does the Job empty promptly after the
  pre-Init terminal, with the blocked main thread's COM/profile owners never dropped?).
  Installed-mode profile handover (`engine.rs:93-99`) with registry windows is out of
  scope; owner: the KEL-254 installed-root boot spec.
- **U9 (A2) Linux cfg shape.** Code-shape only for T1.
- **U10 (A21) Zero-window persistent Quit — RESOLVED (U10 bundle, §2).** A facade
  boot on a persistent profile that quits without ever building a window held the
  record `Starting`, and `Quit` then needs a transition `advance` refuses
  (`lifecycle.rs:153-166`). Decision (D7): write `Running` as the last step of
  `from_selected_environment`, once the environment is bound to the verified user-data
  folder and the exit observer is registered, for every persistent boot; `advance` and
  the phase table are unchanged.
  *KEL-135 reading.* KEL-135 requires `starting` before any engine object and `running`
  only after "the selected store/context is attached". On Windows the store is the
  user-data folder, and the environment is the context bound to it: its creation sets
  exclusive-UDF access and is verified against the leased folder handle and ACL. That
  matches macOS, which marks `running` at store bind. No KEL-135 text changes; approval
  of this spec confirms the reading.
  *Device fact, with its limit.* On WebView2 156.0.4314.50 and Windows 11 26300.9711,
  the environment with its read-back and exit observer launched no browser process; the
  first controller launched it (A21 evidence, 10/10 runs). From T2 on, that fact is no longer
  load-bearing: a pid-0 `Idle` is admitted only by the host-death Job census (D7), which
  observes every process the host or the runtime started, on whatever runtime is
  installed (review finding, rpipaliya 2026-10-29). A future runtime that launched a
  browser at environment creation would leave a Job member and fail closed. Before T2 no
  product release rests on the fact either: at T1's head no product build admits a facade
  boot (D2, AC23), and T1's test-admitted cells use ephemeral stages only.
  *Rejected, with reason.* (1) Widening `advance` with `Starting → Stopping`: a stop
  transition snapshotted while still `Starting` could then overwrite a durable
  `Running` record (lost update). (2) A narrow `Starting → Idle` edge admitted only if
  "no environment or controller creation was attempted": every boot creates the
  environment in `WebView2Engine::new` (`:2932`) before any window, so the
  edge would never apply. (2') The same edge keyed on "no controller creation
  attempted": the probe supports it equally, but it adds a transition to the phase
  table KEL-135 owns, and it gives Windows a stop path macOS does not have. Moving the
  `Running` write changes no table and matches macOS. (3) Deferring environment creation and the `starting` write to the first
  window: sound, but it needs late binding of the exit observer into the
  already-built `ProfileReleaseWait`, an engine-start-failure rule, a typed
  never-started teardown state, and two construction shapes. It also leaves Windows
  shaped differently from macOS.
  *Residual.* The KEL-135 crash rules are unchanged: death in `Running` quarantines,
  and the exclusive-UDF probe recovers it, exactly as death in `Starting` does today.
- **U11 (AC18) Lifecycle record reader — RESOLVED.** The test-side reader already
  exists: `read_lifecycle_record` and `LifecycleObservation` in
  `keld-host/tests/no_flag/windows/profiles/sessions.rs` (`:43-56`, `:394`),
  which `sessions.rs` and `handover.rs` use today. AC18 reuses it; keld-wv needs no new
  reader.
- **U13 Retired-generation device hook — WITHDRAWN (r8).** The Job census (D7) removed
  the retired-generation list that needed it.
- **U14 (A10, D7) Census capability and Bun Job creation — decided at T2.** keld-wv has no
  keld-runtime dependency; it defines a release-witness seam that `finish_profile_release`
  calls, keld-core implements it with the parameterised `exit_census` (D7), and
  `keld-host` lends the host-death Job capability (`main.rs:96-104`) to keld-core's run
  entry (the signature T1 already changes). Open within T2: whether the Windows primary is
  created suspended and assigned before its first instruction (reuse `WindowsSuspendedChild`)
  or assigned right after spawn (a child started first is non-family and fails closed).
  Review gates: public API (keld-runtime, keld-core, keld-wv, keld-host) and unsafe: the
  existing Job FFI operations are reused, but the `crates/keld-runtime/AGENTS.md` unsafe
  scope today reads "exit census … terminate+wait non-family only", so the observe policy
  is a new production path that needs that edit plus independent unsafe evidence on the
  exact diff.
- **U15 (D7) Profile handover path — RESOLVED (r9).** The in-host handover runs the same
  census (D7), so one release proof covers every transition.
- **U16 (A10) Census availability — disclosed.** Members that legitimately outlive the
  browser make a quit fail closed (`KELD-WV-009`, then inline quarantine recovery on the
  next launch): a host-spawned long-lived process (for example a future `openExternal` that
  cold-starts the user's browser as a host child; owner: that API's Windows spec) or a
  console host that briefly outlives its process (`windows_job.rs:3793-3796`; retaken, not
  failed, while the deadline lasts). Bun's console host is INFERRED to join its generation's
  Bun Job; T2 verifies it. The successor's exclusive environment refuses a WebView2 browser of
  another process still on the folder (the existing `Busy` path, `ERROR_INVALID_STATE` in
  `probe_exclusive_udf_with_observer`, `webview2/mod.rs:1924-1933`); it does not detect an
  arbitrary process's open file handle, and whether it refuses a folder held by a non-browser
  or service-started runtime process is UNKNOWN (D7).
- **U17 (D7) Strict containment of the user-data folder — dependency on #1187 (owner:
  the #1187/KEL-210 Strict-primary owner; gate: permission model), UNKNOWN until its probe
  passes; no Strict claim may be made before then.** `validate_profile_acl` today accepts allow-ACEs for AppContainer and capability
  SIDs (`S-1-15-2-*`, `S-1-15-3-*`, `webview2/mod.rs:1420-1425`), and WebView2 may grant its
  own sandbox SIDs on the folder. If any ACE reaches an LPAC Bun (for example "ALL
  RESTRICTED APPLICATION PACKAGES", `S-1-15-2-2`), the Strict claim is false. *Probe:* dump
  the folder's DACL after a real session, and have a zero-capability LPAC child (the #1187
  Strict launch) try to open a folder file; access denied is the pass. Until then the Strict
  claim is INFERRED.
- **U12 (A19, D14) Frame-delta exactness.** Tao's `set_inner_size` derives the outer
  rect with `AdjustWindowRectExForDpi` (tao `util.rs:91-101`, `:143-150`) while
  `outer_size()` reads `GetWindowRect` (`window.rs:264-270`); whether the two agree on
  Windows 11 frames (DWM invisible borders) is INFERRED, so the measured delta may differ
  from the adjusted frame and the first `set_inner_size` may need one re-measure.
  *Probe:* AC6's witness; if it fails by the border width, record the delta rule from
  the observed values and the Electron 44.4.5 oracle cell for `getSize()`.
