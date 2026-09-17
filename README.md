# KELD

**Keep your JavaScript. Change the desktop foundation.**

KELD is an open-source desktop framework for JavaScript and TypeScript.
Your app logic runs in **Bun**. A **Rust host** manages the native window and
application lifetime. Your interface runs in the platform's **system webview**,
without bundling a separate Chromium engine.

The goal is a path beyond Electron **without rewriting your application's core
in another language**.

> **Pre-alpha.** Start with the source-built demo below. KELD is not yet a
> drop-in Electron replacement or a production-ready distribution toolchain.

[Run the demo](#run-the-demo) · [How it works](#how-it-works) ·
[Coming from Electron](#coming-from-electron) · [Benchmarks](#early-benchmarks) ·
[Docs](docs/onboarding/README.md) · [Roadmap](ROADMAP.md)

## Early benchmarks

**KIPC round-trip latency · Apple M4 Mac mini · macOS 26.5.1 / arm64**

| Message payload | Recorded p99 round trip |
|---|---:|
| **6 bytes** | **9.375 µs** |
| **1,024 bytes** | **10.25 µs** |

These September 10, 2026 measurements use two separate **Rust processes** over
an authenticated Unix socket. They measure the KIPC library, **not** Bun-to-host
latency, app startup, or a complete KELD application. P99 is the latency threshold
covering 99% of the sampled round trips.

[Fixture and reproduction steps](https://github.com/gyldlab/keld-benches/tree/b7137c1c2107e0f622eb350819d32605be7a781f/macos/keld/kipc-rust-echo) ·
[Raw sessions](https://github.com/gyldlab/keld-benches/tree/b7137c1c2107e0f622eb350819d32605be7a781f/macos/bench/results/ipc-rtt) ·
[More measurements](docs/engineering/budget-scoreboard.md)

<details>
<summary>Measurement details</summary>

Each payload tier uses **20 independent sessions × 100,000 calls**. The first
call is recorded with the handshake; the remaining **99,999 calls per session**
are timed separately. The table reports pooled p99, not the median of session
percentiles. Reported session-block bootstrap 95% intervals are
**9.25–9.625 µs** and **10.083–10.459 µs**, respectively.

KELD source: `4fbf94bbb755854058067b986877177f00b25a39`.
Fresh client/server processes per session; client-owned monotonic clock;
handshake excluded from the per-call timings.
[Campaign record](https://github.com/gyldlab/keld-benches/pull/21).
These are recorded results for that revision and machine, not a fresh benchmark
of the current branch or a comparison with another framework.

</details>

## Run the demo

You need **Git**, **Rust through rustup**, and **Bun on `PATH`**, plus the
prerequisites for your platform. The repository selects its Rust toolchain.

| Platform | Before you build |
|---|---|
| **macOS** | Apple command-line developer tools. WKWebView comes with macOS. |
| **Windows** | Rust's MSVC build tools and the WebView2 runtime. Use an interactive PowerShell session. |
| **Linux** | The current product path targets Ubuntu/Debian x86_64 on Wayland. Install GTK3/WebKitGTK 4.1 development libraries, a C compiler, `pkg-config`, and `bwrap`; use an owner-controlled checkout and a host that supports the required containment. |

[Platform setup and troubleshooting](docs/onboarding/README.md#prerequisites).
Contributors running the full `just ci` suite need **Bun 1.4.2**.

### Build KELD

```sh
git clone https://github.com/gyldlab/keld.git
cd keld
cargo build --locked -p keld-cli -p keld-host
```

Build **both** crates: the development command needs the host beside the CLI.
On Linux this also builds the required role launcher. No npm wrapper or prebuilt
release is available yet.

### Create and launch an app

**macOS / qualified Linux environment:**

```sh
./target/debug/keld create hello-keld
cd hello-keld
../target/debug/keld doctor
../target/debug/keld dev
```

**Windows PowerShell:**

```powershell
.\target\debug\keld.exe create hello-keld
Set-Location hello-keld
..\target\debug\keld.exe doctor
..\target\debug\keld.exe dev
```

You should see a native window titled **hello-keld**. Behind it, the demo starts
a Bun application process and exchanges an authenticated echo with the Rust host.
Captured Bun output appears **when the session shuts down**, not while you wait
at the open window:

```text
ipc-echo ok: message="keld" count=1
hello-keld: main process ready (IPC echo ok)
```

A successful `doctor` checks setup; the window, echo, clean exit, and successful
relaunch are separate checks. The [run guide](docs/onboarding/README.md#run-the-current-demo)
explains normal close, interrupt shutdown, and platform-specific diagnostics.

### Make your first change

Open `hello-keld/index.html`, change the visible text or styles, then stop and
restart `keld dev`. **Live reload is not implemented yet.**

| File in your generated app | What it controls |
|---|---|
| `index.html` | The interface rendered in the native window. |
| `src/main.ts` | The Bun-side app logic, including the demo's echo and shutdown handling. |
| `keld.config.ts` | The app's configuration, including its name and entry paths. |

The scaffold also includes its KIPC transport; there are no package dependencies
to install for this demo. Start it through `keld dev`, not `bun src/main.ts`:
the host supplies the authenticated application link.

## How it works

**Your interface and your main process are different things.** Browser-side
JavaScript runs in the webview. Application-side JavaScript or TypeScript runs
in Bun. Rust owns the native window, starts and supervises Bun, and coordinates
the application session.

KIPC is KELD's inter-process communication protocol. It carries authenticated
messages between Bun and the Rust host; it is not another JavaScript runtime.

```mermaid
flowchart TB
    accTitle: Current KELD runtime responsibilities
    accDescr: The Rust host owns the native window and application session and supervises a separate Bun process running src/main.ts. Bun and the host exchange authenticated KIPC messages. The host owns the system webview, where index.html and browser JavaScript run. The engine is WKWebView on macOS, WebView2 on Windows, or WebKitGTK on Linux. A general renderer-to-host bridge is not implemented.

    HOST["Rust host<br/>Window + app lifetime<br/>Supervises Bun"]
    BUN["Bun process<br/>src/main.ts"]
    VIEW["System webview<br/>index.html<br/>Browser JavaScript"]
    ENGINE["OS webview engine<br/>macOS: WKWebView<br/>Windows: WebView2<br/>Linux: WebKitGTK"]

    HOST <-->|"Authenticated<br/>KIPC"| BUN
    HOST -->|"Owns"| VIEW
    VIEW -->|"Chosen by OS"| ENGINE

    classDef current fill:#dcfce7,stroke:#15803d,color:#052e16,stroke-width:2px
    classDef external fill:#e2e8f0,stroke:#475569,color:#0f172a,stroke-width:2px
    class HOST current;
    class BUN,VIEW,ENGINE external;
```

This is the current demo's runtime layout, not a diagram of every planned API.
The general **renderer-to-host bridge is still incomplete**. See the
[implementation status](docs/engineering/product-status.md) and
[architecture overview](docs/architecture/01-overview.md) for the detailed boundaries.

### Closing the window closes the session

The stock demo does more than display a page. When its last window closes, the
host sends `LastWindowClosed` to the Bun app. The app requests an orderly quit over
the same KIPC link,
then exits after the host replies.

```mermaid
sequenceDiagram
    accTitle: Current stock demo normal-close sequence
    accDescr: After the last native window closes, the Rust host sends LastWindowClosed over the authenticated KIPC link. The stock Bun app sends a Quit request on the same link, waits for its reply, closes its connection and exits. Session cleanup and captured output forwarding finish before relaunch. Physical normal-close evidence is scoped to the linked Windows and Ubuntu captures.
    participant H as Rust host
    participant B as Bun app

    H->>H: Last window closes
    H-->>B: Window-closed event
    B->>H: Quit request
    H-->>B: Quit reply
    B->>B: Close link<br/>and exit
    Note over H,B: Cleanup completes<br/>Output reaches the terminal
```

[Template implementation](crates/keld-cli/templates/hello/src/main-body.ts).
Native Close → cleanup → relaunch has recorded device evidence on
[Windows 11 x64](docs/onboarding/README.md#qualified-windows-native-close-evidence)
and [Ubuntu 26.04.1 / GNOME Wayland](docs/onboarding/README.md#qualified-linux-native-close-evidence).
Those records qualify their captured revisions and environments; macOS native
Close is not qualified by those captures.

## Coming from Electron

The destination is familiar JavaScript and TypeScript, with fewer changes to your
application. **Today, begin with the demo rather than migrating a production app.**
The current `@keld/electron` module implements a small lifecycle surface, not the
full Electron API.

| Surface | What to expect today |
|---|---|
| `app.whenReady()` and lifecycle events | Implemented in the current compatibility module. |
| `window-all-closed` | With no listener, the default is to quit. With a listener, the application decides. |
| `app.quit()` | Returns `Promise<void>` so callers can await the host's reply. Electron returns `void`. |
| Event-listener errors | Listeners are isolated: one throwing listener does not abort the KIPC reader. This differs from Electron's EventEmitter behavior. |
| `BrowserWindow`, `ipcMain`, broader Electron modules | Not implemented by the current compatibility module. |
| Native addons and automatic migration | No general native-addon compatibility guarantee; `keld migrate` is not implemented yet. |

[Compatibility details and tests](docs/engineering/compat-scoreboard.md) ·
[Current module exports](packages/@keld/electron/src/index.ts)

### What changes with system webviews?

KELD uses the platform's webview instead of including its own Chromium engine.
That changes the rendering environment: macOS and Linux use WebKit-based
backends; Windows uses WebView2. **Identical browser behavior across all three
platforms is not guaranteed.** Test the web APIs, CSS, media, and rendering your
app actually uses on each intended platform.

The [webview architecture](docs/architecture/05-webview-and-native.md) describes
the design; [current implementation status](docs/engineering/product-status.md)
distinguishes it from available functionality. Linux X11, other distributions,
and other architectures remain separately unverified for the product launch path.

## What is ready, and what is next?

The source-built window, supervised Bun process, authenticated application link,
and limited lifecycle compatibility are implemented. KELD is ready for
**evaluation and contributions**, not production adoption yet.

| Area | Current boundary |
|---|---|
| Permissions | Manifest parsing and guard-before-handler checks exist. The scoped filesystem broker is wire-tested but not connected to the production host; the broader native-service surface is unfinished. |
| Process isolation | Linux strict launch is implemented for the current primary process. Full strict-profile admission across platforms remains incomplete; authenticated IPC alone is not a complete sandbox. |
| Building and shipping apps | `keld build`, packaged releases, installers, signing, and signed updates are not available yet. |

The [product-status ledger](docs/engineering/product-status.md) tracks the
implementation; the [roadmap](ROADMAP.md) describes the destination.

## Explore and contribute

Built by [GYLDLAB](https://github.com/gyldlab), in the open.

| Your next step | Start here |
|---|---|
| Run or troubleshoot an app | [Onboarding](docs/onboarding/README.md) · [Error reference](docs/engineering/keld-error-codes.md) |
| Understand the runtime | [Architecture overview](docs/architecture/01-overview.md) |
| Explore performance | [KELD Benches](https://github.com/gyldlab/keld-benches) |
| Work with a coding agent | [llms.txt](llms.txt) · [Local read-only MCP server setup](docs/onboarding/07-mcp-server.md) |
| Make a contribution | [Contributing](CONTRIBUTING.md) · [Open issues](https://github.com/gyldlab/keld/issues) |

A useful first contribution is a real run report: OS, source revision, Bun
version, commands, and what worked or failed. Public code, docs, and issues are
sufficient; private research access is not required.

Report vulnerabilities through [SECURITY.md](SECURITY.md), not a public issue.

## License

**MIT OR Apache-2.0** · [MIT](LICENSE-MIT) · [Apache-2.0](LICENSE-APACHE)
