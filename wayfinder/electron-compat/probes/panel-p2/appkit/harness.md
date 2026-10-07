# PANEL-P2 AppKit close/quit veto harness (gyldlab/keld#419)

This is scratch evidence only. Nothing here touches the Keld repository, and nothing has been posted.
This is the AppKit half of #419. The Electron 44.4.5 half is in `../electron/` and is owned by a sibling task. A cross-diff of the two halves is **not** part of this file.

Labels:
- **FACT** means the transcripts in this directory or a quoted Apple doc show it directly.
- **INFERENCE** means it is reasoned from those facts and is not proven.

## Environment (FACT, from `harness-start` events)

- macOS 26.5.1 (25F80), arm64.
- Apple Swift 6.3.2 (`swiftc`, `-swift-version 5`). Rust objc2 was not needed.
- The harness ran in a logged-in GUI session. Windows really appeared on screen. All input was programmatic: no human clicks.

## Files

| Path | What it is |
|---|---|
| `harness.swift` | The single-file Cocoa harness: host, simulated role, scenarios and JSONL logger. |
| `run_all.sh` | Runs 13 scenarios × `N=5` runs. Writes `transcripts/<scenario>.run<k>.jsonl` and the exit codes to `transcripts/_runs.tsv`. |
| `check.py` | Applies criteria C1–C7 to every transcript. Also computes the ordering signatures and the re-entrancy table. Writes `transcripts/_results.json`. |
| `transcripts/_check_output.txt` | The checker's printed output for the recorded runs. |
| `appledocs/*.json`, `extract_docs.py`, `appledocs.txt` | The Apple doc JSON that was fetched, plus the extracted text quoted below. |
| `show.py` | Pretty-prints one transcript. |

## Commands

```sh
cd p2/appkit
swiftc -swift-version 5 -O harness.swift -o harness
./run_all.sh                       # 13 scenarios x 5 runs, ~2 s each
python3 -I check.py transcripts    # criteria matrix + orderings + probe table
python3 -I show.py transcripts/s2a_quit_two_dirty_allow.run1.jsonl
```

Recorded results:
- `transcripts/_runs.tsv` shows exit 0 for 60/65 runs.
- The other 5 runs are `s3d`. They exit 2 on the watchdog by design (see below).

## Design

### Host

**Close (`windowShouldClose:`).** `windowShouldClose:` calls one owner, `requestClose(w, entry)`, and the host's programmatic close API calls the same function. `requestClose` does three things:
1. It logs `close`.
2. In `async-gated` mode it gates the request. A tombstoned window returns NO. A window with a pending transaction is coalesced and returns NO.
3. Otherwise it opens a new transaction (`txn`), asks the role asynchronously, and returns **NO**.

There is no timeout path, except in negative control `n2`.

**Role reply.** The reply arrives on the main thread through the chosen delivery mechanism. A reply for a window that is already tombstoned, or for a stale `txn`, is dropped and logged as `stale-reply-dropped`. The possible replies are:
- `allow`: run `commitClose`.
- `veto`: the transaction ends and the window stays.
- `veto_prompt`: show an app-modal `NSAlert` with `runModal()`. The dialog result goes back to the role, which then replies `allow` (for discard) or `veto` (for cancel or abort, which fail closed).

**`commitClose`.** It flips the tombstone first, then calls `NSWindow.close()`. `windowWillClose:` logs `destroy`. A common-modes `CFRunLoopPerformBlock` then logs `closed` and the `tombstone-before-closed` check.

**Quit (`applicationShouldTerminate:`).**
1. `applicationShouldTerminate:` logs `before-quit` and returns **`NSTerminateLater`**.
2. A serializer then calls `performClose:` on one live window at a time. It waits for each window's outcome before moving to the next.
3. If any window vetoes, it calls `replyToApplicationShouldTerminate:NO`. The default policy is `stop-at-first-veto`. `s2d` uses the `continue` policy instead.
4. If no window vetoes, it calls `replyToApplicationShouldTerminate:YES`. The app then reaches `applicationWillTerminate:` (logged as `will-quit`), and an `atexit` hook logs `quit`.

### Simulated role

The role is a serial background `DispatchQueue("role")` with `asyncAfter(80 ms)`. Its decision rules are:
- Dirty window → `veto_prompt`. Clean window → `allow`.
- `discard` → `allow`. `cancel` / `abort` → `veto`.

The default delivery to main is `CFRunLoopPerformBlock(main, kCFRunLoopCommonModes)` + `CFRunLoopWakeUp`. `s3a` uses GCD main and `s3d` uses default mode.

### Dialog dismissal

The dialog is dismissed by a `Timer` added to `RunLoop.main` in **`.modalPanel`** mode. Which method is used is recorded per dialog in `dialog-dismiss.method`:
- `NSButton.performClick(nil)` on the scripted alert button: 45 dialogs. This returns 1000 (Cancel) or 1001 (Don't Save).
- `NSApp.abortModal()`: 20 dialogs. This returns `NSModalResponseAbort` = −1001.

### Event vocabulary (maps to the Electron oracle)

| Electron | This harness (`ev`) |
|---|---|
| `before-quit` | `before-quit` (`applicationShouldTerminate:` entry), then `should-terminate{returned: terminateLater}` |
| BrowserWindow `close` | `close` (with `entry` = `windowShouldClose:` or `host-close-api`), then `close-return{returned}` |
| `preventDefault` / `isModified` | `close-return{returned:false}` + `role-ask` / `role-send` / `role-reply{reply}` |
| `showMessageBoxSync` | `dialog{phase: open}`, `dialog-dismiss{method}`, `dialog{phase: result, response_raw}` |
| `destroy` | `tombstone` → `destroy-call` (`NSWindow.close`) → `destroy` (`windowWillClose:`) |
| `closed` | `closed`, then `tombstone-before-closed{ok}` |
| `will-quit` | `reply-to-should-terminate{value:true}` → `will-quit` (`applicationWillTerminate:`) |
| `quit` | `quit` (`atexit` after `applicationWillTerminate:`) |
| (quit cancelled) | `reply-to-should-terminate{value:false}`, `quit-cancelled`, `terminate-returned` |

Every event carries `seq`, `t_ms` (monotonic), `wall` (ISO-8601 ms), `thread` and, on main, `rl_mode` (`CFRunLoopCopyCurrentMode`) and `ns_modal_window`.

## Criteria (`check.py`)

| ID | Criterion |
|---|---|
| C1 | **Fail-closed.** A dirty window is destroyed only after an explicit role `allow`, and never after a synchronous YES. |
| C2 | **No auto-close on timeout.** No `destroy` follows a `close-timeout` without an intervening `allow`. |
| C3 | **Single prompt.** Two dialogs are never open at once, and there is at most one dialog per close transaction. |
| C4 | **Tombstone before closed.** Every `closed` is preceded by that window's `tombstone`. |
| C5 | **Quit ordering.** `before-quit` comes before the serialized closes, which come before the reply. `will-quit` → `quit` happen only if the reply is YES. A NO reply keeps the app alive and returns from `terminate:`. |
| C6 | **Quit serialization.** The next window's close starts only after the previous window's outcome. |
| C7 | **No hang.** The run ends with `quit` or `harness-end`, not with the watchdog. |

## Results: 13 scenarios × 5 runs (FACT)

Each scenario produced exactly **one** distinct ordering across its 5 runs. Each ordering below is the normalized `check.py` signature.

| Scenario | Ordering (all 5 runs identical) | Criteria |
|---|---|---|
| s1 close veto then allow | close(W1) → veto_prompt → dialog-open → result:discard (1001) → allow → **tombstone → destroy → closed** → tbc:true; then W2 (clean): close → allow → tombstone → destroy → closed | C1–C4, C7 pass 5/5 |
| s2a quit, 2 dirty, both discard | **before-quit** → close(W1) → veto_prompt → dialog → discard → allow → tombstone → destroy → closed → close(W2) → … → closed → reply(YES) → **will-quit → quit** | all 7 pass 5/5 |
| s2b quit, W1 discard, W2 cancel | before-quit → W1 … closed → close(W2) → dialog → cancel (1000) → veto → reply(**NO**) → quit-cancelled → terminate-returned (W1 gone, W2 visible) | all pass 5/5 |
| s2c quit, W1 cancels (stop policy) | before-quit → close(W1) → dialog → cancel → veto → serial-stop → reply(NO) → terminate-returned. W2 is never asked. | all pass 5/5 |
| s2d quit, W1 cancels (continue policy) | before-quit → W1 veto → close(W2) → dialog → discard → W2 closed → reply(NO) | all pass 5/5 |
| s3a / s3b re-entrancy (dialog) | close → veto_prompt → dialog-open → [probes] → result:abort (−1001) → veto | all pass 5/5 |
| s3c re-entrancy (terminateLater) | before-quit → W1 allow → closed → W2 allow → closed → reply(YES) → will-quit → quit | all pass 5/5 |
| s3d role reply delivered in default mode during quit | before-quit → close(W1) → **watchdog (3 s)**. The reply is never delivered. | C5, C7 fail 0/5 (hazard demo) |
| **n1** hook returns YES synchronously while dirty | close → `close-return{returned:true}` → destroy → closed → tbc:**false** | **C1 0/5, C4 0/5: fails as required** |
| **n2** 300 ms timeout auto-closes; role answers at 1.5 s | close → close-timeout → tombstone → destroy → closed → *(late)* veto_prompt → stale-reply-dropped | **C1 0/5, C2 0/5: fails as required** |
| p3 second close during modal, gated | close → dialog-open → close(host-close-api) → **close-coalesced** → discard → allow → closed | all pass 5/5 |
| **n3** second close during modal, ungated | close → dialog-open(1) → close(host-close-api) → veto_prompt → **dialog-open(2) while (1) is open** → abort → veto → abort → veto | **C3 0/5: fails as required** |

### Facts behind the table

- **FACT.** `NSApp.terminate(nil)` does not return while the reply is pending. The `NSTerminateLater` wait runs in `NSModalPanelRunLoopMode`: the serializer's `close-request` events are logged in that mode.
  - On reply NO, `terminate-returned` is logged immediately after `reply-returned`, back in `kCFRunLoopDefaultMode`.
  - On reply YES, `will-quit` and `quit` follow inside that same call.
  - This matches the `terminate(_:)` / `terminateLater` docs.
- **FACT.** The dialog dismissal response codes were: `performClick` gave 1000 or 1001, and `abortModal` gave −1001. Counts: 1001 ×30, 1000 ×15, −1001 ×20.
- **FACT.** `NSWindow.close()` did not call `windowShouldClose:` (seen in the `commitClose` path). This matches the docs.
- **FACT, a correction to the n3/p3 setup.** `performClose:` sent to W1 while the app-modal `NSAlert` was up did **not** invoke `windowShouldClose:` in 10 of 10 attempts. The `close-request` was immediately followed by `close-request-returned`, with no `close` event between them. So AppKit itself blocks the AppKit-originated second close during the modal.
  - The only path that can produce a second request is the host's own close API. That is a role calling `win.close()` over kipc, which this harness models as `requestClose(…, "host-close-api")`.
  - The controls therefore exercise that path. The gated owner coalesces the request (p3). The ungated owner opens a nested second `NSAlert` (n3).
- **FACT.** `performClose:` enters `NSEventTrackingRunLoopMode` for about 80–100 ms. That is the button-highlight tracking loop. 52 role replies, and several dialogs, were serviced in that mode.

## Re-entrancy (task item 4): what runs while the modal is up

The probes were armed at `dialog open` (s3a/s3b) or in `applicationShouldTerminate:` (s3c), each firing at +100 ms.
- "during" means the probe fired while the modal (or the terminateLater wait) was still active.
- "after" means it fired only after `runModal` returned. "never" means it did not fire before the process exited.

All counts are out of 5 runs. **FACT.**

| Probe (how it was scheduled) | s3a: modal opened inside a **GCD main-queue block** | s3b: modal opened inside a **CFRunLoopPerformBlock(common)** callout | s3c: `NSTerminateLater` wait |
|---|---|---|---|
| `Timer` default mode (`scheduledTimer`) | after 5 (Default) | after 5 | never 5 |
| `Timer` in `.common` | during 5 (ModalPanel) | during 5 | during 5 (EventTracking) |
| `Timer` in `.modalPanel` | during 5 (ModalPanel) | during 5 | during 5 |
| `perform(_:with:afterDelay:)` (default) | after 5 | after 5 | never 5 |
| `DispatchQueue.main.asyncAfter` | **after 5** | **during 5** | during 5 |
| bg → `DispatchQueue.main.async` | **after 5** | **during 5** | during 5 |
| bg → `performSelector(onMainThread:)` (common) | during 5 | during 5 | during 5 |
| bg → `CFRunLoopPerformBlock` default | after 5 | after 5 | never 5 |
| bg → `CFRunLoopPerformBlock` common | during 5 | during 5 | during 5 |
| bg → `CFRunLoopPerformBlock` modalPanel | during 5 | during 5 | during 5 |
| background queue itself (`probe-bg-sender-runs`) | runs (bg:role) | runs | runs |

What the table shows:
- **FACT.** The `NSAlert.runModal()` loop and the `NSTerminateLater` wait both run the main run loop in `NSModalPanelRunLoopMode`. Every source that fired during them reported that mode, except the common-mode timer in s3c, which hit the `performClose:` tracking window.
- **FACT.** Default-mode sources never run during the modal: default-mode timers, `perform…afterDelay`, and `CFRunLoopPerformBlock(default)`. They run after the modal ends. During `NSTerminateLater` followed by quit, they never ran at all.
- **FACT.** Common-mode and modal-panel-mode sources do run during the modal, on the main thread, nested inside `runModal`:
  - common-mode and modal-panel-mode timers;
  - `performSelectorOnMainThread` (whose doc says common modes);
  - `CFRunLoopPerformBlock` in common or modal-panel mode.
- **FACT.** The GCD main queue drained during the modal **only when the modal was not opened from inside a main-queue block**: s3b and s3c yes, s3a no (5/5 each).
  - **INFERENCE:** the main queue is serial and does not re-enter while one of its own blocks is on the stack. If a Keld host delivered role replies with `DispatchQueue.main.async`, and the prompt was opened from such a reply, every later role message would stall until the modal closed.
- **FACT.** In s3d, delivering role replies in default mode deadlocks `NSTerminateLater` (5/5, watchdog). The reply never runs because the wait is in `NSModalPanelRunLoopMode`.
- **INFERENCE (design consequence for Keld, not proven against keld-wv).** The host's role/kipc wake-up source must be registered in `kCFRunLoopCommonModes`, or explicitly in `NSModalPanelRunLoopMode`. It must not depend on GCD main or default mode. Any role message handled during the modal is re-entrant into host state, and must therefore go through the same txn/tombstone gate. This harness did not inspect how keld-wv's event loop registers its wake-up source.

## Negative controls (task item 5)

| Control | Criterion it must fail | Result (FACT) |
|---|---|---|
| n1: `windowShouldClose:` returns YES synchronously while dirty | C1 (and C4: no tombstone) | Fails 5/5 as required. AppKit closes the window inside `performClose:`, and no role reply ever arrives. |
| n2: 300 ms timeout auto-closes | C2 (and C1) | Fails 5/5 as required. The late reply at about 1.5 s is dropped because the window is already tombstoned. |
| n3: second close during the modal, ungated | C3 | Fails 5/5 as required: a nested second `NSAlert` opens, with `open_dialogs` = 2. The gated twin p3 passes 5/5 (`close-coalesced`). |

## AppKit / Foundation APIs used, with Apple doc citations

The text below is quoted from `https://developer.apple.com/tutorials/data/documentation/<path>.json`, fetched on 2026-10-07 (local copies in `appledocs/`).

- `NSWindowDelegate.windowShouldClose(_:)`, documentation/appkit/nswindowdelegate/windowshouldclose(_:): "Tells the delegate that the user has attempted to close a window or the window has received a performClose(_:) message." "This method may not always be called during window closing. Specifically, this method is not called when a user quits an application." This is why the quit path calls `performClose:` itself.
- `NSWindow.performClose(_:)`, documentation/appkit/nswindow/performclose(_:): "If the windowShouldClose(_:) method returns false, the window doesn't close… the system emits the alert sound."
- `NSWindow.close()`, documentation/appkit/nswindow/close(): "It does not attempt to send a windowShouldClose(_:) message"; "The close method posts a willCloseNotification".
- `NSWindow.isReleasedWhenClosed`, documentation/appkit/nswindow/isreleasedwhenclosed: "Swift and Automatic Reference Counting (ARC) clients need to set this property to false". The harness sets it to false.
- `NSWindowDelegate.windowWillClose(_:)` / `NSWindow.willCloseNotification`, documentation/appkit/nswindow/willclosenotification: "A notification that the window object is about to close."
- `NSApplicationDelegate.applicationShouldTerminate(_:)`, documentation/appkit/nsapplicationdelegate/applicationshouldterminate(_:): "you might delay termination … by calling the reply(toApplicationShouldTerminate:) method."
- `NSApplication.TerminateReply.terminateLater`, documentation/appkit/nsapplication/terminatereply/terminatelater: "causes Cocoa to run the run loop in the NSModalPanelRunLoopMode until your app subsequently calls reply(toApplicationShouldTerminate:)".
- `NSApplication.reply(toApplicationShouldTerminate:)`, documentation/appkit/nsapplication/reply(toapplicationshouldterminate:): "your code must subsequently call this method".
- `NSApplication.terminate(_:)`, documentation/appkit/nsapplication/terminate(_:): "If the method returns terminateLater, the app runs its run loop in the NSModalPanelRunLoopMode mode until the reply(toApplicationShouldTerminate:) method is called".
- `NSApplicationDelegate.applicationWillTerminate(_:)`, documentation/appkit/nsapplicationdelegate/applicationwillterminate(_:): "The app will terminate after this method returns."
- `NSAlert.runModal()`, documentation/appkit/nsalert/runmodal(): "Runs the alert as an app-modal dialog"; "can return values other than those specific to the alert buttons … if the alert is canceled programatically."
- `NSApplication.runModal(for:)`, documentation/appkit/nsapplication/runmodal(for:): "it does not respond to any other events (including mouse, keyboard, or window-close events) unless they are associated with the window. It also does not perform any tasks (such as firing timers) that are not associated with the modal run loop."
- `NSApplication.abortModal()`, documentation/appkit/nsapplication/abortmodal(): "use abortModal() … when responding to an NSTimer that you have added to the NSModalPanelRunLoopMode mode". `ModalResponse.abort`: "Modal session was broken with abortModal()."
- `NSApplication.stopModal()` / `stopModal(withCode:)`: "In macOS 10.9 and later, you can use this method to stop a runModal(for:) loop outside of an event callback, such as from within a method repeatedly invoked by an Timer". These are used indirectly, through `NSAlert`'s button action via `performClick`. **INFERENCE:** `NSAlert` stops the modal through `stopModal(withCode:)`. The response codes are consistent with that, but the internals were not inspected.
- `RunLoop.Mode.modalPanel`: "The mode set when waiting for input from a modal panel". `RunLoop.Mode.common`: "A pseudo-mode that includes one or more other run loop modes."
- `Timer.scheduledTimer(withTimeInterval:repeats:block:)`: "schedules it on the current run loop in the default mode."
- `NSObject.perform(_:with:afterDelay:)`: "The timer is configured to run in the default mode … otherwise, the timer waits until the run loop is in the default mode."
- `NSObject.performSelector(onMainThread:with:waitUntilDone:)`: "queues the message on the run loop of the main thread using the common run loop modes".
- `CFRunLoopPerformBlock(_:_:_:)`: "does not automatically wake up the specified run loop … you must explicitly wake up that thread using the CFRunLoopWakeUp(_:) function". The harness calls `CFRunLoopWakeUp` after each block.
- `DispatchQueue.main`, documentation/dispatch/dispatchqueue/main: the main queue is executed via `NSApplicationMain` / a CFRunLoop on the main thread. The doc says nothing about nested-modal re-entrancy; the s3a vs s3b difference above is observed only.
- Also used: `NSApplication.setActivationPolicy(.regular)`, `activate(ignoringOtherApps:)`, `NSApp.modalWindow`, `NSWindow.isDocumentEdited`, `CFRunLoopCopyCurrentMode`.

## Limitations (stated, not hidden)

- Input is programmatic:
  - `performClick` stands in for the user's click.
  - The timers run in `.modalPanel` mode.
  - There are no real mouse or keyboard events. The `runModal(for:)` doc says real clicks on other windows are not delivered during the modal; this was not exercised with HID events.
- The prompt is an app-modal `NSAlert`. Sheets (`beginSheetModal`, document-modal) were not tested.
- There is no WKWebView, no keld-wv, and no real Bun role. The role is a background queue.
- Determinism is 5 runs per scenario on one machine and one OS build.
- The Electron cross-diff and the ▲/✔ marks for #419 are left to the caller.
- Host condition: during this work the data volume had about 130–400 MB free. One tool call failed with ENOSPC. The cause is outside this directory: `p2/appkit` totals about 1.5 MB, and the harness caps each transcript at 3,000 events.
