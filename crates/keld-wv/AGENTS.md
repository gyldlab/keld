# keld-wv — adds root AGENTS.md

Spec: `docs/architecture/05-webview-and-native.md`; platform truth: `docs/research/library/host-platforms/06-webview-reality.md`; v0 trait: `src/engine.rs`.

- FFI follows root's unsafe rules. Windows: folders/files/volumes/windows/ACL/WebView2/process; `OpenProcess` is SYNCHRONIZE-only. At most one live `RegisterWaitForSingleObject`, replaced only after checked `UnregisterWaitEx` success; pool callbacks only post tao `EventLoopProxy`. GH1188 `src/webview2/{mod,browser_instance,controller_custody,managed_windows,view_build,view_build/resources,view_host}.rs`: `WaitForSingleObject`/`ProcessFailed`, controller creation/settings/`Close`, `IsWindow`, focus subscriptions/activation, visibility/bounds. macOS `wkwebview/{macos_profile,macos_bridge}.rs` only (KEL-142 bridge): WK store/config, CFRunLoop, boot sysctl, self-PID proc info, Keld metadata and parent ACL reads (`acl_get_fd_np`, `acl_get_entry`, `acl_get_tag_type`, `acl_free`), rejecting `ACL_EXTENDED_ALLOW` and read errors. Debug `profile-test-hooks`: host camera/mic reads, public-sheet observation with owned UI-thread blocks; GH1188 `src/webview2/managed_test_control/page_readback.rs` fixed `ExecuteScript` reads only. No module-wide allow. Core SecCode FFI: `keld-core/AGENTS.md`.
- Engine/window mutations MUST stay on tao UI thread; I/O/pool threads MUST NOT touch platform handles.
- `WebEngine` trait changes require design review. Backends MUST use it; new methods need a live backend in the same PR.
- Platform quirks MUST cite OS/version/source; revert uncited workarounds.
- Linux MUST probe and apply GPU safe-mode before GTK/WebKit; never ask users to export env vars. `detect_gpu_safe_mode` is pure; `prepare_gpu_safe_mode_process` exact-self-reexecs before non-repeatable state with explicit argv/envp for NVIDIA proprietary + Wayland on WebKitGTK ≤2.54 (`WEBKIT_DISABLE_DMABUF_RENDERER=1`). Engine init fails closed if skipped. Emit `degraded-rendering`; `gpu_safe_mode().is_degraded()` reports applied state. Upstream: [#9394](https://github.com/tauri-apps/tauri/issues/9394), [#14924](https://github.com/tauri-apps/tauri/issues/14924).
- Cross-engine diffs MUST enter the baseline matrix; polyfill pack + doctor smooth them. No silent workarounds.
- Tests MUST follow `.agents/testing.md`.
- Camera/microphone MUST use `keld-guard` (`web.camera`/`web.microphone`)
  as the host-minted requesting `Principal::Webview`.
  MUST NOT use `Principal::AppProcess`: `/app` grants would reach other windows.
  No minted principal: deny `KELD-GUARD007`, with no AppProcess fallback.
  v0 `evaluate` MUST keep `KELD-GUARD006` denial of webview principals until
  window grants exist. Per backend:
  - macOS 12+ (wry interim): agents MUST NOT omit wry `with_permission_handler`;
    wry auto-grants new media requests when absent. Older debug hosts lose
    that delegate; oldest-OS proof is open (`src/media.rs`).
  - Linux (wry interim): WebKitGTK 2.52.6 and wry 0.56.1 default-deny an
    unhandled new request, but that fallback is not proof Keld evaluated the
    right principal/manifest ([source](https://webkitgtk.org/reference/webkit2gtk/stable/class.UserMediaPermissionRequest.html)); explicit callback provenance stays mandatory.
  - Windows (direct COM, KEL-65): agents MUST register guarded
    `add_PermissionRequested` and deny-all `add_NewWindowRequested` before
    minting `GuardInstalled`; first navigation MUST require that proof. WebView2
    defaults to media prompts and unguarded popups. Agents MUST NOT bypass
    either handler or the first-navigation proof (KEL-168).
- Architecture 01 section 5 **first paint** is KEL-64's external double-rAF image
  beacon on a pre-spawn monotonic clock, never `PageLoadEvent::Finished`,
  `WindowBuilder::build`, titled HWND or `window-visible` (KEL-62/KEL-64).
  `PageLoadEvent::Finished` means navigation completion. macOS `startup`
  records `nav_finished` for construction diagnostics; dump with
  `KELD_STARTUP_TRACE=1`. Never compile `startup` on Linux/Windows (CodeRabbit #10).
