# GH-1189 Windows dialog examples

This file adds rules to `../AGENTS.md`. These rules apply only to `gh1189_taskdialog_fixture.rs`, `gh1189_winevent_observer.rs`, `gh1189_build.ps1`, and `gh1189_negative_controls.ps1`. They do not grant production FFI access.

Follow the root `AGENTS.md` requirement-word rules. Use uppercase BCP 14 words as defined by RFC 2119 and RFC 8174.

- The build MUST use the `gh1189-dialog-fixture` Cargo feature. Production targets MUST NOT import these examples.
- An independent reviewer MUST approve the exact source, manifest, Cargo features, unsafe calls, and process custody before an operator builds or runs a fixture.
- Each unsafe callback MUST use the Windows system ABI. Each unsafe operation MUST have a local `// SAFETY:` proof. The source MUST deny `unsafe_op_in_unsafe_fn`. It MUST NOT add a module-wide unsafe grant.
- The fixture MAY call `TaskDialogIndirect`, `SendMessageW` for its own dialog, `GetCurrentThreadId`, and `IsWindowVisible`. It MAY retain the fixture-owned `State` pointer until its one-use process exits. It MUST NOT free the state while a callback can use it.
- The observer MAY call `SetWinEventHook` for the exact child PID, `UnhookWinEvent`, `GetProcessTimes`, `GetWindowThreadProcessId`, `GetWindow(GW_OWNER)`, `EnumWindows`, `IsWindowVisible`, `PeekMessageW`, `TranslateMessage`, `DispatchMessageW`, `MsgWaitForMultipleObjectsEx`, and `WaitForSingleObject` on its owned child handle. Other native operations require new review.
- The observer MAY read the local `Census` pointer only during the synchronous `EnumWindows` callback. It MUST NOT retain that pointer.
- The observer MUST check the owned parent, child PID, UI thread, dialog identity, visibility, destruction, modal return, and child exit. The observer MUST reject decoy windows and extra records.
- The observer MAY use `Child::kill` to rescue its own failed fixture. Rescue MUST count as failure, never as native acceptance. A fixture MUST NOT act on a foreign process or window.
- Each control MUST change exactly one fixture behavior; the runner MUST verify a specific semantic rejection and a new healthy child. Timeout or rescue MUST NOT earn success credit. Reviewed commit and executable digests are mandatory.
- The operator MUST obtain a fresh GH-1189 issue claim and exclusive Windows device slot before native execution. A test-only PASS MUST NOT count as WebView2 or draw.io product acceptance.
