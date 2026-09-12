# KELD

Keld is a desktop framework being built around a Rust host, Bun application processes,
and the operating system's webview. Its goal is to make Electron applications smaller
and easier to secure while measuring compatibility against real application behavior.
By [GYLDLAB](https://github.com/gyldlab).

**Pre-alpha: build from source.** The current runnable slice scaffolds an application,
opens a native webview window, and runs an authenticated IPC echo in a supervised Bun
process. Electron migration, application installers, and signed updates are future work.

## What works today

- `keld create`, `keld dev`, and `keld doctor` support the current hello application.
- The Rust host owns the window and Bun lifecycle; kipc provides authenticated app-link
  communication, framed messages, and typed codecs.
- Permission parsing and guard-before-handler checks exist. Broad native services and
  complete strict-profile admission remain incomplete.
- `@keld/electron` implements a limited application-lifecycle facade. Its
  [compatibility board](docs/engineering/compat-scoreboard.md) records the known
  matches and divergences; broad Electron compatibility is not measured yet.

The generated [Current/Target/Evidence ledger](docs/engineering/product-status.md)
is the source of truth for implemented scope and its evidence. These rows summarize
its no-flag app-session implementation and the latest acceptance limitations; they
do not establish release support:

| Platform | Current app-session slice | Qualification still needed |
|---|---|---|
| macOS / WKWebView | Native window, app link, recovery, ordered Quit/CLI-loss cleanup | Stock app native Close remains unverified; complete strict profiles and release packaging |
| Windows / WebView2 | Native window, named-pipe app link, recovery, Ctrl-C cleanup and relaunch | Stock app native Close remains incomplete; remaining strict admission and release packaging |
| Ubuntu/Debian x86_64 / WebKitGTK / Wayland | Window, authenticated link, strict Bun generations, recovery | Native Close/cleanup/relaunch qualified on the [PR #228 candidate](https://github.com/gyldlab/keld/pull/228) for Ubuntu 26.04.1 / GNOME Wayland; X11 product runs, other distributions/architectures, and release packaging remain unverified |

## Try it

Use the [source-build quick-start](docs/onboarding/README.md#run-the-current-demo)
for prerequisites, expected output, and Windows instructions. With Rust and Bun
installed, run this in a macOS or Ubuntu/Debian x86_64 Wayland desktop terminal
after checking the platform prerequisites and current limitations below:

```bash
git clone https://github.com/gyldlab/keld.git
cd keld
cargo build --locked -p keld-cli -p keld-host
./target/debug/keld create hello-keld
cd hello-keld
../target/debug/keld doctor
../target/debug/keld dev
```

The expected result is a `hello-keld` window. Captured Bun output, including
`IPC echo ok`, appears at shutdown. Maintainer-recorded, source-pinned public evidence
covers the [Windows build/window/Ctrl-C/relaunch run](https://github.com/gyldlab/keld/issues/174#issuecomment-5589117723)
and the [Ubuntu build/create/doctor/window/Ctrl-C/relaunch run](https://github.com/gyldlab/keld/issues/175#issuecomment-5588845871).
Both used Bun 1.4.0; Windows used interactive PowerShell. These dated interrupt runs remain
separate from native-Close acceptance.

The [qualified Linux native-Close evidence](docs/onboarding/README.md#qualified-linux-native-close-evidence)
for [PR #228](https://github.com/gyldlab/keld/pull/228), source `a843b32`, updates the
historical failure status for that tested environment only. The earlier
[Ubuntu/Wayland failure record](https://github.com/gyldlab/keld/issues/167#issuecomment-5575535917)
remains linked as history. The [lifecycle issue](https://github.com/gyldlab/keld/issues/176)
tracks the stock-app correction. The Rust executables are built from source; there is
no npm installation or packaged app release yet.

## Evidence

- [Product status](docs/engineering/product-status.md): code and tests behind every
  current capability, with target-only work called out.
- [Electron compatibility](docs/engineering/compat-scoreboard.md): implemented
  lifecycle behavior and divergences. No product corpus percentage is published yet.
- [Benchmarks](https://github.com/gyldlab/keld-benches) and the
  [measurement scoreboard](docs/engineering/budget-scoreboard.md): OS-qualified
  fixtures, pinned measurements, and limitations. Historical hello measurements do not
  establish a performance claim for a complete migrated application.
- [CI](https://github.com/gyldlab/keld/actions/workflows/ci.yml): current automated
  checks. A CI run and a real desktop acceptance run are different evidence.
- Public, source-pinned device records: [Windows/WebView2](https://github.com/gyldlab/keld/issues/174#issuecomment-5589117723),
  [initial Ubuntu/WebKitGTK](https://github.com/gyldlab/keld/issues/167#issuecomment-5575535917),
  and the [Ubuntu refresh](https://github.com/gyldlab/keld/issues/175#issuecomment-5588845871).
  These maintainer-recorded observations preserve their platform limits and do not
  establish acceptance for another OS, session type, or exit path.

## Roadmap

The public [roadmap](ROADMAP.md) explains the next prerequisites and their exit
criteria. [GitHub Issues](https://github.com/gyldlab/keld/issues) is the public place to
report defects, find contribution opportunities, and follow work.

## Target

Keld aims to provide a prebuilt Rust host, supervised Bun application roles, typed
kipc, and generated host-enforced default-deny permissions. Electron applications
would migrate through a compatibility facade with unsupported behavior visible.

`keld migrate` and `keld build` are reserved commands today. VS Code migration is a
future stress workload that depends on those framework contracts; it is not a current
demo. The [architecture](docs/architecture/01-overview.md) defines the destination.

## Contribute

Start with [CONTRIBUTING.md](CONTRIBUTING.md), the
[maintainers](MAINTAINERS.md), and the [Code of Conduct](CODE_OF_CONDUCT.md).
Report vulnerabilities through [SECURITY.md](SECURITY.md). Public code, documentation,
and tests are sufficient to contribute; private research is optional.

## License

MIT OR Apache-2.0 — [LICENSE](LICENSE), [LICENSE-MIT](LICENSE-MIT),
[LICENSE-APACHE](LICENSE-APACHE).
