# Run and contribute to Keld

Keld is a pre-alpha desktop framework. This guide runs its current source-built
application slice: a native webview window, a supervised Bun main process, and an
authenticated kipc echo. The [product-status ledger](../engineering/product-status.md)
distinguishes this implementation from the target architecture. There is no npm
wrapper or packaged application release to install yet.

## Prerequisites

- Git and Rust installed through rustup. The checked-in
  [rust-toolchain.toml](../../rust-toolchain.toml) selects the compiler and components;
  Cargo uses the committed lockfile in the commands below.
- Bun on `PATH`. [CI](../../.github/workflows/ci.yml) pins Bun 1.4.0; record `bun --version`
  and `bun --revision` when reporting a run with another version. The scaffold has no
  package dependencies to install.
- A desktop session and platform build prerequisites:

  | Platform | Prerequisites and qualification |
  |---|---|
  | macOS | Apple command-line developer tools (`xcode-select --install` if absent); WKWebView is supplied by macOS |
  | Windows | Rust's MSVC build tools and the WebView2 runtime; use PowerShell commands below |
  | Ubuntu/Debian x86_64, Wayland | GTK3/WebKitGTK 4.1 development libraries, `pkg-config`, a C compiler, and the strict-launch prerequisites in the [Linux runtime contract](../../crates/keld-runtime/src/linux_strict.rs); X11 product runs and other distributions remain unverified |

The Ubuntu build packages are recorded in the [CI workflow](../../.github/workflows/ci.yml).
A desktop session and the required Linux containment primitives are separate from
compilation. If `dev` reports an unavailable security primitive, preserve its diagnostic
and report the unsupported environment instead of disabling the check.

## Run the current demo

Clone the public repository, or start in an existing checkout. Record the exact source
and runtime versions with a result:

```bash
git clone https://github.com/gyldlab/keld.git
cd keld
git rev-parse HEAD
bun --version
bun --revision
cargo build --locked -p keld-cli -p keld-host
```

**Build both crates.** `keld dev` requires `keld-host` next to the CLI executable;
Linux also needs the sibling `keld-role-launcher` binary built by `keld-host`.
Building only `keld-cli` is insufficient in a fresh checkout. If using a custom Cargo
target directory, substitute that directory for `target` in the following paths.

On macOS or qualified Linux, in the repository root:

```bash
./target/debug/keld create hello-keld
cd hello-keld
../target/debug/keld doctor
../target/debug/keld dev
```

On Windows, in PowerShell at the repository root:

```powershell
.\target\debug\keld.exe create hello-keld
Set-Location hello-keld
..\target\debug\keld.exe doctor
..\target\debug\keld.exe dev
```

The command creates a new `hello-keld` directory; it refuses to overwrite an existing
one. Expected behavior:

1. `create` reports the new project path; `doctor` reports successful environment and
   project checks.
2. `dev` opens a native window titled `hello-keld`. Its content says that IPC echo runs
   in the Bun main process.
3. Close the window to end the session (or use Ctrl-C). Captured Bun output is
   forwarded at shutdown; do not wait for these lines before closing the window:


   ```text
   ipc-echo ok: message="keld" count=1
   hello-keld: main process ready (IPC echo ok)
   ```

4. The host and its supervised process are cleaned up; rerunning `dev` starts a fresh
   session.

The generated app is the canonical current example. Its `index.html` is the renderer;
`src/main.ts` contains the app-link client and echo. It is not an Electron migration
example, and changing HTML requires restarting the current dev session.
`just hello` is a separate diagnostic backend window; it does not exercise the Bun
application session above.

## Report a run or a problem

Post to a relevant [public issue](https://github.com/gyldlab/keld/issues), or open one
with your source SHA, OS/architecture/session, Bun version/revision, exact commands,
expected versus actual behavior, and sanitized output. State separately whether the
build, doctor, rendered window, IPC output, normal close, and relaunch succeeded.
Never infer other-platform acceptance from a successful local run.

For `KELD-CLI-*`, `KELD-CORE-*`, or `KELD-WV-*` failures, keep the full diagnostic and
follow its suggested correction. The [error registry](../engineering/keld-error-codes.md)
explains the contracts. A missing host usually means both crates were not built into
the same output directory. A successful `doctor` alone is not a desktop acceptance run.

## Contribute and verify

Start with [CONTRIBUTING.md](../../CONTRIBUTING.md). Install `just`, `cargo-nextest`,
and `cargo-deny` for the complete local gate; its Mermaid renderer also requires a
running Docker-compatible engine. The exact gate inventory belongs to the
[justfile](../../justfile):

```bash
just ci
```

The [development guide](05-development-guide.md) explains test commands and maintainer
procedures. Report real results and unavailable checks when opening a draft PR.

## Explore the contracts

| Document | Purpose |
|---|---|
| [Project summary](01-project-summary.md) | Product purpose and source/target distinction |
| [Architecture guide](02-architecture-guide.md) | Crate ownership, processes, and trust boundaries |
| [API and CLI surface](03-api-and-cli-surface.md) | Implemented command shapes and errors |
| [Wire formats and contracts](04-wire-formats-and-contracts.md) | kipc bytes, handshake, and codec contracts |
| [Development guide](05-development-guide.md) | Verification and maintainer coordination |
| [Documentation map](06-documentation-map.md) | Authoritative and exploratory documentation |
| [MCP client setup](07-mcp-server.md) | The local read-only MCP server |
| [Optional contributor memory](08-optional-agent-memory.md) | An optional external pilot, unrelated to running Keld |

## Agent-readable documentation

Tracked authoritative docs are projected into [llms.txt](../../llms.txt) and
[llms-full.txt](../../llms-full.txt). The compact index defines the exact corpus;
exploratory research and local-only material are excluded. After changing an included
source, run `just llms`, `just llms-test`, and `just llms-check`.
