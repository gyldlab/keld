# keld-wv — adds root AGENTS.md

Spec: `docs/architecture/05-webview-and-native.md`. Platform truth: `docs/research/library/host-platforms/06-webview-reality.md`. v0 trait: `src/engine.rs` (not the full spec 05 sketch).

- `unsafe` MAY appear in platform backends only. Those modules MUST `deny(unsafe_op_in_unsafe_fn)` and MUST cite a platform contract in `// SAFETY:`.
- All engine/window mutations MUST run on the UI thread (today: tao's main-thread event loop; later: keld-core command queue). Agents MUST NOT touch platform handles from I/O or pool threads.
- `WebEngine` trait changes MUST go through design review. Backends MUST stay within the trait API. Agents MUST NOT add a trait method until a live backend implements it in the same PR (root YAGNI).
- Platform quirks MUST comment OS + version + source link; uncited workarounds MUST be reverted.
- Linux: agents MUST probe the GPU stack and apply safe-mode before init — MUST NOT instruct env-var exports. Emit `degraded-rendering`. `webkitgtk::detect_gpu_safe_mode` is the pure query; `prepare_gpu_safe_mode_process` applies the mitigation by exact-self re-exec and MUST run only from a process-entry dispatcher before non-repeatable state. `WebKitGtkEngine::new` MUST fail closed before GTK/WebKit when preparation is missing. Mitigates NVIDIA proprietary driver + Wayland on `WebKitGTK` ≤ 2.54 by giving the replacement process `WEBKIT_DISABLE_DMABUF_RENDERER=1`. Upstream: [tauri-apps/tauri#9394](https://github.com/tauri-apps/tauri/issues/9394), [#14924](https://github.com/tauri-apps/tauri/issues/14924). `gpu_safe_mode()` exposes the result for `keld doctor`.
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
  - Windows (direct COM, KEL-65): agents MUST register the guarded
    `add_PermissionRequested` handler before the first navigation — WebView2's
    fallback is a user prompt (default-ask, not default-deny). The first
    navigation MUST present the `GuardInstalled` proof; agents MUST NOT add a
    second navigation path that bypasses it.
  Agents MUST NOT pass `AppProcess` to inherit `/app` media grants.
- Architecture 01 §5 **first paint** is the KEL-64 external double-rAF image
  beacon on a pre-spawn monotonic clock — not wry `PageLoadEvent::Finished`,
  not `WindowBuilder::build`, and not titled HWND / `window-visible` (KEL-62,
  KEL-64). `PageLoadEvent::Finished` is navigation completion only; the macOS
  `startup` trace records it as `nav_finished` for construction diagnostics.
  Dump that trace with `KELD_STARTUP_TRACE=1`. The `startup` module is
  macOS-only; do not compile it on Linux/Windows (CodeRabbit #10).
