# keld-core invariants

Extends root `AGENTS.md`; this file owns the shipping host-session composition.

- `app_session.rs` is the one no-flag lifecycle/router owner. It composes
  runtime and webview primitives; it MUST NOT duplicate generation, Job, LPAC,
  ACL, guard, or wire policy.
- Windows production `unsafe` is limited to reviewed lease pipe state/flag
  operations and cleanup-sentinel process acquire/image/wait operations in
  `app_session.rs`. Every call needs a local `// SAFETY:` proof and human
  unsafe/security review.
- Startup remains resource-free until validated boot and immutable guard
  preflight pass. Revocation, link close, child reap, window exit, and cleanup
  errors remain ordered and independently observable.
- Recovery and shutdown gates fail closed: a pre-attach revocation, accepted
  shutdown, stale generation, or missing owner MUST NOT provision a successor.
  Real platform tests and a mutation/negative control are required for changes
  to these transitions.
