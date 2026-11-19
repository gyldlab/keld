# keld-native invariants

Owner/load: native/always; path crates/keld-native; root applies.

- Production unsafe only:
  - KEL-89 `src/auth/macos_http.rs`: AuthHttpDelegate NSObject ABI/super-init; NSObjectProtocol/NSURLSession{,Task,Data}Delegate; sessionWithConfiguration_delegate_delegateQueue.
  - KEL-89 `src/auth/macos_session.rs`: MainThreadOnly PresentationProvider NSObject ABI/super-init, NSObjectProtocol/ASWebAuthenticationPresentationContextProviding; method_id presentationAnchorForWebAuthenticationSession (+0 NSWindow return); callbackWithCustomScheme, initWithURL_callback_completionHandler via RcBlock::as_ptr; setPresentationContextProvider(Some/None), start/cancel (main); invocation-local nullable SDK NSURL/NSError borrows; ASWebAuthenticationSessionErrorDomain read.
  - GH478 `src/dialog/macos_file/delegate.rs`: MainThreadOnly NSObject ABI/super-init, NSObjectProtocol/NSOpenSavePanelDelegate; panel:shouldEnableURL: and panel:validateURL:error:_ only; typed weak setDelegate(Some/None); retain through callback + physical drain, clear weak before drop; main only.
- No other production unsafe, manual Send/Sync, unchecked pointer adoption/casts or trust overrides. Auth session/provider/anchor on main; callback borrows never escape.
- Test unsafe: `tests/support/windows_handle_census.rs` GetCurrentProcess/GetProcessHandleCount; current-process pseudo-handle only, never close.
- `src/fs.rs` cfg(test): WaitForSingleObject only on retained Child; bounded wait. Retire if moved.
