# Windows WebView2
Owner/load: webview-windows/always. Extends ../../AGENTS.md.
- Independent exact-diff review MUST precede new unsafe builds. Keep COM on its STA and same controller until callbacks quiesce.
- `native_text.rs`: bounded UTF-16 scans/slices of getter PWSTR and borrowed PCWSTR. CoTaskMemFree task PWSTRs once. Tests may CoTaskMemAlloc controlled inputs.
- `original_resource/native.rs`: WRR registration, fixed file:* source-kind filters/worker-filter transfer, bounded URI/header/context/method/body-presence getters, nullable IStream ownership, GetDeferral, SHCreateMemStream, CreateWebResourceResponse/SetResponse, one-shot Complete. Fixed denial before fallible admission. Verified 200 needs same ticket/source/lease.
- `original_callbacks.rs`: navigation-start/content-loading/completion subscriptions on actual sender. Bound Source/URI/header/navigation ID/redirect/error/success getters and SetCancel. Retire tokens only on same-controller Close and callback exit.
- `original_load.rs`: same-view Environment/BrowserVersionString for Q22 a/b before +1. Selected GET/no-body CreateWebResourceRequest, exact readback, one NavigateWithWebResourceRequest after final admission.
- `mod.rs`: one live RegisterWaitForSingleObject per browser/reader owner. Replace only on checked UnregisterWaitEx success. Reader: zero-time WaitForSingleObject before join. Pool callback only posts Tao EventLoopProxy. Reader wakes grant no browser-exit credit.
- `view_build.rs`: selected original STA Settings2 UserAgent get/set/get.
