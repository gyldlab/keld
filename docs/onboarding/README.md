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
- Bun on `PATH`. **Use CI-pinned Bun 1.4.2 for `just ci` and the full workspace test
  gate.** The [Linux strict fixture](../../crates/keld-runtime/tests/linux_strict_boundary.rs)
  asserts that exact version. Record `bun --version` and `bun --revision` before a
  run. Some earlier demo observations used Bun 1.4.2; the newer
  [Ubuntu](https://github.com/gyldlab/keld/issues/175#issuecomment-5588845871) and
  [Windows](https://github.com/gyldlab/keld/issues/174#issuecomment-5589117723)
  quick-start observations used the then-pinned Bun 1.4.0. Those dated observations
  do not establish a current full-gate result. The scaffold has no package
  dependencies to install.
- A desktop session and platform build prerequisites:

  | Platform | Prerequisites and qualification |
  |---|---|
  | macOS | Apple command-line developer tools (`xcode-select --install` if absent); WKWebView is supplied by macOS |
  | Windows | Rust's MSVC build tools and the WebView2 runtime; use an interactive PowerShell session for the source-build/Ctrl-C path; stock native Close/cleanup/relaunch is qualified only for the [Windows 11 x64 capture](https://github.com/gyldlab/keld/issues/174#issuecomment-5644140405) |
  | Ubuntu/Debian x86_64, Wayland | GTK3/WebKitGTK 4.1 development libraries, `pkg-config`, a C compiler, and trusted checkout/strict-launch prerequisites below; native Close/cleanup/relaunch qualified on Ubuntu 26.04.1 / GNOME Wayland; X11 product runs and other distributions remain unverified |

The Ubuntu build packages are recorded in the [CI workflow](../../.github/workflows/ci.yml).
A desktop session and the required Linux containment primitives are separate from
compilation. If `dev` reports an unavailable security primitive, preserve its diagnostic
and report the unsupported environment instead of disabling the check.
Keld's strict Linux launch also requires `/usr/bin/bwrap` and a host policy that permits
its unprivileged user-namespace containment; distribution policies differ, so preserve
the exact diagnostic instead of weakening the host policy.

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

The [initial public Linux run](https://github.com/gyldlab/keld/issues/167#issuecomment-5575535917)
rejected its new `0775` worktree; narrowing that directory
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

On Windows, these are the source-checked PowerShell commands. The
[maintainer-recorded final-source Windows run](https://github.com/gyldlab/keld/issues/174#issuecomment-5589117723)
used an interactive PowerShell session and verified the source build, native window,
captured shutdown output, Ctrl-C cleanup, and relaunch. A redirected PowerShell 5
pipeline or a shell/PTY that exits immediately is not that acceptance recipe. Start in
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
3. After inspecting the demo, use native Close on the qualified Ubuntu environment
   above; Ctrl-C remains a separate interrupt-shutdown path. Captured Bun output is
   forwarded at shutdown; do not wait for these lines while the window is open:

   ```text
   ipc-echo ok: message="keld" count=1
   hello-keld: main process ready (IPC echo ok)
   ```

4. Check that the host/Bun session and its nonce directory under `.keld/dev` are gone
   before a new run. The linked Windows and Ubuntu quick-start records observed Ctrl-C
   cleanup; Windows stock native-Close evidence is separately source-qualified below.
   See [qualified Linux native-Close evidence](#qualified-linux-native-close-evidence)
   for that environment's normal-close path.

### Qualified Linux native Close evidence

The historical
[public Ubuntu/Wayland failure record](https://github.com/gyldlab/keld/issues/167#issuecomment-5575535917)
reports that clicking the stock app's Close button removed the renderer but left CLI,
host, Bun, and the launch stage alive beyond an independent 20-second observation.
The later [Ubuntu quick-start refresh](https://github.com/gyldlab/keld/issues/175#issuecomment-5588845871)
verified Ctrl-C cleanup and relaunch but did not retry native Close. The stock-app
correction is in [PR #228](https://github.com/gyldlab/keld/pull/228). Two untouched stock
runs on its source `a843b32` passed native Close, cleanup, and relaunch on Ubuntu
26.04.1 x86_64 with GNOME 50.1 Wayland, Bun 1.4.2, and WebKitGTK 2.52.6. Both CLI
processes exited 0; 20 observed process identities exited, two distinct launch stages
were absent, and no forced cleanup was used. This evidence qualifies that source and
environment only. No public Mac device record is cited here, so macOS native Close
remains unverified.

### Qualified Windows native Close evidence

The earlier
[Windows build/window/Ctrl-C/relaunch record](https://github.com/gyldlab/keld/issues/174#issuecomment-5589117723)
remains historical and does not establish stock native Close.
The [Windows stock native-Close capture](https://github.com/gyldlab/keld/issues/174#issuecomment-5644140405)
qualifies two runs on source `62f4cc1a71a02db8d3b3a5959f2bbf6ba1a9a0c8`, Windows 11 x64,
and Bun 1.4.2 revision `1.4.2+744846f84`; both CLI exits were zero, captured process
identities exited, and both stages were absent at 20 seconds without forced cleanup.
Complete strict-profile admission and release packaging remain unverified.

The [newer Linux relaunch/early-interrupt record](https://github.com/gyldlab/keld/issues/175#issuecomment-5588845871)
tested [PR #184](https://github.com/gyldlab/keld/pull/184) and produced no `KELD-CORE`
diagnostic while passing cleanup/relaunch observables. That cancellation fix remains a
separate unmerged candidate, so this documentation change does not claim it landed on
`main`.

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

Start with [CONTRIBUTING.md](../../CONTRIBUTING.md). Install the verification tools
listed in the [development guide](05-development-guide.md#1-prerequisites), including
Python and a running Docker-compatible engine. The exact gate inventory belongs to the
[justfile](../../justfile). Put the CI-pinned Bun 1.4.2 binary on this shell's `PATH`
without replacing another project's global runtime, then verify the selected version:

```bash
bun --version   # must report 1.4.2 for the full gate
bun --revision
just ci
```

On Windows, run the shell-based full gate from Git Bash or another environment with
the required GNU tools. Keep the interactive PowerShell session for the separate
native `dev`/Ctrl-C acceptance run.

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
