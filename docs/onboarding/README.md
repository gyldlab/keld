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
- Bun on `PATH`. **Use CI-pinned Bun 1.4.0 for `just ci` and the full workspace test
  gate.** The [Linux strict fixture](../../crates/keld-runtime/tests/linux_strict_boundary.rs)
  asserts that exact version. Record `bun --version` and `bun --revision` before a
  run. The recorded Mac and Linux demo observations used Bun 1.4.2; those observations
  do not satisfy the full gate's version prerequisite. The scaffold has no package
  dependencies to install.
- A desktop session and platform build prerequisites:

  | Platform | Prerequisites and qualification |
  |---|---|
  | macOS | Apple command-line developer tools (`xcode-select --install` if absent); WKWebView is supplied by macOS |
  | Windows | Rust's MSVC build tools and the WebView2 runtime; the current Windows acceptance lane has not built/run this candidate and awaits an existing endpoint-security prerequisite |
  | Ubuntu/Debian x86_64, Wayland | GTK3/WebKitGTK 4.1 development libraries, `pkg-config`, a C compiler, and trusted checkout/strict-launch prerequisites below; stock native Close currently fails, and X11 product runs and other distributions remain unverified |

The Ubuntu build packages are recorded in the [CI workflow](../../.github/workflows/ci.yml).
A desktop session and the required Linux containment primitives are separate from
compilation. If `dev` reports an unavailable security primitive, preserve its diagnostic
and report the unsupported environment instead of disabling the check.

On Linux, the [strict-launch path validator](../../crates/keld-runtime/src/linux_strict.rs)
also checks trusted ancestry: directories must be real, root- or current-user-owned
paths, and another principal must not be able to replace descendant launch entries
before an owner-private directory protects them. Group/world-writable ancestors can
therefore reject a launch even after a successful build and `doctor`.

Use an owner-controlled checkout. For a newly created checkout that you own, inspect
its mode before launching; a private `0700` checkout is a suitable starting point:

```bash
# Linux, from your newly created checkout root
stat -c '%U %a %n' .
# Only if this is your own checkout and it needs narrowing:
chmod 0700 .
```

The recorded Linux run rejected its new `0775` worktree; narrowing that directory
alone to `0700` admitted launch. Other ancestor layouts still need the diagnostic's
specific check. Do not recursively change permissions or alter shared/system
ancestors to force acceptance.

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

On macOS or Ubuntu/Debian x86_64 Wayland, with the prerequisites above, in the
repository root:

```bash
./target/debug/keld create hello-keld
cd hello-keld
../target/debug/keld doctor
../target/debug/keld dev
```

On Windows, these are the source-checked PowerShell commands. The current Windows
acceptance lane has not built or run this candidate; its requalification awaits an
existing endpoint-security prerequisite. An unrun acceptance test does not change
the ledger's implemented scope. Start in
the repository root:

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
3. After inspecting the demo, Ctrl-C requests shutdown. Captured Bun output is
   forwarded at shutdown; do not wait for these lines while the window is open:

   ```text
   ipc-echo ok: message="keld" count=1
   hello-keld: main process ready (IPC echo ok)
   ```

4. Check that the host/Bun session and its nonce directory under `.keld/dev` are gone
   before a new run. Ctrl-C cleanup was observed on the recorded Mac and Linux devices;
   it is a separate acceptance case from native window Close.

**Normal-close acceptance is currently incomplete.** On physical Ubuntu/Wayland,
clicking the stock app's Close button removed the renderer but left CLI, host, Bun,
and the launch stage alive beyond an independent 20-second observation. Subsequent
Ctrl-C cleaned them up; it did not fix normal Close. The
[lifecycle issue](https://github.com/gyldlab/keld/issues/176) tracks the separate
stock-app correction and its required native-close regression. The Mac
probe exercised SIGINT rather than the Close button, and Windows was not executed.

The Linux relaunch/early-interrupt probe also retained `KELD-CORE-037` during startup,
although cleanup succeeded. Treat that as unresolved evidence; a fresh frame or
successful cleanup alone is not a fully ready, clean relaunch result.

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
[justfile](../../justfile). Put the CI-pinned Bun 1.4.0 binary on this shell's `PATH`
without replacing another project's global runtime, then verify the selected version:

```bash
bun --version   # must report 1.4.0 for the full gate
bun --revision
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
