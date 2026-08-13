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
- 2026-08-13 [cli] tao `EventLoop::run` never returns, so `keld dev` must reap the Bun echo child before opening the hello window or the child is orphaned. (evidence: crates/keld-wv/src/wkwebview/mod.rs `run_until_closed`, crates/keld-cli/src/dev.rs `run_dev_echo` then `open_dev_window`)
- 2026-08-13 [cli] rmcp 3.1.2 accepts `server/discover` only after `initialize`, with namespaced `io.modelcontextprotocol/*` metadata keys; unnamespaced keys return JSON-RPC -32602. (evidence: crates/keld-cli/tests/doctor_mcp.rs `mcp_server_discover_advertises_versions_and_identity`)
- 2026-08-13 [ci] cargo-deny MPL-2.0 is a per-crate pin (`option-ext@0.2.0` via wry→dirs→dirs-sys), not a global allow; packed binaries must offer that crate's source. (evidence: deny.toml, docs/engineering/third-party-licenses.md, KEL-54)
- 2026-08-13 [deps] wry crate is Apache-2.0 OR MIT; GitHub's license API reports Apache-2.0 only. tao is Apache-2.0 only. Do not treat MPL as wry's license. (evidence: crates.io wry 0.55.1 LICENSE.spdx, tao 0.35.3 Cargo.toml.orig)
- 2026-08-13 [ci] gitleaks GitHub Action requires a GITLEAKS_LICENSE on organization repos; pin the OSS CLI tarball with sha256 instead. (evidence: gitleaks-action README org-license note, gyldlab/keld, KEL-39)
- 2026-08-13 [ipc] kipc `decode` uses postcard `take_from_bytes` and rejects leftover bytes; `from_bytes` would ignore them. (evidence: crates/keld-ipc/src/codec.rs)
- 2026-08-13 [cli] `keld hello` extra args must be rejected before `run_hello_window` or the test hangs in tao `EventLoop::run`. (evidence: crates/keld-cli/tests/cli_kel29.rs `keld_hello_unknown_flag_exits_2_without_opening_window`)
- 2026-08-13 [ipc] Darwin/Linux `SO_RCVTIMEO` on a live silent UnixStream is `WouldBlock` (EAGAIN), not always `TimedOut`; map both to `KELD-IPC-006` or a deadline is misclassified as `KELD-IPC-001`. Keep the peer socket alive in the test or you get EOF instead. (evidence: crates/keld-ipc/src/link.rs `read_frame_silent_peer_is_ipc_006`)
- 2026-08-13 [cli] Spec-named verbs that are not live (`build`/`migrate`/`gen`/`ext`) must be `KELD-CLI-045` exit 2, not a bare `unknown command` exit 1 — README agents otherwise have no tracking issue. (evidence: crates/keld-cli/src/verb.rs, KEL-29)
- 2026-08-13 [cli] `keld create --template` must be `KELD-CLI-044` (closed flag set), not `KELD-CLI-020` invalid name `--template`; extra `keld dev` tokens must fail before tao `EventLoop::run`. (evidence: crates/keld-cli/src/flags.rs, KEL-29)
- 2026-08-13 [process] `docs/research/` is local-only; tracking it ships 50+ notes in the PR. `git rm --cached` + gitignore, never `git add docs/research`. (evidence: PR #2, `088efc0`)
- 2026-08-13 [cli] Unix echo sockets in shared `$TMPDIR` are world-connectable; bind inside a unique `0o700` session dir and remove the dir on Drop/shutdown. (evidence: crates/keld-cli/src/echo_link.rs `bind_unix_echo`)
- 2026-08-13 [ci] dtolnay/rust-toolchain@6c977a6 (2026-08-05) requires `with: toolchain:` even when rust-toolchain.toml exists; omitting it fails every job with 'toolchain' is a required input. (evidence: GitHub CI on PR #2 @ 6ed01b1)
- 2026-08-13 [ci] Windows clippy `-D warnings` fails on unused `std::sync::mpsc` in keld-ipc `echo_link` tests; gate Unix-only test imports with `cfg(unix)`. (evidence: crates/keld-ipc/tests/echo_link.rs, PR #2 windows-latest)
- 2026-08-13 [ci] rustdoc `-D warnings` fails on public docs linking private items (`rustdoc::private-intra-doc-links`); backtick error codes instead of `[`const`]` when the module is private. (evidence: crates/keld-cli/src/mcp/permissions.rs, PR #2 ubuntu rustdoc)
- 2026-08-13 [ci] taiki-e/install-action on windows-latest fails if `%CARGO_HOME%\bin` is missing (tar: Cannot open: No such file). Create the dir before install. (evidence: GHA 31708711822)
- 2026-08-13 [cli] llms-full.txt corpus split on LF-only markers yields empty chunks on Windows CRLF checkout; normalize \r\n in docs_search::corpus(). (evidence: GHA windows-latest docs_search tests on 6cc9749)
- 2026-08-13 [cli] Windows `fs::canonicalize` yields `\\?\…`; doctor JSON must strip that verbatim prefix so MCP (`project_root`) matches CLI (`current_dir`). (evidence: crates/keld-cli/src/doctor.rs `strip_verbatim_prefix`, windows-latest `mcp_doctor_matches_cli_json_over_stdio`)
- 2026-08-13 [process] Hello size/RSS live in `docs/engineering/budget-scoreboard.md` (tracked); `just hello` is debug; do not put numbers only in gitignored `docs/research/` or Linear. (evidence: KEL-26, origin/main `b93ebb6`)
- 2026-08-13 [pack] bun 1.3.14 this-Mac is 63,096,576 B; gzip -9 = 23,548,666 (over the ≤20 MB bun-installer budget before host); zstd -19 = 16,838,595. UDZO/zlib DMG of host+Bun cannot hit architecture 01 §5; zstd can. (evidence: `stat`/`gzip`/`zstd` on `$(command -v bun)`, budget-scoreboard Win conditions)
- 2026-08-13 [process] just 1.58 still lexes `<<'EOF'` bodies inside shebang recipes as justfile syntax (`unknown start of token '-'`); use bash arrays/printf fixtures instead of heredocs in the justfile. (evidence: `just --list` on agents-md heredocs vs array rewrite)
- 2026-08-13 [process] `docs/research/` must be a nested checkout of private `keld-research`; push with `just research-push` (same turn as edits). If it resolves to the Keld toplevel, research-push refuses — never `git add docs/research` from the monorepo root. (evidence: justfile `research-push`, AGENTS.md § Private research)
- 2026-08-13 [process] Competitor/native hello fixtures live in public `gyldlab/keld-benches`, not Keld `docs/`/`competitors/`/`/tmp`-only; scoreboard numbers stay in `docs/engineering/budget-scoreboard.md` with fixture links. (evidence: AGENTS.md § Public benches, https://github.com/gyldlab/keld-benches)
