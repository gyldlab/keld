# keld-native invariants

Owner/load: native/always. Trigger: crates/keld-native. Extends root AGENTS.md.

- KEL-89 production unsafe is limited to these paths/operations:
  - `src/auth/macos_http.rs`: typed AuthHttpDelegate NSObject class/ABI,
    NSObjectProtocol/NSURLSession{,Task,Data}Delegate conformances, NSObject
    super-init and sessionWithConfiguration_delegate_delegateQueue.
  - `src/auth/macos_session.rs`: typed MainThreadOnly PresentationProvider
    NSObject class/ABI, NSObjectProtocol and
    ASWebAuthenticationPresentationContextProviding conformances; super-init;
    method_id presentationAnchorForWebAuthenticationSession (+0 NSWindow return);
    callbackWithCustomScheme and initWithURL_callback_completionHandler with
    RcBlock::as_ptr; setPresentationContextProvider(Some/None), start and cancel
    on the owned main-thread session; invocation-local nullable SDK NSURL/NSError
    borrows and ASWebAuthenticationSessionErrorDomain symbol read.
  No manual Send/Sync, unchecked pointer adoption/casts, trust overrides or other
  production unsafe. AS session/provider/anchor stay on main; callback borrows never escape.
- Test-only unsafe: `tests/support/windows_handle_census.rs` MAY call
  GetCurrentProcess/GetProcessHandleCount with the current-process pseudo
  handle and local count; never close that handle.
- `src/fs.rs` cfg(test) MAY call WaitForSingleObject only on a retained Child.
  Keep the bounded wait. Retire these allowances if the probes move.
