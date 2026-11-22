# Implementation boundaries

Part of: [index](index.md)
Owns: Implementation boundaries

## 5. Boundaries

- **Implement in:**
  - `crates/keld-core/src/app_session.rs` (`begin_echo_call` refusal type,
    renderer dispatch mapping) and its tests;
  - `crates/keld-wv/src/wkwebview/macos_bridge.rs` (`pagehide` code,
    document-scoped result) and `crates/keld-wv/src/error.rs` (`fix` text);
  - `packages/@keld/api/src/{channels,echo-call}.ts` and their tests;
  - `docs/engineering/keld-error-codes.md`;
  - the KEL-142 spec §7;
  - the host macOS acceptance tests named in §7.
- **Must not touch:**
  - KIPC frame layout, `CallError` schema, the isolated→native
    `keld.wv-link/v1` envelope;
  - `@keld/kipc` transport behaviour;
  - guard, permissions, workspace `Cargo.toml`;
  - navigation policy;
  - restart-notification code ([#1098](https://github.com/gyldlab/keld/issues/1098)).
