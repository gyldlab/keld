# 05 — Development guide

How to work in this repo on day one: what to install, what to run, what will get your PR
sent back, and what to do when something breaks. For context, read
[`01-project-summary.md`](./01-project-summary.md) and
[`02-architecture-guide.md`](./02-architecture-guide.md) first; for what the code
actually exposes, see [`03-api-and-cli-surface.md`](./03-api-and-cli-surface.md) and
[`04-wire-formats-and-contracts.md`](./04-wire-formats-and-contracts.md).

This guide summarizes the rules; it does not replace them. The binding documents are
[`AGENTS.md`](../../AGENTS.md) at the repo root and the per-crate `AGENTS.md` files. When
this guide and those disagree, they win.

---

## 1. Prerequisites

| Tool | Required? | Install | Why |
|---|---|---|---|
| Rust 1.97.1 | Yes | nothing to do — [`rust-toolchain.toml`](../../rust-toolchain.toml) pins `channel = "1.97.1"` with `rustfmt` + `clippy`, and rustup installs it on your first `cargo` command in this directory | Every crate; edition 2024 |
| `cargo-nextest` | Yes | `cargo install cargo-nextest --locked` | The test gate is `cargo nextest run --workspace --profile ci` ([`.config/nextest.toml`](../../.config/nextest.toml)) |
| `cargo-deny` | For `just ci` / the supply-chain gate | `cargo install cargo-deny --locked` | The `deny` job in CI ([`deny.toml`](../../deny.toml)) |
| `just` | Optional, but assumed by the docs | `cargo install just` (or `brew install just`) | [`justfile`](../../justfile) is the canonical local mirror of CI |
| Bun | Yes in practice | https://bun.sh | `keld dev` spawns `bun run src/main.ts`; the `bun_echo` integration test **fails** (does not skip) without it |

You do **not** need Xcode-the-IDE or Node. `@keld/electron` is a zero-dependency
TS package (`bun` runs it directly); `keld create`'s hello template still has
nothing to `bun install`.

Versions this guide was written against (macOS, Darwin 25.5.0, aarch64):

```
rustc 1.97.1 (8bab26f4f 2026-07-14)     # matches rust-toolchain.toml
cargo 1.97.1 (c980f4866 2026-06-30)
cargo-nextest 0.9.140
cargo-deny 0.19.9
bun 1.4.0
just                                    # NOT installed on this machine — see §3.3 for raw equivalents
```

---

## 2. First run

```bash
git clone https://github.com/gyldlab/keld.git
cd keld

cargo build --workspace          # first build pulls tao/wry on macOS; a few minutes
just hello                       # == cargo run -p keld-host -- --hello
cargo run -p keld-cli -- doctor
```

`just hello` opens a 960×640 window titled "Keld" and blocks until you close it. Live
backends exist on all three platforms now: macOS (`WKWebView`), Windows (`WebView2`,
direct COM since KEL-65), and Linux (`WebKitGTK`, wry interim since KEL-28) —
([`crates/keld-wv/src/wkwebview/mod.rs`](../../crates/keld-wv/src/wkwebview/mod.rs),
[`webview2/mod.rs`](../../crates/keld-wv/src/webview2/mod.rs),
[`webkitgtk/mod.rs`](../../crates/keld-wv/src/webkitgtk/mod.rs)). The Linux backend
compiles, passes its unit tests, and its full crate/workspace suite is green on a real
Ubuntu box (GTK3 + `libwebkit2gtk-4.1-dev`) — but a real window opening has **not**
been visually verified anywhere yet (KEL-28 landed from a sandbox with no display
server; `gtk::init()` fails there with "Failed to initialize GTK", not a code defect).
Confirm on real Linux hardware/VM with a display before relying on it. `WvError::UnsupportedPlatform`
(`KELD-WV-001`) still exists for any other target. Everything else in the
workspace (kipc, the CLI, `create`, `doctor`, the echo tests) is cross-platform and
builds and tests on all three today.

Expected `doctor` output outside a project (macOS shown; Windows/Linux differ only in
the `webview` line's detail text — "Windows WebView2..." / "Linux WebKitGTK..."):

```
[ok] bun — found bun 1.4.0
[ok] project — no project directory (run inside a scaffolded app for layout checks)
[ok] webview — macOS WKWebView hello window available via `keld dev`
```

To see the whole vertical slice end to end:

```bash
cargo build -p keld-cli
KELD=$PWD/target/debug/keld          # from the repo root

cd /tmp && "$KELD" create my-app && cd my-app
"$KELD" dev
```

which prints `ipc-echo ok: …` and `my-app: main process ready (IPC echo ok)` and opens
the window. Close the window (or Ctrl-C the terminal) to end the session. Confirmed on
macOS and Windows with a real display. Linux now has both the X11 Xvfb/window-manager
smoke (`xdotool search --name Keld`) and native Ubuntu GNOME Wayland no-flag product
evidence for rendered navigation, two calls, recovery, ordered teardown, strict
descendant reaping, stage cleanup, and relaunch. A real X11 product run remains open.

---

## 3. The verification gate

### 3.1 Mandatory core Rust subset

These three are mandatory, but they are only the core Rust subset of the exact full
local gate, `just ci`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --profile ci
```

Run `cargo fmt --all` (no `--check`) to fix formatting, then run `just ci` before every
push and report **real output** in the PR — never "should work". If a path
only exists on another OS, say so plainly rather than claiming coverage you do not have.

### 3.2 Report fresh results, not an onboarding snapshot

Run `just ci` against the exact checkout being handed off and quote its real exit status
and summary, including the core Rust subset. Test totals, durations, commit ids and dirty-tree shape change
too often to embed here. A platform-gated module compiling on this machine is not proof
that its window, sandbox, installer or updater behavior ran on the target OS.

The additional gates that `just ci` adds beyond the three include:

```console
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    Generated target/doc/keld_cli/index.html and 11 other files
                                            # exit 0

$ cargo deny check
advisories ... (yanked crates may still fail independently), bans ok, licenses ok, sources ok
```

**`cargo deny check` licenses pass on this branch** with a *per-crate* MPL exception
for `option-ext@0.2.0` (KEL-54). Do not add MPL-2.0 to the global `allow` list.

*1 — `licenses`: `option-ext` 0.2.0 is MPL-2.0, reached through wry → dirs → dirs-sys.*
Packed binaries must preserve notices and offer that crate's corresponding source
([`docs/engineering/third-party-licenses.md`](../engineering/third-party-licenses.md)).
The exception is pinned in [`deny.toml`](../../deny.toml):

```
exceptions = [
  { crate = "option-ext@0.2.0", allow = ["MPL-2.0"] },
]
```

*2 — `advisories`: a yanked crate in the lockfile may still fail independently,*
reached through `postcard` → `heapless` → `spin`. That gate tracks live RustSec data
and is unrelated to the MPL pin. Resolving a yanked crate is a dependency review gate,
not a drive-by lockfile bump.

`bans` and `sources` pass. `deny.toml`'s `[graph] targets` includes Darwin triples, so
CI's Ubuntu `deny` job evaluates the same macOS dependency set.

### 3.3 `just` targets, and the raw commands if you skip `just`

| `just` target | Equivalent command |
|---|---|
| `just hello` | `cargo run -p keld-host -- --hello` |
| `just fmt` | `cargo fmt --all` |
| `just fmt-check` | `cargo fmt --all --check` |
| `just clippy` | `cargo clippy --workspace --all-targets -- -D warnings` |
| `just test` | `cargo nextest run --workspace --profile ci` |
| `just doc` | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` |
| `just deny` | `cargo deny check` |
| `just mermaid-test` / `just mermaid-check` / `just mermaid-render-check` | validator tests / tracked structural policy / isolated digest-pinned SVG render |
| `just llms-test` / `just llms-check` | generated-corpus contract tests / freshness check |
| `just ci` | Full local gate; the `justfile` `ci` recipe is the sole source of its inventory and order. |

`just ci` is the local mirror of CI, minus the three-OS matrix and the manual Mermaid
visual-inspection/report step. If it is green and you only touched
cross-platform code, CI usually agrees.

---

## 4. Running tests

```bash
cargo nextest run --workspace --profile ci     # the gate; no retries, a flake fails here
cargo nextest run -p keld-ipc                  # one crate
cargo nextest run -p keld-ipc -- frame         # one crate, substring filter
cargo test --workspace                         # fallback if nextest is unavailable
```

The `ci` profile declares no retries, so a test that fails once fails the gate. That is
a property of the profile, not of the command: `--retries N` and `NEXTEST_RETRIES=N`
still override it, and neither belongs in a verification run (KEL-112).

`cargo nextest run -p keld-ipc -- frame` applies a substring filter. Quote its fresh
summary when using it; the selected test/binary/skipped totals are not a documentation
contract.

Where the tests live:

| Test | File |
|---|---|
| kipc echo over a real app-link transport (UDS / Windows named pipe) | [`crates/keld-ipc/tests/echo_link.rs`](../../crates/keld-ipc/tests/echo_link.rs) |
| Bun main process performing an IPC echo end to end | [`crates/keld-cli/tests/bun_echo.rs`](../../crates/keld-cli/tests/bun_echo.rs) |
| Frame header encode/decode, bad magic, unknown kind | `crates/keld-ipc/src/frame.rs` (colocated `mod tests`) |
| Echo payload round-trip | `crates/keld-ipc/src/echo.rs` |
| Project-name validation, template file writing | `crates/keld-cli/src/create.rs` |
| `WebviewSpec` default, `NavTarget` variants, `WvError` messages carry code + fix | `crates/keld-wv/src/{engine,error}.rs` |
| GPU-stack preparation exact-self re-execs with safe-mode; no live env mutation or export instruction | `crates/keld-wv/src/webkitgtk/mod.rs` `prepare_gpu_safe_mode_process` |

Two things to expect:

- **Test counts differ per platform.** `crates/keld-wv/src/hello/mod.rs` gates its test
  on `#[cfg(all(test, not(target_os = "macos")))]`, and the `wkwebview` module only
  compiles on macOS. Never compare raw totals across target OSes as if they were the
  same executed surface.
- **`bun_echo` needs Bun on `PATH`.** It calls `.expect("spawn bun")`, so a missing Bun
  is a test *failure*, not a skip.

Nextest does not execute doctests. If you add a runnable ```` ```rust ```` example to a
doc comment, run `cargo test --workspace --doc` explicitly and report its fresh result;
do not infer doctest coverage from nextest.

Anti-flake rules from `AGENTS.md` that the existing tests already follow, and yours must
too: no sleep-based synchronization (the echo tests wait on an `mpsc` ready signal), port
`0` for ephemeral ports, `tempfile::tempdir()` for filesystem work, tests colocated with
the code they cover, and a comment explaining *why* a non-obvious assertion exists.

---

## 5. What CI runs on your PR

[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) is the current source for
job count and trigger details. Its stable responsibilities are:

| Job | Runner(s) | What it does |
|---|---|---|
| `rustfmt` | ubuntu | `cargo fmt --all --check` |
| `clippy + test` | ubuntu **and** macos **and** windows, `fail-fast: false` | `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo nextest run --workspace --profile ci`; plus `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS: -D warnings` on ubuntu only |
| `MSRV` | ubuntu | reads `rust_version` out of `cargo metadata` and runs `cargo check --workspace --all-targets` on that exact toolchain — so the job can never drift from `Cargo.toml` |
| `cargo-deny` | ubuntu | licenses / advisories / bans / sources per `deny.toml` |
| `gitleaks` | ubuntu | checksum-pinned OSS CLI 8.30.1 (`gitleaks detect`), not the org-licensed GitHub Action |
| `CODEOWNERS + docs contracts` | ubuntu | compile+run `tools/ci_hygiene.rs`, validate generated llms docs, run Mermaid validator tests/check, then render tracked diagrams in the digest-pinned isolated container |

`fail-fast: false` on the matrix is deliberate: one platform failing must not hide the
other two, because `keld-wv` and `keld-native` diverge per platform by design. Actions
are SHA-pinned (`dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `taiki-e/install-action`,
`EmbarkStudios/cargo-deny-action`). The toolchain action requires `with: toolchain: 1.97.1`;
it does not auto-read `rust-toolchain.toml` (that file is for local rustup). CI does not
use an unpinned `cargo` on a random runner image.

---

## 6. Conventions that will get a PR rejected

Read [`AGENTS.md`](../../AGENTS.md) in full, then use [`.agents/index.md`](../../.agents/index.md)
to load only the task-matched playbooks. The greatest hits,
with the rationale, so you do not learn them from a review comment:

- **No `unwrap`, `expect`, `panic!` in library code.** Return typed errors with the
  repository's hand-written `Display` + stable-code contract. `expect` is allowed in
  tests and at the top level of `keld-cli`, and every sanctioned use states the invariant
  it relies on. `clippy.toml` makes the restriction lints test-aware; a site-local
  `#[allow]` needs a comment saying why.
- **`clippy::pedantic` and `missing_docs` are on workspace-wide** ([`Cargo.toml`](../../Cargo.toml)
  `[workspace.lints]`), and CI denies warnings. Do not re-enable pedantic per crate; do
  not add a bare `#[allow]` without an inline justification.
- **Every public item is documented.** `cargo doc` runs with `-D warnings`, so a missing
  doc comment is a build failure, not a nag.
- **Production `unsafe` lives only in sanctioned path owners**: `keld-wv` platform
  backends, `keld-runtime` Windows modules, and reviewed
  `keld-ipc::windows_named_pipe` today; future `keld-ipc` shm is reserved.
  `unsafe_code = "deny"` workspace-wide, opted out with a module-scope
  `#[allow(unsafe_code)]`, `#![deny(unsafe_op_in_unsafe_fn)]`, and a `// SAFETY:` proof
  citing the platform contract. A new production path requires an issue-scoped
  root/nested owner update and independent `unsafe` gate evidence on the exact final
  diff; tests follow the nearest owner.
- **No async runtime in hot paths.** kipc, the event loop, and the guard are
  callback/state-machine code with no steady-state allocation. Async is permitted only in
  cold tooling (CLI, packager, updater fetches).
- **std-first, minimal dependencies.** `keld-ipc` has two (`postcard`, `serde`);
  `keld-wv` has two, macOS-only. Every addition is a review gate: name it, say what it
  does, say what you evaluated instead, and expect to justify it.
- **No `todo!()`, `unimplemented!()`, or placeholder code on `main`.** Ship vertical
  slices. A documented gap (see the deviation list at the top of
  `crates/keld-wv/src/engine.rs`) is how this repo says "not yet" — not a stub function.
- **Errors state the fix.** `KELD-<AREA>-<NNN>` code, what failed, and the imperative
  next step. `keld-guard`'s `DenyReason` is the floor. In `keld-wv` this is enforced by a
  test that asserts the code *and* the fix hint appear in the message.
- **New behavior needs tests.** Bug fixes need the regression test written first.
  Electron-compat work needs the conformance entry first.
- **Names are fixed.** Crates `keld-*`, libs `keld_*`, npm `@keld/*`, protocol `KI*`. The
  only permitted config filenames are `keld.config.ts`, `keld.permissions.jsonc`,
  `keld.build.ts`, `keld.compat.ts`. A fifth one requires a spec change.
- **Some files are single-writer** (per [`docs/agents/workflow.md`](../agents/workflow.md)):
  workspace `Cargo.toml`, `rust-toolchain.toml`, kipc wire-protocol files, the manifest
  schema, CI workflows, and root `AGENTS.md`. Coordinate before editing them.

Before you touch a crate, read that crate's own `AGENTS.md` — they are 6–8 lines each and
they carry invariants the root file does not:

- [`crates/keld-ipc/AGENTS.md`](../../crates/keld-ipc/AGENTS.md) — wire is a versioned
  protocol; test constants as facts; no async, no steady-state alloc; fuzz the decode
  paths.
- [`crates/keld-wv/AGENTS.md`](../../crates/keld-wv/AGENTS.md) — all engine/window
  mutations on the UI thread; platform quirks must cite OS version + source, or they get
  reverted; Linux must probe the GPU stack rather than telling users to export env vars.
- [`crates/keld-guard/AGENTS.md`](../../crates/keld-guard/AGENTS.md) — default-deny is
  absolute; deny text is API; bypass fixtures are permanent.
- [`crates/keld-compat/AGENTS.md`](../../crates/keld-compat/AGENTS.md) — Electron's
  documented behavior is the oracle; conformance entry before implementation.

---

## 7. The five review gates

Some changes cannot be merged on green CI alone; they need a human to sign off. From
`AGENTS.md` § Security, performance, and review gates:

1. **`unsafe`** — new or changed
2. **Public API** — new or changed
3. **Permission model**
4. **Dependency addition**
5. **Wire protocol** — kipc frames, manifest schema, update feed

Every PR carries a `## Review gates` section listing which of these it triggers. If none
apply, you write **"none"** — the section is never omitted, because "no gates listed"
and "author did not think about gates" must not look identical to a reviewer. CODEOWNERS
is intended to enforce human approval on `keld-guard`, the `keld-ipc` protocol files, and
the workspace manifests (`docs/agents/workflow.md`). `.github/CODEOWNERS` is tracked
(KEL-39) and requests review on those paths.

---

## 8. Commits and pull requests

**Conventional Commits**, with the scope naming this repo actually uses:

```
feat(ipc): add credit-window backpressure to the app-link reader
fix(wv/macos): release the webview before closing its host window
docs(research): fold the IPC survey into 10-ipc-state-of-the-art
```

**PR body sections**, in this order:

```markdown
## Summary
## Spec refs
## Review gates
## Tests
## Platforms
## Perf impact
```

- **Spec refs** — architecture/spec paths and sections, or `No boundary change`.
  Code/spec mismatch is a bug in one of them; fix both in the same PR or state why not.
- **Review gates** — the five above, or `none`.
- **Tests** — paste real command output.
- **Platforms** — which OSes you actually exercised. "macOS only, Windows path untested"
  is an acceptable answer; silence is not.
- **Perf impact** — which budgets in [`docs/architecture/01-overview.md`](../architecture/01-overview.md)
  §5 could move, or `none`. A regression over 5% needs a written waiver.

**Rebase onto `origin/main` before opening the PR** — linear history; `--force-with-lease`
the feature branch only, never `main` (`.agents/review.md` § Branch and commit contract). Small PRs, one
concern each. No secrets, no `.env*` edits; destructive git operations need human
approval.

---

## 9. The workflow loop

For anything bigger than a bug fix, [`docs/agents/workflow.md`](../agents/workflow.md) is
the process of record. Condensed:

1. **Read before writing** — the issue, the governing spec section in
   `docs/architecture/`, the target crate's `AGENTS.md`, and
   only the relevant-area entries in [`docs/agents/learnings.md`](../agents/learnings.md).
   Then grep the codebase; a surprising amount already exists.
2. **Spec gate** — bigger than a bug fix and no approved spec? Write one from
   [`docs/agents/spec-template.md`](../agents/spec-template.md) into
   `docs/specs/<kebab-name>.md` and stop for human approval. Implementation starts only
   at `Status: approved`. Existing specs in `docs/specs/` are examples of the required
   shape, not implicit approval for a new implementation. Bug fixes skip the spec but
   not the regression test.
3. **Isolate** — one concern per branch; work in a git worktree sibling
   (`../keld-<issue>`) on `agent/kel-<n>-<slug>` from `origin/main`
   (`.agents/review.md` § Branch and commit contract). Never two people building in one
   tree at once.
4. **Write the test first**, then implement. Vertical slices, no placeholders.
5. **Run the gate** (§3) and paste the real output.
6. **Self-review the whole diff** before pushing: boundary violations, missed review
   gates, spec drift, dead code, drive-by refactors.
7. **Open the PR** per §8, and append any learnings (§10) in the same PR.

If a task seems to require breaking a rule in any `AGENTS.md`, stop and escalate — the
rule change is the PR, not the violation. A failing test you did not write is signal:
investigate or report it, never delete, skip, or loosen it.

---

## 10. The self-improvement rule (mandatory)

If you hit a non-obvious gotcha that cost you more than ten minutes, you append **one
line** to [`docs/agents/learnings.md`](../agents/learnings.md) **in the same PR**:

```
- YYYY-MM-DD [area] fact. (evidence: path, issue, or command)
```

`[area]` is a crate short name (`ipc`, `wv`, `guard`, `native`, `runtime`, `update`,
`pack`, `compat`, `core`, `cli`), or `ts`, `build`, `ci`, `process`. Grep the file first
so you do not duplicate an entry, and keep it to facts — opinions do not belong there.
A real entry from the log, which would have saved someone an afternoon:

```
- 2026-08-08 [wv] wry compiles open_devtools/close_devtools only under cfg(any(debug_assertions, feature="devtools")) — keld-wv pins the `devtools` feature or release builds fail. (evidence: competitors/wry/src/lib.rs `pub fn open_devtools`, crates/keld-wv/Cargo.toml)
```

The file is read at the start of every task, so it is kept short; maintainers compact it
into `AGENTS.md` or the specs once entries prove stable. If you find a rule in there that
is now wrong, fixing it *is* the task.

---

## 11. Where the real conventions live

Do not learn the rules from this page alone. Canonical sources, in the order you should
reach for them:

| Question | Document |
|---|---|
| What are the engineering rules? | [`AGENTS.md`](../../AGENTS.md) (root) |
| What are this crate's invariants? | `crates/<crate>/AGENTS.md` (ipc, wv, guard, compat) |
| How does the process work? | [`docs/agents/workflow.md`](../agents/workflow.md) |
| How do I write a spec? | [`docs/agents/spec-template.md`](../agents/spec-template.md) |
| What has already bitten someone? | [`docs/agents/learnings.md`](../agents/learnings.md) |
| What is the system supposed to be? | [`docs/architecture/01..07-*.md`](../architecture/) |
| Why did we choose this? | [`docs/engineering/decisions.md`](../engineering/decisions.md) (engineering narrative, not RFC 2119). [`AGENTS.md`](../../AGENTS.md) still binds. Canonical categorized `docs/research/library/` is exploratory evidence, not required reading. |
| What is Current versus Target? | Generated [`product-status.md`](../engineering/product-status.md); Linear owns live scheduling |
| Which of these actually binds me? | [`06-documentation-map.md`](./06-documentation-map.md) |

There is no `.claude/project-calibration.json` and no `project-conventions` skill in this
repo — if something points you at one, it is describing a different project.

> **Tracked versus local-only.** Keld tracks its architecture, onboarding, agent, and
> engineering docs plus generated `llms.txt` and `llms-full.txt`. `docs/research/` is
> tracked by its separate nested `0monish/keld-research` checkout and ignored by the
> Keld monorepo. The generated corpus deliberately excludes research and every
> unlisted source. Local-only material cannot be repository-status evidence.
> `.github/` is tracked (KEL-39).

---

## 12. Troubleshooting

**The macOS window does not open.**
Check, in order: (1) you are on macOS — Windows and Linux return `KELD-WV-001` by design;
(2) you have a real window server (not a bare SSH session) — window-creation failure
surfaces as `KELD-WV-002` with that hint; (3) the window opened behind your terminal —
the binary is not a `.app` bundle, so check Mission Control / cmd-tab before concluding
it failed. `just hello` and `keld hello` both block until the window closes; that is
expected, not a hang. tao's event loop requires the process main thread, so do not try to
drive it from a spawned thread.

**`bun: command not found` / `doctor` reports `[FAIL] bun`.**
Install Bun from https://bun.sh and make sure it is on `PATH` — that is the exact fix
text `doctor` prints. Without it, `keld dev` aborts at the check phase with
`KELD-CLI-032`, and `cargo nextest run --workspace` fails in `bun_echo`.

**`error: no such command: nextest`.**
`cargo install cargo-nextest --locked`. In a pinch, `cargo test --workspace` covers the
same tests (plus doctests, which nextest does not run), but the `--profile ci` retry
behavior and the CI-identical output are only available through nextest.

**`error: no such command: deny`.**
`cargo install cargo-deny --locked`. It is only needed for `just ci` and `just deny`; the
core Rust subset in §3.1 does not use it. See §3.2 for the license failure that is
already present.

**`command not found: just`.**
`cargo install just`, or use the raw command table in §3.3 — nothing in the repo depends
on `just` being installed.

**`keld dev` says `KELD-CLI-032: environment checks failed`.**
Read the check list it prints; it fails before spawning anything. The usual cause is
running outside a scaffolded project, or in a directory that has `keld.config.ts` but not
`src/main.ts`.

**`KELD-CLI-010: KELD_APP_LINK is unset`.**
You ran `bun run src/main.ts` (or `bun start`) directly. The template main process
requires the link that `keld dev` injects; start it through the CLI.

**A stale `kb-*/app.sock` in the temporary directory.**
Successful authentication consumes and removes the locator before `EchoServer::join()`
returns. `shutdown()` and `Drop` close an outstanding listener and remove its owner-only
session directory. An abrupt process death can leave that unique directory behind; a new
server binds a different directory and never reuses or unlinks the stale path. Confirm the
owning process is gone before removing stale files manually
([`crates/keld-core/src/echo_link.rs`](../../crates/keld-core/src/echo_link.rs)).

**Clippy fails on code you did not touch.**
Much of this tree is uncommitted work in progress (§3.2). Confirm against a clean
checkout of `main` before assuming your change caused it — and if it did not, say so in
the PR rather than fixing it silently in an unrelated diff.
