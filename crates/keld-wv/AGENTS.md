# keld-wv — adds root AGENTS.md

`docs/architecture/05-webview-and-native.md`; `docs/research/library/host-platforms/06-webview-reality.md`; trait: `src/engine.rs`.

- Deny `unsafe_op_in_unsafe_fn`; each unsafe block needs `// SAFETY:`. Windows folders/files/volumes/windows/ACL/WebView2/process: `OpenProcess` is SYNCHRONIZE-only. One live `RegisterWaitForSingleObject`, replaced only after checked `UnregisterWaitEx` success; pool callbacks only post tao `EventLoopProxy`. GH1188 `src/webview2/{mod,browser_instance,controller_custody,managed_windows,view_build,view_build/resources,view_host}.rs`: `WaitForSingleObject`/`ProcessFailed`, controller creation/settings/`Close`, `IsWindow`, focus subscriptions/activation, visibility/bounds. macOS `wkwebview/{macos_profile,macos_bridge}.rs` only (KEL-142 bridge): WK store/config, CFRunLoop, boot sysctl, self-PID proc info, Keld metadata and parent ACL reads (`acl_get_fd_np`, `acl_get_entry`, `acl_get_tag_type`, `acl_free`), rejecting `ACL_EXTENDED_ALLOW` and read errors. Debug `profile-test-hooks`: host camera/mic reads, public sheets with owned UI-thread blocks; GH1188 `src/webview2/managed_test_control/page_readback.rs` fixed `ExecuteScript` reads only. GH478 wkwebview/macos_bridge.rs: owned main-thread CFRunLoop::perform_block with kCFRunLoopCommonModes only. No module-wide allow. SecCode: `keld-core/AGENTS.md`.
- Engine/window mutations MUST stay on tao UI thread; I/O/pool threads MUST NOT touch platform handles.
- `WebEngine` trait changes require design review. Backends MUST use it; new methods need a live backend in the same PR.
- Platform quirks MUST cite OS/version/source; revert uncited workarounds.
- Linux MUST probe/apply GPU safe mode before GTK/WebKit; no user env setup. Pure `detect_gpu_safe_mode`; `prepare_gpu_safe_mode_process` exact-self-reexecs before non-repeatable state with explicit argv/envp for NVIDIA proprietary + Wayland on WebKitGTK ≤2.54 (`WEBKIT_DISABLE_DMABUF_RENDERER=1`), failing closed. Report `degraded-rendering` via `gpu_safe_mode().is_degraded()`. Upstream [#9394](https://github.com/tauri-apps/tauri/issues/9394), [#14924](https://github.com/tauri-apps/tauri/issues/14924).
- Cross-engine diffs MUST enter baseline matrix (polyfill pack + doctor); no silent workarounds.
- Tests MUST follow `.agents/testing.md`.
- Camera/mic MUST use `keld-guard` (`web.camera`/
  `web.microphone`) as host-minted `Principal::Webview`. Agents MUST NOT evaluate capture as
  `Principal::AppProcess`: `/app` media grants would reach all webviews,
  including remote/other windows. If unminted, deny (`KELD-GUARD007`);
  never fall back to AppProcess. v0 `evaluate` denies webview principals
  (`KELD-GUARD006`) until window-level grants exist; retain denial. Per backend:
  - macOS 12+ wry MUST set `with_permission_handler`;
    wry auto-grants new media requests when absent. Older debug hosts lose
    that delegate; oldest-OS proof is open (`src/media.rs`).
  - Linux (wry interim): WebKitGTK 2.52.6 and wry 0.56.1 default-deny
    unhandled new requests; this does not prove Keld evaluated the right principal/manifest ([source](https://webkitgtk.org/reference/webkit2gtk/stable/class.UserMediaPermissionRequest.html)); explicit callback provenance is mandatory.
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
