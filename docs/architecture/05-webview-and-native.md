# keld-wv & keld-native — Engine Layer and Native APIs

## 1. keld-wv: our own webview layer (wry-informed, not wry-bound)

Decision: **build `keld-wv` as Keld's own thin binding layer** over WKWebView (objc2),
WebView2 (windows-rs + WebView2 COM), and WebKitGTK (webkit6/gtk4 bindings), behind a
`WebEngine` trait — with wry/tao kept as reference implementations and their issue
trackers mined as a quirks catalog.

Why not just use wry: (a) Keld needs first-class hooks wry doesn't prioritize —
scheme-streaming as the bulk IPC lane, principal identity per navigation, pre-load
script atomicity guarantees, engine policy switching (system|pinned per platform),
multi-webview composition with native surfaces (Electrobun's OOPIF direction); (b) the
compat layer needs `webContents`-grade control (navigation interception, window.open
handling, print, zoom, find) that means touching platform APIs directly anyway; (c) the
host is prebuilt — we don't need wry's "works in any downstream cargo build" constraint.
What we keep from wry: its hard-won platform workarounds (documented per-commit),
its custom-protocol design shape, and tao's event-loop patterns. This is "from scratch"
the way wry itself is from scratch: direct platform bindings, ~15–20k LOC, no engine forks.

```rust
pub trait WebEngine: Send {
    fn create(&mut self, spec: &WebviewSpec, host: HostHooks) -> Result<WebviewId, WvError>;
    fn navigate(&mut self, id: WebviewId, target: NavTarget) -> Result<(), WvError>;
    fn eval(&mut self, id: WebviewId, script: ScriptRef<'_>, cb: EvalCallback);
    fn post(&mut self, id: WebviewId, frame: ControlFrame) -> Result<(), WvError>;   // control plane
    fn register_scheme(&mut self, scheme: &str, handler: SchemeHandler) -> Result<(), WvError>;
    fn set_bounds(&mut self, id: WebviewId, rect: Rect, anchor: Anchor);
    fn devtools(&mut self, id: WebviewId, action: DevtoolsAction);
    fn destroy(&mut self, id: WebviewId);
}
```

Backends: `wkwebview` (macOS), `webview2` (Windows), `webkitgtk` (Linux) always
compiled; `cef` behind a feature flag, loaded as a runtime-selected backend when the
app's engine policy says `pinned` (CEF binaries fetched at *build* time by `keld-pack`,
never at user runtime). Verso/Servo tracked as a fifth backend the day embedding
stabilizes — the trait is the insurance policy.

Linux resilience (research/06): GPU-driver probe at startup (NVIDIA + Wayland + WebKitGTK
version matrix) → auto-apply safe-mode (`WEBKIT_DISABLE_DMABUF_RENDERER` equivalent set
programmatically before engine init, not by asking users to export env vars), emit a
structured `degraded-rendering` event apps can surface, and record it in `keld doctor`.

## 2. Renderer bridge contract (`window.keld`)

Injected pre-load on every webview, identical across engines (the polyfill pack rides
the same injection):

```ts
window.keld = {
  invoke(channel, payload, opts?): Promise<unknown>,   // control plane
  send(channel, payload): void,
  on(channel, handler): Unsubscribe,
  stream(channel, payload): ReadableStream,            // bulk via keld:// under the hood
  meta: { platform, engine, appVersion, principal },
};
```

`@keld/web` wraps this with generated typed clients; `@keld/electron`'s renderer shim
implements `ipcRenderer`/`contextBridge` over it.

## 3. keld-native: the native API surface (built from scratch, guarded)

Every module is: Rust implementation per platform → guard check → kipc channel → typed
TS in `@keld/api` → (optionally) an Electron-compat facade. No module ships without all
three OS implementations or an explicit documented gap.

| Module | Scope v0.x | Electron facade |
|---|---|---|
| `window` | create/close/show/focus/bounds/state, titlebar styles (hiddenInset/overlay), vibrancy/mica/acrylic, always-on-top, parenting/modal, fullscreen | `BrowserWindow` |
| `menu` | app menu, context menus, accelerators, role items (macOS roles complete) | `Menu`, `MenuItem` |
| `tray` | icon, tooltip, menu, click events, template images | `Tray` |
| `dialog` | open/save/message/error, scoped-path grants flow into fs scopes | `dialog` |
| `notify` | notifications with actions, macOS UNUserNotification / WinRT toast / libnotify | `Notification` |
| `clipboard` | text/html/image/files, change polling where OS allows | `clipboard` |
| `shortcut` | global shortcuts with conflict detection | `globalShortcut` |
| `screen` | displays, scale factors, work areas, DPI change events | `screen` |
| `power` | on-battery, suspend/resume, idle time, power-save blockers | `powerMonitor`, `powerSaveBlocker` |
| `shell` | open URL/path, reveal in folder, trash, beep | `shell` |
| `fs+` | scoped fs ops via broker (watch included — notify crate), drag-out, recent docs | (compat maps Node fs only when sandbox off) |
| `secrets` | keychain/DPAPI/libsecret | `safeStorage` |
| `deeplink` | protocol registration + single-instance handoff | `app.setAsDefaultProtocolClient` |
| `autostart` | login items / registry / .desktop | `app.setLoginItemSettings` |
| `dock/taskbar` | badge, progress, bounce, jump lists, thumbbar | `app.dock`, `setProgressBar` |
| `capture` (Tier 3) | window/screen capture via ScreenCaptureKit / Graphics.Capture / PipeWire | `desktopCapturer` |

Implementation notes: objc2/objc2-app-kit on macOS (no deprecated cocoa crate);
windows-rs on Windows; gtk4 + ashpd (XDG portals — file dialogs, notifications,
screencast on Wayland) on Linux, with portal-first behavior so sandboxed formats
(flatpak/snap) work day one.

## 4. Native extension path (`keld-ext`) — Rust as a plugin, never a requirement

- Plugins are Rust cdylibs against a **stable C ABI** (`keld_ext_abi` versioned struct
  table, abi_stable-style vtables; no Rust ABI exposure). Loaded by the host at startup
  from `keld.config.ts` `extensions: []`, each with a declared capability set the guard
  enforces (a plugin cannot exceed its manifest).
- Each plugin exports kipc channels — so a plugin is *just more typed API*, visible to
  TS with generated types like any built-in.
- Distribution: prebuilt per-platform npm packages (same pipeline as the host), or
  `keld ext build` for local ones (the only place an app repo ever needs cargo, and
  only plugin authors pay it).
- This replaces both Electron's native-module ABI treadmill (N-API modules keep working
  via Bun; host plugins have their own stable ABI) and Tauri's "your app is a Rust
  project" tax.
