# keld-wv — adds root AGENTS.md

Spec: `docs/architecture/05-webview-and-native.md`. Platform truth: `docs/research/06-webview-reality.md`. v0 trait: `src/engine.rs` (not the full spec 05 sketch).

- `unsafe` MAY appear in platform backends only. Those modules MUST `deny(unsafe_op_in_unsafe_fn)` and MUST cite a platform contract in `// SAFETY:`.
- All engine/window mutations MUST run on the UI thread (today: tao's main-thread event loop; later: keld-core command queue). Agents MUST NOT touch platform handles from I/O or pool threads.
- `WebEngine` trait changes MUST go through design review. Backends MUST stay within the trait API. Agents MUST NOT add a trait method until a live backend implements it in the same PR (root YAGNI).
- Platform quirks MUST comment OS + version + source link; uncited workarounds MUST be reverted.
- Linux: agents MUST probe the GPU stack and apply safe-mode before init — MUST NOT instruct env-var exports. Emit `degraded-rendering`.
- Cross-engine diffs MUST go to the baseline matrix; polyfill pack + doctor smooth. Agents MUST NOT silently paper over them.
- Tests MUST follow repository `.agents/testing.md`.
- Camera/microphone capture MUST go through `keld-guard` (`web.camera` /
  `web.microphone`) as `Principal::AppProcess` in v0 — no platform callback
  carries an origin or webview id yet. Per backend:
  - macOS (wry interim): agents MUST NOT omit wry `with_permission_handler`
    on a live `WebViewBuilder` — wry 0.56 auto-grants when the handler is `None`.
  - Windows (direct COM, KEL-65): agents MUST register the guarded
    `add_PermissionRequested` handler before the first navigation — WebView2's
    fallback is a user prompt (default-ask, not default-deny). The first
    navigation MUST present the `GuardInstalled` proof; agents MUST NOT add a
    second navigation path that bypasses it.
  Agents MUST NOT pass a `Webview` principal to inherit `/app` media grants.
