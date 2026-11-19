# Windows desktop sampler
Owner/load: webview-windows-desktop/always.
Extends ../AGENTS.md and the keld-wv owner. GH481 proposal 6686490913 and approval 6800617450 pin the bounded 37da8f08 policy.

- UI-thread reads only. display.rs: GetThreadDpiAwarenessContext, AreDpiAwarenessContextsEqual, GetSystemMetrics(SM_CMONITORS), MonitorFromPoint, GetMonitorInfoW.
- window_dpi.rs: GetCurrentThreadId, synchronous EnumThreadWindows callback, GetWindowThreadProcessId, GetWindowDpiAwarenessContext, AreDpiAwarenessContextsEqual, GetWindowRect, MonitorFromPoint/Window, GetDpiForWindow; only this Host/UI thread's existing PMv2 windows.
- target.rs: bounded GetDisplayConfigBufferSizes/QueryDisplayConfig and DisplayConfigGetDeviceInfo source/target reads. Keep full target tuple,32 retained IDs; no eviction or ID authority.
- appearance.rs: UISettings.TextScaleFactor, AccessibilitySettings.HighContrast, RegGetValueW only for current-user AppsUseLightTheme DWORD.
- Keep inherited unsafe rules and local proofs. No setting writes, probe window, cross-thread native value, fallback facts, or escaping callback unwind. Failed/unsupported/changed reads cannot allocate an ID. Shared DesktopSession preserves captured reader identity and original Init/refresh order.
