# Engineering decisions

Human-facing log of **what we chose, why, what we rejected, and what is not next**.
Last confirmed against the tree on 2026-08-16.

This file is **not** a new RFC 2119 layer. Binding agent rules stay in
[`AGENTS.md`](../../AGENTS.md), crate `AGENTS.md` files, and
[`docs/agents/workflow.md`](../agents/workflow.md). Normative design stays in
[`docs/architecture/01..07-*.md`](../architecture/). Gotchas stay as one-liners in
[`docs/agents/learnings.md`](../agents/learnings.md). This document is the onboarding
pointer for *why* the current engineering looks like this.

Numbered `docs/research/` notes are exploratory evidence. They are not required
reading and must not be treated as a second spec. Cite architecture, `AGENTS.md`,
and `docs/engineering/` instead.

RFC 2119 words below appear only when quoting those binding files.

## How to use this file

| Column | Meaning |
|---|---|
| Chose | What is true in code, config, or a tracked spec today |
| Why | The constraint or failure mode that drove it |
| Why not | Alternatives that were considered and declined |
| Next | What this decision implies we do later — not a commitment to do it this week |
| Evidence | Tracked path a reader can open |

When a choice here disagrees with code, treat that as a bug in one of the two and
fix both in the same change (`AGENTS.md` § Ground truth). When it disagrees with a
spec, the spec is the design target; this file should say what is implemented *now*.

Add a section when a choice is stable enough that a new engineer would otherwise
re-litigate it. Do not dump Linear tickets or research drafts here.

---

## 1. Four uniques — not a fifth

**Chose (2026-07, restated in `AGENTS.md`).** Keld protects exactly four properties:

1. A **prebuilt Rust host** owns windows, webviews, native APIs, keys, and the updater.
   App developers do not compile it (`docs/architecture/01-overview.md` §1–2, principle 5).
2. The developer's JS/TS main runs on a **supervised Bun child with zero ambient OS
   authority** (`docs/architecture/06-runtime-and-tooling.md` §1).
3. Host and child talk over **kipc**, a typed binary plane the host mediates
   (`docs/architecture/02-ipc.md` §1).
4. Privileged calls are **default-deny**: generated manifest, host-enforced
   (`docs/architecture/03-security.md`; `AGENTS.md` § Security).

`AGENTS.md` § Working rules: agents **MUST** protect those four uniques only and
**MUST NOT** invent a fifth.

**Why.** Each competitor already ships some of this and fails a different one
(architecture 01 §1): Electron puts privileged JS in-process with window ownership;
Tauri has native ownership but no JS main process; Electrobun/Deno Desktop give JS a
main process that owns native state. Compatibility is principle 1 — `@keld/electron`
implements Electron's API *on top of* kipc, so the security boundary is also the
compat seam.

**Why not.** Copying Tauri's crate graph, putting Tokio on the kipc/event-loop/guard
hot path, Tauri-style ACL wildcards as culture, or in-process Node/Bun are named as
cargo-cult in `AGENTS.md`. A fifth unique (a new runtime, a new permission dialect, a
framework-owned agent stack, CEF-by-default, …) would be architecture theater unless
it changes who owns a handle, who can crash whom, or who can mint a principal.

**Implemented vs specified.** The four uniques are the design. Today the tree has live
macOS WKWebView, Windows WebView2, and Linux WebKitGTK hello paths (KEL-26/27/28) plus
a kipc echo slice. `keld-guard::evaluate` exists;
`keld_permissions_explain`, the macOS/Linux/Windows webview media-capture handlers, and
`keld_ipc::guard_dispatch::dispatch_privileged` (KEL-69) call it. `keld-native::fs`
(KEL-71) is the first production capability using `dispatch_privileged` — host
`fs.read`/`fs.write`, guard-checked before any disk I/O. The other 14 `keld-native`
modules are still names only. `keld dev`'s app-process spawn
now runs under a real
`keld-runtime::Supervisor` (KEL-70): spawn, stdout/stderr capture, restart-on-crash with
exponential backoff, and a crash-loop breaker (default 3 crashes / 30s → typed
`KELD-RUNTIME-002`). Not yet built: the destination `KELD_LINK`/`KELD_SHM`/`KELD_CONTRACT`
env contract (v0 keeps `KELD_APP_LINK`), Bun pinning/download, `--inspect` passthrough,
Bun watch hot-restart, and OS sandboxing of the child (architecture 03 §4.2, v0.3). Hold
these facts.

**Update (2026-08-17, KEL-70).** `keld-runtime` was a bare `RestartPolicy` struct with
nothing reading it; `crates/keld-cli/src/dev.rs` spawned `bun` with a raw
`Command::new("bun").wait_with_output()` — Electrobun-shaped, not Unique #2. Replaced
with `keld_runtime::Supervisor`: a background thread owns the child, polls `try_wait`
(no async runtime — this crate is cold tooling, not the kipc/event-loop/guard hot path),
captures stdout/stderr via reader threads into a shared buffer, and on a non-zero exit
restarts with exponential backoff until the policy's crash count trips inside its
window, at which point it returns a typed `RuntimeError::CrashLoop`. A zero exit is
graceful completion — no restart. `keld dev`'s window is opened by the caller on its own
thread after the echo step; the supervised child is a fully separate OS process, so
killing/restarting it cannot touch a host-owned window (verified headless: every
supervised child pid is asserted distinct from the host process's own pid across
multiple restart cycles, `crates/keld-runtime/src/lib.rs`
`host_pid_is_unaffected_across_restart_cycles`). Verification: `cargo fmt`/clippy
`-D warnings`/nextest green on Windows (this machine); Unix/Windows test-shell branches
(`sh -c` / `cmd /C`) compile under both `cfg`s but only the Windows path was executed
here — Linux/macOS re-verification is still open.

**Update (2026-08-17, KEL-69).** ACL fixtures (KEL-45) proved the matcher but not that a
privileged `Call` is denied on the wire without its handler running — Unique #4 was
advisory. Added `keld_ipc::guard_dispatch::dispatch_privileged(manifest, principal,
operation, path, handler) -> Result<T, DenyReason>`: the single sanctioned
guard-before-handler entry point, evaluating before `handler` ever runs. Proved
end-to-end with a real kipc session (`crates/keld-ipc/tests/guard_dispatch.rs`): a real
socket, a real v2 `HELLO` handshake, a `Call` on a test-only channel (`test.marker` —
deliberately not `fs.*`, so it can't collide with KEL-71's real capability), and a real
filesystem write as the handler's side effect. Deny-manifest and non-`AppProcess`
principal cases both leave the file unwritten; allow-manifest writes it. Negative control
verified manually (not just asserted): temporarily bypassing `dispatch_privileged` in the
test server made both the deny and the webview-principal tests fail, confirming they
actually exercise the gate. Echo (KEL-30) stays deliberately ungated. `keld-core` still
has no privileged operation of its own to route through this — the mechanism is proven
and ready, not yet load-bearing for a real capability.

**Update (2026-08-17, KEL-71).** `keld-native` was a `MODULES` name list with zero
implementations. Added `keld_native::fs`: `fs_read`/`fs_write` (capability ids
`fs.read`/`fs.write`) and a real `serve_fs_session` kipc channel (`FS_CHANNEL`,
`ChannelId(2)`), every call routed through `dispatch_privileged` (KEL-69) before touching
disk — the first production capability to use it. Cross-platform by construction
(`std::fs::read`/`std::fs::write` are the same call on all three OSes), so this satisfies
architecture 05 §3's "all three OS implementations" without per-platform code.
`..` traversal denial falls out of `keld-guard::evaluate` for free (already rejects any
`..` segment) — proved with a real OS oracle (`dotdot_segment_is_denied_even_inside_a_granted_scope`),
not re-implemented here. Verified end-to-end over a real kipc session
(`crates/keld-native/tests/fs_session.rs`): allow writes-then-reads-back identical bytes
on disk; deny (empty manifest, out-of-scope path, `..`, or a non-`AppProcess` principal)
leaves the file completely unwritten, each checked with a real `std::path::Path::exists()`
stat, not a stub return. Negative control verified manually twice: bypassing
`dispatch_privileged` inside `fs_write` made three fs-specific tests fail; separately,
temporarily adding a `node:fs` import to the hello template made the new
`template_never_imports_node_fs` static check fail (`crates/keld-cli/src/template.rs`) —
both reverted before committing. Bun's own `node:fs` still works unsandboxed
(architecture 03 §4.2 OS-sandbox slice is v0.3, not this ticket); this is the **Keld**
API, not a claim that Bun is jailed.

**Next.** Wire a real `@keld/api` TypeScript surface for `fs.read`/`fs.write` once
`packages/` exists (currently empty — out of scope until a TS package pipeline lands).
Keep shipping Linear Phase 2 (window + kipc echo + crate map) on the four uniques. Do not
add a fifth unique to look complete.

---

## 2. Webview: wry+tao scaffolding, spec 05 destination

**Chose (2026-07-08; wry pin updated 2026-08-14 for KEL-59; Windows superseded
2026-08-15; Linux landed 2026-08-16 — see the updates below).** The live macOS
backend is tao 0.35.3 + wry 0.56.1 (`devtools` feature) in
`crates/keld-wv/src/wkwebview/mod.rs`. Module comment: interim implementation,
replace with direct objc2 bindings per architecture 05 §1.

wry 0.55.1 auto-granted camera/mic (`request_media_capture_permission` → `Grant`)
and had no `with_permission_handler`. 0.56.1 adds the handler; Keld installs it
and default-denies via `keld-guard` (`web.camera` / `web.microphone`). tao stays
0.35.3: wry 0.56 `build` takes `raw_window_handle::HasWindowHandle`.

**Destination (architecture 05 §1).** `keld-wv` is Keld's own `WebEngine` layer over
WKWebView (**objc2**), WebView2 (**windows-rs** + WebView2 COM), and WebKitGTK
(**webkit6/gtk4**). wry/tao stay as reference implementations and a quirks catalog.
CEF is a feature-flagged pinned engine, fetched at *build* time, never the default
(architecture 01 §6). Verso/Servo are tracked as a later backend “the day embedding
stabilizes.”

**Why wry now.** Phase 2 needs a window on screen. wry already talks to WKWebView
through tao's event loop. Replacing it does not change who owns the handle, who can
crash whom, or who can mint a principal — so it fails the first-principles test in
`AGENTS.md` until wry is missing a hook we actually need. KEL-59 was that case for
camera/mic: bump to 0.56.1 for `with_permission_handler`, do not rewrite the backend.

**Update (2026-08-15, KEL-65): Windows create path is now direct windows-rs COM.**
wry crossed the "missing a hook" line on Windows three ways at once: it blocks the
UI thread 96–109 ms injecting a `window.ipc` bridge Keld never uses (reported
upstream, tauri-apps/wry#1813), it ships default browser arguments that disable
SmartScreen (KEL-66), and it owns the environment options Keld needs to control.
`crates/keld-wv/src/webview2/mod.rs` now drives `webview2-com` directly —
environment, controller, `add_PermissionRequested` guard before first navigation
(compile-enforced), bounds, navigate — with tao still providing window + event
loop. wry stays the macOS interim and a quirks catalog. Honest ledger: a
controlled same-session A/B showed first paint unchanged (472 vs 467 ms — the
bridge wait overlapped renderer boot), so the rewrite is carried by security
(SmartScreen on, at 0 measured cost), binary size (−24%, 625,152 → 484,864 B),
and owning the create sequence — not by a speed claim.

**Update (2026-08-16, KEL-28): Linux create path is wry, same interim step
macOS/Windows started with.** `crates/keld-wv/src/webkitgtk/mod.rs` drives wry's
GTK3 + WebKit2GTK 4.1 backend (`default = ["os-webview", "x11"]` — no extra
feature flags needed for the hello slice), mirroring `wkwebview/mod.rs`
structurally, with one deliberate deviation: `WebViewBuilderExtUnix::build_gtk`
against the tao-owned GTK window, not the plain cross-platform `build()`, which
wry's own docs say is X11-only. KEL-28's DoD requires Wayland too. One
Linux-only addition crate `AGENTS.md` requires: `probe_gpu_stack()` detects
NVIDIA + Wayland (`/proc/driver/nvidia/version` + `WAYLAND_DISPLAY`) and sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1` on the process's own environment before any
GTK/WebKit call — never by asking a developer to export a shell variable —
mitigating the documented DMA-BUF crash/flicker class on `WebKitGTK` ≤ 2.54
(tauri-apps/tauri#9394, #14924). Detection is split from the mutation
(`detect_gpu_safe_mode`, pure, vs. `probe_gpu_stack`, which also applies it) so
`keld doctor` can read the state later without side effects. Media-permission
guard (KEL-59) reuses the existing wry helpers unchanged (`media.rs` widened
from macOS-only to `any(macos, linux)`), since Linux's default without a
handler is also "show the platform's own prompt," same category as the old
Windows default.

Verification: compiled, clippy-clean, and 225 tests green (including live
Bun↔Rust kipc integration) on real Ubuntu 26.04 (GTK3/WebKit2GTK 4.1 dev libs
via apt) in WSL. That same WSL sandbox has no display reachable through WSLg
(`gtk::init()` fails there directly: "Failed to initialize GTK"), so a plain
`keld-host --hello` could not be watched — but `Xvfb` + a window manager
(`fluxbox`) + `xdotool search --name Keld` **does** find a real, correctly
titled X11 window within 0.5 s of launch, zero stderr, exactly the headless
smoke test KEL-28's own spec asked for. A root-window screenshot without a WM
running came back blank (no compositor to map the child window for capture);
with `fluxbox` running the window exists per `xdotool` but a follow-up
screenshot attempt did not land it either — worth another pass, not blocking.
Nobody has watched pixels render on a real desktop yet; confirm on Linux
hardware/VM with eyes on the screen before calling this fully closed.

**Why not treat wry as the product.** Architecture 05 lists hooks wry does not
prioritize (scheme-streaming as bulk IPC, principal identity per navigation, engine
policy switching, `webContents`-grade control). The host is prebuilt, so wry's
“works in any downstream cargo build” constraint does not apply.

**MPL is not wry's license.** wry 0.56.1 is Apache-2.0 OR MIT; tao 0.35.3 is
Apache-2.0. The MPL-2.0 crate in the graph is **`option-ext` 0.2.0**, reached
`keld-wv → wry → dirs → dirs-sys → option-ext` (`deny.toml`,
[`third-party-licenses.md`](./third-party-licenses.md), learnings 2026-08-13). Do not
describe wry as MPL.

**Update (2026-08-17, KEL-28 follow-up): the `build_gtk` container was itself
wrong.** §2's `WebViewBuilderExtUnix::build_gtk(window.gtk_window())` call
compiled, created a real titled window `xdotool` could find, and did not
panic — but was silently broken: `window.gtk_window()` returns the
`GtkApplicationWindow`, a `GtkBin` that can hold exactly one child, and tao
already fills that slot with its own vertical `gtk::Box`
(`WindowExtUnix::default_vbox`). GTK logged "Attempting to add a widget...
but as a GtkBin subclass a GtkApplicationWindow can only contain one widget
at a time" and the webview never actually attached to the window — confirmed
live under Xvfb (log capture, `docs/agents/learnings.md`). Every real wry
example (`examples/simple.rs`, `examples/multiwindow.rs`, ...) passes
`window.default_vbox().unwrap()` to `build_gtk`; the `gtk_window()` form only
appears in wry's simplified top-of-crate-doc snippet, which is for the
X11-only `build(&window)` path, not `build_gtk`. Fixed to `default_vbox()`;
re-verified under Xvfb — the GTK warning is gone, and `keld-host --hello`
now stays alive in its event loop (blocks as expected) instead of the prior
run completing regardless. This does not by itself prove pixel-level render
correctness (still nobody has watched it with eyes on a screen), but it is
materially stronger evidence than before: the previous "it works" claim was
based on window *existence*, not the webview being correctly parented into
that window at all.

**Update (2026-08-17): Linux CI wiring landed.** `.github/workflows/ci.yml`
`linux-gui-smoke` now runs the exact Xvfb + `xdotool` smoke test described
above on every PR/push — closing the "CI wiring (`.github/workflows/ci.yml`
untouched)" gap disclosed in KEL-28's original PR and Linear comment. It
installs Xvfb + xdotool + the same WebKitGTK apt packages as the `check`
job, builds `keld-host`, launches `--hello`, and polls `xdotool search`
(up to 30s) before killing the process — a bounded, non-flaky wait, not a
blind sleep. A local dry-run in WSL surfaced a real bug in the *test script
itself*: a `trap '... || true' EXIT` clobbers `$?`, so a genuinely failing
smoke test would have reported success to CI — fixed by capturing the exit
code before cleanup and re-exiting with it explicitly (`docs/agents/learnings.md`).

**Next.** Keep macOS and Linux on wry+tao. Rewrite a backend to objc2/webkit6-gtk4
when that backend needs a wry-missing hook (as happened for Windows, KEL-65), or
when someone can run the smoke on that OS with a real display — not as a
same-week rewrite. Visually confirm the Linux window on real hardware/VM before
claiming KEL-28 fully done. See §11.

---

## 3. Hand-rolled `KELD-*` errors, not thiserror

**Chose.** Typed errors implement `Display` by hand with a `KELD-<AREA>-<nnn>` code
and a sentence that states the fix. `thiserror` is not a workspace or crate
dependency (`Cargo.toml`, grep of `crates/`). `AGENTS.md` § Rust: hand-rolled
`Display` + `KELD-*` codes (not `thiserror`).

Canonical registry: [`keld-error-codes.md`](./keld-error-codes.md). CI
(`crates/keld-cli/tests/error_registry.rs`) requires a 1:1 match with codes emitted
from `keld-ipc`, `keld-wv`, `keld-cli`, `keld-guard`, CLI templates, and `tools/`.
Architecture 07 §2 is the message shape; `keld-guard`'s `DenyReason` is the floor.

Enums in tree that follow this (not an exhaustive forever-list): `IpcError`,
`HeaderError`, `WvError`, `CreateError`, `DevError`, `ManifestError`,
`McpServeError`.

**Why.** Error text is API. A derive macro hides the fix sentence; the registry test
cannot see a code that only exists in a `#[error("...")]` attribute unless we also
hand-write the string. Agents and `keld doctor --json` / MCP need a stable code +
imperative next step (`https://keld.dev/e/<code>` on `KeldErrorObject`, not inlined
into every `Display`).

**Why not.** `thiserror` / `anyhow` for library errors. `anyhow` is for binaries that
do not owe a stable code. Compact `KELD-MCP001` and hyphenated `KELD-CLI-020` both
exist; match the crate that already emits the code (registry intro). Do not invent a
third spelling.

**Next.** New variants add a registry heading in the same change, emit the same code
from `Display`, and test that the message contains the code and the fix. Per-code
website pages stay deferred (registry header).

---

## 4. Verification gate, `just ci`, and git hooks

**Chose — agent “done” bar (`AGENTS.md` § Commands).** All three, every time:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --profile ci
```

Toolchain pin: `rust-toolchain.toml` (`1.93.0`, rustfmt + clippy). Nextest CI
profile: `.config/nextest.toml` (`retries = 1`). Workspace clippy is already
pedantic; do not re-enable it per crate (learnings 2026-07-08).

**Chose — local mirror of CI (`justfile`).** `just ci` runs, in order:
`agents-md` → `llms-test` → `llms-check` → `hygiene` (KEL-39) → `fmt-check` →
`clippy` → `test` → `doc` → `deny`. Gitleaks is **not** in `just ci`; the justfile
says it stays GitHub-only (checksum-pinned OSS CLI in `.github/workflows/ci.yml`).

**Chose — no Husky / no committed git-hook manager.** Tracked config has no
husky, lefthook, pre-commit, prek, or cargo-husky. Do not add one to “match
Electron.” Electron, SWC, and pnpm use Husky because they onboard via npm/yarn;
their CI still re-runs the real lint/fmt. Tauri, wry, tao, Bun, Biome, Deno,
Wails, Electrobun, rustc, tokio, and serde ship **no** hook manager — gate is CI
plus `just` / `./x` / `x.py`. Oxc’s optional format-only hook is untracked
(`just install-hook`). The predictor is **JS package-manager onboarding**, not
“desktop framework.”

Keld has no root `package.json` and no `prepare: husky` step that would wire
hooks reliably. A laptop hook cannot replace GitHub Actions. Putting clippy,
nextest, or gitleaks in pre-commit is the wrong gate (slow; gitleaks is
GitHub-only). Optional later: an Oxc-style **untracked** format-only hook, not
a committed Husky tree. Evidence (exploratory, not required reading):
`docs/research/42-git-hooks-and-precommit.md`.

**Why.** Fmt/clippy/nextest are the contract testers can paste. `just ci` catches
docs corpus drift, crate-`AGENTS.md` for `unsafe`, CODEOWNERS/template/SHA hygiene,
rustdoc `-D warnings`, and cargo-deny without waiting for GitHub. Secret scan stays
on GitHub so local clones do not need a gitleaks binary for the three-command bar.

**Why not.** A fourth always-on local tool (coverage, machete, nursery clippy, Biome
while `packages/` is empty) — listed as later in
[`tooling-audit.md`](./tooling-audit.md). Sleep-sync tests, skipping clippy because
“tests pass,” or writing “should work” in a PR (`AGENTS.md`).

**Next.** Keep the three-command bar as the definition of done. Run `just ci` before
push when you touched docs corpus, `unsafe`, `.github/`, or dependencies. Do not
install a hook manager for Phase 2.

---

## 5. cargo-deny allowlist and first public binary

**Chose.** `deny.toml` is an **allow-list** (MIT, Apache-2.0, Apache-2.0 WITH
LLVM-exception, BSD-2/3, ISC, Zlib, BSL-1.0, Unicode-3.0, Unicode-DFS-2016). New
licenses are a dependency review gate, not a drive-by `allow` edit.

MPL-2.0 is allowed only as a **per-crate, version-pinned** exception:

```toml
exceptions = [{ crate = "option-ext@0.2.0", allow = ["MPL-2.0"] }]
```

Do not add `MPL-2.0` to the global `allow` list (that would bless any future MPL
crate). Keld itself stays `MIT OR Apache-2.0` (`Cargo.toml` `[workspace.package]`).
Unknown registries and git sources are denied; `allow-git = []`.

Yanked crates: `yanked = "deny"`. A prior `spin` 0.9.8 yank via postcard → heapless
was a separate lockfile issue; [`third-party-licenses.md`](./third-party-licenses.md)
records advisories green as of 2026-08-13 with `spin` 0.9.9. Do not allowlist yanked
crates to silence that gate.

**Why.** Packed binaries will contain `option-ext` (Apple may dead-strip Linux-only
use; that is not a compliance basis). MPL is file-level copyleft: static linking
does not relicense Keld, but distribution still requires notices and the exact
corresponding source.

**Why not.** Forking `dirs-sys` or replacing live wry solely to drop one helper.
Replacing wry is architecture 05 work, not a license shortcut.

**Next — packaging checklist before the first public binary**
(from [`third-party-licenses.md`](./third-party-licenses.md); not legal advice):

- Third-party notice lists `option-ext` 0.2.0, MPL-2.0, and upstream URLs.
- MPL 2.0 text included or linked.
- Exact corresponding source offered (lockfile checksum
  `04744f49eae99ab78e0d5c0b603ab218f515ea8cfe5a456d7629ad883a3b6e7d`, not “latest”).
- Confirm Keld still does not modify `option-ext`.
- `cargo deny check licenses` green on the release commit.

`keld-pack` is still a skeleton. When it grows an installer pipeline, this checklist
becomes a release gate there — do not invent a parallel license-scanner stack.
Counsel reviews the first external distribution.

---

## 6. Nested crate `AGENTS.md` only for invariants

**Chose.** Four crate files exist: `keld-wv`, `keld-ipc`, `keld-guard`,
`keld-compat`. Root `AGENTS.md` § Repo map: crate `AGENTS.md` only where invariants
exist. Skeletons (`keld-core`, `keld-native`, `keld-runtime`, `keld-update`,
`keld-pack`, `keld-host`) and `keld-cli` get a spec pointer in the repo-map table,
not a hollow file. `keld-cli` `expect` is already sanctioned in root § Rust.

Nested files **add** constraints. They must not silently weaken root. Root wins on
conflict unless the crate file names a documented exception with justification.
`keld-guard` names one: v0 `$VARS` as literals (see §8).

**Why.** `unsafe` / WebEngine, kipc wire, and default-deny are not obvious from the
root file. Cloudflare-style nested `AGENTS.md` is for those invariants, not for a
README in every crate.

**Enforcement.** `just agents-md` fails if a crate with `unsafe` /
`allow(unsafe_code)` has no `AGENTS.md`. It is not a Codex.

**Why not.** One `AGENTS.md` per skeleton “to look complete” (YAGNI test b). A
`packages/` file before TypeScript exists.

**Next.** Add a crate `AGENTS.md` when that crate gains an invariant the root file
does not already bind (for example `keld-ipc` shm `unsafe`, or a real
`keld-runtime` supervisor). Do not add files in the same PR as a stub.

---

## 7. `llms.txt` pipeline — research excluded from the corpus

**Chose.** [`llms.txt`](../../llms.txt) is a generated compact index;
[`llms-full.txt`](../../llms-full.txt) is the ordered concatenation.
`tools/llms_docs.rs` owns the allowlist. `just llms` writes; `just llms-check`
fails on drift (`KELD-DOCS004`). Architecture 07 §3 requires both from day one.

Allowlist today: onboarding README, onboarding 04 (wire formats — KEL-61),
architecture 01–07, this file, the KELD error registry, the size/RSS scoreboard,
the Electron compat scoreboard placeholder. `validate_source_path`
rejects `docs/research`, `competitors`, `.claude`, `private`, `.private`,
parent-dir components, non-`.md` paths, and symlink escape.

`keld_docs_search` embeds `llms-full.txt` at compile time
(`crates/keld-cli/src/mcp/docs_search.rs`). Changing an allowlisted source without
`just llms` (and a CLI rebuild) means MCP search is stale.

**Why.** Agents need a closed, reviewed corpus. Research is dated and argumentative;
putting it in `llms-full.txt` would teach exploratory claims as if they were spec.
The generator's own tests plant a research sentinel and assert it cannot leak.

**Why not.** Hand-maintaining `llms.txt`. Globbing all of `docs/`. Including
onboarding 02–07 automatically (they are tracked, but they are not in the allowlist
until someone adds them). This decision log *is* in the allowlist because it is the
tracked why-doc.

**Next.** After editing an allowlisted file, run `just llms` and `just llms-check`.
Do not add research to `SOURCES`. A new binding spec or registry belongs on the
list; a new audit usually does not.

---

## 8. MCP stdio v1, and `$VARS` as v0 literals

### MCP

**Chose.** `keld mcp serve` is a **stdio** server inside the `keld` binary (KEL-42).
Workspace `Cargo.toml` pins `rmcp =3.1.2` with `server` + `macros` + `transport-io`
only — no `transport-streamable-http*`, no reqwest/auth. Tokio is enabled only on
that CLI cold path (`rt`, `io-std`, `io-util`, `sync`, `time`); not on kipc, the
event loop, or the guard.

Shipped tools, fixed order (`crates/keld-cli/src/mcp/server.rs`,
[`docs/onboarding/07-mcp-server.md`](../onboarding/07-mcp-server.md)):

1. `keld_doctor`
2. `keld_docs_search`
3. `keld_permissions_explain`

The server opens no sockets, edits no manifest, and has the same local authority as
the `keld` process the client started. Protocol versions advertised: `2026-07-28`
and `2025-11-25`.

**Why.** Architecture 07 §4 / §9: MCP + CLI + `llms.txt` are the agent surfaces; do
not invent a bespoke protocol. Stdio matches a client-spawned local CLI. Wrapping a
hosted SaaS OAuth MCP would add a network listener and an identity provider Keld
does not operate.

**Why not.** Hand-rolled JSON-RPC (own five MCP revisions); a Bun sidecar TS SDK
(second runtime); Streamable HTTP + OAuth in v1 (Cargo.toml alternatives list).

**Specified vs shipped.** Architecture 07 §4 lists a larger destination toolset
(`keld_scaffold_app`, migrate analyze, dev inspect/trace, logs, build errors).
Architecture 07 §8 puts “MCP server v1 (docs/doctor/permissions **+** dev
inspect/trace)” in an AX Phase 2 that is **not** the same numbering as Linear
Phase 2 (see [`linear-roadmap-mapping.md`](./linear-roadmap-mapping.md)). What runs
today is the three-tool stdio slice.

### `$VARS`

**Chose — destination (architecture 03 §2, `crates/keld-guard/AGENTS.md`).** Scope
matching resolves `$VARS`, then symlink/`..`, then matches. Bypass fixtures
(traversal, symlink swap, case folding, wildcard-swallow) stay permanent.

**Chose — v0.** `$VARS` match as **literals**. `..` is rejected. Symlink
canonicalization is not in this slice. That is **not an Allow**.
`keld-guard::evaluate` documents the same contract. Host resolution remains the
destination; v0 must not be silently treated as “we decided not to resolve.”

**Why.** Default-deny with an unresolved `$APPDATA` string is still deny-unless-listed.
Pretending v0 already expands env vars would hide scope-bypass bugs the fixture
corpus exists to catch.

**Next.** Host-side `$VARS` / symlink canonicalization before match, then wire
`evaluate` into privileged IPC — not a matcher rewrite that allows `..`.

---

## 9. GitHub CI (KEL-39)

**Chose.** `.github/` is **tracked**. `.gitignore` comments that explicitly; a
hygiene check fails if `/.github/` is ignored (`tools/ci_hygiene.rs`).

| Piece | What it does |
|---|---|
| `.github/workflows/ci.yml` | rustfmt; clippy + nextest + rustdoc on ubuntu/macOS/windows (`fail-fast: false`); MSRV from `cargo metadata`; cargo-deny; gitleaks; CODEOWNERS/template hygiene |
| Action pins | Full 40-char SHAs; tag in the trailing comment (`tools/ci_hygiene.rs` rejects unpinned `uses:`) |
| Gitleaks | OSS CLI 8.30.1 tarball + `sha256sum -c` (`551f6fc8…`). **Not** `gitleaks/gitleaks-action` (that Action needs `GITLEAKS_LICENSE` on org repos; learnings 2026-08-13) |
| `.github/CODEOWNERS` | `Cargo.toml` / `Cargo.lock` / `rust-toolchain.toml` / `.github/` / `keld-guard` / `keld-ipc`. Owners are named users until `@gyldlab/keld-maintainers` exists (file header: unknown teams make CODEOWNERS a no-op) |
| `.github/PULL_REQUEST_TEMPLATE.md` | Summary, spec refs, the five review gates, gate commands, platforms, perf |
| `.github/ISSUE_TEMPLATE/` | bug.yml, feature.yml, config.yml (Linear link for implementation work) |
| `.github/dependabot.yml` | `github-actions` weekly. Cargo is **not** auto-bumped: each crate dep is a review gate; deny already alerts on advisories |

**Why.** Untracked workflows never run on GitHub. SHA pins stop tag-move supply-chain
surprises. Org-licensed gitleaks-action would fail or require a paid license on
`gyldlab/keld`.

**Why not.** Generated CI from a TypeScript file (tooling-audit P1, not now).
Dependabot on Cargo (would bypass the dependency review gate).

**Next.** Replace individual CODEOWNERS with the org team when it exists. Keep
hygiene green when editing the workflow. Do not edit `.github/workflows/ci.yml` in
the same change as an unrelated in-flight review.

---

## 10. First-principles and YAGNI tests

Quoted from `AGENTS.md` § Working rules (these **do** bind agents):

1. Decompose every design to OS / process / memory / trust-boundary facts across
   host / Bun child / webview. If it does not change who owns a handle, who can
   crash whom, or who can mint a principal, it is not architecture.
2. Treat wry layout, Tauri ACL, Electron docs, and platform event loops as
   **evidence of facts**, not templates.
3. Protect the four uniques only. Do not invent a fifth.
4. Two YAGNI tests: (a) can Linear Phase 2 (window + kipc echo + crate map) ship
   without this? (b) does this file exist only to look complete? Either yes → do
   not land it.
5. Anti-patterns: crate `AGENTS.md` only when it adds binding rules; do not write
   an RFC that restates `docs/architecture/` without binary acceptance tests; do
   not split toward a 100-crate graph; do not add a `WebEngine` method until a live
   backend implements it in the same PR.

Tests must be falsifiable (`.agents/testing.md`): a real contract defect fails the
test. Observable contracts: error code, wire bytes, process status, OS behavior —
not implementation-shaped essays.

**Linear vs ROADMAP numbering.** Linear “Phase 2 — Foundation Scaffolding” is
ROADMAP Phase 0 plus the start of “window on screen,” not architecture 07's “AX
Phase 2.” Use [`linear-roadmap-mapping.md`](./linear-roadmap-mapping.md). Mixing
those numbers is how eval harnesses and CEF backends get scheduled as if they were
the hello window.

---

## 11. Explicitly not next

These are specified or tracked, and they are **not** the current heading. YAGNI
test (a): Linear Phase 2 (window + kipc echo + crate map) ships without them.

| Not this week | Why it waits | Evidence |
|---|---|---|
| **KEL-44 agent-eval harness** | Architecture 07 §5 describes `evals/` (one-shot buildability, nightly + docs-touching PRs). Architecture 07 §8 places eval v0 in *AX* Phase 2, beside a corpus harness that does not exist. No `evals/` tree. Alignment audit (2026-07-08) already noted KEL-44 ≈ KEL-37 as duplicate tracking. | architecture 07 §5, §8; [`alignment-audit-2026-07-08.md`](./alignment-audit-2026-07-08.md) |
| **CEF as default** | Pinned engines are opt-in and per-platform. CEF binaries, if ever, are fetched at build by `keld-pack`, never at user runtime. | architecture 01 §6; architecture 05 §1 |
| **Servo / Verso backend** | Spec 05 tracks them as a fifth `WebEngine` the day embedding stabilizes. That is a later backend, not a wry replacement for the hello window. | architecture 05 §1 |
| **Wrapping SaaS / OAuth MCP** | v1 is stdio, zero listeners, no HTTP transport, no auth crate. Architecture 07 §9: no bespoke agent protocol; we do not host developers' agent identity. | `Cargo.toml` rmcp comment; architecture 07 §4, §9; onboarding 07 |
| **objc2 rewrite this week** | Destination is spec 05; current macOS path is wry+tao scaffolding. The rewrite does not move Phase 2. | architecture 05 §1; `wkwebview/mod.rs`; `AGENTS.md` YAGNI (a) |
| **Watching the KEL-28 Linux window with eyes on a screen** | Windows (KEL-27) and Linux (KEL-28) backends are both implemented and built/tested on their real OS (WSL Ubuntu for Linux, not cross-compiled from macOS as-if-done — `AGENTS.md` forbids that shortcut). Xvfb + `xdotool` confirms a real, correctly titled X11 window exists, and — since the 2026-08-17 `default_vbox` fix — that the webview actually attaches to it without GTK's widget-conflict warning, and that `keld-host --hello` correctly blocks in its event loop instead of exiting. `linux-gui-smoke` now runs this in CI on every push/PR. What's still open: nobody has watched pixels *render* on a real desktop. Confirm on Linux hardware/VM with a display before calling KEL-28 fully closed. | `crates/keld-wv/src/webkitgtk/mod.rs`; `.github/workflows/ci.yml` `linux-gui-smoke`; architecture 05 §1 |
| **Six-framework notes-app bench** | Same app in Keld / Electron / Electrobun / Tauri / Wails / Swift is a later epic. Fair score is **v1** (search, GFM preview, custom-scheme images, native menus, second window, PDF-exists, file watch, autosave) — not v0 CRUD. The two hard blockers (guard-on-IPC, KEL-69; host `fs.read`/`fs.write`, KEL-71) are both done as of 2026-08-17, but the bench itself — six real app builds, `fs.watch`, autosave, second-window, PDF-exists — is still its own epic, deliberately not started here. Bun `node:fs` is still not a default-deny test; a scaffolded app that wants the guarded path uses `keld_native::fs`, not `node:fs` (`crates/keld-cli/src/template.rs` `template_never_imports_node_fs` enforces this statically). | architecture 01 §5; this file §11; research 43 / 43a (exploratory, not required) |

Also parked, for the same YAGNI reason: `bench/` perf CI (budgets exist in
architecture 01 §5; the directory does not), `keld build` / `keld migrate` /
`packages/@keld/*`, OS sandbox on the Bun child (architecture 03 §4, v0.3 target),
and attack-mode `keld doctor` (architecture 07 §6, Phase 3). Until `bench/`
exists, measured hello/installer/RSS rows live in
[`budget-scoreboard.md`](./budget-scoreboard.md) (markdown, not CI).

**What *is* next in this slice:** keep the four uniques, keep the window+echo path
honest, generate the docs corpus, enforce the verification gate, and do not mark
Linear issues Done from this document.

---

## 12. Human review gates

`AGENTS.md` § Review gates — human sign-off; list under `## Review gates` in the PR,
or write **none** (the section is never omitted):

1. **`unsafe`** (new or changed) — only `keld-wv` backends and future `keld-ipc` shm;
   `unsafe_code = "deny"` workspace-wide so those two opt in reviewably.
2. **Public API** (new or changed).
3. **Permission model.**
4. **Dependency addition** — name, purpose, alternatives (see the rmcp block in
   workspace `Cargo.toml`).
5. **Wire protocol** — kipc frames, manifest schema, update feed.

CODEOWNERS requests review on `keld-guard`, `keld-ipc`, workspace manifests, and
`.github/`. That is a GitHub review request, not a substitute for the five-gate
list. `docs/agents/workflow.md`: CI is the arbiter; humans are the architects
(intent, boundaries, spec conformance, API shape — not line-by-line style).

---

## 13. Optional agent memory stays outside Keld

**Chose (KEL-67, approved 2026-08-16).** TencentDB Agent Memory may be evaluated as
an optional, external contributor aid: a pit-crew notebook that points an agent back
to current evidence. It is not a Keld runtime, agent runtime, model proxy shipped by
Keld, MCP extension, or fifth architectural unique. Current instructions, approved
specs, code, tests, Git, and Linear remain authoritative; recalled text is bounded,
read-only, untrusted data.

The compatibility candidate is upstream prerelease `v2.0.1-beta.2` at commit
`29d609a729704ae31ff1848dc6f8acb7e712106d`, synthetic-only. That commit is not a
complete deployment pin; every image still needs a reviewed immutable digest in T4.
The user's default Codex configuration remains unchanged. A later experiment uses a
separate `$CODEX_HOME/<name>.config.toml` overlay (default
`~/.codex/<name>.config.toml`) selected deliberately with `--profile <name>`, following
[OpenAI's profile contract](https://learn.chatgpt.com/docs/config-file/config-advanced#profiles).
Authorization filters project, owner, visibility, team, and agent before ranking;
every returned claim is verified against current evidence before action.

**Why.** A reviewed notebook may reduce repeated investigation and improve
cross-platform handoffs. It changes no OS-handle owner, crash boundary, or principal,
so the work is developer tooling rather than architecture. KEL-44 may later test the
hypothesis with controlled, paired evidence; vendor benchmark claims do not count.

**Why not.** No Keld service, proxy, crate/package, app configuration, permission,
wire change, `.mcp.json` entry, automatic transcript capture, automatic recall
injection, repository copy, credential, or authority channel. Real Keld data, team
use, a remote deployment, and the vendor's managed cloud remain outside the current
approval. Memory cannot authorize commands, widen scope, bypass gates, resolve a
code/spec conflict, or mark work complete.

**Implemented vs approved.** T3 contains policy, a conditional agent playbook, and
a deliberately non-runnable onboarding page. There is no launcher, reviewed image
digest set, provider block, credential, real Keld data flow, or support/security claim.
The official Codex CLI path is documented but remains unexercised until T6; desktop/IDE,
Linux, Windows, WSL2, Docker Desktop, team, and remote behavior remain unverified.

**Next.** T4 may create a separately reviewed launcher outside Keld with exact source
and image pins, loopback-only listeners, no Keld mount, explicit capture/write/injection
off, named providers, secret indirection, and an uninstall manifest. T5 runs hostile
synthetic controls; T6 exercises one explicit Codex CLI profile and proves restoration.
Only then may KEL-44 add a memory-on evaluation arm. If isolation, security, or measured
value fails, remove the external pilot without changing Keld.

**Evidence.** [`optional-agent-memory-pilot.md`](../specs/optional-agent-memory-pilot.md),
[`08-optional-agent-memory.md`](../onboarding/08-optional-agent-memory.md),
[TencentDB Agent Memory beta install guide](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/29d609a729704ae31ff1848dc6f8acb7e712106d/INSTALL.md).

---

## 14. `keld-update` v0 manifest/feed wire contract (KEL-53 trigger)

**Chose (2026-08-18).** KEL-53 ("updater: signed manifest and delta patch vertical
slice") names its own trigger: start only once the update artifact/feed contract in
architecture 06 §4 is concrete enough for executable acceptance tests. §4 was prose —
"signed manifests", "ed25519", "BLAKE3 post-conditions" — with no byte-level shape, so
the ticket was not actually unblocked. Added §4a: a static feed layout keyed by
**channel and target** (`<channel>/<target>/updates.json` + a **detached**
`<channel>/<target>/updates.json.sig` file, not a signature field embedded in the
JSON — avoids a canonicalization requirement between signer and verifier because the
signature covers the exact response bytes, checked before parsing; §4a separately
specifies duplicate-key rejection and an unrecognized-`schema` fail-closed rule, since
detached signing alone does not remove parser-level ambiguity), a v0 `updates.json`
schema (`schema`, `channel`, `target`, `app.id`, `releases[].{version, full,
deltas[]}`, `full` required on every release, no duplicate `version`/`fromVersion`), a
multi-step client verification order (fetch/verify → parse/identity-check →
shape-validity → floor-filter/select → download → transport-hash → decompress/
content-hash → full-fallback on any delta failure → install) ending in
atomic-swap/N-1-rollback. Facts worth holding onto: the ed25519 public key **must** be
compiled into the host binary, never fetched from the feed; every artifact's `blake3`
proves only that *its own downloaded bytes* are intact, so `full.contentBlake3` is a
second, separate hash domain over the *decompressed, installable* content — itself a
fully byte-specified POSIX-tar profile, not "whatever decompresses" — that both the
full path and the post-delta reconstruction path must converge on; `app.id`/`channel`/
`target` are checked fail-closed against the host's own identity so a correctly-signed
manifest for a different app, channel, or target is rejected on principle, not just on
missing content; and a **persisted
version floor** (not the running version) is what a replayed old signed manifest is
checked against, so an authorized local rollback can run an older version without
reopening the door to an attacker replaying a stale feed.

**Why.** `AGENTS.md` §Working rules forbids landing an RFC that "restates
`docs/architecture/` without binary acceptance tests" — the fix for prose that can't be
tested is a concrete schema, not a longer paragraph. This is also explicitly a wire
protocol change (root `AGENTS.md` review gate #5), so it is docs-only and flagged for
human sign-off rather than paired with code.

**Why not.** Did not pick bsdiff vs HDiffPatch (KEL-53 AC2 is an explicit benchmark,
not a docs-time guess), did not add `ed25519-dalek`/`zstd`/a delta crate (KEL-53 AC3,
its own dependency-review gate), and did not spec a TUF-style rotating root — 03
§4 point 4 names one as a future target; v0 here is a single pinned key, stated as a
limitation rather than silently narrowed. All three stay KEL-53's decisions, not this
change's.

**Update (2026-08-18, review pass before human sign-off).** An adversarial review of
the first draft found eight real gaps, not style nits, each fixed in §4a directly
rather than patched around: (1) the feed layout hardcoded a `.bsdiff.zst` extension
while this same record says the algorithm is undecided — renamed to algorithm-neutral
`.delta.zst`, and softened `crates/keld-update/src/lib.rs`'s module doc, which flatly
asserted "bsdiff" two lines above its own "not yet chosen" caveat; (2) `full` read as
optional by omission — now explicit that every release requires it; (3) one `blake3`
field was doing two jobs (artifact-transport integrity and reconstructed-content
correctness) with no way to tell which failure a mismatch meant — split into `blake3`
(downloaded bytes) and `full.contentBlake3` (decompressed, installable bytes both the
full and delta paths converge on); (4) nothing bound a manifest to *this* app or
channel beyond a valid signature, so a correctly-signed manifest for a different app or
a different channel would have passed — added a fail-closed identity check; (5) a
delta that failed content verification would be re-selected forever, since the fallback
said "try `full` on the next poll" but the next poll re-picks the same delta — fixed to
fall back to `full` within the same attempt; (6) atomic swap named the mechanism
(symlink / pointer rename) but not the crash-safety sequence — added the
temp-pointer/fsync/durable-rename discipline (POSIX and Windows), a `.complete`
directory marker, and startup recovery for an absent/corrupt pointer; (7) rollback had
no defense against a stale signed manifest being replayed through the feed to force a
downgrade — added a persisted version floor, separate from the running version, that a
local rollback intentionally does not reset; (8) the signature section's own claim
overstated what detached signing buys (no canonicalization *between signer and
verifier*, not immunity to parser-level ambiguity) — narrowed the wording and added
duplicate-key rejection. None of this reopens KEL-53 AC2/AC3 (algorithm, dependencies)
or the TUF-root deferral — all three exclusions from "Why not" stand unchanged.

**Update (2026-08-18, second review pass).** A second, deeper pass over the now-larger
§4a found seven more gaps — including a real bug in the first pass's own fix. Fixed
directly: (1) nothing bound the manifest to a *platform/architecture* the way `app.id`/
`channel` bound it to an app/channel — added `<target>` to the feed path and a
redundant `target` field, same defense-in-depth shape as the app/channel check; (2)
release/delta selection wasn't fully deterministic — duplicate `version`/`fromVersion`
now invalidate the whole manifest, and among eligible releases the client takes the
single highest version, not "any newer"; (3) `contentBlake3` hashed "decompressed
bytes" with no defined package format, so two clients could verify identical bytes and
extract different trees — defined v0's canonical content stream as a single POSIX tar,
sorted entries, regular files only, uniform modes, named as a v0 limitation for
symlink-heavy formats like macOS `.app` bundles; (4) `size` was carried in the schema
but never checked — made it normative: bounded, streaming downloads that reject over-
and under-sized artifacts before decompression; (5) the full-fallback step only
triggered on a content-hash failure, missing transport-hash, decompression, and
patch-application failures on the delta path — broadened to any delta-path failure;
(6) the `.complete` marker's own fsync was unspecified, so the crash-safety fix from
the first pass had a hole in itself — added explicit fsync-the-marker-then-fsync-the-
directory-again ordering; (7) the real bug: `current` and the version floor (added in
the first pass specifically to stop replay/downgrade attacks) are two separate durable
files with no shared transaction, so a crash between publishing one and the other could
leave `current` ahead of the floor — fixed by making publish order load-bearing (floor
always advances *before* `current` is republished) and adding a startup-recovery case
that completes an interrupted publish from already-verified local state, never from
anything the crash left ambiguous.

**Update (2026-08-18, third review pass).** A deeper pass over the now-much-larger
§4a found a second real bug plus real gaps, not restated nits: (1) nothing bound the
manifest to a target platform/architecture — added `<target>` to the feed path and a
redundant manifest field, mirroring the app/channel check; (2) release/delta selection
still had a documented ambiguity between "the floor rejects the manifest" and "the
floor filters the eligible set" — resolved explicitly as filtering (a normal feed's
historical releases are not a schema violation); (3) `contentBlake3` covered
"decompressed bytes" with no byte-exact package format, so two conforming
implementations could still disagree — replaced with a fully specified POSIX-tar
profile (entry types, name/mode/uid/gid/mtime/magic/version/checksum, sort order,
block padding — an exhaustive list, not an example); (4) that same tar definition
exposed a real vulnerability class: sorted paths alone do not stop path-traversal
during extraction, so added explicit reject-before-write rules for absolute paths,
`..` components, and anything that resolves outside the destination directory
("tar slip"); (5) `size` was in the schema but bounded only the compressed download,
not decompressed output — a valid small artifact could still be a decompression bomb;
added `contentSize` as an explicit, incrementally-enforced decompression ceiling; (6)
**the second real bug**: after extraction, the previous version exists only as a
directory tree on disk, but a delta's base was specified as "the currently-installed
content" with no defined byte stream to patch against — extraction is lossy relative
to exact reproduction, so a re-serialized tree is not guaranteed to match the original
tar bytes; fixed by retaining each version's exact `content.tar` alongside its
extracted form, specifically as the only valid delta base; (7) **the third real bug**:
the first pass's own replay/downgrade fix (the persisted version floor) could be
silently defeated by its own recovery logic — after an intentional rollback, `current`
legitimately sits behind the floor, which is exactly the state the first pass's
"complete an interrupted publish" recovery rule would have auto-corrected, undoing the
rollback on the next restart. Fixed with a `publish-intent` marker that names an
in-flight forward publish and is never written by rollback, so recovery can tell "an
update was interrupted" from "the host deliberately rolled back" instead of conflating
them.

**Next.** KEL-53 can now write its failing-first fixtures (valid/tampered manifest,
corrupted patch, full-package fallback, N-1 rollback, identity mismatch, replay/
downgrade, crash-interrupted install, wrong-target manifest, duplicate release/delta
entries, oversized/undersized artifact, decompression-bomb content, path-traversal
archive entries, rollback surviving a restart) against a concrete shape instead of
inventing one mid-ticket. `crates/keld-update/src/lib.rs`'s module doc points here;
still zero verification code — nothing in this change reads or checks the contract it
describes.

**Evidence.** `docs/architecture/06-runtime-and-tooling.md` §4a; `crates/keld-update/src/lib.rs`.

---

## Related tracked docs

| Need | Document |
|---|---|
| Bindings for agents | [`AGENTS.md`](../../AGENTS.md), crate `AGENTS.md`, [`docs/agents/workflow.md`](../agents/workflow.md) |
| Design target | [`docs/architecture/01..07-*.md`](../architecture/) |
| Error codes | [`keld-error-codes.md`](./keld-error-codes.md) |
| Hello size / RSS / installer | [`budget-scoreboard.md`](./budget-scoreboard.md) |
| Licenses for binaries | [`third-party-licenses.md`](./third-party-licenses.md) |
| Toolchain history | [`tooling-audit.md`](./tooling-audit.md) |
| Linear vs ROADMAP phases | [`linear-roadmap-mapping.md`](./linear-roadmap-mapping.md) |
| One-line gotchas | [`docs/agents/learnings.md`](../agents/learnings.md) |
| Optional external contributor memory | [`optional-agent-memory-pilot.md`](../specs/optional-agent-memory-pilot.md), [`08-optional-agent-memory.md`](../onboarding/08-optional-agent-memory.md), [`.agents/memory.md`](../../.agents/memory.md) |
| Where to read next | [`docs/onboarding/06-documentation-map.md`](../onboarding/06-documentation-map.md) |
