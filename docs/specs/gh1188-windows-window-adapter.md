# Spec: Windows window adapter — Host Ready, BrowserWindow and vetoable Win32 close (W2)
Status: draft
Linear: GH-1188 (parent #1182, program #517) · Owner: GYLDLAB · Updated: 2026-10-30 (r13: exit receipt bound to the current browser instance; failed-build HWND custody; discriminating Quit-arming controls)

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
- Native probe evidence ("the U1/U6 bundle"): `D:\WORK\keld\.git\keld-closeout\claude-feaa86ad-d52c-4d93-8003-dd7422968b19\evidence\gh1188-u1u6\`
  (`results.json` sha256 `0032b352…fb45c18`, `SHA256SUMS` sha256 `8e966958…bb001f91`, every
  listed file verified `OK`; per-run JSON under `runs\`, probe source under `source\`).
  The probe ran on Windows 11 10.0.26300.9711, WebView2 Runtime 156.0.4314.50, tao
  0.35.3, webview2-com 0.38.2, windows 0.61.3: 55 runs, all exit 0, no lingering
  `msedgewebview2` or probe process after any run, using Keld's `webview2/mod.rs` as it
  stood at `2458836f` (sha256 `1247c842…b614a3`) with one declared deviation
  (`with_focused(false)` on the builder). Facts from it are marked FACT (probe).
- Native atom-E evidence ("the U10 bundle", 2026-10-29, Windows 11 26300.9711, WebView2
  156.0.4314.50, `webview2-com` 0.38.2): `D:\WORK\keld\.git\keld-closeout\claude-feaa86ad-d52c-4d93-8003-dd7422968b19\evidence\u10-atom-e\`
  (`SHA256SUMS` sha256 `81bbcc09…f5bb9bb5`; probe source `main.rs`, runs `run1-3.txt`,
  `run-long.txt`; `v2\` repeats the probe in Keld's order, `SHA256SUMS` sha256
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
  behalf is outside the Job; the successor's exclusive environment is the backstop.
- r6 reconciliation bundle: `D:\WORK\keld\.git\keld-closeout\claude-feaa86ad-d52c-4d93-8003-dd7422968b19\evidence\gh1188-w2-reconcile\`
  (checker `reconcile.py`, `mapping.tsv` with every citation's old pin, old and new range and
  match confidence, `deltas.md`; `SHA256SUMS` listed there).
- Doc drift found, not fixed here: `crates/keld-wv/src/webview2/mod.rs:39-41` says async
  completions are delivered by `webview2_com::wait_with_pump`, but production uses
  Keld's own loop (`wait_for_launch_with_message_pump_until` `:597-617` over
  `pump_message_or_handle` `:554-589`); `wait_with_pump` is imported only under
  `media-acceptance` tests (`:98-99`). T2 corrects the comment in its keld-wv diff.
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
   descriptor (no `index.html`) and the host admits it; renderer boots pass the existing
   Windows tests unchanged and write no channel-5 frame (a renderer boot whose stage
   carries the sidecar additionally launches with the preload, AC20). *NC:* a descriptor
   with `schema == 2` and a `renderer` field is refused `KELD-CORE-035`.
4. **Host-owned facade hint.** Given the test launcher exports
   `KELD_NATIVE_WINDOW_FACADE=1`, when a renderer boot starts, then the Bun child
   observes `"0"` (already true on `main`: `windows_primary_config` forces `"0"`,
   `app_session.rs:3368-3370`, from #1181's 53bc9e6e); a schema-2 boot observes `"1"`,
   which is the new behaviour (the fixture prints the value on a control line). *NC 1:*
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
   `process.env.KELD_NATIVE_WINDOW_FACADE` before any import that can reach the
   transport, and `@keld/electron` is loaded only by a later dynamic `import()`; the
   pre-import `app.quit()` then still rejects `KELD-COMPAT-012`, because the transport's
   mode expression (`transport.ts:32-33`) is the literal define, not the live
   environment (macOS shape `facade_window.rs:548-557`). Given a `bunfig.toml` or `.bunfig.toml` in the stage
   root, the launch is refused with the `KELD-CORE-037` "staged runtime config … can
   override canonical boot selection or preload" text (`keld-host/src/main.rs:480-487`),
   with or without a sidecar. Given the
   staged sidecar replaced by a symlink or a junction, the launch is refused (strictest
   rule, D13). *NC 1a:* on a direct-entry stage with no sidecar (so no preload), launching without
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
   sidecar launches. *NC 4:* an inherited `BUN_OPTIONS=--preload=<attacker>` runs before
   the transport.
6. **One host-minted window with a true outer frame.** After Ready,
   `new BrowserWindow({width:640,height:480,x:37,y:91})` returns `id === 1`; census 1;
   `getAllWindows().length === 1`; control line `PUBLIC_CREATED <bun pid> 1 640x480
   contents,window` (`facade_window.rs:659-669`); the witness `GetWindowRect` of that
   HWND equals `Created.state.bounds` converted at the window's scale factor, and its
   size equals 640×480 logical within one device pixel per edge. *NC 1:* returning the
   tao `WindowId` or `WebviewId` fails `id === 1`. *NC 2:* passing the requested outer
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
   the census is 0. *NC:* publishing `app.ready` before the window `Subscribe` replay
   lets the constructor write `Create` first.
9. **Cancel retains everything.** Given a `'close'` listener that calls
   `preventDefault()`, when the test posts `WM_CLOSE`, then `CLOSE_EVENT 1`; after
   `VERIFY_VETO` the reply is `VETO_VERIFIED ReplyStale false 1 0 true 0` (stale `Allow`
   refused); the top-level HWND handle is unchanged and `IsWindow` is true; the set of
   child HWNDs under it (`EnumChildWindows`) is unchanged (INFERRED to be the
   controller's hosting windows; U2's probe records their classes); the WebView2
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
    before it writes `WindowReleased`; census 0; `'closed'` precedes
    `'window-all-closed'`; with a `'window-all-closed'` listener that does not quit, the
    session stays live and a follow-up create/destroy succeeds (
    `facade_window.rs:574`), proving the loop wrote no `LastWindowClosed`; and, under
    `profile-test-hooks` with the hang guard shortened to a few seconds (D16), the
    session outlives that guard, proving no release barrier was armed. *NC 1:* writing
    `WindowReleased` from the `View` drop lets `CLOSED` arrive while `IsWindow` is true.
    *NC 2:* routing a facade session's `CloseRequested` through the legacy arm arms the
    barrier at `CLOSED` (`:3367`), and the shortened guard faults the host
    (`HangGuardExpired`, `:2699-2701`) while the fixture still holds the session.
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
    process's window, the mirror shows unfocused; after re-activation, focused again,
    whether focus lands on the top-level or on the controller. *NC:* forwarding Tao's
    raw `Focused(false)` emitted on the top-level `WM_KILLFOCUS` during `MoveFocus`
    writes a second fact (`false`) for an active window (A14, D11).
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
    (stage `Webview`) only after the half-built window's `Destroyed` was consumed: the
    U7 fixture's stderr line names the failed HWND, and the test reads
    `IsWindow(hwnd)` immediately after it reads the error and finds it FALSE; the D16
    loop-exit readback
    reports an empty `releasing` map; and the next `Create` succeeds and mints `(2, w')`
    with a fresh HWND. The visible-window census cannot decide this case, because D4
    builds `with_visible(false)` and the failed window is never shown. *NC 1:* the mutant
    that answers `WindowBuildFailed` in the failing callback is decided by order, not by
    the race with the 22–61 ms destroy (A20): the U7 fixture's UI-thread stderr lines
    (`FAILED_BUILD_DESTROYED <hwnd>` when the entry is consumed, `FAILED_BUILD_ANSWERED
    <build>` written inside the one function that sends every `WindowBuildFailed`, so
    no answer path can skip it) must appear in that order, and the mutant reverses them. *NC 2:* the mutant that drops the standalone `Window` without a
    `FailedBuild` entry (the pre-`View` path today has no owner for it) leaves no entry to
    answer from, so the caller never gets `KELD-CORE-041` before `PRODUCT_DEADLINE`.
    *NC 3 (missing `Destroyed`):* the U7 fixture's second mode swallows the matching
    `Destroyed`; the releasing bound (D6) expires and the host takes the `Fatal` path and
    exits with a failure status naming the HWND within that bound plus the release tail
    of the armed barrier (at most `WEBVIEW2_UI_END_BOUND`, `webview2/mod.rs:318`), never
    answering `WindowBuilt` and never hanging.
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
    even after the current instance's normal receipt was credited (D8); the mutant that
    admits `Idle` on the receipt alone (today's barrier) writes `idle`. Its mirror, a
    clean witness with no current-instance receipt, is AC21 NC 3/NC 4. No device control
    is claimed for a launched-but-unbound browser: the U7 fixture fails before controller
    creation (D12 case a), so it launches no browser; that case is decided by the census
    unit test (a) above.
19. **Stage disposition (`keld dev` lane only).** On Windows the host never deletes its
    stage; the `keld dev` private cleanup process does (`keld-host/src/main.rs:279-292`,
    `WindowsDevStageCleanup::prepare` `app_session.rs:1526-1562`) once the attempt
    Job reports zero processes. Given a facade boot launched through `keld dev`, when
    the host exits by `Quit` or by the AC2 terminal, then the stage count returns to 0
    (`no_flag_windows.rs:362-374` `wait_for_dev_stage_count`) only after every pid of
    the host's tree is gone. *NC:* deleting before the Job empties removes the stage
    while a WebView2 descendant still runs (`dev_lifecycle.rs:172` is the existing
    shape).
20. **Renderer boot with a staged sidecar (same rule).** Given a renderer boot whose
    project has `src/kipc-transport.ts`, when the Windows primary launches, then Bun
    runs with the same `--preload`, the legacy window/close path is unchanged (AC3), and
    the child observes `KELD_NATIVE_WINDOW_FACADE == "0"`. *NC:* scoping the preload to
    facade boots lets a renderer app evaluate the transport after its own imports.
21. **Release proof across browser generations (three gap regimes, D7/D8), installed
    harness.** On `profiles/` with `InstalledApp` (persistent; dev stages carry no
    lifecycle record), given one window destroyed and then `new BrowserWindow()` issued
    (i) in the same control command (same-pid reuse, 18–86 ms), (ii) after a 150 ms
    fixture delay (new pid while the old is alive, 125–237 ms), and (iii) after a 400 ms
    delay (clean, ≥274 ms), then in each case the new window builds and a following
    `Quit` writes `idle` only after the current browser instance's normal receipt (D8:
    the reused instance in (i), the new one in (ii) and (iii)) and a clean census, with an
    empty post-exit process tree (AC14's oracle) and a successor exclusive environment
    that succeeds. The receipt is bound to the current instance only; older generations
    are never receipted and are covered by the census as one kernel-kept set, so no
    retired-generation list is kept. *NC 1:* AC18 NC 4 (b) is the deciding test for
    regime (ii)'s census half, whose overlap is timing-dependent on a device. *NC 2:*
    dropping the environment before `Quit` suppresses the event (U6, 0/10), so the
    barrier fails `GraceExpired` and the record stays `stopping`; the environment must
    stay referenced until the event. *NC 3 (stale receipt, fail-first, device):* window A
    is destroyed with a `'window-all-closed'` listener that does not quit; the fixture
    waits until every `msedgewebview2.exe` in the host's tree is gone
    (`process_tree_report`), so A's normal receipt is credited to generation 1; it then
    creates window B (a new browser pid, U6 8/8) and issues `Quit` with B's receipt
    withheld by the D16 later-generation hook while B itself exits. Expected: the host
    exits `KELD-WV-009` (`GraceExpired`, host site) within hang guard + the hook's 1 s
    grace and the record reads `stopping`, not `idle`, although B exited and the census
    is clean, so receipt provenance is decided independently of Job emptiness. At-risk
    set: a run counts only when the D16 lines show `RECEIPT_CREDITED gen=1 pid=<A>`
    before `REBIND gen=2 pid=<B>` with B ≠ A; any other order (same-pid reuse, or A's
    receipt not yet credited at the rebind) makes the run void, reported, and counted
    neither as a pass nor as a mutant kill. Positive control: the same at-risk run
    without the hook writes `idle`. Mutant: today's session-wide
    sticky `observed` (`:2668`, accepted by `exit_ready` `:2728-2732`), kept across the
    rebind, writes `idle`. *NC 4 (receipt binding, keld-wv unit):* the binding table of
    D8 driven with stamped receipts and the existing live/exited handle helpers
    (`webview2/mod.rs:4123-4142`): (a) overlap: a generation-1 receipt queued during B's
    build pump and drained after B's completion is discarded as generation 1 and
    never credited to B; (b) the old pid's event fired after the rebind is foreign and
    dropped; (c) same-pid reuse with the bound handle unsignaled keeps generation 1 and
    credits the one receipt at the final close; (d) the same pid with the bound handle
    signaled marks the binding ambiguous, credits nothing, and the armed barrier faults
    at the grace; (e) an abnormal receipt of an older generation, drained after the
    rebind, still latches `AbnormalExit`; (f) a rebind after arming is
    `LifecycleUnproven`; (g) a rebind whose handle cannot be opened (pid 0, or a pid
    that no longer exists) latches `LifecycleUnproven`. Each case has its single-fault
    mutant, which must turn its case red: no generation check, or pid-only matching
    without stamps (a); no foreign-pid filter, so the late event is stamped with B's
    generation (b); a rebind on every completion, which fails the asserted generation 1
    (c); no ambiguity rule (d); discarding an abnormal receipt by generation (e); a
    rebind allowed after arming (f); a grace-only fallback without a handle (g). The `keld dev` lane's counterpart is
    AC19: the stage is deleted only after the same census came out clean.

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
`WM_DESTROY` emits `WindowEvent::Destroyed` (FACT, `event_loop.rs:1070-1079`). So no
second WndProc is needed; Keld chooses *when* to drop.

| # | Atom | Owner / boundary (in → out) | Failure mode | Observable | Edges | First falsifier | Status |
|---|---|---|---|---|---|---|---|
| A1 | Windows Init milestone | keld-wv Windows loop → keld-core coordinator; in: first `NewEvents(StartCause::Init)` of `run_return` (FACT, tao `event_loop/runner.rs:310-312`); out: `HostInitialized { observed_at }`, then Ready | Ready before the loop runs, or never for a zero-window boot | AC1, AC2 | ← A13 | Ready on `HELLO` | FACT: the loop already observes its first turn (`webview2/mod.rs:3262-3270`); emission is the delta; reuse `first_host_init_turn` generalised over the user-event type (`wkwebview/mod.rs:1039-1045`) |
| A2 | Schema-2 admission | keld-core `validate_windows_boot_files` / `run_app_direct`; in: descriptor; out: `ValidatedInitialWindow::None` | unverified descriptor admitted; renderer boot changed | AC3 | → A12, A17, A18 | `schema 2` plus `renderer` admitted | FACT: refused at `:1447` and inline at `:3431-3438`; today `main` matches on `parsed.initial` (`:1450-1451`); its `None` arm exists (`:1458`) but is unreachable because `:1447` refuses first. The `main` test `app_session/tests/windows_profile_and_boot.rs::windows_boot_files_refuse_a_schema_2_facade_boot` (1372fbd3) pins the `KELD-CORE-034` refusal that D2 removes; T1 replaces it with AC3's admission test as a recorded contract change, not a weakened test |
| A3 | Multi-controller, one lease, one environment | keld-wv engine; in: `CreateWindow` N; out: N `View`s on the one `ICoreWebView2Environment` | per-window environment or lease | AC15 | ← W6 for profile kind | second `webview2*` directory | FACT: one environment per engine (`:2851-2855`) |
| A4 | Close-veto custody | loop `CloseRequested` arm → registry; out: `WindowCloseRequested { view }` only | immediate drop (`close_requested_view`) | AC9 | → A8, A9, A20 | HWND changes on Cancel | FACT primitive; delta is policy |
| A5 | Last window / quit / relaunch | registry T11 → facade policy → `Quit` → loop terminal → views dropped, `Stopping` committed, barrier armed → the current browser instance's normal exit receipt under the hang guard and grace (A24) → host-death Job census, Bun family excluded → `Idle` (persistent) or deletion (ephemeral) or handover → scavenge | residual descendant or lease blocks relaunch; an unarmed barrier waits without a bound | AC14 | ← A4, A9, A10, A21, A24 | the arm readback (D16) lacks `armed_by=Quit`; a withheld receipt is not bounded | FACT: terminal path exists (`:3317-3329`, `:3380-3397`). FACT: arming does not decide whether the browser exits: `poll` drains a receipt before the armed check (`:2662-2670`, `:2685`) and `View::drop` closes the controller (`:2836-2843`); arming only bounds the wait (hang guard, grace), so AC14's controls observe the arm and the bound, not a lingering process |
| A6 | Stale/forged pair | gh531 admission (`KELD-CORE-038`) before any UI command | forged pair reaches a successor | AC11, AC12 | shared | B destroyed by `(B.id, A.gen)` | FACT: shared registry; proved end-to-end here |
| A7 | In-loop build with a nested pump | loop; in: `CreateWindow` + `&EventLoopWindowTarget`; out: `WindowBuilt`/`WindowBuildFailed` | controller creation pumps messages inside Tao's callback with Keld's own loop (`:597-617`, pump `:554-589`; not `webview2_com::wait_with_pump`); Tao buffers re-entrant events (`runner.rs:208-227`); loss/reorder of a buffered `CloseRequested` | AC6, AC15 | → A4, A20 | a `WM_CLOSE` during the pump is lost | FACT (probe, U1 resolved): the pump completes inside `run_return`'s callback for a first controller in 306–394 ms (median 342, n=10, delay 0) and 311–387 ms (n=10, delay 30), for a second controller in 48–85 ms (n=9), with zero windows in 352–477 ms (n=3); a `WM_CLOSE` to another window and a user event posted during the pump were delivered after the callback returned, exactly once, in post order (`close_requested_A_counts` and `second_counts` all 1; 10/10, 10/10, 6/6 with the stimulus inside the pump, 9/9 delivered, 3/3 quiet); `reentrant_total` 0, `panics_total` 0; the pumping thread spends 0–47 ms CPU per build (no busy spin). Rejected: asynchronous completion (`WindowsLoopEvent::ControllerReady { build }` posted from the handler), no longer needed for correctness |
| A8 | Released = `WM_DESTROY` | loop; in: `DestroyWindow { view }`; out: `WindowReleased` after `WindowEvent::Destroyed` for the dropped window's id | release reported at `View` drop | AC10, AC16 | ← A4; → A20 | `CLOSED` while `IsWindow` | FACT (tao, above) |
| A9 | Profile `Stopping` is irreversible | keld-wv profile lifecycle; in: `commit_profile_stop`; out: on-disk record | committing on `CloseRequested` strands a vetoed session | AC18 NC2 | → A5, A21 | `Stopping` after Cancel | FACT: Starting→Running→Stopping→Idle only (`lifecycle.rs:153-176`); committed today at `:2822-2825` |
| A10 | Release witness for a persistent profile | keld-runtime host-death Job (`windows_job.rs:3167`, installed before any WebView2 process, `keld-host/src/main.rs:96-104`, KILL_ON_JOB_CLOSE with no breakaway, `:2464-2500`, unnamed and non-inheritable, `:3068-3071`, so only descendants of members can join); in: settled snapshots of the Job's process list taken after the current instance's receipt (A24), with the host and the members of the session's Bun generation Jobs (the census "family") excluded; out: released when no other member is listed | proof from one watched browser pid misses an overlapping older browser (regime ii) or a launched-but-unbound one; without the Bun family exclusion the census never comes out clean while Bun lives (Bun is a direct member today, `app_session.rs:3359`, `:3706-3737`); a reused listed id can only force a retake, because reuse implies the member died; terminating a member mid-write could corrupt the profile; a clean census proves no listed member is alive, not that the current browser raised its normal exit (A24 owns that half) | AC14, AC18, AC21 | ← A3, A21; ↔ A24 (both required, neither implies the other); → D7 | `idle` with a live WebView2 process, or every `Quit` failing | FACT (research, §2): every WebView2 process inherited the Job (284/284; 430 across 11 kinds, 0 outside); the Job emptied 0.2–0.4 ms after the last `BrowserProcessExited`; the folder was quiet after it emptied (0 sharing violations in 245,430 opens); `GetProcessInfos` and the lockfile fail open. FACT (code): `exit_census` (`windows_job/host_death_census.rs`) observes **and terminates** members for the update exit; the profile witness must be observe-only |
| A11 | UI thread is a COM STA | `ComSta` `!Send` (`:489-510`); thread-affine controllers; `View::drop` closes on the engine thread (`:2836-2843`) | handle touched off-thread | every AC | gh531 A2 | off-loop `DestroyWindow` | FACT; no change |
| A12 | Host-owned facade hint | `windows_primary_config` (main `:3359-3372`); out: child env | app or launcher forces the barrier | AC4 | ← A2; → A18 | inherited `"1"` on a renderer boot | FACT: Windows already forces `"0"` for its renderer boot (`:3368-3370`); macOS derives the mode from the validated boot kind once and passes it both as a strict private Guardian argument and in the environment (`configure_guardian_boot_mode`, `app_session.rs:2342-2352`), and the Guardian adds Bun's literal define (`keld-host/src/main.rs:491-495`); the transport's mode expression is `transport.ts:32-33` |
| A13 | 15 s Init watch | keld-core `InitTerminalWatch` (`:4543`, macOS-only `:4606`, armed `:2603`); Windows terminal `fn(&HostAppError) -> !` | unbounded windowless boot | AC2 | → A1 | a stalled child lives past 15 s | FACT: macOS-cfg today; `run_guarded`/`run_unprivileged` take the terminal only on macOS (`:1888-1890`, `:1947-1949`; `keld-host/src/main.rs:147-156`) |
| A14 | Focus fact = activation | loop; in: Tao `Focused`, controller `GotFocus`/`LostFocus`, and a synchronous `GetActiveWindow()` query on the UI thread; out: `WindowFocus` | Tao's fact dies once a child HWND holds focus; a raw forward emits a transient blur | AC13 | ← A11 | raw `Focused(false)` after `MoveFocus` | FACT from source: Tao emits `Focused` only when `is_active && is_focused` changes (`window_state.rs:191-193`, `:196-207`); `WM_KILLFOCUS` on the top-level clears `is_focused` (`event_loop.rs:1800-1806`); activation alone (`WM_NCACTIVATE`, `:1779-1781`) then never re-emits, and Tao exposes no activation event. `GetActiveWindow` returns the active window of the calling thread's queue (<https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getactivewindow>), which is this UI thread. INFERRED: the controller hosts a child HWND that takes keyboard focus after `MoveFocus` (`:3637-3641`); U2 confirms. Rejected: deferring Tao's blur until a controller `GotFocus` cancels it (the COM event is asynchronous, INFERRED, so a transient blur cannot be excluded); a WndProc subclass (forbidden) |
| A15 | Zero-window census oracle | test `support/window.rs`; `EnumWindows` + `GetWindowThreadProcessId` + `IsWindowVisible` | `MainWindowHandle == 0` cannot prove zero | AC1, AC6, AC10 | test-only; → A23 | hidden window counted as zero | INFERRED; macOS states the same limit (`facade_ready.rs:19-24`) |
| A16 | Close stimulus | test; `PostMessageW(hwnd, WM_CLOSE, 0, 0)` | not the user path | AC9, AC10 | test-only; → A23 | no `CLOSE_EVENT` | INFERRED; `WM_CLOSE` is what the caption close produces (<https://learn.microsoft.com/windows/win32/winmsg/wm-close>); U5 |
| A17 | Legacy path unchanged | loop legacy arm; `create_app` single slot (`:3176-3184`); the launcher is the one place renderer boots change (a staged sidecar is preloaded for them too, A18/AC20) | renderer boots routed through the veto path; or a renderer boot exempted from the preload rule | AC3, AC20 | compat fallback; ← A18 | existing lifecycle test fails; a renderer app evaluates the transport late | FACT: gh531 criterion 15; the macOS guardian preloads for every primary it spawns (`main.rs:392`) |
| A18 | Transport sidecar: one admission owner, one preload rule | one helper used by the Linux strict launcher, the macOS guardian and the Windows `PrimaryRoleConfig` (`crates/keld-runtime/src/role.rs:390`); in: stage root; out: admitted sidecar path (then `--preload=`), `None` when absent, or `KELD-CORE-037` | three copies today with different strictness: Linux `admit_kipc_transport_sidecar` uses `symlink_metadata` and refuses symlinks (`app_session.rs:3899-3915`, called `:3929`); macOS uses `std::fs::metadata`, which follows symlinks (`keld-host/src/main.rs:497`); Windows file admission refuses reparse points (`open_relative_file_windows`, `:1731-1783`). Also: `bunfig` overrides define and preload, and inherited `BUN_OPTIONS` runs first; macOS on `main` refuses the first on every launch and removes the second (`keld-host/src/main.rs:478-520`) | AC5, AC20 | ← A12 (removes the env hazard at its root) | a junction sidecar is preloaded on one platform | FACT (three copies read); decision: fold into one owner with the strictest rule (refuse symlinks and reparse points everywhere; `bunfig` refusal on every launch; the boot-mode define; `BUN_OPTIONS` removal), applied to every boot kind whose stage carries the sidecar (the compiler stages it regardless of boot kind, `boot.rs:483-493`; the macOS guardian preloads for every guardian-spawned primary, `main.rs:392`). Rejected: a Windows exemption; keeping three copies |
| A19 | Outer-frame size | loop build; in: `CreateWindow` outer logical `width`/`height` (`engine.rs:104-115`); out: window whose `GetWindowRect` matches | Tao has only `with_inner_size` (tao `src/window.rs:337`) and no outer setter; passing outer as inner inflates the frame | AC6 | ← W4 units; → A23 | witness larger by the frame | INFERRED method (D14): build `with_visible(false)`, read the frame delta from `outer_size() - inner_size()` at the window's scale (`outer_size` is `GetWindowRect`, tao `platform_impl/windows/window.rs:264-270`), `set_inner_size(outer − delta)`, then `set_visible(true)`; no new FFI in Keld. Rejected: calling `AdjustWindowRectExForDpi` from Keld, only because it adds a new FFI entry point and unsafe gate; Tao already applies it inside `set_inner_size` (tao `platform_impl/windows/util.rs:91-101`, `:143-150`). Whether that adjustment and `GetWindowRect` agree on Windows 11 frames is U12 |
| A20 | HWND-value reuse in `releasing` under asynchronous destroy | loop release map keyed by Tao `WindowId`, which is the HWND value (`event_loop.rs:1064`) | a late `Destroyed` releases a successor that reused the value; an entry assumed to resolve within the dropping callback | AC10, AC16; unit test | ← A7, A8, A25 | reused key releases the wrong view; an entry never consumed | Corrected premise: Keld never calls `DestroyWindow`; dropping a Tao `Window` only posts `DESTROY_MSG_ID` (tao `window.rs:1104-1113`), which Tao's WndProc turns into `DestroyWindow` in a later message-loop turn (`event_loop.rs:2238-2240`). FACT (probe): `Destroyed` arrived 22–61 ms after the callback that dropped the views (median 31 ms; 70 window drops over 38 runs, derived from the `runs\u1-*.json` timelines as `WindowEvent(X, Destroyed)` minus the `UserEvent(Drain)` dispatch), so a `releasing` entry outlives its callback and any commands dispatched in between. Reuse argument under asynchronous destroy: the HWND value cannot be reissued while the window exists, and it exists until that posted message runs on this same UI thread; inside that `DestroyWindow` call `WM_DESTROY` is delivered synchronously (<https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-destroywindow>) and Tao dispatches it (directly or from the buffer, `runner.rs:225-226`) before the loop takes another message, so the old `Destroyed` always precedes any build that could receive the value. Rule: an entry is consumed once; intervening events for a releasing HWND are ignored (D5); a build whose id collides with a live managed or releasing key is `WindowBuildFailed { Window }` (fail closed; reachable only if a `Destroyed` was lost); every entry, `Destroy` or `FailedBuild`, is bounded by the releasing bound (D6), whose expiry is a loop invariant fault on the `Fatal` path, so a lost `Destroyed` ends the session observably instead of leaving a pending answer |
| A21 | Persistent `Running` moment | `from_selected_environment` (the one constructor `WebView2Engine::new` and the media-acceptance fixture share, `media_acceptance.rs:1157`); in: the environment bound to the verified user-data folder (`create_environment_for_profile_with_options_until`, `:1455-1470`: exclusive-UDF option, `environment_user_data_folder` equals the leased handle, ACL validated) and the `BrowserProcessExited` observer registered (`:3000-3018`); out: record `Running` written as that constructor's last step, so the loop-entry `PersistentStopTransition` (`:3250` `run_app_until_quit`, `:3039` `run_until_closed`; `:433-464`) snapshots `Running` for every boot kind | today `Running` is written only by the first build (`mark_profile_running`, `:3149-3162`, called `:3560`), so a facade boot that quits with zero windows holds `Starting` and `Quit` fails `KELD-WV-009`; with the write moved earlier, a launched-but-unbound browser must not be missed, which the census (A10) covers by Job membership rather than by pid | AC18 | ← A9, A10 | `KELD-WV-009` on zero-window facade `Quit` | FACT (device, U10 bundle, WebView2 156.0.4314.50 on Windows 11 26300.9711 only): creating the environment with the exclusive-UDF option, then reading its user-data folder back and registering `BrowserProcessExited` in Keld's order, launches no browser process (0 `msedgewebview2.exe` descendants: 5 s settle 8/8, 15 s settle 2/2; no exit event); the first controller then launches 6 (10/10, positive control). Keld's own environment doc already states the same (`:1432-1437`). FACT (code): macOS writes `Running` as soon as its store is bound at boot (`wkwebview/macos_profile.rs:457-461`); Windows recovery treats `Starting` and `Running` alike (`lifecycle.rs:469-479`). Decision D7 |
| A22 | Readiness barrier order | `@keld/api` readiness adapter; public `app.ready` only after Host Ready + accepted window `Subscribe` + replay (`app.ts:293-299`); W4 desktop facts join the same barrier later (#481) | construction before replay | AC8 | → W4 | `Create` before `Subscribe` | FACT (shared); Windows adds nothing |
| A23 | Test FFI feature | test crate; `EnumWindows`, `EnumChildWindows`, `GetWindowThreadProcessId`, `IsWindow`, `IsWindowVisible`, `GetWindowRect`, `PostMessageW` need `windows-sys` feature `Win32_UI_WindowsAndMessaging`, enabled neither in the workspace line (`Cargo.toml:142`) nor on keld-host's dev-dependency (`crates/keld-host/Cargo.toml:36`) | silent build failure of the proposed tests | §7 | test-only | the helper does not compile | FACT; the feature is added on the keld-host dev-dependency line only (`windows-sys = { workspace = true, features = [...] }`), never on the workspace line, so production crates gain nothing (dependency gate, §8) |
| A24 | Exit receipt bound to the current browser instance | keld-wv `ProfileReleaseWait`, Host site only (the one barrier owner); in: the binding `(generation, pid)` the loop writes at each controller completion, the handler's reported pid and exit kind (`:1559-1593`); out: a normal receipt credited to the current generation, or a fault | a session-wide sticky `observed` (`:2668`, accepted by `exit_ready` `:2728-2732` and `finish_profile_release` `:3124-3128`) lets an earlier browser's receipt satisfy a later browser's barrier; the receipt is drained even before arming (`:2662-2670`) and the loop polls every turn (`:3271`); the handler matches by pid only (`:1568`, `:1576`), and a queued receipt carries no pid, so one queued for A and drained after B's bind would be credited to B | AC21 NC 3, NC 4; AC14 NC 2 | ← A7 (completion order), A3; ↔ A10; → A5 | stale receipt plus clean census writes `idle` | FACT (code, above). FACT (probe, U6): a new controller after the last close gets a new pid (8/8); the old pid's event can arrive during the new creation pump (12/12). FACT (docs): a process id "is valid from the time the process is created until all handles to the process are closed and the process object is freed" (<https://learn.microsoft.com/windows/win32/api/processthreadsapi/ns-processthreadsapi-process_information>), and Keld holds the bound instance's handle until the rebind, so a new browser cannot reuse the bound pid; D8 treats a same-pid report with a signaled handle as inconsistent and fails closed. Decision D8 |
| A25 | Failed-build HWND custody | keld-wv in-loop build; in: a failure at any step after the Tao `Window` exists; out: `WindowBuildFailed { Webview }` on the matching `Destroyed` | before `View` construction there is no `View` to drop: main `create` builds the `Window` (`:3527-3541`), then `create_controller` (`:3544`) or `CoreWebView2` (`:3545-3547`) can fail, and only then is the `View` built (`:3551`); a controller created before a `CoreWebView2` failure is only closed implicitly at its final release (<https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#close>); D12 closes it explicitly | AC16 | ← A7, A8; → A20 | `IsWindow(hwnd)` true when `KELD-CORE-041` is read; no answer at all | FACT (code, above); decision D12 |

Promoted edges: A4→A8→A5; A9 and A10 and A21 are hidden couplings inside today's
`CloseRequested`/loop-entry code and are each their own atom; A7→A4/A20 (ordering of
buffered events during a build); A13→A1; A18→A12 (the launch define fixes the hint before
any app code); W4→A19/A22; A24↔A10 (the receipt and the census are separate halves of
one release proof, each with its own falsifier); A7→A24 (the rebind happens at
controller completion, so an old receipt fired inside the nested pump carries the old
generation); A25→A20 (a failed build parks its HWND in the same `releasing` map).

Ownership, process, memory, I/O, lifecycle, trust and failure facts:

- Handles: HWND, controller and webview stay in keld-wv's `View` (`:2337-2341`) on the
  UI thread; the registry maps `WindowId → WebviewId` (gh531 §4.a). No handle crosses kipc.
- Process: one browser process per user-data folder (A3), possibly overlapping a
  retiring one (U6). Release is proven by two separate halves, both required: the
  current browser instance's normal `BrowserProcessExited` receipt through
  `ProfileReleaseWait` (`:2529-2749`), bound to that instance (A24, D8), and the
  observe-only host-death Job census (A10, D7), which covers every older generation and
  any launched-but-unbound browser.
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
| Existing WebView2 engine: `create_controller`, `install_guarded_media_permissions`, `fit_controller`, `View` drop, `ProfileReleaseWait`, `PersistentStopTransition`, `BrowserExitWake` | reused; `create()` cannot run in-loop (needs `self.event_loop`, `:3522-3526`), so the build body is factored to take the `target` the `run_return` closure already receives (`:3263`); the in-loop nested pump is proven (A7). `ProfileReleaseWait` stays the one barrier owner and its Host site gains the instance binding (A24, D8); the Probe site is untouched. **Rejected:** a second engine; a per-window environment; asynchronous `ControllerReady` completion (unneeded after U1); a second, per-window or per-generation barrier object, or a retired-generation receipt list (older generations are the census's, A10). |
| Host-death Job census (`keld-runtime` `windows_job.rs`, `windows_job/host_death_census.rs`: `exit_census`, `list_job_process_ids`, `open_census_member`, the settled-snapshot rule and the family exclusion; attempt Job + `assign_child`) | reused as the one release witness: `exit_census` gains a member policy (`Terminate` | `Observe`) and a rights parameter, `open_census_member` takes the rights instead of the hard-coded `CENSUS_MEMBER_RIGHTS` (`:107-108`), and the session's Bun generation Jobs are the family (A10, D7, U14). **Rejected:** a second observe-only loop (parallel copy of the census); `GetProcessInfos` and the Chromium lockfile (both fail open, §2); per-pid custody in keld-wv (r4–r7); retiring Bun before the census (r8; needs a reorder, keeps handover on the old barrier, and fails every quit on a lingering Bun descendant) |
| macOS helpers `initial_renderer_pending` (`engine.rs:248-253`), `first_host_init_turn` (`wkwebview/mod.rs:1039-1045`), checked `next_view_id` and `checked_logical_bounds` (`managed_windows.rs:334-374`), borderless `SetFullScreen` (`:198-203`) | hoisted into one platform-neutral keld-wv module and reused; macOS keeps its behaviour, only import paths move. **Rejected:** Windows copies. |
| Sidecar admission: Linux `admit_kipc_transport_sidecar` (`app_session.rs:3899-3915`, the strictest existing copy), macOS preload + `bunfig` refusal (`keld-host/src/main.rs:478-512`), Windows `open_relative_file_windows` (`:1731-1783`) | the Linux helper is generalised into the one owner (symlink refusal everywhere, plus the Windows reparse-point check, the `bunfig` refusal on every launch, the boot-mode define and `BUN_OPTIONS` removal that macOS runs today) and consumed by the Linux launcher, the guardian and `windows_primary_config` (A18). **Rejected:** a Windows exemption; a fourth copy in keld-core. |
| Profile lease | reused per session. **Rejected:** per-window lease. |
| Windows host test harness (`ProductFixture`, TCP `KELD_T1B_CONTROL`, `dev_stage_command`, `wait_for_host_window`, `run_product_cycle`, `process_tree_report`, `InstalledApp`) | reused; census/witness helpers added to `support/window.rs` (A15, A19, A23). **Rejected:** a shared three-OS harness (kel259 §4). |
| Facade boot hint | the macOS rule (A12) plus the preload (A18). **Rejected:** faking Ready from the first navigation (gh531 criterion 13; `KELD-CORE-044`). |
| Mirror reads | reused. **Rejected:** per-getter RPC (gh531 criterion 17). |
| Second registry for Windows | rejected (issue non-goal; gh531 A1 sole minter). |

Named unmet requirement: no live Windows component can hold a close pending, build a
window after loop start, report a release only after `WM_DESTROY`, or bind the profile
stop record after Init. Compatibility fallback: renderer-declared boots keep today's path
(A17). No performance claim (§9).

### 4.a Windows deltas, by gh531 section

| # | Delta | Plugs into | Evidence |
|---|---|---|---|
| D1 | Windows leaves the parked list; Linux stays. T1 builds facade sessions with `window_native_support: Some(false)`; renderer boots keep `None`, so the non-macOS `NavigationReady` gate (`app_session.rs:4628`) still signals their Ready (Subscribe accepted, replay empty, calls answer `042`, facade Ready published); T2 flips to `Some(true)`. No interim where the CLI/Host admit a facade app that dies `045` at connect | gh531 §1, §4.b, Platform notes | `app_session.rs:7613-7615`, `:7707-7708`; `window_router.rs:387-395`, `:475-478` |
| D2 | `validate_windows_boot_files` admits `InitialWindow::None`; `run_app_direct` drops its inline refusal; `ensure_initial_window_supported` becomes Linux-only; `create_app` is skipped for facade boots | §4.c | `:1340-1351`, `:1447`, `:3431-3438`; main `:3667` |
| D3 | `AppReadyTrigger` and `AppWindowEvent::HostInitialized` cfg widened to Windows and re-exported from `webview2`; `run_app_until_quit` gains `ready_trigger` and a loop-entry callback (the macOS `startup` pair, `wkwebview/mod.rs:342-352`); `HostInitialized` emitted once at `first_loop_turn` (`:3262-3270`); `coordinate_window_events` takes `ready_trigger` and `init_watch` on Windows; `run_guarded`/`run_unprivileged` take the pre-Init terminal on Windows and `keld-host/src/main.rs` supplies it | §4.c; gh531 "No unbounded rendererless Init" (`:268`) | `engine.rs:149-153`, `:290-299`; `app_session.rs:4600-4643`, `:2603`, `:1888-1890`, `:1947-1949`; `main.rs:147-171` |
| D4 | `CreateWindow { build, … }` builds in-loop: Tao window via `target` with `with_visible(false)`, D14 frame correction, controller via `create_controller` (A7), guard handlers before any navigation (`:3570-3574`), `SetBounds` + `SetIsVisible` (`:3592-3606`; learnings 2026-08-14), empty initial document, `mark_profile_running` (a no-op after A21), `set_visible(true)`; snapshot from the built window (`outer_position`, `outer_size`, `scale_factor`, `is_maximized`, `fullscreen()`; tao `window.rs:213`, `:264`, `:508`, `:613`, `:669`) through `checked_logical_bounds`; the initial focused field reuses D11's UI-thread activation sampler (`GetActiveWindow() == hwnd`), not Tao keyboard focus; ids from `next_view_id`; answer `WindowBuilt` | §4.a snapshot source, O1 | main `:3527-3645` |
| D5 | Close path by boot kind: in a facade session every `CloseRequested` whose HWND is a managed view sends `WindowCloseRequested { view }` and nothing else; an HWND in `releasing` or unknown is ignored with one diagnostic (gh531 C3/D3). Renderer sessions keep the legacy arm untouched | §4.c `:764-765`, §4.e P3/Q3/C3/D3, criterion 15 | main `:3355-3370`, `:2816-2834`; `wkwebview/mod.rs:555-573` |
| D6 | `DestroyWindow { view }`: remove the `View` (controller `Close()`, then Tao's posted destroy), record its Tao `WindowId` in `releasing`, write `WindowReleased { view }` only on the matching `WindowEvent::Destroyed`, which arrives in a later loop turn (22–61 ms after the drop, A20); consume the entry once; while the entry is pending, events for that HWND (`Resized`, `Focused`, `CloseRequested`, `Moved`) are ignored (D5) and the registry stays in `Closing` (gh531 C-rows). **Releasing bound (one rule for every entry, `Destroy` here and `FailedBuild` in D12):** each entry records the instant it was parked, and the loop schedules `WaitUntil` its expiry at `RELEASING_DESTROYED_BOUND` = `PROFILE_LAUNCH_DEADLINE` + 5 s, derived from that one constant (`:209`), never copied: a `Destroyed` is buffered while another window's in-loop build pumps inside the callback and is delivered only after the callback returns (A7), and that pump may legally last up to `PROFILE_LAUNCH_DEADLINE` (15 s, `create_controller_observed`, `:1710`), so the bound must exceed it; the 5 s margin is far above the measured 22–61 ms `Destroyed` latency (A20); an entry still unconsumed at its bound is a loop invariant fault taken on the existing `Fatal` path (main `:3323-3329`: stop committed, views dropped, barrier armed) with one diagnostic naming the HWND, so the host exits with a failure status instead of holding a pending answer forever | §4.e C9 "Released" | tao facts §4.0; U1/U6 bundle |
| D7 | Facade boots: the loop never writes `LastWindowClosed`; `commit_profile_stop` runs only on `Quit`/`Fatal`, after every managed `View` is dropped (controllers closed first, `:2628-2629`). `from_selected_environment` writes `Running` as its last step, once the environment is bound to the verified user-data folder and the exit observer is registered (A21), so every persistent boot stops through the existing Running→Stopping→Idle chain; `advance` is unchanged. **Release proof (A10), on every Windows engine release** (persistent `Idle`, ephemeral deletion, and the in-host handover): `finish_profile_release` first requires the Host barrier's normal receipt **for the exact current browser instance** (D8: bound by generation and pid; a receipt of an earlier, delayed or foreign instance never satisfies it), under today's hang guard and post-exit grace (`HangGuardExpired`, `GraceExpired` and an abnormal exit of the current instance stay faults), then calls a release witness injected by keld-core with the remaining post-exit budget. Release needs **both**, and neither implies the other: the receipt no longer admits `Idle` on its own, and a clean census does not prove that the current browser raised its normal exit. Only when no browser instance was ever bound (a zero-window facade `Quit`, AC18 b) is there no receipt to require, and the census alone decides. `ProfileReleaseWait` stays the one barrier owner; only its Host site gains the instance binding (D8). Its Probe site (`ReleaseSite::Probe`: the hidden exclusive-UDF probe, startup recovery and the post-release scavenge) is built with its one instance, never rebinds, keeps today's semantics, and runs no census. The witness is the **one** census owner, keld-runtime's `exit_census`, parameterised by a member policy (`Terminate` for the update exit as today, `Observe` here) and by the rights it opens members with (`Observe` needs no `PROCESS_TERMINATE`). It takes settled snapshots of the host-death Job, excludes the host and the members of the session's Bun generation Jobs (the existing "family" rule, `host_death_census.rs:153-241`, extended from one attempt Job to the session's list), treats any other listed member as unclean, retakes until clean, and never terminates. A clean census writes `Idle` (persistent) or permits deletion (ephemeral) or handover; a deadline, snapshot, open or query error yields `LifecycleUnproven` (`KELD-WV-009`) and the record stays `stopping`. **Bun family:** keld-core gives every Bun primary generation its own attempt Job nested in the host-death Job (the existing single-process attempt-Job type and `assign_child`, `windows_job.rs:1289`, which refuses a second assignment, `:1293-1298`), so a generation started by recovery is family too; whether a generation is created suspended and assigned before its first instruction (the `WindowsSuspendedChild` path the update candidate uses) or assigned right after spawn is U14, and a child Bun starts before assignment is a non-family member, which fails closed. **What the census proves, and what it does not:** it proves that no WebView2 process and no other host child uses the folder. It does not and cannot prove that a Bun-family process holds no handle in the folder: a Bun process with the user's authority could equally reach the folder through an out-of-Job broker. Bun's access to the folder is therefore bounded only by OS containment, never by the census (arch 03: the non-breakaway Job is descendant cleanup, not authority, `03-security.md:237-238`). **Today, and for every session W2 ships, no such containment exists:** the Windows primary runs plain `bun` on the user's token (`windows_primary_config`, `app_session.rs:3359-3372`; §1 non-goals), because the Strict LPAC primary (#1187, KEL-210) is not built yet. This is not the opt-in Legacy profile (arch 03 forbids auto-downgrading into it, `03-security.md:246-248`); it is the unbuilt Strict target. Until #1187 lands, the release proof covers WebView2 processes and other host children only, for every Windows session, and makes no claim about Bun-family file handles. Once the Strict primary exists, its containment of the folder is #1187's claim to prove (U17), not this spec's; retiring Bun before the census would neither close that gap nor survive the KEL-53 handover tail, which needs the live Bun after the UI end (`app_session.rs:3298-3324`). **Pacing (observe policy):** every unclean round waits with the already-sanctioned `WaitForSingleObject` on the first remaining non-host member's handle, up to the remaining deadline, before the next snapshot; an unsettled list is retaken only after the existing `CENSUS_DENIED_MEMBER_WAIT` (25 ms, `host_death_census.rs:114`). A slow member therefore costs waiting, not a spin, and no new FFI operation is added. This removes the per-pid custody of r4–r7 (retired-generation list, attribution of events to older generations, the unresolved-controller flag and the zero-window probe witness); the only attribution kept is the binding of the one receipt to the current instance (D8). Job membership is kept by the kernel, covers every generation and a launched-but-unbound browser, and does not depend on any runtime's start-up order | §4.e T11; gh531 `:1465-1467` | main `:3250`, `:3317-3329`; arm doc `:2628-2629` |
| D8 | **Exit receipt bound to the current browser instance (A24; U6 resolved).** Today the Host barrier's proof is one session-wide sticky flag: `poll` drains a normal receipt even before arming and sets `observed` for good (`:2662-2670`), the loop polls every turn (`:3271`), and `exit_ready` (`:2728-2732`) and `finish_profile_release` (`:3124-3128`) accept it; the handler matches by pid (`:1568`, `:1576`) and queues a receipt without one. Re-registering the wake alone cannot reset that flag, so a receipt of browser A (last window closed, session kept by a `'window-all-closed'` listener) would satisfy browser B's barrier. Rules, Host site only, in the one owner `ProfileReleaseWait`: (1) **Binding.** The `expected_browser_pid` atomic (`:2391-2400`, `:2819-2829`, `:3006-3018`) widens to one binding `(generation: u32, pid: u32)` in an `AtomicU64`, written only by the loop and read by the handler; both run on the UI thread (A11). A facade session starts unbound (generation 0); a generation advances only by `checked_add`, and overflow latches `LifecycleUnproven`. The ambiguity flag (rule 2) is a loop-side `Cell<bool>`, not part of the atomic. The legacy path binds once at loop entry (`remember_browser_process_id`, `:2388-2402`) and never rebinds; its second store in `close_requested_view` (`:2828-2830`) is removed, because the loop-entry bind already holds the one legacy view's pid, so renderer boots keep today's behaviour. The Probe site stores `(1, pid)` (today `:1943-1945`) through the same type and the same handler (`observe_profile_browser_exit`, `:1548`). (2) **Rebind at controller completion.** After every in-loop controller completion the loop reads `BrowserProcessId`. The same pid while the bound instance's SYNCHRONIZE handle is unsignaled is the same instance (regime i, same-pid reuse at ≤86 ms, one event at the final close): nothing changes. A different pid is a new instance: the loop writes `(generation + 1, pid)`, opens the new handle, moves the one wake registration onto it (unregister, then register: still one registration, `keld-wv/AGENTS.md:5`), and clears the credited receipt and the post-exit deadline; the first bind of a facade session registers the wake. The same pid with the bound handle already signaled cannot name a new process, because a process id stays reserved until all handles to it are closed (FACT, A24) and Keld still holds the old handle; such a report is inconsistent, so the binding is marked ambiguous, no receipt for that pid is credited, and an armed barrier fails closed at the grace. If the new instance's handle cannot be opened (`open_browser_process` returns `None`, `:2373-2385`, which `browser_exited` would read as exited, `:2637-2639`) or its wake cannot be registered, the rebind latches `LifecycleUnproven`: a rebind never falls back to grace-only waiting. (3) **Stamp and credit.** The handler stamps a matching receipt with the binding's generation at the moment the event fires; a receipt for any other pid is foreign and dropped with one diagnostic. `poll` credits a drained receipt only if its generation is the current one; an older receipt is discarded. Draining before arming stays: a current-instance receipt drained before `Quit` is genuine (its browser exited with the session still open). An abnormal receipt latches `AbnormalExit` whatever its stamp and no rebind clears it: the handler queues a receipt only for the pid bound when the event fires, so every stamped abnormal receipt is a crash of a browser this session bound, and the fault ends the session as today (`:3272-3274`). Disclosed limit: an abnormal exit of a browser that was never bound (launched by a failed build) or that fires after its rebind (then a foreign pid) is not reported as abnormal; the census proves only that it is gone. (4) **Arming.** Arming happens only at the terminal command, after which no build completes; a rebind after arming is an invariant fault (`LifecycleUnproven`). (5) **Bounds unchanged.** The hang guard from arming while the current handle is unsignaled, the post-exit grace once it is signaled, and the faults `HangGuardExpired`, `GraceExpired` and abnormal exit apply to the current instance. Older generations are never receipted and are covered by the census (D7), so no retired-generation list exists (the 125–237 ms regime raises the old pid's event during the new creation pump, 12/12, which carries the old generation's stamp and is therefore discarded whenever it is drained). The environment stays referenced until the event, because dropping it suppresses the event (0/10, U6). The landed KEL-53 h9 cell (`profiles/handover.rs:621-633`, its withheld-event hook `KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY`, `mod.rs:2541-2555`) still withholds every receipt and keeps its meaning; the Probe site never rebinds, so the exclusive-UDF probe's `Unproven` on a withheld event (`:1523`) keeps its meaning too. Fail-first proof: AC21 NC 3 (device) and NC 4 (unit) | KEL-53 release barrier | main `:2389-2402`, `:2618-2627`, `:2656-2683`, `:2728-2732`, `:3124-3128`, `:3271`; U1/U6 bundle `U6_summary` |
| D9 | The v0 first-navigation deadline is armed only by the legacy `create_app` window (`initial_renderer_pending`); managed views live in their own map and never arm it | §4.c compat arm | main `:3240`, `:3293-3315`; `engine.rs:248-253` |
| D10 | `Resized` re-fits managed controllers too (`fit_controller`); `Moved`/`Resized` → read state → forward `WindowBounds`/`WindowMaximized`/`WindowFullScreen` observations (no keld-wv filtering, B4); `SetMaximized` → `set_maximized`; `SetFullScreen` → borderless (the macOS choice, `managed_windows.rs:198-203`; Tao Windows also implements exclusive mode, `window.rs:699-705`, which Keld does not use) | §4.b ordering rules 2–3, P12–P15 | tao `event_loop.rs:1212`, `:1227`; `window.rs:601`, `:675` |
| D11 | Focus fact derived from activation: on every Tao `Focused(_)` and every controller `GotFocus`/`LostFocus`, the loop reads `GetActiveWindow() == hwnd` on the UI thread and forwards `WindowFocus { focused }` from that value only; Tao's `Focused(false)` during `MoveFocus` therefore produces no fact, since the window stays active. The controller events (WebView2 COM handlers registered at build) and the `GetActiveWindow` query are new production `unsafe` (§8). Rejected alternatives in A14 | §4.b P13 | A14 facts |
| D12 | `WindowBuildFailed { stage }` with explicit custody of whatever exists when the step fails (A25). `Window` stage: the Tao build fails, no HWND exists, answer at once. `Webview` stage, (a) **after HWND creation, before `View` construction** (main `create` builds the `Window` at `:3527-3541`; `create_controller` at `:3544` or `CoreWebView2` at `:3545-3547` fails before the `View` exists at `:3551`, so there is no `View` to drop): the build keeps the standalone Tao `Window` and its `WindowId`; if a controller was created before `CoreWebView2` failed, it calls that controller's `Close()` first, as `View::drop` does (`:2836-2843`); it records `releasing[window_id] = Releasing::FailedBuild { build }` and then drops the `Window`, which posts the destroy (A20). (b) **After `View` construction** (guard install, `SetBounds`/visibility, initial document, D14): it drops the `View` (controller `Close()`, then the posted destroy) and records the same entry. In both cases `WindowBuildFailed { Webview }` is written only when the matching `WindowEvent::Destroyed` consumes the entry, never in the failing callback; the window id was never published, so no `WindowReleased` is written. The entry obeys the one releasing bound (D6); a lost `Destroyed` ends in the `Fatal` path, never in a silent answer. A failed build never rebinds the receipt (no `BrowserProcessId` read, D8); a browser its controller launched is the census's (D7) | §4.e O2 | A8, A20, A25 |
| D13 | `windows_primary_config` sets `KELD_NATIVE_WINDOW_FACADE` to `"1"` for a schema-2 boot (it already forces `"0"` for renderer boots, A12) and consumes the one launch owner for every boot kind: the literal define `--define=process.env.KELD_NATIVE_WINDOW_FACADE:"0|1"`, `BUN_OPTIONS` removed, `KELD-CORE-037` when a `bunfig.toml`/`.bunfig.toml` exists (sidecar or direct entry), `--preload=<root>/src/kipc-transport.ts` when the staged sidecar is a regular non-reparse, non-symlink file, `KELD-CORE-037` when it is a symlink, junction/reparse point or not a regular file, and legacy direct entry when absent (A18). The same owner replaces the macOS and Linux copies in the same change | §4.c Ready declaration; gh531 `:821-831` | main `:3359-3372`; `keld-host/src/main.rs:478-520`, `app_session.rs:3899-3934` |
| D14 | Outer-frame conversion (A19): requested outer logical → physical at the created window's `scale_factor`; frame delta measured on the invisible window; `set_inner_size(outer − delta)`; snapshot read after; `x`/`y` through `with_position` (outer) | §4.a `Created.state` | tao `src/window.rs:337`, `:387`, `:475`, `:766`, `:845` |
| D15 | CLI: `stage_dev_boot` and `keld doctor` refuse `initialWindow: "none"` only on Linux | consumed by §4.c | `boot.rs:380-390`, `doctor.rs:258-272` |
| D16 | Debug observability, `profile-test-hooks` class only (as `KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY`, `:2548-2555`, scrubbed from the Bun env by keld-core, main `:3378`): `KELD_TEST_WINDOWS_HANG_GUARD_SECS` shortens `BROWSER_SHUTDOWN_HANG_GUARD` so an erroneously armed barrier faults observably (AC10); `KELD_TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT=1` withholds only receipts stamped with generation ≥ 2 (D8) under the h9 hook's 1 s grace (`TEST_BARRIER_GRACE_EXPIRY_GRACE`), so the first browser's receipt is still credited and only a later instance's is lost (AC21 NC 3), while the h9 hook keeps withholding every receipt (AC14 NC 2); setting both hooks is refused at engine construction with a typed error naming both, so no run mixes the two faults; the loop writes `RECEIPT_CREDITED gen=<g> pid=<p>` and `REBIND gen=<g> pid=<p>` on stderr from the UI thread (AC21 NC 3's at-risk set); stderr readbacks at loop exit of the environment-creation count (AC15), of the barrier's arm record (`armed_by` = the terminal command that first armed it, or `none`; AC14 NC 1) and of the `releasing` map size (AC16). Every new hook name is scrubbed from the Bun env with the existing one. Never compiled into release | test evidence for T11/arming/receipt binding | main `:305`, `:2699-2701` |

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
    releasing: BTreeMap<tao::window::WindowId, (Releasing, Instant)>, // awaiting Destroyed; parked-at for the D6 bound
}
enum Releasing { Destroy(WebviewId), FailedBuild { build: u64 } } // FailedBuild also covers a pre-`View` failure (D12 a)

// loop-private (D8): the Host barrier's binding, written by the loop, read by the
// BrowserProcessExited handler; replaces the bare `expected_browser_pid: AtomicU32`
struct BrowserBinding(AtomicU64);                       // (generation: u32, pid: u32); generation 0 = unbound
// generation advances by checked_add (overflow = LifecycleUnproven); the ambiguity flag is
// a loop-side Cell<bool>; the Probe site stores (1, pid) through the same type and handler

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

- Windows (this spec): veto primitive per §4.0; release observed at `WM_DESTROY`;
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
  to the current browser instance and rebinds at each controller completion (D8); the
  stop record binds `Running` at loop entry (A21); Bun preloads the staged sidecar. OS
  grants: none new. Crash domain: host process. Handle lifetime: `View` from
  `WindowBuilt` to `WindowEvent::Destroyed`; a failed build's standalone `Window` from
  its creation to its `Destroyed` (D12).
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
  `crates/keld-wv/AGENTS.md` (D11's `GetActiveWindow` query and controller focus
  handlers named in the Windows FFI sentence), `crates/keld-runtime/src/windows_job.rs` and
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
  each with a nested `AGENTS.md` update plus independent unsafe evidence on the exact
  diff (root `AGENTS.md`).

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 **Windows Ready without a window.** D1 with `Some(false)`, D2, D3, D13, D15;
  the one sidecar admission owner replacing the Linux, macOS and Windows copies (with
  a symlink/junction negative per platform); PROPOSED `facade_ready.rs` (AC1–AC5, AC8,
  AC20) plus the stalled-Init child (AC2); the `keld dev` lane (AC19); replacing the
  `main` test `windows_boot_files_refuse_a_schema_2_facade_boot` with AC3 (A2). Because T1 edits
  `keld-host/src/main.rs` and the guardian launcher, the macOS `facade_ready`/
  `facade_window` suites run on a real Mac at T1's head before it lands. gh531 §1/notes
  and 04 amendment text.
- [ ] T2 **BrowserWindow build, veto, release.** D1 flip to `Some(true)`, D4–D12, D14,
  hoisted helpers; D6's releasing bound and D12's pre-`View` custody; D8's receipt
  binding lands here, because T2 is the first change that can build a second browser
  instance in one session, with its unit control (AC21 NC 4); PROPOSED
  `facade_window.rs` (AC6, AC7, AC9–AC13, AC15, AC16 if U7 is approved),
  census/witness helpers, the mutant receipts (§7).
- [ ] T3 **Quit, relaunch, recovery, persistent profile.** The one census with its
  `Observe` policy and rights parameter, the per-generation Bun Jobs, the keld-wv release-witness
  seam and the Running write site (D7, D8, A21, U14), with the `crates/keld-runtime/AGENTS.md`
  unsafe-scope edit and arch 05 text; AC14, AC17 in `facade_lifecycle.rs`; AC18 and AC21 in
  `profiles/facade_lifecycle.rs` on `InstalledApp`; AC19's census-before-deletion;
  product-status row; W0 receipt handoff (device, commit, WebView2 runtime version).

## 7. Test plan

All real-Windows tests live in the existing `no_flag_windows` target
(`crates/keld-host/tests/no_flag_windows.rs:18-58`; kel259 families) and use its TCP
control protocol (`support/product_cycle.rs:28-50`), `dev_stage_command`
(`no_flag_windows.rs:376-383`) and `ProductFixture` (a schema-2 variant writes
`keld.config.ts` with `initialWindow: "none"` and no renderer, mirroring 
`facade_ready.rs:44-52`). The persistent-profile row uses `support/installed.rs`
`InstalledApp` (KEL-254 T3 Part B fixture).

| AC | PROPOSED test (under `tests/no_flag/windows/`) | Kind |
|---|---|---|
| 1, 3, 4, 5, 8, 20 | `facade_ready.rs::windows_schema_two_host_emits_ready_without_an_initial_native_window`; `::windows_primary_preloads_the_staged_transport_and_refuses_bunfig_and_junctions` (facade and renderer boots; AC5's first-module env deletion and dynamic facade import) | real-Windows integration |
| 19 | `dev_lifecycle.rs::shipping_windows_keld_dev_cleans_a_facade_stage_after_the_attempt_job_empties` (reuses `wait_for_dev_stage_count`, `no_flag_windows.rs:362-374`; Quit and stalled-Init terminal variants) | real-Windows, `keld dev` lane |
| macOS guard (N7) | existing `facade_ready.rs` / `facade_window.rs` suites on a real Mac at T1's head, because T1 edits the shared launcher and `keld-host/src/main.rs` | existing, prerequisite |
| 2 | `facade_ready.rs::windows_schema_two_stalled_init_exits_through_the_binary_terminal_once` (`cfg(all(feature = "profile-test-hooks", debug_assertions))`): asserts terminal once, Bun gone, failed exit status, stage directory present | real-Windows, debug fixture |
| 3 (renderer unchanged) | existing `lifecycle.rs`, `recovery.rs`, `dev_lifecycle.rs` unchanged, plus one assertion that no channel-5 frame is written | existing + one assertion |
| 6, 7, 9, 10, 11, 12 | `facade_window.rs::windows_electron_browser_window_uses_native_host_and_vetoes_wm_close` (vocabulary `PUBLIC_CREATED`, `VETO`, `CLOSE_EVENT`, `VERIFY_VETO`, `VETO_VERIFIED`, `STALE`, `ALLOW`, `CLOSED` as `facade_window.rs:70-95`, `:632-736`; stimulus A16; census A15; witness A19; child-HWND set and lease share-denied probes) | real-Windows integration |
| 5 NC1a/NC1b/NC4, 9 NC, 10 NC1/NC2, 7 NC, 16 NC1/NC2, 14 NC1, 21 NC3 | mutation runs: documented local patches (no define; no `--preload`; an inherited `BUN_OPTIONS` preload; immediate `View` drop on `CloseRequested`; `WindowReleased` at drop; legacy arm in a facade session under the D16 shortened hang guard; managed views kept in the legacy `views` map; `BuildFailed` before `Destroyed`; a pre-`View` failure that drops the `Window` without a `FailedBuild` entry; only the `Quit` arm call removed; today's sticky `observed` kept across the D8 rebind); each is one fault; record the failing assertion and line in the PR evidence; patches are never committed | mutation control |
| 13 | `facade_window.rs::windows_focus_fact_is_activation_without_transient_blur` (counts focus facts across the build's `MoveFocus`; activates a helper process window, then the host window) | real-Windows integration |
| 15 | `facade_window.rs::windows_two_browser_windows_share_one_profile_lease_and_browser_process` (counts `webview2*` directories under the control dir) | real-Windows integration |
| 16 | `facade_window.rs::windows_controller_failure_after_minting_releases_the_hwnd` (needs U7: failure right after the HWND is minted; `IsWindow(hwnd)` FALSE when read immediately after the error; empty `releasing` readback; fresh next build) and `::windows_lost_destroyed_after_a_failed_build_ends_in_a_bounded_fatal` (U7 second mode, NC 3) | real-Windows, debug fixture |
| 14 | `facade_lifecycle.rs::windows_facade_last_window_quits_releases_profile_and_relaunches` (two cycles; default policy and draw.io's listener; pre-Quit `process_tree_report` snapshot; NC 1 asserts the D16 `armed_by=Quit` readback) and `::windows_facade_quit_with_a_withheld_receipt_fails_bounded` (NC 2: h9 hook, `KELD-WV-009` within the bound, positive control without the hook) | real-Windows integration, debug fixture |
| 17 | `facade_lifecycle.rs::windows_facade_recovery_keeps_the_same_hwnd` | real-Windows integration |
| 21 | `profiles/facade_lifecycle.rs::windows_release_proof_survives_browser_generation_changes` (`InstalledApp`; regimes i and iii on the device; regime ii's census half decided by the keld-runtime observe-only census unit test, AC18 NC 4) and `::windows_a_previous_browser_receipt_never_releases_a_later_browser` (NC 3: later-generation hook, record `stopping`; positive control `idle`; only runs whose D16 lines show the at-risk order count) | real-Windows, installed, debug fixture |
| 18 | `profiles/facade_lifecycle.rs::windows_installed_facade_profile_record_reaches_idle` (`InstalledApp`; record reader U11) | real-Windows, installed |
| D3 cfg, D14 arithmetic, A20 release map, D6 releasing bound, D8 receipt binding (AC21 NC 4), D11 focus derivation | keld-wv unit tests without `EventLoop::new` (pure functions; release map with a reused key and an expired entry; the D8 binding table with stamped receipts and the live/exited handle helpers; focus state table) | unit |

Helpers (PROPOSED, `support/window.rs`): `top_level_windows(pid)`, `child_windows(hwnd)`,
`window_rect(hwnd)`, `post_close(hwnd)` via `windows_sys` with the A23 feature (the test
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
  exit wake (moved, never doubled), and D12's `Close()` of a controller created before
  a `CoreWebView2` failure (the `View::drop` operation);
  (2) D11 registers `ICoreWebView2Controller` `GotFocus`/`LostFocus` handlers (new
  blocks under the sanctioned WebView2 class) and adds one new Win32 FFI entry point,
  the `GetActiveWindow()` query on the UI thread (no hook, no WndProc); both must be
  named in the `crates/keld-wv/AGENTS.md` Windows FFI sentence. D14 uses Tao getters
  only. (3) The census observe policy is a new production path in keld-runtime's
  sanctioned Job FFI and needs the `crates/keld-runtime/AGENTS.md` scope edit plus
  independent evidence (U14). Test-only `unsafe` follows the test crate's allowance.
- **public API:** yes. `AppReadyTrigger`/`HostInitialized` on Windows; the
  `WebView2Engine::run_app_until_quit` signature; `run_guarded`/`run_unprivileged`
  taking the terminal on Windows; the shared launcher helper; the keld-wv release-witness
  seam, the `exit_census` policy/rights parameters and the host lending the host-death Job
  capability (U14); Windows stops answering
  `KELD-CORE-034`/`KELD-CLI-047` for schema-2 and `SubscribeRefused`/`KELD-CORE-045` for
  window traffic (documented behaviour change).
- **permission model:** none. No capability, manifest key or guard evaluation changes;
  the window channel stays host-internal (gh531 §4.g); the facade hint selects a
  barrier, not authority (gh531 `:828-829`); guard handlers unchanged
  (`:3570-3574`); the preload adds no authority (it is Keld's own transport).
- **dependency:** yes, feature-only, test-only. `Win32_UI_WindowsAndMessaging` is added
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
unchanged. D14 adds one invisible build step per window. D6's releasing bound (`PROFILE_LAUNCH_DEADLINE` + 5 s) is a
fault deadline only; a healthy `Destroyed` (22–61 ms, A20) never waits for it.

## 10. Open questions

Each is an UNKNOWN with the probe that decides it, or RESOLVED with the evidence that
decided it. **Blocking for approval: none.** Approval confirms the A21 reading of
KEL-135 recorded under U10 and the census direction in D7 (owner-approved
2026-10-29, gyldlab/keld#1188 comment 6498281882); U14 is decided at T3 under its review
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
  rejected alternative (A7).
- **U2 (A14) Child-HWND focus transfer.** Answered from Tao source (A14): Tao cannot
  re-emit focus after a child HWND holds it. *Confirmation probe:* log Tao `Focused` and
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
- **U7 (AC16) Controller-failure fixture.** No debug control injects a controller
  creation failure. *Decision needed:* owner approval for a `profile-test-hooks`
  child-only env (same class as `KELD_T1B_STALL_BEFORE_INIT`) that fails the next
  controller creation once, immediately after the HWND is minted (D12 case a), and
  writes the failed HWND on one stderr line, then `FAILED_BUILD_DESTROYED <hwnd>` and
  `FAILED_BUILD_ANSWERED <build>` (written inside the one function that sends every
  `WindowBuildFailed`) from the UI thread in the order the loop does them (AC16 NC 1); a second mode of the same hook also
  swallows that HWND's `Destroyed` in Keld's dispatch, so the releasing bound (D6) is
  exercised (AC16 NC 3); otherwise AC16 is unit-tested only.
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
  first controller launched it (A21 evidence, 10/10 runs). That fact is no longer
  load-bearing: a pid-0 `Idle` is admitted only by the host-death Job census (D7), which
  observes every process the host or the runtime started, on whatever runtime is
  installed (review finding, rpipaliya 2026-10-29). A future runtime that launched a
  browser at environment creation would leave a Job member and fail closed.
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
- **U14 (A10, D7) Census capability and Bun Job creation — decided at T3.** keld-wv has no
  keld-runtime dependency; it defines a release-witness seam that `finish_profile_release`
  calls, keld-core implements it with the parameterised `exit_census` (D7), and
  `keld-host` lends the host-death Job capability (`main.rs:96-104`) to keld-core's run
  entry (the signature T1 already changes). Open within T3: whether the Windows primary is
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
  Bun Job; T3 verifies it. The successor's exclusive environment is a backstop only for a
  WebView2 browser still on the folder; it does not detect an arbitrary process's open file
  handle in the folder.
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
