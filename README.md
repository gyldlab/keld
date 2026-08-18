# KELD

**The desktop framework that replaces Electron without requiring a full app rewrite.**
Rust core · Bun-powered JS/TS main process · system webviews · security by default.
By [GYLDLAB](https://github.com/gyldlab).

> Status: **pre-alpha** — this checkout contains implementation and tracked architecture
> specs; private research is maintained in the separate nested research repository.

## The idea in 30 seconds

Keld starts from Electron's observable API and process contracts, measures them against
a versioned app corpus, and keeps unsupported behavior explicit. Median apps should
migrate through configuration; demanding apps may drive targeted runtime, host or app
patches behind the same compatibility facade.

The following is the **target product flow**. `migrate` and `build` are not implemented
in the current pre-alpha CLI:

```bash
cd my-electron-app
bunx keld migrate   # analyzes your app, generates config, aliases electron → @keld/electron
bunx keld dev       # no bundled Chromium/Node executable; exact gaps reported
bunx keld build     # signed installers + kilobyte-scale delta updates
```

The target architecture replaces Electron's architecture, not its API:

- **A prebuilt Rust host** owns windows, webviews, and every native API — you never
  install a Rust toolchain.
- **Your JS/TS main process and named compatibility roles run on Bun** as supervised,
  strict-profile principals — npm/Node behavior is corpus-tested, ambient OS access is
  denied, and a child crash does not take your windows down.
- **System webviews by default** (WebView2 / WKWebView / WebKitGTK) with a polyfill
  pack and per-platform engine policy.
- **Typed binary IPC** (schema-first and backpressured; optional per-role shared-memory
  bulk lanes only after workload and sandbox measurements justify them).
- **Default-deny permissions** generated from your code, reviewed like a lockfile.
- **Delta updates (bsdiff+zstd, signed)** and cross-target-assembled installers;
  signing/notarization remains an exercised per-platform credential flow.

The current implementation is a vertical slice: the CLI can scaffold and diagnose a
hello project, run an authenticated Bun-to-Rust kipc echo, and open the project HTML in
the platform webview. Bun supervision, the TypeScript packages, general native brokers,
packaging, migration, updates, and strict-profile containment remain roadmap work.

## Workspace layout

```
crates/   keld-core · keld-wv · keld-ipc · keld-guard · keld-native · keld-runtime
          keld-update · keld-pack · keld-compat · keld-host (bin) · keld-cli (bin)
packages/ @keld/electron (KEL-72) · @keld/api · @keld/web · @keld/cli · @keld/schema · create-keld (others upcoming)
```

## Development

See [`AGENTS.md`](AGENTS.md) for engineering rules and verification gates.

```bash
cargo nextest run --workspace --profile ci
just hello    # launch the current platform hello backend (Phase 1 slice)
```

## License

MIT OR Apache-2.0 — [`LICENSE`](LICENSE), [`LICENSE-MIT`](LICENSE-MIT),
[`LICENSE-APACHE`](LICENSE-APACHE), and workspace `Cargo.toml`. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) and [`SECURITY.md`](SECURITY.md).
