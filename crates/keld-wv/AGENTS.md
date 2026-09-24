# keld-wv — adds root AGENTS.md

Spec: `docs/architecture/05-webview-and-native.md`; platform truth: `docs/research/library/host-platforms/06-webview-reality.md`; v0 trait: `src/engine.rs`.

- Backend `unsafe` MUST deny `unsafe_op_in_unsafe_fn`; each block needs `// SAFETY:`. Windows KEL-135 FFI: folders/files/volumes/process/windows/ACL/WebView2. macOS `wkwebview/macos_profile.rs` only: WK store/config, CFRunLoop, boot sysctl, self-PID proc info, Keld metadata and parent ACL reads (`acl_get_fd_np`, `acl_get_entry`, `acl_get_tag_type`, `acl_free`), rejecting `ACL_EXTENDED_ALLOW` and read errors. Debug `profile-test-hooks` may read host camera/mic status and observe public sheets with owned blocks on UI thread. No macOS module-wide allow. Core SecCode FFI: `keld-core/AGENTS.md`.
- Engine/window mutations MUST stay on tao UI thread (later core queue); platform handles MUST NOT be touched on I/O/pool threads.
- `WebEngine` trait changes require design review; backends MUST use its API. No new method until a live backend implements it in the same PR (root YAGNI).
- Platform quirks MUST cite OS, version, source; revert uncited workarounds.
- Linux MUST probe and apply GPU safe-mode before GTK/WebKit; never ask users to export env vars. `detect_gpu_safe_mode` is pure; `prepare_gpu_safe_mode_process` exact-self-reexecs before non-repeatable state with explicit argv/envp for NVIDIA proprietary + Wayland on WebKitGTK ≤2.54 (`WEBKIT_DISABLE_DMABUF_RENDERER=1`). Engine init fails closed if skipped. Emit `degraded-rendering`; `gpu_safe_mode().is_degraded()` reports applied state. Upstream: [tauri-apps/tauri#9394](https://github.com/tauri-apps/tauri/issues/9394), [#14924](https://github.com/tauri-apps/tauri/issues/14924).
- Cross-engine diffs MUST go to the baseline matrix; polyfill pack + doctor smooth. Agents MUST NOT silently paper over them.
- Tests MUST follow repository `.agents/testing.md`.
- Camera/microphone capture MUST go through `keld-guard` (`web.camera` /
  `web.microphone`) as the requesting `Principal::Webview` when the host
  has minted that webview's id. Agents MUST NOT evaluate capture as
  `Principal::AppProcess` — that applies `/app` media grants to every
  webview, including a remote/other window. If the requesting webview
  principal has not been minted yet, deny (`KELD-GUARD007`); do not fall
  back to AppProcess. v0 `evaluate` still denies webview principals
  (`KELD-GUARD006`) until window-level grants exist — that is fail-closed,
  not a reason to present AppProcess. Per backend:
  - macOS 12+ (wry interim): agents MUST NOT omit wry `with_permission_handler`;
    wry auto-grants new media requests when absent. Pinned wry cfg-removes its
    delegate on older debug hosts; oldest-OS proof is open ([source](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/build.rs)).
  - Linux (wry interim): WebKitGTK 2.52.6 and wry 0.56.1 default-deny an
    unhandled new request, but that fallback is not proof Keld evaluated the
    right principal/manifest ([source](https://webkitgtk.org/reference/webkit2gtk/stable/class.UserMediaPermissionRequest.html)); explicit callback provenance remains mandatory.
  - Windows (direct COM, KEL-65): agents MUST register guarded
    `add_PermissionRequested` and deny-all `add_NewWindowRequested` before
    minting `GuardInstalled`; first navigation MUST require that proof. WebView2
    defaults to media prompts and unguarded popups. Agents MUST NOT bypass
    either handler or the first-navigation proof (KEL-168).
- Architecture 01 §5 **first paint** is the KEL-64 external double-rAF image
  beacon on a pre-spawn monotonic clock — not wry `PageLoadEvent::Finished`,
  not `WindowBuilder::build`, and not titled HWND / `window-visible` (KEL-62,
  KEL-64). `PageLoadEvent::Finished` is navigation completion only; the macOS
  `startup` trace records it as `nav_finished` for construction diagnostics.
  Dump that trace with `KELD_STARTUP_TRACE=1`. The `startup` module is
  macOS-only; do not compile it on Linux/Windows (CodeRabbit #10).
