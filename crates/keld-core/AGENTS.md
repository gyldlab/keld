# keld-core invariants

Extends root `AGENTS.md`; this file owns the shipping host-session composition.

- `app_session.rs` is the one no-flag lifecycle/router owner. It composes
  runtime and webview primitives; it MUST NOT duplicate generation, Job, LPAC,
  ACL, guard, or wire policy.
- Windows production `unsafe` is limited to reviewed lease pipe state/flag,
  cleanup-sentinel process acquire/image/wait, and KEL-135 Authenticode
  trust-state/signer/SPKI reads in `app_session.rs`. Every call needs a local
  `// SAFETY:` proof and independent unsafe/security review.
- macOS KEL-135/T3 Core `unsafe` is limited to `SecCodeCopySelf` →
  `SecRequirementCreateWithString` → `SecCodeCheckValidity` → `SecCodeCopyStaticCode` →
  `SecCodeCopySigningInformation` and owned CF-reference conversion in
  `src/macos_profile_identity.rs`. It accepts current-process code; identity
  fields follow Apple-chain validation.
  API or missing-fact failure is `KELD-WV-009`; no arbitrary code/path or
  blanket `unsafe`. Every block needs a local `// SAFETY:` proof.
- Startup remains resource-free until validated boot and immutable guard
  preflight pass. Revocation, link close, child reap, window exit, and cleanup
  errors remain ordered and independently observable.
- Recovery and shutdown gates fail closed: a pre-attach revocation, accepted
  shutdown, stale generation, or missing owner MUST NOT provision a successor.
  Real platform tests and a mutation/negative control are required for changes
  to these transitions.
