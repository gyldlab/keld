# keld-native invariants
Owner/load: native/always; path crates/keld-native.

- Unsafe only (NSObject ABI/super-init + NSObjectProtocol):
  - KEL-89 `src/auth/macos_http.rs`: AuthHttpDelegate; NSURLSession{,Task,Data}Delegate; sessionWithConfiguration_delegate_delegateQueue.
  - KEL-89 `src/auth/macos_session.rs`: MainThreadOnly PresentationProvider/ASWebAuthenticationPresentationContextProviding; method_id presentationAnchorForWebAuthenticationSession (+0 NSWindow); callbackWithCustomScheme, initWithURL_callback_completionHandler/RcBlock::as_ptr; setPresentationContextProvider(Some/None), start/cancel (main); local nullable NSURL/NSError borrows; ASWebAuthenticationSessionErrorDomain read.
  - GH478 `src/dialog/macos_file/delegate.rs`: MainThreadOnly NSOpenSavePanelDelegate; panel:shouldEnableURL:, panel:validateURL:error:_; weak setDelegate(Some/None), retain through callback+physical drain; clear before drop.
  - GH480 `src/menu/macos/target.rs`: MainThreadOnly MenuTarget/activate:(NSMenuItem*); weak setTarget/Action(Some/None), retain until cleared; tryToPerform_with only undo:/redo:/cut:/copy:/paste:/pasteAndMatchStyle:/selectAll:.
- No other unsafe, manual Send/Sync, unchecked pointer adoption/casts or trust overrides. Auth session/provider/anchor: main; callback borrows never escape.
- Test unsafe: `tests/support/windows_handle_census.rs` GetCurrentProcess/GetProcessHandleCount; self pseudo-handle, never close. `src/fs.rs` cfg(test): WaitForSingleObject on retained Child only; bounded, retire if moved.
