# Windows native adapters
Owner/load: webview-windows/always.
Extends ../../AGENTS.md. Exact approved GH1184 contract; independent unsafe/design review before compiling new boundaries.

- native_text.rs: bounded COM-task PWSTR reads, strict conversion, one matching free; tests may allocate controlled COM strings and write out-pointers.
- original_callbacks.rs: creating-STA navigation subscriptions/removal, canonical identity QI, bounded URI/header/ID/error getters, SetCancel.
- Reuse the sole checked document owner. No URL/order-inferred generation, Rust borrow across native entry, escaping unwind, or cross-thread native handle. Pending navigation holds posts; actual replacement revokes. Navigation reentry inside a post retains outcome-unknown charge and fails qualification.
- original_resource/native.rs: STA file:* filters/WRR registration, bounded request/source-kind/body getters, deferrals, SDK memory stream, fixed 403/413/503 or verified 200 response, one-shot Complete; shared parser/census/intake owners only.
- mod.rs: existing registered wait owns the reader thread handle; zero-time signal, checked unregister before join. Closed wake kind and pool unwind barrier; no COM there.
- Deferral plus refusal failure: revoke, canonical retained Close before return; closed hook requires actual success. Keep worker/body credit until quiescent.
- original_load.rs: source-selected GET/no-body CreateWebResourceRequest and bounded readback; single-use NavigateWithWebResourceRequest after checked admission.
