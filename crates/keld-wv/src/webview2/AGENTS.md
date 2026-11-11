# Windows native adapters
Owner/load: webview-windows/always.
Extends ../../AGENTS.md. GH1184 exact approved contract; independent unsafe/design review before compiling a new boundary.

- native_text.rs: bounded returned COM-task PWSTR reads, strict conversion, and one matching free.
- original_callbacks.rs: creating-STA WebView2 navigation subscriptions/removal, canonical identity QI, bounded URI/header/ID/error getters, SetCancel.
- Reuse the sole checked document owner. No generation inferred from URL/order, callback-held Rust borrow across native entry, escaping unwind, or cross-thread native handle. Pending navigation holds posts; actual replacement revokes. Unexpected navigation reentry inside a post retains outcome-unknown charge and fails qualification.
