# keld-core invariants

Extends root `AGENTS.md`; owns shipping host-session composition.

- `app_session.rs` owns no-flag lifecycle/routing. Reuse runtime/WebView owners;
  no duplicate generation, Job, LPAC, ACL, guard or wire policy.
- Windows unsafe: reviewed lease-pipe flags/state and cleanup-sentinel
  acquire/image/wait in `app_session.rs`. Authenticode FFI stays in Guard
  (KEL-270 D4). Require local SAFETY and independent unsafe/security review.
- macOS unsafe: only `src/macos_profile_identity.rs`, current-process
  `SecCodeCopySelf`, `SecRequirementCreateWithString`, `SecCodeCheckValidity`,
  `SecCodeCopyStaticCode`, `SecCodeCopySigningInformation`, `SecCodeCopyPath`,
  `SecStaticCodeCheckValidityWithErrors` and owned typed CF conversions.
  KEL-135 identity requires Apple-chain validation; failure is KELD-WV-009.
  GH-1404 local facts require dynamic/full static validation, ad-hoc origin
  and sealed metadata. No caller-selected code/path or blanket unsafe.
  Every block needs local SAFETY; independently review the exact final diff.
- Boot and immutable Guard preflight precede resources. Preserve observable order:
  revoke, link close, child reap, window exit, cleanup; errors stay independently
  observable. Never delete installed apps.
- Recovery/shutdown fail closed: pre-attach revocation, accepted shutdown, stale
  generation or missing owner cannot provision a successor. Transition changes
  require real-platform tests plus mutation/negative controls.
