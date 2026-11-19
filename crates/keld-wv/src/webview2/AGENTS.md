# Windows WebView2 original resources (GH1185)

Owner/load: webview-windows/always for this subtree. Extends ../../AGENTS.md.

- Independent exact-diff review MUST precede new unsafe builds. Keep COM on its STA and the same controller through callback quiescence.
- `native_text.rs`: bound pointer scans and UTF-16 slices of getter PWSTR/borrowed PCWSTR. Free each task allocation once with CoTaskMemFree; tests may allocate controlled inputs via CoTaskMemAlloc.
- `original_resource/native.rs`: WRR registration, fixed file:* source-kind filters and worker-filter transfer, bounded URI/header/context/method/body-presence getters, nullable IStream ownership, GetDeferral, SHCreateMemStream, CreateWebResourceResponse/SetResponse, one-shot Complete. Install fixed denial before fallible admission; verified 200 needs the same ticket, source and lease.
- `original_callbacks.rs`: navigation-start/content-loading/completion subscriptions on the actual sender; bounded Source, URI, header, navigation ID, redirect/error/success getters and SetCancel. Retire tokens only on same-controller Close and callback exit.
- `original_load.rs`: selected GET/no-body CreateWebResourceRequest, exact readback, single-use NavigateWithWebResourceRequest after final admission.
- `mod.rs`: one live RegisterWaitForSingleObject per browser/reader owner. Replace only after checked UnregisterWaitEx success. Reader: zero-time WaitForSingleObject before join. Pool callback only posts Tao EventLoopProxy; reader wakes grant no browser-exit credit.
