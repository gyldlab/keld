# KELD

**The desktop framework that replaces Electron without a rewrite.**
Rust core · Bun-powered JS/TS main process · system webviews · security by default.
By [GYLDLAB](https://github.com/gyldlab).

> Status: **pre-alpha** — public repo contains the implementation; research and
> architecture specs are maintained privately by the GYLDLAB team.

## The idea in 30 seconds

Every Electron alternative asks you to rewrite your app. Keld doesn't.

```bash
cd my-electron-app
bunx keld migrate   # analyzes your app, generates config, aliases electron → @keld/electron
bunx keld dev       # your app runs — no Chromium bundle, no Node, no rewrite
bunx keld build     # signed installers + kilobyte-scale delta updates
```

Under the hood, Keld replaces Electron's architecture, not its API:

- **A prebuilt Rust host** owns windows, webviews, and every native API — you never
  install a Rust toolchain.
- **Your JS/TS main process runs on Bun** as a supervised, unprivileged child process —
  full npm ecosystem, no ambient OS access, crashes don't take your windows down.
- **System webviews by default** (WebView2 / WKWebView / WebKitGTK) with a polyfill
  pack and per-platform engine policy.
- **Typed binary IPC** (schema-first, shared-memory bulk lane, backpressured).
- **Default-deny permissions** generated from your code, reviewed like a lockfile.
- **Delta updates (bsdiff+zstd, signed)** and cross-compiled installers.

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
just hello    # macOS: open WKWebView hello window (Phase 1 slice)
```

## License

MIT OR Apache-2.0 — [`LICENSE`](LICENSE), [`LICENSE-MIT`](LICENSE-MIT),
[`LICENSE-APACHE`](LICENSE-APACHE), and workspace `Cargo.toml`. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) and [`SECURITY.md`](SECURITY.md).
