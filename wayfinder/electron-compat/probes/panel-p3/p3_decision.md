# PANEL-P3 (gyldlab/keld#420): authority classification and first-proof verdict for draw.io

Scope: drawio-desktop `src/main/*.js` @ `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`, webapp `drawio/` @ `24b76c2cbd55d88e354042e8d329a2e4708972bc` (both FACT, from `git log -1` in each tree). The target is macOS under the explicit legacy profile (`legacy_sandbox_off`, from kel74-compat-evidence-schema.md §4.1). All evidence paths are relative to `scratchpad/p3/`.

Evidence used:
- `trace.jsonl`: packaged, update on. 518 lines.
- `trace.unpackaged.jsonl`
- `negctl-disable-update/trace.jsonl` and `negctl-disable-update/run.log`: new in this session. This is the same harness with `DRAWIO_DISABLE_UPDATE=true`. Its copy of `recorder.cjs` was edited in one line so that APP points at the real app tree.
- `negctl-codeurl-mismatch/run.log`: new in this session. Here the app is reached through a symlinked path, so the loaded URL does not match `codeUrl`.
- The source, read line by line.

The rows in `static_inventory.tsv` were re-checked against the source (see §g).

**VERDICT: config + 1 edit** (detail in §d).

## (a) Pre-evaluation descriptor set: what must exist before `src/main/electron.js` evaluates

**A1. Module resolution.**
- FACT: `electron.js:5-6` statically imports 12 names: `Menu, shell, dialog, session, screen, ClipboardItem, clipboard, nativeImage, nativeTheme, ipcMain, app, BrowserWindow`. If the module lacks any one of them, Bun fails at link time with `SyntaxError: Export named 'BrowserWindow' not found` (`harness/baseline-error.txt`; trace_summary §6).
- FACT: dependencies import `electron` themselves, in three forms:
  - ESM named imports: `electron-dl/index.js:3-8` imports `{app, BrowserWindow, shell, dialog}`.
  - ESM default imports: `electron-store/index.js:3,6` (`{app, ipcMain, shell}`), `electron-context-menu/index.js:2` and `electron-is-dev/index.js:1`.
  - CJS `require`: electron-log, and electron-updater (`require("electron").autoUpdater/net/session/app/Notification`).
- FACT: `electron-is-dev/index.js:3-4` throws `Not running in an Electron environment!` when the resolved `electron` is a string. That is exactly what the npm `electron` package is. So the alias must cover `node_modules`, and `@keld/electron` must provide a default export as well as the named ones.
- INFERENCE: tsconfig `paths` remaps CJS `require('electron')` inside `node_modules` (X02 alias-probe, Bun 1.4.2). Whether it also remaps ESM `import` from packages that have `exports` maps is UNKNOWN (X02 unknowns). For that reason a `package.json` `npm:` alias is listed as the primary fix-it (C1).
- FACT: the preload calls `require("electron")` for `contextBridge` and `ipcRenderer` (`electron-preload.js:1-4`). It reads `process.argv`, which carries `additionalArguments` (`--initial-adaptive-colors=`, `:14-15`), and it reads `process.type` and `process.versions` (`:125-129`).

**A2. Runtime.**
- FACT: Bun without `--no-install` auto-installed `electron@44.6.0` and started downloading the Electron binary. That is network access plus a third-party install script at evaluation time (trace_summary §6 item 6, §10).
- The role launch must therefore disable auto-install.

**A3. Session facts that must be answered synchronously at import or ready time.**

Read at import, as FACT:

| Fact | Value | Evidence |
|---|---|---|
| `app.isPackaged` | `true` | seq 18/19 electron-log, seq 40 electron-is-dev |
| `app.getVersion()` | `31.7.0` | `package.json:3`; seq 32 electron-updater |
| `app.name` | `draw.io` | `package.json:2` |
| `app.getPath('userData')` | `~/Library/Application Support/draw.io` | seq 41; electron-store constructor at `electron.js:30` |
| `process.type` | `browser` | trace_summary §5 "process shape" |
| `process.versions.electron` | (set) | trace_summary §5 |
| `process.resourcesPath` | (set) | trace_summary §5 |
| `process.platform` | `darwin` | |

After `ready`, the app reads these synchronously:
- FACT: `nativeTheme.shouldUseDarkColors` (seq 111), `screen.getAllDisplays/getPrimaryDisplay` (seq 112-113), `app.getLocale` (seq 122) and `BrowserWindow.getFocusedWindow` (seq 240).
- FACT: on close, `getSize/getPosition/isMaximized/isFullScreen` (seq 468-471).
- INFERENCE: the facade has to answer these from a state mirror that the host pushes. A Promise would break `createWindow` (`:717-741`).

The argv shape also matters:
- FACT: `args.js:199` drops two leading tokens.
- FACT: `electron.js:1059-1061` prepends `null` unless `process.defaultApp == true`.
- INFERENCE: if Bun's raw `[bun, entry]` argv meets a packaged-style `defaultApp`, the entry path becomes a file argument. It is then blessed at `:1801-1803` and opened as a diagram.
- So the facade must present `process.argv = [execPath, ...userArgs]` with `defaultApp` unset, or `defaultApp = true` with two leading tokens.

FACT: the event order is `will-finish-launching` before `ready`. `open-file` is registered inside the `will-finish-launching` handler (trace_summary §9.7).

**A4. Environment keys.**
- FACT: `DRAWIO_DISABLE_UPDATE=true` (declared). It is read at seq 47.
- FACT: with that key set, `DRAWIO_NO_SILENT_UPDATE` and `/.flatpak-info` are short-circuited and never evaluated (`electron.js:74-78`; absent from the negctl trace).
- These keys must be absent: `DRAWIO_ENV` (`:181`), `ELECTRON_IS_DEV` (`electron-is-dev:8`), `DEBUG` and `NODE_DEBUG`.
- `HOME` and `TMPDIR` must be present (env-paths, seq 36-37).
- FACT: `debug/src/node.js:124` enumerates the whole env (seq 27). The role env is therefore fully visible to dependency code and must be composed by the host.

**A5. userData paths.**
- FACT: `~/Library/Application Support/draw.io/` is created at import (`mkdirSync`, seq 46). It holds `config.json` plus atomic `config.json.tmp-*` siblings with fsync, chown and rename (seq 95-99).
- FACT: the boot code probes `Local Storage/leveldb` (seq 83).
- FACT: `.updaterId` and `~/Library/Logs/draw.io/main.log` are touched only on the update-on path (seq 171-229; both absent from the negctl trace).
- FACT: `config.json` is re-read on every `store.get`, 32 reads in the run (trace_summary §9.3). This is a cost under legacy, not a blocker.

**A6. IPC channels that exist before the first window.**

| Registered by | When | Channels |
|---|---|---|
| electron-log (library) | import | `__ELECTRON_LOG__`: `on` (seq 21) and `handle` (seq 22) |
| electron-store (library) | import | `electron-store-get-data`: `on`, answered through `event.returnValue` (seq 43; `electron-store/index.js:25-27`) |
| src/main | import | `export` (seq 72), `rendererReq` (seq 73) |
| src/main | boot | `openDevTools, newfile, isModified-result, app-load-finished`(once), `toggleSpellCheck, toggleStoreBkp, toggleGoogleFonts, toggleFullscreen, checkForUpdates, zoomIn, zoomOut, resetZoom` (seq 106-148) |
| src/main | later, as literal `once` registrations | `saveAndClose-result` (`:913`), `draftRemoved` (`:927`), `export-finalize` (`:3144`), `import-success/error` (`:1452-1453`), `svg-data, render-finished, export-error, xml-data, xml-data-error` (`:3349-3385`) |

INFERENCE: `ipcMain` must accept registrations before `ready`. The grant generator must enumerate every registration, library ones included, so that it can exclude them by name.

**A7. Other listeners and objects that must exist at import.**
- FACT: `app.on` listeners registered at import include electron-context-menu's `browser-window-created` (seq 51). The facade has to emit that event for every window.
- FACT: the native `autoUpdater.on('error'|'update-downloaded')` is called at import from `MacUpdater` (seq 33-34). It must exist as a recorded no-op.

**A8. Web resource identity.**
- FACT: the window loads `file://<codeDir>/index.html?<query>` (seq 123).
- FACT: every app handler gates on `senderFrame.url.startsWith(codeUrl)` (`validateSender`, `:694-700`; 21 call sites).
- FACT (negative control run in this session): when the frame URL differed from `codeUrl` (symlinked path), the app's own `onBeforeRequest` blocked `index.html`. Every `rendererReq` was dropped silently, 22 harness steps failed, and there were 0 fs events in open/save (`negctl-codeurl-mismatch/run.log`).
- So the host-derived `senderFrame.url` must be exactly the `file://` URL under the KEL-79 resource root (F03 note: file:// is served through the KEL-79 adapter).
- FACT: `will-navigate` is always prevented (`:2192-2194`). Host-initiated `loadURL` must not surface as `will-navigate` (F03 note).

## (b) Committed denominator (draft)

There are three `keld.compat.denominator/v1` documents. Each document carries exactly one `kind` (kel74 spec §4.2). All three share:
- panel: `product`
- `corpus_id`: `drawio-desktop.2edf9fb.drawio.24b76c2`
- `corpus_sha256`: UNKNOWN until the corpus manifest pinning both object ids is committed.

Every evidence record uses `authority_profile: legacy_sandbox_off`, `artifact.platform: macos`, `arch: aarch64`, Bun `1.4.2`. An external process checks each observable: it hashes files, enumerates windows through the host registry, and monitors sockets with `nettop -p`. None of the checks uses the app's own API.

| kind | operation_id / oracle_id | Binary observable (pass iff all hold) | Negative control (must flip the cell to fail) |
|---|---|---|---|
| install | `drawio.install` / `installed-tree-digest` | The tree installed from the built artifact has the content digest recorded in the evidence `artifact.sha256`, AND the install makes 0 outbound connections. | 1. Remove `drawio/src/main/webapp/index.html` from the staged tree: digest mismatch.<br>2. Re-enable Bun auto-install with the `electron` devDependency present: connection observed (trace_summary §10). |
| activation | `drawio.activation` / `args-obj-handshake` | On launch, the role import completes with no uncaught error; exactly 1 window loads `file://…/webapp/index.html`; main receives `el:app-load-finished` from that window's host-minted principal; main sends `args-obj` with `args:[]` (shape of seq 186); 0 outbound connections. | 1. Drop `el:app-load-finished` from `windows.main.channels`: guard deny, no `args-obj`.<br>2. Remove the alias: `Export named 'BrowserWindow' not found`.<br>3. Unset `DRAWIO_DISABLE_UPDATE`: connect to github.com (seq 216). |
| primary_workflow | `drawio.open` / `fixture-bytes` | After the fixture is picked in NSOpenPanel, the `mainResp` for `readFile` carries bytes whose SHA-256 equals the fixture's (seq 262-266), and the editor shows the fixture's page. | Renderer-issued `readFile` of the never-picked `fixtures/secret.txt` returns `path not authorised` (seq 294). Also: removing `el:rendererReq` makes open fail. |
| primary_workflow | `drawio.edit` / `draft-sibling` | After a scripted edit inserts vertex `KELD-EDIT-<nonce>`, `.$<name>.dtmp` exists in the fixture's directory and contains the nonce. The original file's bytes are unchanged. | Open with no edit: no `.dtmp` is written (shows the draft is caused by the edit). |
| primary_workflow | `drawio.save` / `saved-file-nonce` | After Cmd+S: the file on disk contains the nonce and parses as `<mxfile>`; `.$<name>.bkp` exists with the pre-save SHA-256 (`enableStoreBkp` defaults true, `:201`); `.dtmp` is removed (seq 350). | Renderer-issued `saveFile` to an unpicked path is refused and the target stays absent (seq 436-450). |
| primary_workflow | `drawio.close-unsaved-prompt` / `prompt-cancel-discard` | With unsaved edits, close shows exactly 1 sheet with `[Save, Cancel, Discard Changes]` (`:862-866`). Cancel: the window survives and the file is unchanged. A second close followed by Discard: the window is destroyed, `.dtmp` is deleted, the file is unchanged, the process stays alive with 0 windows and no `app.quit` (trace_summary §9.8). | 1. Close with no edits: no sheet, immediate destroy (`:941-944`).<br>2. Remove `el:isModified-result`: the window is never destroyed, and the host does not auto-close on timeout (map decision). |

Parked as full_feature: Save from the prompt, Save As, export, CLI `-x`, `open-file`, `second-instance`, Open Recent.

## (c) Fix-it list (`keld migrate` output)

| # | Edit | Label | Reason |
|---|---|---|---|
| C1 | `package.json`: `"electron": "npm:@keld/electron@<pin>"`. Also write the tsconfig `paths` and the bunfig mapping (arch 04 §3 v0 note). | config-only | A1. The dependency imports and the ESM default export must resolve to the facade, not to a path string. |
| C2 | Role launch with Bun auto-install disabled. Remove the `electron` binary devDependency and its postinstall from the runtime package. | config-only | A2 |
| C3 | `keld.config.ts`:<br>- `app {id:"com.jgraph.drawio.desktop", name:"draw.io", version:"31.7.0"}` (`electron-builder-linux-mac.json:2`, `package.json:2-3`)<br>- entry `src/main/electron.js`<br>- runtime bun `1.4.x`<br>- explicit authority profile `legacy`<br>- role env `DRAWIO_DISABLE_UPDATE=true` | config-only | A4. negctl: 31/31 harness steps pass, 0 net, 0 `.updaterId`, 0 log writes. The upstream build switch `npm run sync -- disableUpdate` (`sync.cjs:29`) is rejected: it rewrites a source file. |
| C4 | `keld.permissions.jsonc` `windows.main.channels` = `el:rendererReq, el:app-load-finished, el:isModified-result, el:saveAndClose-result, el:draftRemoved, el:newfile, el:toggleSpellCheck, el:toggleStoreBkp, el:toggleGoogleFonts, el:toggleFullscreen, el:zoomIn, el:zoomOut, el:resetZoom, el:export, el:export-finalize`. **Excluded:** `el:__ELECTRON_LOG__`, `el:electron-store-get-data`, `el:openDevTools` (devtools off in release, 03-security.md §4.3) and `el:checkForUpdates` (the handler returns null when disabled, `:1894`). | config-only | This is the intersection of the main-side registrations (A6) with what the renderer sends (`ElectronApp.js:474-2649`), minus library and dev channels. The first-proof subset is `rendererReq, app-load-finished, isModified-result`. |
| C5 | `windows.main.web.csp` = the policy at `electron.js:1003-1005`, host-injected. `webRequest.onHeadersReceived` becomes a recorded no-op. | config-only (needs a Keld CSP vocabulary that accepts a reviewed custom policy) | `file://` responses cannot be header-rewritten on WKWebView (F05 note). |
| C6 | KEL-79 resource root = `drawio/src/main/webapp`. `onBeforeRequest({urls:['file://*']})` becomes a recorded no-op. | config-only | A8 |
| C7 | `keld.build.ts`: keep `src/main/` and `drawio/src/main/webapp/` in their relative layout. Do not bundle in a way that relocates `import.meta.url`. | config-only | `codeDir`, the preload path and `appBaseDir` are all `__dirname`-relative (`:207-211`, `:722`). |
| C8 | `keld.compat.ts`: native `autoUpdater` and `webRequest` set to recorded no-op. No `sendSync` policy is needed. | config-only | 0 `sendSync` calls (briefing corpus facts). |
| E1 | `electron.js:876`: `let response = dialog.showMessageBoxSync(mainWindow, {...})` becomes `let response = (await dialog.showMessageBox(mainWindow, {...})).response;` | **source-edit** | The facade cannot honour this call: it needs a synchronous Bun-to-host round trip, and the Bun kipc client cannot block (map decision; `wayfinder/electron-compat/probes/park-probe/main.ts`). The worker-owned single-link transport is not live. The enclosing handler is already `async` (`:851`), and `modifiedModalOpen` (`:853`, `:858`) guards re-entry. The upstream comment about a crash (`:875`) is about Linux under Electron and does not apply to a macOS Keld host (INFERENCE). Removable once a sync transport lands. |

Not counted in N: upstream hardening (see f1). It is advisory, because the first proof does not need it.

## (d) VERDICT

**config + 1 edit (N = 1; the edit is E1).**

Decisive evidence:
1. FACT: the env key alone makes the updater inert. With `DRAWIO_DISABLE_UPDATE=true`, all 31 harness steps pass with 0 net, 0 child_process, 0 `.updaterId` and 0 log-file events (`negctl-disable-update/run.log`, `trace.jsonl`). In the update-on run, by contrast, the trace shows seq 171-229.
2. FACT: none of the 43 fs rows the first proof touches is unscopable. 24 are dialog-minted and 19 are declared-role (`authority_classification.tsv`). The first proof touches 0 net and 0 child_process rows. So under legacy no fs/net/process edit is needed, and draw.io's own blessed-path gate still refuses all 6 out-of-scope probes (seq 294-450).
3. FACT: the close cell calls `dialog.showMessageBoxSync` (seq 486, `:876`). That is the only Electron API the first proof touches whose contract is a synchronous host round trip. Facade work and config can provide every other member.

## (e) Unknowns and falsifiers

- **U1, stub fidelity (UNKNOWN).** The fake `electron` was never compared against Electron 44:
  - its stubs return plausible values;
  - dialogs resolve immediately from queues;
  - `showMessageBoxSync` returns synchronously by construction, so the harness proves nothing about whether a sync call is feasible;
  - the `will-finish-launching`/`ready` order is assumed;
  - the "renderer" is a script.

  Falsifier: an oracle recording of real Electron 44.4.5 running the same 4 steps diverges in members touched, ordering or the return shape of any member listed in A3/A7.
- **U2, KEL-79 URL identity (UNKNOWN).** If the resource adapter cannot present `senderFrame.url` as `file://<codeDir>/…`, then `validateSender` and `onBeforeRequest` silently disable the app (see the negctl-codeurl-mismatch run). N becomes 2, with an extra edit to `codeUrl`.
- **U3, sync transport.** If the worker-owned single-link sync transport passes before the first proof, E1 is unnecessary and N = 0, which is "config-only under legacy".
- **U4, sync getters after `ready` (UNKNOWN whether planned).** If the facade cannot mirror the screen, nativeTheme, locale, focused-window and geometry getters synchronously, `createWindow` (`:717-741`) and `rememberWinSize` (`:824-831`) need edits, so N ≥ 3.
- **U5, ESM alias through `node_modules` (UNKNOWN; X02).** If the `npm:` alias fails under `bun install`, C1 falls back to tsconfig `paths`. This is still config.
- **U6, async swap ordering (INFERENCE).** The awaited dialog opens an interleaving window that the sync dialog did not have. It is guarded by `modifiedModalOpen`, and the macOS sheet is window-modal. Falsifier: during the prompt, a second close or Cmd+Q produces a second sheet or destroys the window.
- **U7.** These are not proven:
  - CSP vocabulary support for draw.io's policy (C5);
  - WKWebView localStorage persistence under `file://`. No cell depends on persistence across launches.
  - the Keld macOS install path that the install cell needs (arch 06).

## (f) Security findings that change scoping

- **f1. The preload forwards any channel.**
  - FACT: `electron-preload.js:111-114` exposes `sendMessage(action,args)` as a raw `ipcRenderer.send` with a channel name the renderer chooses. `registerMsgListener` and `listenOnce` (`:104-121`) subscribe to any channel.
  - Scoping consequence: the renderer can reach every main-side registration, library channels included. So the Keld window grant must be an explicit enumerated list (C4), never `el:*`. A missing grant or missing handler is a deny.
  - Owners: the Keld window channel-grant vocabulary in keld-guard (03-security.md:172-173; not live) and the `keld migrate` grant generator (X02 on #391). An upstream allow-list in the preload belongs to jgraph/drawio-desktop. The exact Keld ticket id is UNKNOWN; the first action is to search the #391 children for `el:<channel>`.
- **f2. electron-store and electron-log register unvalidated handlers.**
  - FACT: the `electron-store-get-data` handler (`electron-store/index.js:25-27`) does not validate the sender and returns `{defaultCwd: userData, appVersion}` through `returnValue`.
  - INFERENCE: through draw.io's `send`-only preload that `returnValue` is discarded, so nothing is disclosed today.
  - FACT: electron-log's `__ELECTRON_LOG__` `on`/`handle` (`electron-log/src/main/index.js:16-43`) do not validate either.
  - INFERENCE: a renderer could inject arbitrary entries into `main.log`.
  - Both channels are excluded from C4.
  - Owners: sindresorhus/electron-store and megahertz/electron-log upstream; on the Keld side, the exclude-by-default rule in the grant generator.
- **f3. The updater makes a network call.**
  - FACT: when packaged with updates on, boot sends GET `https://github.com/jgraph/drawio-desktop/releases.atom` with header `x-user-staging-id` (seq 215). The value comes from a persistent per-install id that is written to `userData/.updaterId` (seq 213).
  - FACT: legacy gives the Bun role ambient network, so Keld policy cannot block this call. Only the env key stops it (negctl).
  - FACT: legacy same-user mode refuses direct update (03-security.md:305-306).
  - So `DRAWIO_DISABLE_UPDATE` is mandatory, not optional, for the legacy proof.
  - Owners: the Keld updater adapter / bridge recipe (arch 04 §7; KEL-53 transaction). jgraph owns the header.
- **f4. Supporting findings.**
  - FACT: under legacy the 42 dialog-minted fs rows run with ambient authority. Scoping is enforced only by draw.io's in-app gate (`assertReadablePath`/`assertWritablePath`, `:3787-3866`). The strict-profile owner is KEL-78.
  - FACT: `backup-file.js:11-24` creates `.drawio-bkp-<uuid>.tmp` without checking draw.io's own gate. A future strict dialog grant therefore needs a same-directory sibling-create right, not a single-path grant (INFERENCE).
  - Bun auto-install (A2) is owned by the role launcher (KEL-75/KEL-96).

## (g) Class counts and inventory corrections

**Class counts.** `authority_classification.tsv` has 131 rows, deduplicated by (site, api):

| Subset | Rows | declared-role | dialog-minted | unscopable |
|---|---|---|---|---|
| All rows | 131 | 80 | 45 | 6 |
| fs/net/child_process rows | 86 | 39 | 42 | 5 |

The 5 unscopable fs/net/child_process rows are:
- config-declared read paths (`:513`);
- the two TOFU localStorage migrations (`:577`, `:632`);
- persisted Open Recent (`:229`);
- `execFile('fc-list')` through `PATH` (`:4413`).

Renderer-supplied `openExternal` (`:4355`) is the 6th unscopable row overall.

Three `fs.constants` reads are excluded because they are not authority operations: `export-files.js:9,12` and `electron.js:3432`.

**Inventory corrections.** FACT:
- `static_inventory.tsv` mislabels these contexts:
  - `1974` is the update-interval menu;
  - `2235/2339/2366` are updater events;
  - `3295` is a print error;
  - `4132` is `saveFile`;
  - `1114-1801` are CLI export and argv handling, not `ipcMain_export`.
- It omits:
  - all 25 electron-store sites;
  - backup-file's tmp/rename;
  - `open-file`;
  - the `setWindowOpenHandler` → `openExternal` path (`:2219`).

Also FACT: the requested `03-permissions.md` does not exist. The grant-shape spec is `docs/architecture/03-security.md`.
