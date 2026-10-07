# Electron 44.4.5 oracle for gyldlab/keld#419 (PANEL-P2) - scratch only

Nothing here was posted. All paths are relative to
`/private/tmp/claude-501/-Users-centillionaire-WORK-keld/9d46ef65-830a-4067-90bc-f68a26ad7f85/scratchpad/p2/electron/`
(`$B`). Label legend: FACT = observed in the transcripts cited, N/N runs. UNKNOWN = not
observed / not reproducible here; the reason is given.

## 1. Provenance

| Item | Value |
|---|---|
| Artifact | `electron-v44.4.5-darwin-arm64.zip`, 130,418,529 bytes, from `https://github.com/electron/electron/releases/download/v44.4.5/` |
| sha256 (computed) | `a212eee63ba2f45fd83bd28f77a3e3313a336ad17a4c25adf617942eef5e0e2c` |
| sha256 (SHASUMS256.txt line) | `a212eee63ba2f45fd83bd28f77a3e3313a336ad17a4c25adf617942eef5e0e2c *electron-v44.4.5-darwin-arm64.zip` - match, `shasum -a 256 -c` printed `OK` |
| Runtime | `dist/version` = `44.4.5`; Electron 44.4.5, Chromium 152.0.7977.130, V8 15.2.124.28-electron.0, Node 24.21.0 |
| Host | macOS 26.5.1 (build 25F80), arm64, Darwin 25.5.0 |
| No npm | no `npm install`, no postinstall, no other package. Only `curl`, `ditto`, `shasum`, `osascript`, `jq`/`python3 -I` (analysis only) |
| Pinned docs fetched | `docs/api/{app,auto-updater,browser-window,dialog}.md` at tag `v44.4.5` into `docs/`; sha256 app `49238ddf...bf`, auto-updater `d27fac2e...f60`, browser-window `49061d5e...29b9b`, dialog `bc0b63b7...ffeba` (full hashes: `shasum -a 256 docs/*.md`) |

Commands (exact):

```sh
cd $B/dl
curl -fsSL -O https://github.com/electron/electron/releases/download/v44.4.5/electron-v44.4.5-darwin-arm64.zip \
            -O https://github.com/electron/electron/releases/download/v44.4.5/SHASUMS256.txt
grep ' \*electron-v44.4.5-darwin-arm64.zip' SHASUMS256.txt | shasum -a 256 -c -     # -> OK
ditto -x -k electron-v44.4.5-darwin-arm64.zip $B/dist
# one run of one fixture (runfx.sh loops this 5x and kills at 30 s):
env -u ELECTRON_RUN_AS_NODE -u CLAUDECODE ORACLE_OUT=$B/runs/<label>/runN.jsonl ORACLE_USERDATA=$B/userdata \
    <VAR=val> $B/dist/Electron.app/Contents/MacOS/Electron $B/fixtures/<fx>/main.js
# whole suites:
$B/run-all.sh        # E5, Q1 (no dialogs) and, in its first form, everything
$B/run-dialogs.sh    # E1 and E3 with the final AX dismissal
python3 -I $B/analysis/analyze.py   # signatures, variants, canonical + per-run copies in transcripts/
```

Configurations (`<VAR=val>`): `e1-discard E1_ANSWER=discard`, `e1-cancel E1_ANSWER=cancel`;
`e3-plain-sheet E3_MODE=plain E3_PARENT=1`, `e3-plain-appmodal E3_MODE=plain E3_PARENT=0`,
`e3-inclose-sheet E3_MODE=inclose E3_PARENT=1`, `e3-inclose-appmodal E3_MODE=inclose E3_PARENT=0`;
`e5-window-veto E5_MODE=window-veto`, `e5-beforequit-veto E5_MODE=beforequit-veto`,
`e5-control E5_MODE=control`; `q1-veto Q1_MODE=veto`, `q1-noveto Q1_MODE=noveto`.

## 2. Method notes (read before trusting any number)

- Transcript format: one JSON line per event: `proc` (main|renderer), `pid`, `ev`, `perf_ms`
  (`performance.now()`, per-process origin), `hr_ns` (`process.hrtime.bigint()`, ns), `epoch_ms`.
  `fixtures/lib.js` appends synchronously (`appendFileSync`), so **file order is the cross-process
  order**. FACT: `hr_ns` never goes backwards inside one process (0 inversions in 3,703 lines) but
  adjacent lines from different processes invert by up to 2.35 ms (48 cases) because the stamp is
  taken before the write. Do not order cross-process events by `hr_ns` alone.
- Renderer events are written straight to the same file from the renderer (nodeIntegration on,
  contextIsolation off, sandbox off) so a stalled main process is visible as renderer lines with no
  main lines.
- Windows are `show:true` for E1/E3 (a sheet needs a shown window); `show:false` for E5/Q1.
  Fully headless is impossible for the dialog fixtures; a small window flashes on screen.
- **Modal dismissal method (asked: record which)**: FACT: `dialog.showMessageBoxSync` has no
  `signal`/timeout option in v44.4.5. `dialog.md` (`showMessageBoxSync`, lines 282-330) lists no such
  option; `signal` appears only for async `showMessageBox` and even there "On macOS, `signal` does
  not work with message boxes that do not have a parent window". So the fixture spawns a detached
  `fixtures/dismiss.sh` (500 ms for E1; 1500 ms for E3 so several probe ticks land inside the modal)
  that presses the dialog button through **osascript / System Events**. Final runs use an
  accessibility `click button "Discard"|"Cancel"` on the sheet or window of the Electron pid
  (retry every 0.5 s until `modal:after`/`dialog:after` appears in the transcript).
  The requested keystroke form (`set frontmost` + `key code 36`/`53`) was used first and failed to
  dismiss in 2 of ~35 dialog runs (e1-discard run2 and e3-plain-sheet run5 of the second batch, kept in
  `runs-keystroke-batch2/`; sidecar rc=0 yet the modal stayed up, plausibly a focus race; cause UNKNOWN).
  A keystroke that loses that race lands in whichever app is frontmost, so the targeted AX click was
  substituted. `dismiss.sh` sidecars: `transcripts/*.runN.dismiss.jsonl` (all `rc=0`, `result` sheet or window).
- **Disclosed incident**: the first full batch ran while the shared volume was at ENOSPC; the
  orchestrator reported an Electron "JavaScript error in the main process" dialog from
  `fixtures/e3/main.js:37` (`appendFileSync`). That batch (and two hung E3 runs in it) is discarded.
  `lib.js` now caps 2,000 lines per process, swallows write errors, and probes stop after 60 ticks.
  The data below is from the re-runs: E5/Q1 from `run-all.sh` after the fix, E1/E3 from
  `run-dialogs.sh` with the AX dismissal. Final data: 25 E5/Q1 runs and 30 E1/E3 runs, all `exit=0`, 0 hung
  (the only hangs were the discarded batch and the 2 keystroke failures above). `e5-beforequit-veto` was re-run
  after fixing its fixture (windows no longer veto in that mode; the first batch exited 97 via its watchdog).
- Side effect outside scratch: Electron wrote `~/Library/Preferences/com.github.Electron.plist`
  (237 bytes, standard AppKit defaults). Not deleted (outside the write scope). `userData` was
  redirected to `$B/userdata`.

## 3. E1 - close veto (`fixtures/e1`)

Flow: renderer sets `window.dirty = true` and sends it; main stores `isModified`, sets
`setDocumentEdited(true)`, 300 ms later calls `win.close()`. `'close'` handler: if modified,
`event.preventDefault()`, `dialog.showMessageBoxSync(win, {buttons:['Cancel','Discard']})`; Discard ->
`win.destroy()`; Cancel -> 300 ms later clear modified and `win.close()` again.
Transcripts: `transcripts/e1-discard.*`, `transcripts/e1-cancel.*`. Tombstone = `[win.isDestroyed(), wc.isDestroyed()]`.

Canonical order, discard path (`e1-discard`, 5/5 identical, full cross-process signature):

```
trigger:win.close
win:close                     tomb [false,false]  isModified=true
win:close:preventDefault
dismiss:scheduled  (osascript AX click, 500 ms)
dialog:before
dialog:after   result=1       tomb [false,false]
destroy:call
win:closed                    tomb [true,false]   <- emitted SYNCHRONOUSLY inside destroy()
app:window-all-closed         (fixture quits from it)
app:before-quit
app:will-quit
app:quit                      <- still inside destroy()
destroy:returned              tomb [true,false]
trigger:win.close:returned    tomb [true,false]
wc:destroyed                  tomb [true,true]    <- AFTER app 'quit'
tick:setImmediate-after-destroy [true,true]
```

No `renderer:beforeunload`/`unload` and no second `close` on the destroy() path.

Canonical order, cancel path (`e1-cancel`, 5/5 identical):

```
win:close [false,false] -> preventDefault -> dialog:before -> dialog:after result=0 [false,false]
close:cancelled-by-user -> trigger:win.close:returned [false,false]   (window stays alive)
trigger:win.close#2 -> win:close [false,false]
renderer:beforeunload -> renderer:unload
wc:destroyed   [win false, wc TRUE]      <- before 'closed'
win:closed     [true, true]
app:window-all-closed -> app:before-quit -> app:will-quit -> app:quit
```

Answers:
- FACT (5/5): `'close'` precedes the dialog; the dialog is entered inside the `'close'` handler after
  `preventDefault`; both tombstones are false in `'close'` and right after the dialog returns.
- FACT (5/5, destroy path): neither `win.isDestroyed()` nor `wc.isDestroyed()` is true before `'closed'`
  fires; at `'closed'` `win.isDestroyed()` is true and `wc.isDestroyed()` is **false**; the webContents
  tombstone flips only at the later `wc 'destroyed'` event, after `destroy()` returned and after the app `'quit'`.
- FACT (5/5, normal close path): `wc.isDestroyed()` is already true before `'closed'` (the `'destroyed'` event comes
  first); `win.isDestroyed()` is false until `'closed'`, true inside it.
- FACT: `'close'` is emitted before the renderer's `beforeunload`/`unload` (cancel path ordering).
- Dialog dwell ~1.0 s (0.5 s delay + osascript start-up).
- Variance: none (5/5 identical in both modes).

## 4. E3 - re-entrancy during `showMessageBoxSync` (`fixtures/e3`)

Probes started before the modal: main `setInterval(100 ms)`, `setTimeout(250 ms)`, `setImmediate`, plus a
renderer that every 100 ms sends `ipcRenderer.send('ping')` and `ipcRenderer.invoke('rpc')`. In
`inclose` mode the dialog is inside the `'close'` handler and a main `setTimeout(300)` plus a renderer-sent IPC
both call `win.close()` again while the modal is open. Four configs x 5 runs: `e3-plain-sheet`,
`e3-plain-appmodal`, `e3-inclose-sheet`, `e3-inclose-appmodal`. Modal lasted 1.7-2.1 s (`hr_ns`).

- **FACT (20/20 runs, all 4 configs): zero main-process events of any kind between `modal:before` and `modal:after`.**
  No `setInterval` tick, no `setTimeout`, no `setImmediate`, no `ipcMain` handler (`on` or `handle`), no re-entrant `win.close()`/`'close'`.
- **FACT (20/20): the renderer process kept running** during the modal (8-21 `renderer:tick` lines per modal; run5 of
  `inclose-appmodal` had only 8, renderer throttling suspected, cause UNKNOWN) so a dialog stalls main, not renderers.
- **FACT (20/20): nothing was lost, everything was deferred.** Every ping/invoke the renderer sent during the modal
  was delivered to main after `modal:after`, in send order, within 6-12 ms of the modal ending (plain), 13-28 ms (`inclose-appmodal`, run5 118 ms),
  112-122 ms (`inclose-sheet`; the first `win.close()` returned ~100 ms after the handler). No catch-up replay of missed interval ticks: after the
  modal, tick n=5 fires once (n=4 was the last before; ~19 ticks were skipped, not replayed) and the cadence resumes at ~100 ms.
- Canonical post-modal order (plain, 10/10): queued `ipc-ping`/`ipc-rpc-handled` pairs, `setImmediate`, `interval-tick`, `timeout-250ms`.
  Inclose: queued pings/rpc, `ipc-request-close-received`, `setImmediate`, `interval-tick`, `timeout-250ms`, `timeout-300ms`.
  Variance: in 1/20 runs (`e3-inclose-sheet` run2) the first `interval-tick` came before the first queued ping; the
  relative order of timers vs queued IPC after the modal is therefore not guaranteed, only "after the modal".
- `inclose` canonical (10/10 identical in main events): `win.close-call[start]` -> `win:close` -> `dialog` -> `modal:after result=0`
  (Cancel) -> `close-handler:after-modal` -> `win.close-returned[start]` -> `ipc-request-close-received` -> `win.close-call[ipc]` ->
  nested-free `win:close` (second, allowed) -> `setImmediate` -> timers -> `win.close-call[timer-during-modal]` -> `win:close` ->
  `win:closed [true,true]` -> `app:window-all-closed` -> `before-quit` -> `will-quit` -> `quit`.
  FACT: the `'close'` handler is **not re-entered** while the modal is open; each extra `win.close()` is serialized after the handler returns.
- Variance: sheet vs app-modal made no difference to the "no JS during modal" result. Full cross-process signature was identical 5/5
  in 3 configs; `inclose-appmodal` had 4/1 only because `renderer:arm-close-request-received` interleaved differently with main lines
  (cross-process timing, main-only order identical 5/5).
- UNKNOWN: Windows/Linux behaviour; async `showMessageBox`; the native reason (nested modal run loop vs main thread block) - not instrumented.

## 5. E5 - two dirty windows and quit (`fixtures/e5`)

Windows `w1`,`w2` (hidden), both dirty, each vetoes its **first** `'close'` and allows afterwards. `app.quit()` at +300 ms (#1) and +900 ms (#2).
`window-all-closed` handler logs only (quits only in `control`).

`e5-window-veto` (5/5 identical, full signature including renderer lines):

```
trigger:app.quit:#1
app:before-quit
win:close w2 -> preventDefault          <- close order is REVERSE creation (w2, w1), 5/5
win:close w1 -> preventDefault          <- w1 still gets 'close' after w2 vetoed
app.quit:returned:#1                    (no will-quit, no window-all-closed, no quit)
trigger:app.quit:#2
app:before-quit                         <- emitted AGAIN
win:close w2 (allowed)
win:close w1 (allowed)
app.quit:returned:#2                    <- returns BEFORE any 'closed' (closing is async)
renderer:beforeunload x2, renderer:unload x2
win:closed w2 [true,true]
win:closed w1 [true,true]
app:will-quit
app:quit
```

`e5-beforequit-veto` (windows never veto; `before-quit` vetoes its first emission; 5/5): quit #1 -> `before-quit`,
`before-quit:preventDefault`, **no window `close` events at all**; quit #2 -> `before-quit` again, `close` w2, `close` w1, `closed` w2, `closed` w1, `will-quit`, `quit`.

`e5-control` (no `app.quit`, `win.close()` on both twice; 5/5): `close w1`(veto), `close w2`(veto), second round allowed, `closed w1`, `closed w2`,
**`window-all-closed`**, then the fixture quit: `before-quit`, `will-quit`, `quit`.

- FACT (10/10): `window-all-closed` is absent from both `app.quit()` flows (veto and before-quit-veto) and present in the control (5/5), matching `app.md` "in this case the `window-all-closed` event would not be emitted".
- FACT (5/5 + 5/5): a second `app.quit()` after a vetoed quit re-emits `before-quit` and re-runs the window close sequence; no state of the first attempt blocks it.
- FACT (5/5): a window veto aborts the whole quit (no `will-quit`) but does not stop the other window's `'close'`.
- Variance: none in E5 (all three modes 5/5 identical).
- UNKNOWN: veto by `beforeunload` returning false (not tested; docs `app.quit()` mention only).

## 6. quitAndInstall inversion (`fixtures/q1`) - PARTIAL, final order UNKNOWN

Reproduced without an update server, `autoUpdater` has no feed URL and no downloaded update. Two windows, `Q1_MODE=veto` (each vetoes once)
and `noveto`; `autoUpdater.quitAndInstall()` at +300 ms and +1200 ms. Transcripts: `transcripts/q1-veto.*`, `transcripts/q1-noveto.*`.

Pinned doc sentences (v44.4.5):
- `auto-updater.md` Event `before-quit-for-update`: "When this API is called, the `before-quit` event is not emitted before all windows are closed. As a result you should listen to this event if you wish to perform actions before the windows are closed while a process is quitting, as well as listening to `before-quit`."
- `auto-updater.md` `quitAndInstall()`: "Under the hood calling `autoUpdater.quitAndInstall()` will close all application windows first, and automatically call `app.quit()` after all windows have been closed."
- `app.md` `before-quit` note: "If application quit was initiated by `autoUpdater.quitAndInstall()`, then `before-quit` is emitted _after_ emitting `close` event on all windows and closing them."

Documented order (UNKNOWN as observed end-to-end): `before-quit-for-update` -> `close` on every window -> windows closed -> `app.quit()` -> `before-quit` -> `will-quit` -> `quit`.
That is the inverse of `app.quit()` (E5: `before-quit` first, then `close`).

Observed (FACT, 10/10): `autoUpdater:before-quit-for-update` is the first event, then `win:close` w2, `win:close` w1 (veto or allow as configured).
`app:before-quit` was **never** emitted and `will-quit` never, because the native step ends with `autoUpdater:error` "No update available, can't quit and install"
instead of `app.quit()`; the process stayed alive until the fixture's `app.exit`. In `noveto`, after both windows closed `app:window-all-closed` **is** emitted
(unlike `app.quit()` flows), then a second `before-quit-for-update` and the error. In `veto`, quitAndInstall #1 leaves both windows open, #2 closes them.
FACT (5/5 `veto`): calling it again while the first is pending logs on stderr `ERROR:base/observer_list.h:371 NOTREACHED hit. Observers can only be added once!`
(non-fatal in this build). Variance: the order of the two `win:closed` events differs (w1/w2 vs w2/w1, 3 vs 2 runs in both `q1` configs); the order of `win:close`
(w2 then w1) and everything else is identical 5/5.

Verdict: FACT that no `before-quit` precedes the window `close` events and that `before-quit-for-update` does; UNKNOWN whether `before-quit` follows the closes with a real
Squirrel.Mac update (needs a signed app and a feed; not available here). The inversion is therefore documented, not observed end to end.

## 7. Determinism summary (5 runs each)

| Label | Full cross-process variants | Main-only variants | Note |
|---|---|---|---|
| e1-discard, e1-cancel | 1, 1 | 1, 1 | deterministic |
| e3-plain-sheet, e3-plain-appmodal, e3-inclose-sheet | 1, 1, 1 | 1, 1, 1 | deterministic (structural events; high-volume tick/IPC families collapsed) |
| e3-inclose-appmodal | 2 (4/1) | 1 | only renderer-vs-main log interleaving |
| e5-window-veto, e5-beforequit-veto, e5-control | 1, 1, 1 | 1, 1, 1 | deterministic |
| q1-veto, q1-noveto | 2 (3/2), 2 (3/2) | 2, 2 | order of the two `win:closed` events only |

Structural signatures: `analysis/analysis.json`. Counts of modal-window events and tick numbers are derived from `transcripts/*.runN.jsonl` by the snippets in `analysis/analyze.py`.

## 8. Files

- `transcripts/<label>.run1..5.jsonl` raw per-run; `transcripts/<label>.canonical.jsonl` run 1 with tick/ping/rpc families collapsed to one counted line per gap; `transcripts/<label>.runN.dismiss.jsonl` AX attempt sidecar.
- `fixtures/{lib.js,dismiss.sh,e1,e3,e5,q1}`, `runfx.sh`, `run-all.sh`, `run-dialogs.sh`, `analysis/`, `docs/` (pinned doc copies), `dl/` (zip + SHASUMS256.txt), `dist/` (extracted), `runs/` and `runs-keystroke-batch2/` (raw).
- Footprint ~440 MB (zip 124 MB, dist 307 MB); transcripts < 1 MB.
