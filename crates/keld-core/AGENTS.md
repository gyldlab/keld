# keld-core invariants

Extends root `AGENTS.md`; host sessions.

- `app_session.rs`: no-flag lifecycle/routing; reuse runtime/WebView owners;
  no duplicate generation, Job, LPAC, ACL, guard or wire policy.
- Windows unsafe: reviewed lease-pipe flags/state and cleanup-sentinel
  acquire/image/wait in `app_session.rs`. Authenticode in Guard (KEL-270 D4).
- Windows facts unsafe: only `src/app_session/launch_facts/windows_native.rs`:
  `GetUserPreferredUILanguages`, `SHGetKnownFolderPath` without CREATE, bounded
  output reads and owned `CoTaskMemFree`. Own before entry; no shutdown lock.
- macOS unsafe: only `src/macos_profile_identity.rs`, current-process
  `SecCodeCopySelf`, `SecRequirementCreateWithString`, `SecCodeCheckValidity`,
  `SecCodeCopyStaticCode`, `SecCodeCopySigningInformation`, `SecCodeCopyPath`,
  `SecStaticCodeCheckValidityWithErrors` + owned typed CF conversions.
  KEL-135: Apple-chain validation or KELD-WV-009. GH-1404 local:
  dynamic/full static validation, ad-hoc origin and sealed metadata.
  No caller-selected code/path or blanket unsafe.
- Every unsafe block: local SAFETY; exact-diff independent unsafe/security review.
- Boot/immutable Guard preflight before resources. Observable order:
  revoke, link close, child reap, window exit, cleanup. Independent errors;
  never delete installed apps.
- Recovery/shutdown: no successor on pre-attach revocation, accepted shutdown,
  stale generation or missing owner. Transition changes require real-platform
  tests + mutation/negative controls.
