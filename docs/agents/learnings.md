# Learnings log

Append-only log of non-obvious facts discovered during work. Read this before starting
a task; append in the same PR when you learn something (protocol: root `AGENTS.md`
§ Self-improvement).

Format — one line each, newest last:

```
- YYYY-MM-DD [area] fact. (evidence: path, issue, or command)
```

`[area]` is a crate short name (`ipc`, `wv`, `guard`, `native`, `runtime`, `update`,
`pack`, `compat`, `core`, `cli`), `ts`, `build`, `ci`, or `process`.

Compaction (maintainers, roughly when this list passes ~40 entries): move learnings
that proved stable into the root `AGENTS.md`, the relevant spec, or a crate
`AGENTS.md`; delete entries that were compacted, superseded, or wrong; keep this file
small — it is loaded by every agent session.

## Log

- 2026-07-08 [process] Perplexity exports in docs/research/from-outside/ are raw inputs; the polished numbered docs in docs/research/ are the citable corpus — never cite from-outside directly. (evidence: docs/research/00-landscape.md header)
- 2026-07-08 [build] `cargo clippy --workspace --all-targets -- -D warnings` is the CI form; pedantic is already on via workspace lints, do not re-enable it per-crate. (evidence: Cargo.toml [workspace.lints])
- 2026-07-08 [ipc] Wire-size constants (HEADER_LEN=16) are protocol facts tested independently of struct layout — change requires a version bump + review gate. (evidence: crates/keld-ipc/src/frame.rs tests)
- 2026-07-08 [tooling] cargo-deny `unmaintained` accepts `all|workspace|transitive|none`, not `warn`. (evidence: deny.toml, cargo-deny 0.19)
- 2026-07-08 [wv/macos] Phase 1 hello window uses tao+wry as interim scaffolding; replace with direct objc2 bindings per arch/05. (evidence: crates/keld-wv/src/wkwebview/mod.rs)
- 2026-08-08 [wv] wry compiles open_devtools/close_devtools only under cfg(any(debug_assertions, feature="devtools")) — keld-wv pins the `devtools` feature or release builds fail. (evidence: competitors/wry/src/lib.rs `pub fn open_devtools`, crates/keld-wv/Cargo.toml)
- 2026-08-12 [process] IETF RFC 2119 keywords bind agents in AGENTS.md / docs/agents/* only — not architecture prose or Rust comments. (evidence: docs/research/28-post-integration-audit.md, RFC 2119)
- 2026-08-12 [wv/macos] Unit tests must not call `WkWebViewEngine::new` / tao `EventLoop::new` (starts AppKit). Smoke title/HTML/spec instead; `just hello` is the GUI check. (evidence: KEL-26)
- 2026-08-12 [wv/macos] tao 0.35 `EventLoopExtMacOS` already defaults to `NSApplicationActivationPolicyRegular`; do not set it again. (evidence: tao-0.35.3 `platform/macos.rs` trait docs)
- 2026-08-13 [ipc] postcard `from_bytes` ignores trailing bytes; a no-op echo test must assert re-encoded output ≠ input, not expect a codec error. (evidence: crates/keld-ipc/src/echo.rs `echo_roundtrip_copies_fields_not_input_bytes`)
