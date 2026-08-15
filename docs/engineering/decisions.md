# Engineering decisions

Human-facing log of **what we chose, why, what we rejected, and what is not next**.
Last confirmed against the tree on 2026-08-13.

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

**Implemented vs specified.** The four uniques are the design. Today the tree is a
macOS hello window plus a kipc echo slice. `keld-guard::evaluate` exists;
`keld_permissions_explain` and the macOS webview media-capture handler call it;
`keld-core` / `keld-native` do not invoke the guard on privileged IPC.
`keld-runtime` is still a `RestartPolicy` struct; `keld dev` spawns `bun` from the
CLI. Hold both facts.

**Next.** Keep shipping Linear Phase 2 (window + kipc echo + crate map) on these four.
Do not add a fifth unique to look complete.

---

## 2. Webview: wry+tao scaffolding, spec 05 destination

**Chose (2026-07-08; wry pin updated 2026-08-14 for KEL-59; Windows superseded
2026-08-15 — see the update below).** The live macOS backend is tao 0.35.3 +
wry 0.56.1 (`devtools` feature) in `crates/keld-wv/src/wkwebview/mod.rs`.
Module comment: interim implementation, replace with direct objc2 bindings per
architecture 05 §1. Linux remains a compiled layout slot returning
`KELD-WV-001` and naming KEL-28.

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

**Why not treat wry as the product.** Architecture 05 lists hooks wry does not
prioritize (scheme-streaming as bulk IPC, principal identity per navigation, engine
policy switching, `webContents`-grade control). The host is prebuilt, so wry's
“works in any downstream cargo build” constraint does not apply.

**MPL is not wry's license.** wry 0.56.1 is Apache-2.0 OR MIT; tao 0.35.3 is
Apache-2.0. The MPL-2.0 crate in the graph is **`option-ext` 0.2.0**, reached
`keld-wv → wry → dirs → dirs-sys → option-ext` (`deny.toml`,
[`third-party-licenses.md`](./third-party-licenses.md), learnings 2026-08-13). Do not
describe wry as MPL.

**Next.** Keep the Phase 2 window on wry+tao. Rewrite a backend to objc2/windows-rs/
webkitgtk when that backend needs a wry-missing hook, or when landing KEL-27/KEL-28
on a machine that actually runs that OS — not as a macOS-week rewrite. See §11.

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
| **KEL-27 / KEL-28 on a Mac** | WebView2 and WebKitGTK slots compile everywhere and return `KELD-WV-001`. A live backend needs that OS's window server and a machine that can run the smoke. Implementing them on macOS as if they were done would be a stub, which `AGENTS.md` forbids on main. | `crates/keld-wv/src/webview2/mod.rs`, `webkitgtk/mod.rs`; architecture 05 §1 |
| **Six-framework notes-app bench** | Same app in Keld / Electron / Electrobun / Tauri / Wails / Swift is a later epic. Fair score is **v1** (search, GFM preview, custom-scheme images, native menus, second window, PDF-exists, file watch, autosave) — not v0 CRUD. Keld cannot host that without guard-on-IPC and host `fs.read`/`fs.write`. Bun `node:fs` is not a default-deny test. | architecture 01 §5; this file §11; research 43 / 43a (exploratory, not required) |

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
| Where to read next | [`docs/onboarding/06-documentation-map.md`](../onboarding/06-documentation-map.md) |
