> **Status:** non-normative planning reference for the Electron compatibility program (map #391). Architecture 04 and the [compat scoreboard](compat-scoreboard.md) stay normative; counts are from the Electron v44.4.5 `electron-api.json` release asset.

# Electron Compatibility Reference for KELD

**Baseline:** Electron v44.4.5  
**Purpose:** Architecture, compatibility planning, conformance testing, and migration strategy for KELD.

---

## 1. Executive Summary

Electron compatibility is much larger than a simple list of API names.

Using Electron v44.4.5's official generated API model as the baseline:

| Metric | Count |
|---|---:|
| Top-level entities | **180** |
| Modules | **33** |
| Classes | **44** |
| Structures | **102** |
| Custom elements | **1** |
| Callable entries | **1,004** |
| Events | **294** |
| Runtime properties | **264** |
| Structure/config fields | **566** |
| Raw documented member entries | **2,128** |
| Direct runtime-facing contracts after detected inheritance overlap | **1,378** (historical estimate ≈1,383) |
| Direct documented members overall after detected inheritance overlap | **1,941** (historical estimate ≈1,946) |
| Explicitly read-only properties | **104** |
| Callable parameter slots | **781** |
| Event argument slots | **460** |

> **Important:** The earlier number `75` should not be used as the Electron API total. That was only a documentation-search match count. The generated API model is the stronger planning baseline.

---

## 2. What This Document Is For

This document is intended to be a shared engineering reference for KELD.

Use it to:

- understand the actual size of Electron's public compatibility surface;
- explain the scope to developers in simple terms;
- decide which compatibility work belongs in KELD core versus app-specific ports;
- design conformance tests;
- track platform-specific implementation work;
- reason about whether an Electron app can be migrated with no changes, small changes, or a maintained fork.

The central design principle is:

> **Clone Electron's observable contract, not Electron's internal implementation.**

KELD can have a completely different architecture internally—Rust host, Bun runtime, WKWebView/WebView2/WebKitGTK—while still exposing Electron-compatible behavior to applications.

---

## 3. Source and Counting Method

| Item | Reference |
|---|---|
| Electron baseline | **v44.4.5** |
| Primary source | Official generated `electron-api.json` release artifact |
| Supporting source | Generated `electron.d.ts` |
| Counting approach | Top-level entities + documented methods/events/properties/constructors/structure fields |
| Inheritance treatment | Raw totals include inherited members; direct totals remove detected repeated inherited entries |
| Pinned input | `https://github.com/electron/electron/releases/download/v44.4.5/electron-api.json`, sha256 `0f309fd2513694ca932e44fc87750d72e43a76a434a1e5fbaecc5fa4b933a4d6` |
| Counting steps | (1) count top-level entries by `type`: Module, Class, Structure, Element. (2) Emit one row per documented member (methods, events and properties at module, static and instance level, constructors, and structure fields): **2,128 raw**. (3) Drop rows a subclass repeats from its parent (BaseWindow 182, View 2, InputEvent 2, MouseInputEvent 1 = 187): **1,941 direct**: 563 directly introduced structure fields and **1,378** callable/observable members. The ≈1,946 / ≈1,383 figures elsewhere in this document are historical estimates that counted 182 inherited duplicates; 1,941 / 1,378 are the current census. |
| Reproduction | `wayfinder/electron-compat/compat-matrix.tsv` and its generator on branch `research/electron-compat-map` emit one row per member from the pinned asset |

Electron's generated API data is more useful than manually counting documentation pages because it models the public API structure directly.

---

# 4. Top-Level API Categories

## 4.1 Modules — 33

A **module** is like a toolbox or service namespace.

Examples:

- `app`
- `dialog`
- `clipboard`
- `ipcMain`
- `ipcRenderer`
- `protocol`
- `screen`
- `shell`
- `globalShortcut`
- `nativeTheme`

Conceptually:

```ts
app.quit()
dialog.showOpenDialog()
clipboard.readText()
ipcMain.handle(...)
```

### What KELD must provide

KELD must expose the same application-facing module or a compatibility facade with matching:

- method names;
- arguments;
- return values;
- asynchronous behavior;
- defaults;
- errors;
- events;
- platform-specific behavior.

---

## 4.2 Classes — 44

A **class** is a stateful object.

Examples:

- `BrowserWindow`
- `BaseWindow`
- `WebContents`
- `Session`
- `Tray`
- `Menu`
- `MenuItem`
- `DownloadItem`
- `Notification`
- `UtilityProcess`

Example:

```ts
const win = new BrowserWindow(...)
win.show()
win.focus()
win.close()
```

`win` has its own state and lifecycle.

### What KELD must provide

For classes, KELD must reproduce:

- construction;
- object lifetime;
- methods;
- properties;
- events;
- destruction behavior;
- ownership relationships;
- lifecycle semantics.

---

## 4.3 Structures — 102

A **structure** is primarily a typed configuration/result object.

Example:

```ts
new BrowserWindow({
  width: 1200,
  height: 800,
  resizable: true
})
```

The object passed to `BrowserWindow` is a structure-like configuration contract.

Structures define:

- field names;
- optional vs required fields;
- defaults;
- accepted values;
- nested objects;
- return/result shapes.

### Why this matters

Even if KELD implements `BrowserWindow`, apps can still break if the options object behaves differently.

---

## 4.4 Custom Elements — 1

Electron also exposes a browser/renderer-side custom element surface.

This means compatibility is not limited to the main process. Some behavior exists inside the renderer/browser environment as well.

---

# 5. Runtime API Surface

## 5.1 Callable Entries — 1,004

A callable entry is something application code can invoke.

| Callable category | Count |
|---|---:|
| Module methods | **279** |
| Class constructors | **25** |
| Class instance methods | **616** |
| Class static methods | **19** |
| Element methods | **65** |
| **Total** | **1,004** |

There are **781 declared parameter slots** across these callables.

### Important

Implementing a function name is not sufficient.

For example:

```ts
app.quit()
```

Compatibility includes:

- what happens before quitting;
- what events fire;
- event order;
- whether quitting can be cancelled;
- what happens to windows;
- process exit behavior;
- OS-specific details.

---

# 6. Events — 294

Electron is heavily event-driven.

| Event category | Count |
|---|---:|
| Module events | **57** |
| Class instance events | **202** |
| Element events | **35** |
| **Total** | **294** |

There are **460 declared event argument slots**.

Examples:

```ts
app.on('before-quit', ...)
win.on('close', ...)
webContents.on('did-finish-load', ...)
```

Events are one of the biggest compatibility traps because applications can depend on exact ordering and timing.

---

# 7. Runtime Properties — 264

Runtime properties are values exposed while the application is running.

Examples can conceptually look like:

```ts
object.someProperty
object.someProperty = value
```

Properties may be:

- writable;
- read-only;
- platform-specific;
- dynamically changing;
- deprecated;
- experimental.

The generated model contains **104 explicitly read-only properties**.

---

# 8. Structure / Configuration Fields — 566

Structures contain **566 documented fields** in the raw generated model.

These represent options and returned data.

They matter because compatibility must include:

- accepted fields;
- field types;
- optionality;
- defaults;
- nested objects;
- validation;
- ignored/unsupported values.

---

# 9. Total Documented Member Surface — 2,128

Raw documented members:

```text
1,004 callable entries
+ 294 events
+ 264 runtime properties
+ 566 structure fields
= 2,128
```

This does **not** mean KELD needs 2,128 completely separate implementations.

A large amount of behavior can be expressed through shared primitives.

---

# 10. Inheritance: Why Raw Counts Overstate the Work

Electron's generated documentation repeats inherited members on child classes.

Example:

```text
BaseWindow
    ↑
BrowserWindow
```

`BrowserWindow` inherits much of `BaseWindow`.

### Raw vs directly introduced members

| Surface | Raw | Inherited duplicates | Directly introduced |
|---|---:|---:|---:|
| Class instance methods | 616 | 126 | **490** |
| Class instance events | 202 | 28 | **174** |
| Class instance properties | 186 | 22 | **164** |
| Class static methods | 19 | 3 | **16** |
| Class static properties | 14 | 0 | **14** |
| Structure fields | 566 | 3 | **563** |

After removing detected inheritance overlap:

- direct runtime-facing contracts: **1,378** (historical estimate ≈1,383)
- direct documented members overall: **1,941** (historical estimate ≈1,946)

These are much more useful planning figures.

---

# 11. Example: BrowserWindow vs BaseWindow

Raw model:

### BaseWindow

- 130 instance methods
- 29 events
- 23 properties

### BrowserWindow

- 141 instance methods
- 34 events
- 23 properties

But `BrowserWindow` shares approximately:

- **126 methods**
- **28 events**
- **22 properties**

with `BaseWindow`.

So KELD should not implement these as unrelated APIs.

A better architecture is:

```text
KELD native window primitive
        ↓
BaseWindow compatibility
        ↓
BrowserWindow-specific layer
```

This is a major example of why compatibility work should be designed around reusable capabilities rather than API-count brute force.

---

# 12. Largest Electron Surfaces

## Major classes

| Surface | Raw documented members | Meaning for KELD |
|---|---:|---|
| BrowserWindow | **204** | Large public surface, but much inherited from BaseWindow |
| WebContents | **195** | Renderer lifecycle/navigation/browser integration is a major subsystem |
| BaseWindow | **187** | A strong reusable native window primitive gives huge leverage |
| Session | **88** | Cookies, network state, permissions, storage, cache, protocols |
| Tray | **36** | OS-native status/tray integration |
| Notification | **30** | OS notification integration |
| DownloadItem | **28** | Browser/download lifecycle |
| WebFrameMain | **25** | Renderer frame integration |
| MenuItem | **21** | Native menu behavior |

## Major modules

| Module | Raw members |
|---|---:|
| `app` | **115** |
| `process` | **29** |
| `systemPreferences` | **28** |
| `webFrame` | **27** |
| `protocol` | **18** |
| `powerMonitor` | **16** |
| `screen` | **12** |
| `ipcRenderer` | **11** |
| `autoUpdater` | **10** |
| `ipcMain` | **9** |

---

# 13. Platform-Specific API Surface

Raw tagged member entries:

| Platform / status | Count |
|---|---:|
| macOS-specific | **281** |
| Windows-specific | **189** |
| Linux-specific | **50** |
| Mac App Store-specific | **1** |
| Deprecated | **46** |
| Experimental | **34** |

These are raw tag occurrences, so inherited members can be repeated.

The important architectural point is that Electron exposes one public abstraction while mapping it to different native operating-system behavior.

KELD must do the same.

---

# 14. KELD Platform Mapping

## macOS

```text
KELD app
  ↓
Electron-compatible facade
  ↓
Rust host
  ↓
AppKit / Cocoa
  ↓
WKWebView
```

Examples of native responsibility:

- windows;
- menus;
- Dock;
- file dialogs;
- clipboard;
- notifications;
- native shortcuts;
- OS lifecycle.

---

## Windows

```text
KELD app
  ↓
Electron-compatible facade
  ↓
Rust host
  ↓
Win32 / Windows APIs
  ↓
WebView2
```

Examples:

- native windows;
- taskbar;
- tray;
- global shortcuts;
- dialogs;
- clipboard;
- notifications;
- process integration.

---

## Linux

```text
KELD app
  ↓
Electron-compatible facade
  ↓
Rust host
  ↓
GTK / Linux desktop APIs
  ↓
WebKitGTK
```

Linux is more fragmented because desktop environments and protocols vary.

KELD will need a capability-based implementation rather than assuming every Linux environment behaves identically.

---

# 15. Semantics: The Most Important Compatibility Layer

**API signature** tells you what you can call.

**Semantics** tells you what actually happens.

Example:

```ts
win.close()
```

Questions KELD must match:

- Which event fires first?
- Can the app cancel closing?
- When is the renderer destroyed?
- When does `closed` fire?
- Are object references still valid?
- What happens if `close()` is called twice?
- What happens on each operating system?

## Semantic compatibility includes

| Area | Example |
|---|---|
| Event ordering | `close` before `closed` |
| Timing | sync vs next event-loop turn |
| Errors | throw vs Promise rejection |
| Defaults | omitted window options |
| Lifecycle | destroyed object behavior |
| Return values | exact result shapes |
| Serialization | IPC object transfer |
| Cancellation | `preventDefault()` behavior |
| OS differences | focus/menu/tray behavior |

KELD should use **differential conformance tests**:

```text
Run operation in Electron
        ↓
record observable behavior

Run same operation in KELD
        ↓
compare result
```

---

# 16. Native Addons

Electron applications often use compiled native modules.

Examples include:

- PTY/terminal modules;
- filesystem watchers;
- SQLite bindings;
- OS keychain helpers;
- native keyboard APIs;
- Windows registry/process helpers.

A package can successfully `require()` under Bun and still fail at runtime.

KELD therefore needs multiple strategies:

1. **Reuse** — addon already works under Bun.
2. **Upstream** — fix a general Bun Node/N-API/libuv compatibility issue.
3. **Broker** — replace privileged native behavior with a Rust service.
4. **Isolate** — run an opaque addon in a restricted worker/process.
5. **Reject** — explicitly unsupported in a strict security profile.

For KELD, PTY is a good example:

```text
node-pty compatible facade
        ↓
KELD IPC
        ↓
Rust PTY broker
        ↓
openpty / ConPTY
```

---

# 17. Browser Engine Compatibility

Electron bundles Chromium.

KELD intends to use:

- macOS → WKWebView
- Windows → WebView2
- Linux → WebKitGTK

Most web standards are shared, but exact behavior is not identical.

Potential differences include:

- Service Workers;
- custom schemes;
- iframe origins;
- clipboard permissions;
- drag/drop;
- IME/focus;
- WebGL/WebGPU;
- codecs;
- media capture;
- DevTools/CDP;
- storage;
- WebRTC;
- offscreen rendering.

These are often called **browser quirks**.

KELD does **not** need to reproduce every browser quirk in existence.

It needs to reproduce or adapt the quirks that real target Electron applications depend on.

---

# 18. OS Integration

Electron also abstracts native desktop functionality.

Important surfaces include:

- menus;
- tray/status icons;
- dialogs;
- clipboard;
- global shortcuts;
- notifications;
- screen capture;
- safe storage;
- keychain/credential systems;
- Dock/taskbar;
- jump lists;
- protocol handlers;
- file associations;
- power monitoring;
- updater;
- native window decorations.

KELD should expose Electron-compatible behavior while implementing these internally with native Rust/OS integrations.

---

# 19. API Parity Is Necessary — But Not Sufficient

Even perfect coverage of the 1,378 direct runtime contracts would not automatically mean every Electron application works unchanged.

Real migration compatibility requires several layers.

| Layer | What must match |
|---|---|
| Electron APIs | methods, events, properties, lifecycle |
| Semantics | exact behavior, timing, ordering, errors |
| Node/Bun | Node APIs, streams, process behavior, N-API/libuv/V8 expectations |
| Native addons | compiled module compatibility or safe replacements |
| Browser engine | Chromium-dependent assumptions |
| OS integration | menus, tray, shortcuts, dialogs, clipboard, etc. |
| Packaging | build/install/signing/notarization |
| Update system | download, activation, recovery, rollback |
| Security | authority/permission behavior |
| Undocumented behavior | real behaviors applications depend on |

---

# 20. Recommended KELD Compatibility Maturity Model

| Level | Meaning | Developer experience |
|---|---|---|
| **L0 — Surface** | API/type exists | App imports/compiles |
| **L1 — Basic behavior** | Happy-path works | Simple apps begin to run |
| **L2 — Semantic parity** | Timing/events/errors/defaults tested | Normal apps need very small changes |
| **L3 — Ecosystem parity** | Native/browser/OS/package layers covered for a declared corpus | Large apps become viable |
| **L4 — North-star parity** | VS Code-class workloads pass declared conformance | Strong minimal-change migration claim becomes defensible |

An API should **not** be marked complete merely because its method name exists.

It should be marked complete only after behavioral conformance passes.

---

# 21. What Easy Migration Should Mean

## Zero-rewrite migration (migration effort, not the L0 API maturity level)

```text
Existing Electron source
        ↓
change runtime/package/config
        ↓
KELD
```

No meaningful source rewrite.

---

## Minor-adaptation migration (migration effort, not the L1 API maturity level)

Small mechanical changes:

- import/package substitutions;
- config changes;
- a few documented adapters.

No architecture redesign.

---

## Port

A maintained application-specific patch set is required.

Example:

```text
Upstream app
  + large patchset
  + custom process changes
  + custom webview changes
  + custom native integration
```

That is a port, not a drop-in migration.

---

# 22. Strategic Goal for KELD

KELD should continuously move reusable migration work from apps into the framework.

Bad model:

```text
App A solves PTY
App B solves PTY
App C solves PTY
```

Good model:

```text
KELD solves PTY once
        ↓
App A
App B
App C
all reuse it
```

The same applies to:

- IPC;
- utility processes;
- window lifecycle;
- permissions;
- native addons;
- protocols;
- browser adapters;
- updates;
- packaging;
- native OS integration.

This is how migration cost falls over time.

---

# 23. If KELD Supports All of This, Does Migration Become Easy?

## Short answer

**Yes — substantially easier.**

If KELD reproduces the complete **observable Electron contract**, including:

- API surface;
- semantics;
- lifecycle;
- events;
- Node/Bun behavior;
- native addon behavior or facades;
- browser compatibility;
- OS integrations;
- packaging/update behavior;

then many Electron applications can move from:

```text
heavy maintained port
        ↓
small compatibility patch
        ↓
near drop-in migration
```

This is conceptually similar to Bun's Node-compatibility strategy.

Bun does not use Node's internal implementation.

It attempts to reproduce enough of the Node-visible contract that existing Node applications and packages can run.

KELD can pursue the same strategy with Electron.

---

# 24. Important Limitation

There will never be a meaningful permanent claim of:

> “Every future Electron application works forever with zero changes.”

Electron itself evolves.

So do:

- Chromium;
- Node;
- operating systems;
- native addons;
- app-specific behavior.

Compatibility should therefore be versioned and evidence-based.

Example:

```text
KELD Electron Compatibility Profile

Electron baseline: 44.x
macOS: tested
Windows: tested
Linux: tested

Corpus:
✓ App A
✓ App B
✓ App C
✓ VS Code snapshot X

Conformance:
✓ BrowserWindow
✓ IPC
✓ Session
...
```

That is a credible compatibility claim.

---

# 25. Developer Checklist

For every Electron API/member, track:

- Electron entity;
- member name;
- type: method/event/property/etc.;
- main/renderer/utility process;
- platform availability;
- experimental/deprecated status;
- KELD implementation status;
- KELD subsystem;
- semantics status;
- conformance test;
- OS tests;
- app evidence;
- known differences;
- security implications.

Suggested status values:

```text
UNSUPPORTED
SCAFFOLDED
PARTIAL
BEHAVIOR_MATCH
CONFORMANCE_PASS
CORPUS_VERIFIED
```

These describe implementation maturity, not scoring. They map onto the existing scoreboard and evidence terms (`compat-scoreboard.md`, KEL-74) as follows: `UNSUPPORTED` → scoreboard *unsupported*; `SCAFFOLDED` and `PARTIAL` → *compatible with caveats* (evidence `fail` or `unknown` on the uncovered cells); `BEHAVIOR_MATCH`, `CONFORMANCE_PASS` and `CORPUS_VERIFIED` → *compatible* once every mapped cell is `pass` (or `waived` with a recorded reason). The scoreboard terms stay authoritative.

---

# 26. Recommended Next Artifact

The next engineering artifact should be a living compatibility matrix.

Recommended columns:

| Column |
|---|
| Electron entity |
| Member |
| Kind |
| Process |
| Platform |
| Deprecated/experimental |
| KELD status |
| KELD subsystem |
| Implementation owner |
| Electron oracle test |
| KELD result |
| App evidence |
| Known differences |
| Notes |

This matrix would turn this static scope reference into an actionable engineering scoreboard.

---

# 27. Final Mental Model

Do **not** think:

> “Electron has 2,128 things, so we must write 2,128 unrelated features.”

Think:

```text
Electron public contract
        ↓
1,378 direct runtime contracts
        ↓
collapse into reusable capability families
        ↓
windowing
IPC
processes
session/network
storage
protocols
permissions
native integration
browser adaptation
packaging/update
        ↓
Rust + Bun + system webviews
```

The architectural objective is:

> **Implement the generic compatibility problem once inside KELD so application developers do not repeatedly solve it themselves.**

That is the path from **porting** to **migration**.


---

# 28. Performance Expectations: What We Can Quote and What We Cannot

This section separates **measured evidence**, **internal engineering targets**, and **hypothetical full-product outcomes**.

The distinction is critical.

> Rust does not automatically make the entire desktop application faster. Rust primarily helps KELD reduce native-host overhead, improve control over memory and process ownership, and provide predictable low-level performance. Renderer performance still depends on WKWebView, WebView2, WebKitGTK, application JavaScript, CSS, GPU behavior, and the workload itself.

KELD's performance advantage is expected to come from the **combined architecture**:

```text
lean Rust native host
+ Bun for JavaScript runtime work
+ system webviews instead of bundling an entire Chromium runtime
+ brokered native services
+ purpose-built IPC
+ fewer framework-owned heavyweight processes
```

not from Rust alone.

---

## 28.1 Current Measured Evidence

These are measurements that have actually been observed. They are **not forecasts for a completed KELD product**.

### Windows native/main-process memory

In a Windows direct-COM benchmark session:

| Framework | Main/native process RSS |
|---|---:|
| KELD | **19,552 KB** |
| Electron | **89,140 KB** |

In that specific session, KELD's native host used approximately:

- **78% less main-process RSS**
- Electron's main process used approximately **4.6×** as much memory

Important limitation:

> This is **main/native process memory**, not total application memory. In the broader Windows process-tree measurements, Electron still had lower total RSS.

Therefore the correct public statement is:

> **“In the cited Windows native-host benchmark, KELD's main process used about 78% less RSS than Electron. This does not yet imply lower total application memory.”**

Source: `gyldlab/keld-benches` and KELD `docs/engineering/budget-scoreboard.md`.

---

## 28.2 Current First-Paint Evidence

Windows direct-COM host diagnostic:

| Framework | First-paint proxy |
|---|---:|
| Electron | **275 ms** |
| KELD | **469 ms** |
| Tauri | **479 ms** |

Electron was faster in this benchmark.

Therefore KELD should **not** currently claim that it starts faster than Electron overall.

The useful conclusion is:

> The current KELD host is already lean in native-process memory, but startup optimization is still unfinished.

This is important evidence against assuming that “Rust automatically means faster startup.”

---

## 28.3 Current Bun / VS Code Extension-Host Evidence

A separate KELD research experiment tested the compiled VS Code extension-host bootstrap under Bun and Electron-as-Node.

30/30 sequential runs reached the tested Ready → Initialized path.

| Runtime | Median Ready | Median Initialized | Sampled RSS | Child CPU |
|---|---:|---:|---:|---:|
| Bun canary `1.4.0-canary.1+39fde480e` | **80.915 ms** | **87.564 ms** | **73,280 KiB** | **0.10 s** |
| Electron 42.8.0 as Node 24.18.0 | **177.699 ms** | **187.588 ms** | **128,208 KiB** | **0.22 s** |

For this deliberately narrow workload, Bun canary was measured at approximately:

- **53.3% sooner to Initialized**
- **42.8% lower sampled child RSS**
- **54.5% lower child CPU time**

This is promising evidence for KELD's Bun-based process model.

However:

> This was an empty-extension/bootstrap workload. It does **not** prove that a complete VS Code or every Electron application will receive the same improvement.

Source: `0monish/keld-research/campaigns/vscode/reports/20-vscode-on-keld.md`.

---

# 29. KELD Engineering Performance Targets

KELD already has architecture-level budgets. These are useful as the intended target envelope for a mature implementation.

| Metric | KELD target | Historical Electron comparison used in KELD planning |
|---|---:|---:|
| Installer, Bun runtime | **≤ 20 MB** | **85–150 MB** |
| Installer, no bundled runtime | **≤ 6 MB** | Electron comparison not directly equivalent |
| Cold start → first paint | **≤ 300 ms** | **1–3 s** survey/reference range |
| Idle RSS, one window (host + guardian + Bun; webview engine helpers excluded) | **≤ 90 MiB** | **150–300 MB** historical Electron reference; process scope not stated, so not directly comparable |
| KIPC small-message p99 | **≤ 100 µs** | approximately ms-class comparison |
| KIPC bulk/shared-memory throughput | **≥ 1 GB/s** | no direct Electron baseline in current scoreboard |
| One-line JS update patch | **≤ 50 KB** | full-installer style comparison in planning baseline |
| `keld dev` cold → window | **≤ 2 s** | no direct Electron comparison |

These are **targets**, not all currently achieved product measurements.

---

# 30. Approximate Improvement Ranges If the Targets Are Achieved

These are the approximate numbers that may be used internally for planning **if clearly labelled as target-based estimates**.

## 30.1 Installer / Download Size

KELD Bun-runtime target:

```text
≤ 20 MB
```

Planning Electron baseline:

```text
85–150 MB
```

Approximate reduction:

- vs 85 MB: **~76.5% smaller**
- vs 150 MB: **~86.7% smaller**

Reasonable planning statement:

> **Target: approximately 75–87% smaller packaged footprint than the historical Electron baseline, if KELD's ≤20 MB Bun-runtime budget is achieved.**

The biggest reason is not Rust by itself. It is that KELD intends to use the operating system's web engine instead of shipping a complete private Chromium engine with every application.

---

## 30.2 Cold Startup / First Paint

KELD target:

```text
≤ 300 ms
```

Historical planning baseline for Electron:

```text
1–3 seconds
```

If both baselines were achieved under an equivalent workload:

- 1.0 s → 0.3 s = **70% less startup latency**
- 3.0 s → 0.3 s = **90% less startup latency**

Equivalent latency ratio:

- approximately **3.3× to 10× lower time-to-first-paint**

Reasonable planning statement:

> **Long-term target: roughly 70–90% lower cold-start latency versus the historical 1–3 second Electron reference range.**

But this must **not** be presented as a current benchmark result.

Current Windows evidence still has Electron ahead on first paint.

---

## 30.3 Memory

KELD architecture target:

```text
≤ 90 MiB
```

Historical Electron reference:

```text
150–300 MB
```

The KELD target covers the host, guardian and Bun processes and excludes webview engine helpers; the Electron reference does not state its process scope. No percentage comparison is derived until both sides are measured over the same process tree.

Important caveat:

Total memory depends heavily on:

- renderer workload;
- WebView helpers;
- number of windows;
- GPU processes;
- extensions;
- Bun children;
- native services;
- operating system;
- engine implementation.

The current strongest direct Electron evidence is narrower:

> **~78% lower native/main-process RSS in the cited Windows session.**

Do not convert that 78% figure into a total-app-memory claim.

---

## 30.4 JavaScript Runtime / Extension Host

The VS Code bootstrap experiment provides real evidence that Bun can materially reduce runtime overhead for some Node-shaped workloads.

Observed narrow-workload gains:

| Metric | Observed improvement |
|---|---:|
| Time to Initialized | **53.3% sooner** |
| Sampled child RSS | **42.8% lower** |
| Child CPU | **54.5% lower** |

For KELD planning, it is reasonable to hypothesize that Bun-based main/utility/extension processes could provide meaningful improvements in this range for compatible workloads.

It is **not** reasonable to guarantee that every Electron application's JS workload becomes 50% faster.

---

## 30.5 IPC

KELD target:

```text
small-message p99 ≤ 100 µs
```

Current Windows Bun↔Rust diagnostic:

```text
fresh p99: ~101.6 µs
warm p99:  ~100.5 µs
```

So the current diagnostic is already close to the architecture target.

The result is not yet publication-grade under the project's own benchmark rules because the required sample/session protocol was not fully met.

If KELD can reliably sustain ~100 µs p99 where an equivalent framework path is ~1 ms, that would represent approximately an **order-of-magnitude latency reduction** for that specific IPC path.

Never apply that ratio to whole-app performance.

---

# 31. What Rust Actually Improves in KELD

Rust is especially valuable in the following areas:

| Area | Why Rust helps |
|---|---|
| Native host memory | No GC runtime is required for the host itself; ownership is explicit |
| CPU overhead | Native compiled code can keep framework bookkeeping cheap |
| Predictable latency | Fewer GC pauses in host-controlled services |
| Process supervision | Strong ownership and explicit lifecycle control |
| IPC implementation | Efficient binary framing, zero-copy/shared-memory options |
| Native services | PTY, filesystem, process launch, watchers, security brokers |
| Security | Memory safety plus explicit authority boundaries |
| Binary size | A focused host can remain much smaller than a bundled browser runtime |

Rust does **not** automatically speed up:

- React rendering;
- DOM/CSS layout;
- WebView JavaScript;
- GPU rendering;
- arbitrary application algorithms;
- network servers;
- extension code.

Those workloads depend on the renderer engine, Bun, application code, and operating system.

---

# 32. Recommended Performance Claim Levels

KELD should maintain three distinct categories of performance statements.

## A. Measured

Example:

> “KELD's native host used about 78% less main-process RSS than Electron in the cited Windows direct-COM session.”

This is a benchmark fact.

## B. Architecture Target

Example:

> “KELD targets ≤300 ms cold-start-to-first-paint and ≤90 MiB one-window idle RSS.”

This is a design objective.

## C. Target-Based Projection

Example:

> “If KELD reaches its ≤20 MB packaging budget, that would be approximately 75–87% smaller than the historical 85–150 MB Electron range.”

This is a calculated projection, **not a measurement**.

These categories must never be blended.

---

# 33. Practical Performance Envelope for a Mature KELD

If KELD reaches full Electron-compatibility while also meeting its architecture budgets, the intended product envelope would approximately be:

| Dimension | Mature KELD planning envelope | Confidence today |
|---|---:|---|
| Packaged size | **~75–87% smaller** than historical Electron range | Medium as target, not yet product-proven |
| Native/main-process memory | **~78% lower** already seen in one Windows host benchmark | Strong for that exact benchmark only |
| KELD-owned-process idle RSS budget (host + guardian + Bun; webview engine helpers excluded) | **≤ 90 MiB target** | Electron reference scope is unspecified; no percentage comparison |
| Cold start | **~70–90% lower latency target** | Low today; Electron currently wins cited Windows first-paint test |
| Bun runtime bootstrap | **~53% faster initialization observed** in narrow VS Code extension-host PoC | Strong for that PoC only |
| Bun process RSS | **~43% lower observed** in same PoC | Strong for that PoC only |
| Bun process CPU | **~55% lower observed** in same PoC | Strong for that PoC only |
| IPC | **~100 µs p99 target**, near that in current Windows diagnostic | Promising but diagnostic |
| Update delta | **≤50 KB target** for one-line JS change | Design target; updater incomplete |

---

# 34. Recommended Public / Developer Wording

A safe statement today is:

> **KELD is designed to reproduce Electron's application-facing contract on a lean Rust host, Bun processes, and system webviews. Early evidence shows a substantially smaller native-host memory footprint and promising Bun runtime efficiency, while startup and total-process performance are still being validated. The architecture targets a ≤20 MB Bun-runtime package, ≤300 ms first paint, ≤90 MiB one-window idle RSS for the host, guardian and Bun processes (excluding webview engine helpers), and ~100 µs small-message IPC p99. These are engineering targets, not blanket claims that KELD already beats Electron on every metric.**

A future statement—only after full-product benchmarks validate it—could become:

> **For the verified application corpus, KELD delivers Electron-compatible behavior with substantially lower packaging, memory, and startup overhead while preserving near-drop-in migration.**

---

# 35. How Performance and Compatibility Fit Together

The most important goal is **not maximum benchmark speed at the cost of compatibility**.

The target is:

```text
Electron-compatible observable behavior
        +
lean Rust host
        +
Bun runtime compatibility
        +
system webviews
        +
efficient IPC/native services
        =
migration without carrying Electron's full architectural overhead
```

If KELD implements the Electron contract but becomes just as heavy as Electron, the project misses a major purpose.

If KELD is extremely fast but requires every Electron app to be rewritten, it also misses the migration goal.

The intended outcome is:

> **high compatibility + lower framework overhead + native security boundaries + minimal application rewrite**


---

# 36. Where KELD Can Potentially Produce Strong Benchmark Numbers

The most promising performance opportunities are not all equally proven. They should be separated into **already measured**, **architecture-backed targets**, and **new benchmark opportunities**.

| Area | Potential KELD advantage | Evidence level | Why KELD may do well |
|---|---|---|---|
| Native/main-process memory | **~78% lower already observed in one Windows session** | Measured, narrow | Lean Rust host instead of Electron's heavier main-process stack |
| Packaged size | **~75–87% smaller target projection** | Architecture target | System webview means KELD does not need to bundle a complete Chromium engine |
| Total runtime memory | **≤ 90 MiB KELD-owned processes (target)** | Target, not yet proven; no Electron percentage until same-scope measurements | Lean host + Bun + system webview; helper processes still matter |
| Cold start / first paint | **~70–90% lower target projection** | Target only; current Windows result does not lead Electron | Smaller framework boot path may help once optimized |
| Bun process bootstrap | **53.3% sooner to Initialized in narrow VS Code PoC** | Measured, narrow | Bun startup/runtime behavior in that workload |
| Bun child RSS | **42.8% lower in same PoC** | Measured, narrow | Lower runtime overhead in tested extension-host bootstrap |
| Bun child CPU | **54.5% lower in same PoC** | Measured, narrow | Less CPU accumulated in tested bootstrap path |
| IPC latency | **~100 µs p99 architecture target** | Diagnostic is already near target | Purpose-built KIPC, binary framing, host-mediated topology |
| Update delta | **≤50 KB target for one-line JS change** | Architecture target | Content/delta-oriented updater can avoid redistributing the whole package |
| Native host executable | Very small host binary already demonstrated | Measured host lane | Focused Rust host without embedded browser runtime |

## 36.1 Update Size Could Become a Major Headline Metric

If a one-line JavaScript change can be delivered in **≤50 KB**, then compared only against an 85–150 MB full-installer baseline:

- 50 KB vs 85 MB is approximately **99.94% smaller**
- 50 KB vs 150 MB is approximately **99.97% smaller**

This is potentially a very strong user-facing metric:

> “Tiny application changes can ship as tiny updates instead of redownloading the application.”

However, this must not be presented as a blanket Electron comparison because Electron applications can also use differential/blockmap update systems depending on tooling.

The fair benchmark should compare:

```text
same application
same one-line change
same signing/update policy
actual bytes transferred
KELD updater vs selected Electron updater
```

---

# 37. Additional Areas Worth Benchmarking Next

These areas could produce strong numbers, but KELD currently needs proper measurement before quoting percentages.

## 37.1 Idle CPU and CPU Wakeups

Measure:

- CPU % while app is idle;
- context switches;
- timer wakeups;
- background process activity;
- CPU time over 5/30/60 minutes.

Why it matters:

A framework that wakes the CPU less often can improve laptop battery life even if RSS is similar.

Do not quote a percentage until the same application is measured against Electron on the same machine.

---

## 37.2 Battery / Energy Consumption

Measure a realistic workload:

```text
open app
idle 10 min
scroll workload
background 20 min
repeat
```

Track:

- package energy impact / platform energy metric;
- CPU time;
- GPU time;
- battery percentage or power draw;
- wakeups.

This could become a more meaningful end-user metric than raw Rust microbenchmarks.

---

## 37.3 Process Count and Baseline Overhead

Measure the number of framework-owned processes for:

- empty app;
- one window;
- two windows;
- utility process;
- crash/guardian/update state.

Also measure memory by role.

KELD's goal should not simply be “fewest processes”; isolation has security value. The useful metric is:

> **overhead per required isolation boundary**

---

## 37.4 Window Creation Latency

Separate whole-app startup from:

```text
request BrowserWindow
→ native window created
→ WebView attached
→ first usable renderer frame
```

This directly tests whether KELD's Rust/native shell gives a responsive desktop feel after the app is already running.

---

## 37.5 IPC Throughput

In addition to small-message latency, benchmark:

- 64 B;
- 4 KiB;
- 64 KiB;
- 1 MiB;
- 16 MiB;

and compare:

- messages/sec;
- bandwidth;
- p50/p95/p99;
- CPU cost per GB transferred;
- copies per transfer.

Shared memory could make large-payload IPC a strong KELD metric if real workloads justify it.

---

## 37.6 Native Service Latency

KELD's Rust brokers can be benchmarked for:

- file stat/open/read;
- process spawn;
- PTY creation;
- watcher event delivery;
- clipboard;
- safe-storage operations;
- dialogs;
- global shortcuts.

The important comparison is end-to-end API latency, not an isolated Rust function benchmark.

---

## 37.7 Crash Recovery and Relaunch

Measure:

```text
renderer crash
→ detection
→ restart/recovery
→ usable UI
```

and:

```text
Bun child crash
→ supervisor detects
→ replacement process authenticated
→ service restored
```

KELD's explicit Rust supervision model may produce strong reliability numbers even when raw speed differences are small.

Useful metrics:

- detection latency;
- recovery latency;
- lost requests;
- state recovery success rate.

---

## 37.8 Memory Growth / Long-Running Stability

Instead of only measuring launch RSS, run applications for:

- 1 hour;
- 8 hours;
- 24 hours.

Measure:

- RSS growth;
- leaked handles;
- process count;
- WebView helper growth;
- native allocation growth.

A lower memory leak slope can be more valuable than a small startup-memory advantage.

---

## 37.9 Multiple-Window Scaling

Measure 1, 2, 5, 10 windows.

Track:

- incremental RSS per window;
- window creation latency;
- process count;
- CPU idle overhead.

This can reveal whether system webviews scale better or worse than Electron's Chromium process model for the target workload.

---

## 37.10 Dev Experience Performance

Benchmark:

- `keld dev` cold start;
- hot reload / refresh latency;
- TypeScript change → visible UI;
- Rust host change → rebuilt app;
- package/build time;
- incremental build time.

The existing architecture goal is:

```text
keld dev cold → window ≤ 2 s
```

Developer-cycle latency can become a major adoption advantage even if end-user runtime performance is only moderately better.

---

# 38. Benchmark Priority for KELD

A practical evidence-building order is:

1. **Total process-tree RSS** — because current host-memory results are strong but do not yet prove total-memory superiority.
2. **Cold start → first paint** — because Electron currently leads the cited Windows session.
3. **Installer/package size** — likely one of KELD's strongest architectural advantages.
4. **Update bytes transferred** — potentially an extremely strong product metric.
5. **Idle CPU / energy** — directly meaningful to laptop users.
6. **IPC latency + throughput** — important for architecture validation.
7. **Window creation latency** — useful user-perceived responsiveness metric.
8. **Multi-window scaling** — tests whether the architecture stays lean under realistic desktop use.
9. **Crash recovery** — turns supervision architecture into a measurable reliability claim.
10. **Developer iteration latency** — important for framework adoption.

The objective should be a **balanced scorecard**, not one synthetic “KELD is X times faster” number.
