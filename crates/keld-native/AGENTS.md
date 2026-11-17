# keld-native invariants
Owner/load: native/always. Trigger: crates/keld-native. Extends root AGENTS.md.

- Production unsafe only at:
  - `src/auth/macos_http.rs` (KEL-89): typed AuthHttpDelegate NSObject ABI, NSObjectProtocol, NSURLSession{,Task,Data}Delegate; super-init; sessionWithConfiguration_delegate_delegateQueue.
  - `src/auth/macos_session.rs` (KEL-89): typed MainThreadOnly PresentationProvider NSObject ABI, NSObjectProtocol, ASWebAuthenticationPresentationContextProviding; super-init; method_id presentationAnchorForWebAuthenticationSession (+0 NSWindow); callbackWithCustomScheme and initWithURL_callback_completionHandler with RcBlock::as_ptr; setPresentationContextProvider(Some/None), start, cancel on owned main-thread session; invocation-local nullable NSURL/NSError borrows and ASWebAuthenticationSessionErrorDomain read.
  - `src/macos_dev_stage_volume.rs` (GH-1563): opened-fd ACL via acl_get_fd_np, acl_valid, acl_get_entry, acl_get_tag_type, acl_get_permset_mask_np, acl_get_flagset_np, acl_get_flag_np, acl_free.
- Forbid manual Send/Sync, unchecked pointers/casts, trust overrides or other unsafe. AS sessions, providers and anchors stay on main; callback borrows never escape. Each unsafe block needs local SAFETY proof.
- Test-only: `tests/support/windows_handle_census.rs` MAY call GetCurrentProcess/GetProcessHandleCount on retained pseudo handle; never close it.
- `src/fs.rs` cfg(test) MAY call WaitForSingleObject only on retained Child under bounded wait. Retire these allowances if probes move.
