# Zettlr cross-OS lanes — per-engine primitives with primary-source receipts

Planning record for [#1358](https://github.com/gyldlab/keld/issues/1358) on map [#391](https://github.com/gyldlab/keld/issues/391).
Generated 2026-10-31 by an AI agent (Claude Code, Fable 5.1) on behalf of @0monish under the user's
delegation to take technical decisions. Every decision-bearing cell cites a primary source with the
retrieval date so an implementing agent can re-fetch it. Labels: FACT (quoted), INFERENCE, UNKNOWN.
Nothing here is normative; approved specs and live GitHub state outrank it. All receipts retrieved
2026-10-31 unless stated. Rendered reference sites (webkitgtk.org, docs.gtk.org, learn.microsoft.com
in part) were unreachable from the session; doc-comment sources on raw.githubusercontent.com (the
text those sites render) are cited instead, at branch tip, with `Since:` lines where version matters.

## 0. Sequencing decision (D-Z14)

**macOS → Linux → Windows**, each lane consuming the same shared contracts (#540 IPC v2, #541/#552
grants, #542 identity, #544 codec, #449/#450 registry+close, #478 dialogs, #481 theme mirror,
#586/#588 profile) and its own OS program (#1124 Linux, #1182 Windows) for native adapters.

- FACT: Keld main `231a212`: `@keld/electron` `BrowserWindow` is macOS-only; on Linux/Windows the
  Host refuses window subscription and direct window calls fail with KELD-CORE-045
  (`docs/engineering/product-status.tsv` row `package.keld-electron`). No Keld renderer bridge exists
  for WebView2 (direct COM, injects nothing: `crates/keld-wv/src/webview2/mod.rs:12-15`) or WebKitGTK;
  on Linux, however, wry 0.56.1 injects its own page-world `window.ipc` bridge and `ipc` message
  handler unconditionally (refuter FACT, wry `webkitgtk/mod.rs:348,748`), which #1127 must disable or
  replace.
- FACT: Linux window/close adapter design is approved (#1125 comment 6373060597; PR #1133 carrier
  `4f78d486`, Status: approved); the Linux backend is the interim wry + WebKit2GTK 4.1 / GTK3 seam
  (`crates/keld-wv/src/webkitgtk/mod.rs:3-8`), with webkitgtk-6.0/GTK4 as the spec destination.
- FACT: Windows preload isolation is route D (no isolated world; Host-enforced facade) in draft
  spec PR #1307 with seven open approval blockers B1–B7; transport/startup/test blockers #1073,
  #814, #1114 remain open (#1182).
- INFERENCE: Linux reaches a Zettlr proof before Windows because its primitives (named worlds,
  WebKit sync scheme path, approved close adapter) exist, while Windows still needs the route-D
  approval and has no isolated world. Falsifier: #1307 approved and W1–W6 landed before #1125–#1129.

## 1. Zettlr's OS-dependent code (corpus zettlr@v4.9.0 `04128af4`; identical at e6c7fd8 for these files)

| Area | macOS | Windows | Linux | Source |
|---|---|---|---|---|
| Window chrome | `titleBarStyle:'hiddenInset'` + `vibrancy:'under-window'` (unless reduced transparency) | `frame:false`, `titleBarStyle:'hidden'`, `titleBarOverlay{color,symbolColor,height}` | same as Windows unless `window.nativeAppearance` (default `true` only on darwin → Linux default is non-native chrome) ; `icon` PNG | `windows/set-window-chrome.ts:33-72`; `config/get-config-template.ts:318` |
| Splash/onboarding | `sandbox:false`, `skipTaskbar:true`, `frame:false`, `alwaysOnTop` | same | same (Electron: `skipTaskbar` is `_macOS_ _Windows_`; Linux ignores) | `splash-screen.ts:46-65`, `onboarding-window.ts:64-77` |
| Theme | `systemPreferences.subscribeNotification('AppleInterfaceThemeChangedNotification')` unconditional | `nativeTheme.themeSource` writes; `isWindows` branch | `isLinuxWithoutNativeAppearance` branch | `appearance/index.ts:77-96, 117-124` |
| Save accelerator | `Cmd+S` (menu.darwin.ts:216) | `Ctrl+S` (menu.win32.ts:183) | `Ctrl+S` (`BLUEPRINTS[process.platform]`, menu.win32 blueprint reused; app menu only on darwin/win32 — `menu/index.ts:120`) | `menu/*.ts` |
| Last window close | veto → async dialog → `window.close()` | `app.quit()` **without** `preventDefault` when `nWindows === 1 && !leaveAppRunning` | same as Windows | `windows/index.ts:364-399` |
| `window-all-closed` | no quit | `app.quit()` unless `leaveAppRunning` | same | `main.ts:183-195` |
| Tray probe | `true` | `true` | `gsettings get org.gnome.shell enabled-extensions` spawn on GNOME; else `true` | `app/util/is-tray-supported.ts` |
| Recent docs | `app.getRecentDocuments` | same | not used (`['darwin','win32'].includes`) | `recent-docs/index.ts:61-94` |
| `safe-file` path mapping | `//host/x` → `/Volumes/host/x` | `/C:/…` → `C:/…`; `//host` → `/host` | UNC → 404 | `app/util/custom-protocols.ts:60-85` |
| Updater | dmg | nsis | AppImage/deb/rpm (`updates/index.ts:310-316`) | parked |
| Pandoc/git/quarto probes | `pandoc` | `pandoc.exe` | `pandoc` | `environment-check.ts:74` |

## 2. Capability × engine matrix

Status vocabulary (NOT scoreboard status; scoreboard ✔/▲/✘ belong to KEL-74 cells only): **present** = engine primitive exists (cited) · **emulated** = documented emulation/divergence · **absent** = no primitive · **unknown** = needs the named probe. Engine receipts are cited at branch tip; before any receipt backs a cell it MUST be re-fetched at the pinned engine identity (D-Z18: WebKitGTK 2.52.6 / wry 0.56.1 per arch 05 §148; WebView2 runtime 156.0.4314.50 / SDK 1.0.4314.35 per PR #1307; macOS WebKit = the recorded OS build). Keld owner tickets in the last column; the macOS entries link to the existing first-proof tickets.

### 2.1 Renderer isolation (preload world)

| Engine | Primitive | Status | Receipt | Keld owner |
|---|---|---|---|---|
| WKWebView | `WKContentWorld` named worlds; world-scoped `WKScriptMessageHandler` | present (live: `macos_bridge.rs` uses `defaultClientWorld`) | https://developer.apple.com/documentation/webkit/wkcontentworld (cited in #484 receipts, 2026-10-25) | #484 (app world, PANEL-D21 A) → #467 → #571 |
| WebKitGTK | `webkit_user_script_new_for_world(..., world_name, ...)` (Since 2.22); `webkit_user_content_manager_register_script_message_handler(manager, name, world_name)` (webkitgtk-6.0, Since 2.40; `_in_world` variant in 4.x, Since 2.22); `webkit_web_view_evaluate_javascript(..., world_name, ...)` (Since 2.40) | present in WebKitGTK; **not reachable through Keld's seam today**: wry 0.56.1 injects a page-world `window.ipc` user script unconditionally (`UserScript::new`, not `new_for_world`; wry `webkitgtk/mod.rs:348`) and registers the `ipc` handler (`:748`); named worlds need `WebViewExtUnix::webview()` or direct bindings (#1127 owns that choice) | https://raw.githubusercontent.com/WebKit/WebKit/main/Source/WebKit/UIProcess/API/glib/WebKitUserContent.cpp — "Creates a new user script for script world with name @world_name." ; …/WebKitUserContentManager.h.in — "If %NULL is passed as the @world_name, the default world will be used." ; …/WebProcess/InjectedBundle/API/glib/WebKitScriptWorld.cpp — "Scripts executed in isolated worlds have access to the DOM but not to other variables or functions created by the page." | #1127 (Linux preload/bridge spec) |
| WebView2 | none: no API runs script in a world other than the page's | absent → route D (emulated; `process.contextIsolated === false`) | #1184 body and PR #1307 (keld-research note 264: SDK 1.0.3650.58 bindings, release notes 1.0.3065.39–1.0.4314.35, WebView2Feedback #554 "we don't currently have a way to execute scripts in a context other than the current page") | #1184 / #1307 (W1) |

Zettlr consequence on Windows (refuter-corrected): under route D the preload is **flagged** by #1307's classifier (it reads `process.argv`, copies `process.env`, wraps constant-argument channels), so its window receives grants only with a digest-bound developer attestation, and `ipcRenderer.invoke` is not a route-D sink until the #1154/#547 revision (#1307 AC13) — Zettlr's 186 `invoke` sites get **zero** grants on Windows as drafted. The Host grant (#541/#552) remains the only boundary on all three engines (D-Z5).

### 2.2 Synchronous renderer→host call (`ipcRenderer.sendSync`, 6 sites)

| Engine | Candidate carrier | Status | Receipt | Keld owner |
|---|---|---|---|---|
| WKWebView | sync XHR → `WKURLSchemeHandler` | present in WebKit, **precondition**: the sync endpoint must be same-origin with the page document — XHR runs in CORS mode (`XMLHttpRequest.cpp:626`), `DocumentThreadableLoader::makeCrossOriginAccessRequest` → `checkURLSchemeAsCORSEnabled` refuses non-HTTP schemes ("Cross origin requests are only supported for HTTP."; `builtinCORSEnabledSchemes()` = {http, https}, `LegacySchemeRegistry.cpp:275-279`); a handler-backed scheme has a tuple origin (`SecurityOriginData` `shouldTreatAsOpaqueOrigin` = `!schemeIsHandledBySchemeHandler`), so a `file:` page is never same-origin with it and WKWebView exposes no public CORS-enable API (only `_corsDisablingPatterns` SPI). Measurement #401 arms 1–8 after the origin precondition | WebKit `WebLoaderStrategy.cpp` `tryLoadingSynchronouslyUsingURLSchemeHandler` → `WebURLSchemeHandlerProxy::loadSynchronously` → `sendSync(Messages::WebPageProxy::LoadSynchronousURLSchemeTask(...))`; `WebPageProxy.messages.in`: `LoadSynchronousURLSchemeTask(...) -> (...) Synchronous`; `WebURLSchemeTask.cpp` fires `m_syncCompletionHandler` on `didComplete`; `XMLHttpRequest.cpp` `setTimeout` throws `InvalidAccessError` for sync document requests (host must enforce the deadline) | #401 → #548 / lane ticket |
| WebKitGTK | same WebKit core: `webkit_web_context_register_uri_scheme` handler | present (FACT: `class WebKitURISchemeHandler final : public WebURLSchemeHandler` with `platformStartTask` override in `UIProcess/API/glib/WebKitWebContext.cpp`; `WebURLSchemeHandler::startTask(..., SyncLoadCompletionHandler&&)` in `WebURLSchemeHandler.h`); the GLib callback runs on the UI-process main loop and finishes via `webkit_uri_scheme_request_finish*` | https://raw.githubusercontent.com/WebKit/WebKit/main/Source/WebKit/UIProcess/API/glib/WebKitWebContext.cpp ; …/UIProcess/WebURLSchemeHandler.h ; …/UIProcess/API/glib/WebKitURISchemeRequest.cpp (`finish`, `finish_with_response` Since 2.36, `finish_error` Since 2.2, `get_http_body` Since 2.40) | #1127 sub-arm (Linux run of the #401 arms); same origin precondition, or `webkit_security_manager_register_uri_scheme_as_cors_enabled` + `Access-Control-Allow-Origin` from the handler (wry registers schemes only `as_secure`, `web_context.rs:131-144`) |
| WebView2 | none admitted: #1307 §1 names `sendSync` a non-goal and admits calls through top-level `WebMessageReceived` only; `hostObjects.sync` is rejected (the Host-side `IDispatch` call would make the Host wait on Bun — #425 / KEL-142 "no host UI thread blocks on Bun" — and hands the page a host object, #1184 AC ii); sync XHR + `WebResourceRequested` deferral is unknown and needs a #1307 revision first | absent (typed throw #548) | §4.2 | #548 on Windows; a later #1307 revision |

Decision D-Z2 (v2): one engine-neutral contract (same wv-link envelope, nonce-bound, host-enforced deadline, rate limit, dev warning; typed throw #548 as the live state). macOS/Linux candidate lane A has four gates in order: (1) the app document is served from the host resource origin (GH473 scheme, #1195) and the sync endpoint lives on that same origin; (2) #401 arms 1–8 pass on a real device (Linux arm under #1127); (3) KEL-142 revision owned on #484 plus a #540 amendment admit the lane as the same envelope; (4) a new lane ticket carries wire-protocol, public-API and permission-model gates. Windows: typed throw only.

### 2.3 Cross-world synchronous returns (`contextBridge` function/sync values)

| Engine | Mechanism | Status | Receipt | Keld owner |
|---|---|---|---|---|
| WKWebView / WebKitGTK | page `dispatchEvent` → isolated-world listener runs synchronously; `CustomEvent.detail` structured-cloned across worlds; result written to shared DOM state | present (WebCore, engine-core → both engines; `JSEventListener::handleEvent` gates only on the listener's own world, `EventTarget::innerInvokeEventListeners` has no world filter); readback+cost measured by #399 arms | WebKit `LayoutTests/http/tests/security/isolatedWorld/dispatchEvent.html` + `-expected.txt`; `JSCustomEventCustom.cpp` `detail()` → `cachedPropertyValue` → `cloneAcrossWorlds` (`JSDOMWrapper.cpp`: `SerializedScriptValue::create(..., SerializationContext::CloneAcrossWorlds)`) | #399 → #549 |
| WebView2 | page and preload share the main world (route D): function values and sync returns are plain JS | emulated (isolation only) | #1307 AC23 | #1184 |

### 2.4 Custom scheme (`protocol.handle('safe-file')`, 11 consumer files)

| Engine | Registration | Status | Receipt | Keld owner |
|---|---|---|---|---|
| WKWebView | `WKWebViewConfiguration.setURLSchemeHandler(_:forURLScheme:)` at configuration time (before view creation) | present; Keld has no registration yet (`engine.rs:8-12` omits `register_scheme`; GH473 #1195 reserves the native seam) | Apple docs cited in #471/#1195; research-branch probe `probes/f05-wk-scheme/main.swift` | #554 → #471 |
| WebKitGTK | `webkit_web_context_register_uri_scheme(context, scheme, cb, ...)`; handler is pushed to already-existing views; invalid/special/duplicate schemes rejected; `WebKitSecurityManager` privilege bits (`local`, `no_access`, `display_isolated`, `secure`, `cors_enabled`, `empty_document`) | present (wry registers builder-time schemes `as_secure`, not CORS-enabled) | https://raw.githubusercontent.com/WebKit/WebKit/main/Source/WebKit/UIProcess/API/glib/WebKitWebContext.cpp — "Register @scheme in @context, so that when a URI request with @scheme is made … the #WebKitURISchemeRequestCallback registered will be called" ; body `g_critical("Cannot register URI scheme %s more than once", scheme)`; …/WebKitSecurityManager.cpp quotes per bit | #471 (table) + #1126/#1127 Linux cells |
| WebView2 | environment-creation-time `CoreWebView2CustomSchemeRegistration`; served through `WebResourceRequested` | present (§4.3) | §4.3 | #471 + #1185 (W3) |

Static host-owned scheme table (#471) holds on all three: WKWebView and WebView2 need the scheme
before view/environment creation; WebKitGTK allows later registration but Keld keeps one rule.

### 2.5 Window close veto and quit

| Engine/OS | Primitive | Status | Receipt | Keld owner |
|---|---|---|---|---|
| macOS | `windowShouldClose:` NO + programmatic close (tao `CloseRequested` held) | present (gh531 approved; live `BrowserWindow.close` two-phase) | gh531 spec; PANEL-P2 #419 | #450 → #451 |
| Windows | `WM_CLOSE` → tao `CloseRequested` with immediate veto (no second WndProc) | design in gh1188 draft (#1280 merged docs) | `docs/specs/gh1188-windows-window-adapter.md` §4.0, A4, A16 (`WM_CLOSE` is what the caption close produces, learn.microsoft.com/windows/win32/winmsg/wm-close) | #1188 (W2) |
| Linux GTK3 (interim) | `GtkWidget::delete-event` — "Returns: %TRUE to stop other handlers from being invoked" (default handler destroys) | present; Keld removes the view today (#1125 problem statement) | https://raw.githubusercontent.com/GNOME/gtk/gtk-3-24/gtk/gtkwidget.c | #1125 (approved design, T2/T3 open) |
| Linux GTK4 (destination) | `GtkWindow::close-request` — "Return: true to stop other handlers"; `gtk_window_close()` | present | https://raw.githubusercontent.com/GNOME/gtk/main/gtk/gtkwindow.c | #1125 |
| Logout/shutdown | macOS `applicationShouldTerminate`; Windows `WM_QUERYENDSESSION` (Electron: `before-quit` not emitted on Windows shutdown — app.md); GTK `GtkApplication::query-end` + `gtk_application_inhibit(LOGOUT)` ("not guaranteed … users will be given the option to force the action") | rows only | https://raw.githubusercontent.com/GNOME/gtk/main/gtk/gtkapplication.c ; Electron v44.4.5 app.md `before-quit` | #451 (rows) |

D-Z12: the unsaved-close cell is OS-specific. On Windows/Linux, when `nWindows === 1 && !leaveAppRunning`, Zettlr's last-window `close` handler calls `app.quit()` without `preventDefault` (`windows/index.ts:374-381`); with two or more windows the macOS path applies. Electron v44.4.5 source (FACT): `close` is emitted synchronously from `NativeWindow::NotifyWindowCloseButtonClicked`, and if not prevented the native close proceeds (`electron_api_browser_window.cc:158-176`); `Browser::Quit` runs `HandleBeforeQuit` and a `preventDefault` leaves `is_quitting_` false, so `window-all-closed` fires when the window goes (`browser.cc:120-132, 314-322`). INFERENCE (not observed): the window is destroyed while the first unparented dialog is open, `window-all-closed` → `app.quit()` re-enters `before-quit`, `isClean()` is still false, and a **second** message box appears. Recorder observables per OS: order of `closed` vs first dialog; `before-quit` count; message-box count and parent; `will-quit` count. Both per-window `close` handlers (`windows/index.ts:364` and `create-main-window.ts:118` storage clear) must run in Keld's adapter. Cells stay `unknown` until recorded on that OS (D-Z17).

### 2.6 Native dialogs (open/save/message, all async in Zettlr)

| OS | Primitive | Status | Receipt | Keld owner |
|---|---|---|---|---|
| macOS | NSOpenPanel/NSSavePanel/NSAlert sheets (parented) | in flight (#1306/#1308) | #478 | #478 → #561 |
| Windows | `IFileOpenDialog`/`IFileSaveDialog` (Common Item Dialog), `TaskDialogIndirect` | present (§4.5) | §4.5 | #1189 (W5) |
| Linux GTK3 (interim) | `GtkFileChooserNative` (portal-first in sandbox; `gtk_native_dialog_run` "Blocks in a recursive main loop" Since 3.20) ; `GtkMessageDialog` + `response` (non-modal) or `gtk_dialog_run` | present | https://raw.githubusercontent.com/GNOME/gtk/gtk-3-24/gtk/gtkfilechoosernative.c ; …/gtknativedialog.c ; …/gtkmessagedialog.c | #478 Linux cells via #1124 demand trace |
| Linux GTK4 (destination) | `GtkFileDialog` (Since 4.10, async `open`/`save` with callback, built on `GtkFileChooserNative`); `GtkAlertDialog.choose` (4.10); blocking `gtk_dialog_run` removed | present | https://raw.githubusercontent.com/GNOME/gtk/main/gtk/gtkfiledialog.c ; …/gtkalertdialog.c ; …/docs/reference/gtk/migrating-3to4.md — "GtkDialog, GtkNativeDialog, and GtkPrintOperation removed their blocking API using nested main loops." | same |
| Linux portal | `org.freedesktop.portal.FileChooser` v4 (`OpenFile`/`SaveFile`, results via `Request::Response`; `current_folder` option) | present | https://raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.FileChooser.xml | same |
| Electron Linux notes | `defaultPath` needs portal backend ≥ 4; `openFile`+`openDirectory` → directory selector; save title may not display | oracle | Electron v44.4.5 `dialog.md` | #477/#554 cells |

Keld's async dialog broker (#478) is the only shape Zettlr needs on every OS (0 sync dialogs);
the GTK4 destination is async-only, which matches; the GTK3 interim can use the non-modal
`response` path and never `gtk_dialog_run` (which would nest the host loop).

### 2.7 Theme, accent, display facts

| OS | Primitive | Status | Receipt | Keld owner |
|---|---|---|---|---|
| macOS | `NSApp.effectiveAppearance` KVO; `AppleInterfaceThemeChangedNotification` | present | #481 mirror; #1356 for `systemPreferences` | #481 → #558 → #1356 |
| Windows | `UISettings.ColorValuesChanged` / `GetColorValue(Accent)`; `AppsUseLightTheme` registry | present (§4.6) | Electron 44.5.0 commit `ea1a2714` "report the system accent colour consistently on Windows" (HKCU read) | #1186 (W4) |
| Linux | portal `org.freedesktop.portal.Settings` v2: `org.freedesktop.appearance` `color-scheme` (u: 0 none/1 dark/2 light), `accent-color` ((ddd) sRGB, documented 1.17.1), `contrast`; `SettingChanged` signal; GTK3 `gtk-application-prefer-dark-theme`; libadwaita `AdwStyleManager` reads the portal | present | https://raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.Settings.xml ; …/NEWS.md ("1.12.0 … settings: Add color-scheme key"); https://raw.githubusercontent.com/GNOME/libadwaita/main/src/adw-settings-impl-portal.c | #481 Linux cells (#1124) |
| Electron per-OS | `systemPreferences.subscribeNotification` is `_macOS_`; `getAccentColor` has no platform tag ("only available on macOS 10.14 Mojave or newer"; Windows implementation evidenced by commit `ea1a2714`); `getColor` `_Windows_ _macOS_` | oracle | Electron v44.4.5 `system-preferences.md:62,185-200` | #1356 (macOS); Windows/Linux rows ▲ in #482 |

Zettlr only calls `subscribeNotification` on darwin, so #1356 is macOS-only by demand; Windows
and Linux need no `systemPreferences` member for the proof (`getAccentColor` sits in try/catch).

### 2.8 File watching (chokidar 5 at FSAL boot)

| OS | Primitive | Status | Receipt | Keld owner |
|---|---|---|---|---|
| macOS | FSEvents via Bun `fs.watch` | unknown (arms in #1355) | — | #1355 |
| Linux | inotify: not recursive, per-directory watches, `max_user_watches` limit, `IN_Q_OVERFLOW` | unknown: Bun's `fs.watch({recursive:true})` Linux implementation not located (four candidate Zig paths 404) | https://raw.githubusercontent.com/mkerrisk/man-pages/master/man7/inotify.7 — "Inotify monitoring of directories is not recursive"; https://raw.githubusercontent.com/oven-sh/bun/main/docs/guides/read-file/watch.mdx — "By default the watch is _shallow_ … pass the `recursive: true` option"; chokidar README (v6, Oct 2026): "native per-directory watching elsewhere" | #1355 Linux arm |
| Windows | libuv `uv_fs_event_t` over ReadDirectoryChangesW (recursive) | present (§4.7) | §4.7 | #1355 Windows arm |

Note: chokidar's current major is v6 (requires Node 22+, native recursive on macOS/Windows);
Zettlr pins `^5.0.0`. The v5 README was not fetched (UNKNOWN); the arms pin chokidar at Zettlr's lock.

### 2.9 Tray, notification, single instance, misc

| Surface | macOS | Windows | Linux | Receipt | Keld owner |
|---|---|---|---|---|---|
| Tray | NSStatusItem | Shell_NotifyIcon | StatusNotifierItem (`org.kde.StatusNotifierItem`: `Activate`, `ContextMenu`, `Menu` property) with `GtkStatusIcon` fallback (deprecated GTK 3.14; absent in GTK4) | KDE introspection XML; GTK3 `deprecated/gtkstatusicon.c`; Electron `tray.md` "Tray icon uses StatusNotifierItem by default … `GtkStatusIcon` will be used instead" | #557 (parked; default config off) |
| Notification | UNUserNotification (signed app) | AppUserModelID + toast | libnotify-class (Electron docs do not name libnotify in v44.4.5 `notification.md`; `timeoutType`/`urgency` are `_Linux_ _Windows_`) | Electron v44.4.5 `notification.md`, `tutorial/notifications.md` | #482 rows |
| Single instance | Keld host lock (#453) | same | same; GApplication D-Bus uniqueness is the platform reference, not the mechanism (Keld owns the lock) | https://raw.githubusercontent.com/GNOME/glib/main/gio/gapplication.c ; Electron app.md (32 MB message cap on macOS/Linux) | #453 |
| `open-file` | `_macOS_` event | argv parse | argv parse | Electron app.md | #524 |
| `skipTaskbar` | present | present | tag `_macOS_ _Windows_` only; Linux behaviour unknown until recorded (no doc sentence says "ignored") | Electron `base-window-options.md` | #456 rows |
| `setDocumentEdited` / `setRepresentedFilename` | `_macOS_` (Zettlr calls `setDocumentEdited` only under darwin; never calls `setRepresentedFilename`) | tag only; unknown | tag only; unknown | Electron `base-window.md:1128,1139` | #534 (D-Z7) |
| Wayland | — | — | no programmatic resize/move/focus without user input; modal type `dialog`; `alwaysOnTop` unsupported | Electron `browser-window.md`, 43→44 doc delta | #1125 cells |

## 3. Electron oracle delta (D-Z1 evidence)

- FACT: `breaking-changes.md` is byte-identical at v44.4.5, v44.9.1 and v44.10.1; the 44.0 section
  removes the renderer `clipboard` module and rearchitects `clipboard` to Promises (`writeImage`
  removed). Zettlr e6c7fd8 calls `clipboard.writeImage` and sync `readText`; v4.9.0 calls
  `await clipboard.write([...])` / `await clipboard.readText()`.
- FACT: docs for ipc-main, ipc-renderer, context-bridge, dialog, system-preferences, native-theme,
  tray, shell, net, web-utils, browser-window-options, sandbox, process-model are byte-identical
  between v44.4.5 and v44.9.1; app.md (app shims, `child-process-gone` fields), protocol.md
  (`registerSource`/`getSource`), notification.md (`actions` tags), web-contents.md (blur note,
  zoom clamp) differ. No 44.4.5→44.9.1 commit subject mentions ipcMain/ipcRenderer/contextBridge/
  webUtils/close event (160 commits); relevant subjects: 44.5.0 Linux XDG portal version check
  for file dialogs, Windows accent colour consistency, Wayland window fixes; 44.6.0
  `protocol.registerSource`; 44.9.0 opener session for link-opened windows.
- FACT: Electron's versioning table maps "Electron non-breaking API changes" to minor releases
  and "Electron bug fixes" to patch releases (`docs/tutorial/electron-versioning.md`); 44.4.5→44.9.1
  spans five minors, so additive API is possible and the deltas above are the recorded ones.
- FACT: 45.0 removes `events`/`timers`/`url`/`Buffer` from the sandboxed preload `require`; it
  does not apply to any 43.x/44.x tag. Zettlr's bundled preload imports only `electron` at runtime.

## 4. WebView2 (Windows) receipts — fills the U cells above

All from the published sources of learn.microsoft.com (MicrosoftDocs/webview2-win32-reference, SDK dir `1-0-4314-35`, `ms.date: 10/19/2026`; MicrosoftDocs/win32; MicrosoftDocs/sdk-api; MicrosoftDocs/winrt-api; MicrosoftDocs/edge-developer) on raw.githubusercontent.com, retrieved 2026-10-31; learn.microsoft.com itself was unreachable from the session.

### 4.1 Isolation (§2.1)
- FACT: `AddScriptToExecuteOnDocumentCreated` — "Add the provided JavaScript to a list of scripts that should be run after the global object has been created, but before the HTML document has been parsed and before any other script included by the HTML document is run." and "If an HTML document is running in a sandbox of some kind using sandbox properties or the Content-Security-Policy HTTP header affects the script that runs." (https://raw.githubusercontent.com/MicrosoftDocs/webview2-win32-reference/main/1-0-4314-35/icorewebview2.md). INFERENCE: the injected script shares the page's execution environment (main world).
- FACT: no `isolated`/`world` API in `icorewebview2.md`, `_2/_3/_4/_21.md`, `icorewebview2frame.md`, `icorewebview2executescriptresult.md`; GitHub code search over MicrosoftDocs/webview2-win32-reference and edge-developer for "isolated world" → 0 results. WebView2Feedback #554 ("multi JS Context in webView2") open; #2510 ("Does Webview2 have a concept of context isolation?") closed, resolution UNKNOWN. This confirms #1184's route-D premise.

### 4.2 Synchronous page→host (§2.2a) — two candidate carriers, both device-gated
| Carrier | Receipt | Consequence |
|---|---|---|
| `chrome.webview.hostObjects.sync.{name}` | FACT (icorewebview2.md `AddHostObjectToScript`): "These are not promises and function runtimes or property access synchronously block running script waiting to communicate cross process for the host code to run." ; "it is recommended that you use the promise-based asynchronous `window.chrome.webview.hostObjects.{name}` API." ; "While JavaScript is blocked on a synchronous run to native code, that native code is unable to run back to JavaScript. Attempts to do so fail with `HRESULT_FROM_WIN32(ERROR_POSSIBLE_DEADLOCK)`." ; VARIANT types only ("generic `IUnknown`, `VT_DECIMAL`, or `VT_RECORD` variant is not supported"). Threading (edge-developer `concepts/threading-model.md`): "The WebView2 must be created on a UI thread that uses a message pump. All callbacks occur on that thread" ; "If you use methods that block the message pump … your WebView2 event handlers and async-method completion handlers don't run." | The host-side `IDispatch` method must return the value synchronously, so the Host UI thread waits for Bun's REPLY for the call's duration (bounded by the deadline) and may not evaluate JS meanwhile. Documented, but blocks the Host pump; divergence from Electron, where only the renderer blocks. |
| sync XHR → registered custom scheme → `WebResourceRequested` + deferral | FACT: "This event is by default raised for file, http, and https URI schemes. This is also raised for registered custom URI schemes." ; "If a deferral is taken, then the web resource requested is blocked until the deferral is completed." (icorewebview2.md `add_WebResourceRequested`); "the `WebResourceRequested` event is raised on the WebView2 UI thread of your host app process" (`concepts/working-with-local-content.md`). Chromium: "This class owns the context necessary to perform an asynchronous request while the main thread is blocked so that it appears to be synchronous." (`third_party/blink/renderer/platform/loader/fetch/url_loader/sync_load_context.h`). | UNKNOWN whether a synchronous XHR is serviced through `WebResourceRequested` deferrals without stalling (no Microsoft doc or WebView2Feedback issue found). If it is, the deferral lets the Host return from the handler and complete later, matching the WebKit shape (renderer blocked, Host pump free). |

Decision D-Z2/D-Z16 (Windows, v2 after refutation): **no synchronous carrier is adopted**. `hostObjects.sync` is rejected — the Host-side `IDispatch` call completes on the UI thread, so the Host would wait on Bun (#425 closed: "The host never blocks its UI thread on Bun"; #401 packet's invariant at risk) and the page would hold a host object (#1184 AC ii). A sync XHR to a `WebResourceRequested`-served scheme is outside #1307's "top-level `WebMessageReceived` only" admission rule and its stated `sendSync` non-goal; it also needs a tuple-origin scheme with the page served from it (opaque-origin schemes refuse cross-origin XHR; same-origin POST still needs `AllowedOrigins`). Windows `sendSync` = #548 typed throw until a #1307 revision admits a second carriage.

### 4.3 Custom scheme (§2.4a)
- FACT (`icorewebview2customschemeregistration.md`, Win32 `1.0.1587.40`): "This allows the WebView2 app to be able to handle WebResourceRequested event for requests with the specified scheme and be able to navigate the WebView2 to the custom scheme." ; "Once the environment is created, the registrations are valid and immutable throughout the lifetime of the associated WebView2s' browser process" ; "any WebView2 environments sharing the browser process must be created with identical custom scheme registrations, otherwise the environment creation will fail." ; properties `SchemeName`, `TreatAsSecure` ("only effective when HasAuthorityComponent is also set to `true`"), `HasAuthorityComponent` ("If … `false`, URIs with this scheme will have an opaque origin similar to a data URI"), `AllowedOrigins` ("If the list is empty, no cross-origin request to this scheme is allowed." ; "POST requests always contain an Origin header, therefore AllowedOrigins must be set for even for same origin POST requests."); "The registered custom schemes also participate in CORS and adheres to CSP."
- FACT: `SetVirtualHostNameToFolderMapping` serves "static files on disk … not possible to dynamically modify this local content" (`concepts/working-with-local-content.md`); Zettlr's `safe-file` serves arbitrary absolute paths, so it is a `WebResourceRequested` producer, not a folder mapping.
- Decision: the static host-owned scheme table (#471) is mandatory on WebView2 (environment-time, immutable); `safe-file` is registered with `HasAuthorityComponent=false` and `AllowedOrigins` empty — supported by the corpus (refuter grep at v4.9.0: no `fetch(`/`XMLHttpRequest` to `safe-file://`; the 11 consumers are `src`/`href`/path mapping, i.e. no-origin subresources which the doc says "are always allowed"). Any future sync endpoint must not share that registration.

### 4.4 Close veto (§2.5)
- FACT (`MicrosoftDocs/win32 … winmsg/wm-close.md`): "An application can prompt the user for confirmation, prior to destroying a window, by processing the WM_CLOSE message and calling the DestroyWindow function only if the user confirms the choice." ; "By default, the DefWindowProc function calls the DestroyWindow function to destroy the window."
- FACT (`Shutdown/wm-queryendsession.md`): "If any application returns zero, the session is not ended." ; "If your application must block or postpone system shutdown, use the ShutdownBlockReasonCreate function." Electron: `before-quit` "will not be emitted if the app is closed due to a shutdown/restart of the system or a user logout" (Windows). Owner #1188 (tao `CloseRequested` veto, no second WndProc).

### 4.5 Dialogs (§2.6a)
- FACT (`shell/common-file-dialog.md`): "IFileDialog and its descendant interfaces inherit from and extend IModalWindow. Show takes as its only parameter the handle of the parent window." ; `HRESULT_FROM_WIN32(ERROR_CANCELLED)` on cancel. `TaskDialogIndirect` (`commctrl/nf-commctrl-taskdialogindirect.md`): "creates, displays, and operates a task dialog." ; "The parent window should not be hidden or disabled when this function is called." ; `Controls/task-dialogs-overview.md`: "Task dialogs require the single-threaded apartment (STA) model." INFERENCE: both return results by out-parameter, so they block the calling thread in a modal loop (which still pumps). Owner #1189 (W5), consuming #478's async broker shape; Zettlr needs no sync dialog.

### 4.6 Theme and accent (§2.7a)
- FACT (`winrt-api … uisettings_getcolorvalue`): "Returns the color value of the specified color type." ; `UIColorType.Accent:5 — The accent color.` ; `ColorValuesChanged`: "Occurs when color values have changed." (thread UNKNOWN).
- FACT (Chromium `ui/native_theme/os_settings_provider_win.cc`): reads `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` `AppsUseLightTheme` (`in_dark_mode_ = !apps_use_light_theme`) and `StartWatching` with one-shot re-arm. No official Microsoft reference page for the value was located (UNKNOWN). Electron 44.5.0 commit `ea1a2714` fixed Windows accent reporting. Owner #1186 (W4).
- Electron doc inconsistency (FACT): `systemPreferences.getAccentColor()` heading has no platform tag, its body says "only available on macOS 10.14 Mojave or newer", while `accent-color-changed` is tagged `_Windows_ _Linux_`; Zettlr calls it inside try/catch, so no decision is needed for the proof.

### 4.7 File watching (§2.8a)
- FACT (Bun `main` `src/runtime/node/win_watcher.rs`): "Windows-only filesystem watcher backed by libuv `uv_fs_event_t`." ; "UV_FS_EVENT_RECURSIVE only works for Windows and OSX" ; on overflow "Forward `(event, null)` to every handler like node". Release correspondence UNKNOWN (branch tip).
- FACT (chokidar `5.0.0` README): "v5 is out. Makes package ESM-only and increases minimum node.js requirement to v20." ; "Recursive watching is always supported" ; "chokidar will initiate watchers recursively for everything within scope of the paths". Owner #1355 Windows arm.

## 5. Resulting per-OS proof cells (macOS cells on #1354; Linux and Windows cells on their lane tickets under #1358, not on #1354)

| Cell | macOS | Linux | Windows |
|---|---|---|---|
| activation | first (all tickets) | after #1125 T2/T3, #1127, #1128 | after #1188, #1184/#1307 approval, #1187 |
| launch | #456 chrome (hiddenInset) | non-native chrome: `frame:false` + WCO (`titleBarOverlay.color`/`symbolColor` tagged `_Windows_ _Linux_`; the option itself untagged), `icon` | same WCO |
| open | #478 sheet | GTK3 `GtkFileChooserNative` (portal-first) / GTK4 `GtkFileDialog` | `IFileOpenDialog` (#1189) |
| edit | #546/#547 | same shared contracts over the WebKitGTK bridge (#1127) | same over route D (#1184) |
| save | Cmd+S (#556) | Ctrl+S (menu blueprint win32 on linux) | Ctrl+S |
| unsaved-close | veto → async `showMessageBox` → close | `app.quit()` without `preventDefault` on last window: Electron observable UNKNOWN until recorded (D-Z12) | same as Linux |
| sendSync (6 sites) | #401 arms after the origin precondition | WebKit sync scheme path (source FACT), Linux arm, same precondition or CORS-enabled scheme | typed throw (#548) |
| safe-file | WKURLSchemeHandler | `register_uri_scheme` | `CustomSchemeRegistration` + `WebResourceRequested` |
| theme boot call | #1356 | not called (darwin-only) | not called |
