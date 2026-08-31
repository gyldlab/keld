# keld-cli invariants

Extends root `AGENTS.md`; this file owns cold developer-tooling boundaries.

- `boot.rs` is the sole dev-stage producer. Windows production `unsafe` is
  limited to atomic `CreateDirectoryW` with the already-built current-user
  security descriptor; every pointer/descriptor lifetime needs a local
  `// SAFETY:` proof and human unsafe/security review.
- The CLI may stage, launch, forward logs, and retain the dev-lease writer. It
  MUST NOT own the application window, app-link, Bun restart loop, principal,
  or permission decision.
- On Windows, namespace guards remain live until both the staged host and the
  verified cleanup sentinel report ownership. The sentinel is the sole nonce
  deleter; failure to establish it shuts down the host and fails `keld dev`.
- CLI changes to staging, cleanup, or lease ownership require real process and
  filesystem oracles plus a failure-first/negative control. Shell janitors,
  caller-selected arbitrary deletion, and duplicated ACL policy are forbidden.
