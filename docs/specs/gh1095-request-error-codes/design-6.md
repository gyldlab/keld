# Design 6: Platform notes

Part of: [index](index.md)
Owns: Design 6: Platform notes

## 4. Design: Platform notes

- **macOS:** the only renderer bridge (`#[cfg(target_os = "macos")]` on
  `begin_echo_call`, `pending_echo` and `start_renderer_dispatch`). Every
  renderer criterion is macOS-only.
- **Windows and Linux:** no renderer bridge. The `@keld/api` (S11–S14) and
  `@keld/kipc` ([historical §4.5](design-5.md)) rules apply on all three OSes.
- **A future Windows named-pipe Echo path:** it MUST re-prove F1 and F3
  (overlapped `WriteFile` can complete despite a cancel) before it emits WV-012.
