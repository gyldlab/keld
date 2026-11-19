# Windows WebView2
Owner/load: webview-windows/always. Extends ../../AGENTS.md.
- Independent exact-diff review MUST precede new unsafe builds. Same STA/controller until callbacks quiesce.
- `native_text.rs`: bounded UTF-16 getter PWSTR/borrowed PCWSTR scans/slices. CoTaskMemFree task PWSTRs once. Test CoTaskMemAlloc controlled inputs.
- `original_resource/native.rs`: WRR registration, fixed file:* source-kind filters/worker-filter transfer, bounded URI/header/context/method/body-presence getters, own nullable IStream, GetDeferral, SHCreateMemStream, CreateWebResourceResponse/SetResponse, one-shot Complete. Fixed denial before admit. Verified 200 needs same ticket/source/lease.
- `original_callbacks.rs`: subscribe actual-sender navigation-start/content-loading/completion. Bound Source/URI/header/navigation ID/redirect/error/success getters and SetCancel. Tokens retire only on same-controller Close and callback exit.
- `original_load.rs`: same-view Environment/BrowserVersionString (Q22 a/b before +1). Selected GET/no-body CreateWebResourceRequest, exact readback, one NavigateWithWebResourceRequest after final admit.
- `mod.rs`: one live RegisterWaitForSingleObject per browser/reader. Replace only on checked UnregisterWaitEx success. Reader WaitForSingleObject(0) before join. Pool callback only posts Tao EventLoopProxy. Reader wakes grant no browser-exit credit.
- `view_build.rs`: held managed HWND IsWindow→GetMenu before pump/geometry. Deny invalid/menu. Selected original STA Settings2 UserAgent get/set/get.
