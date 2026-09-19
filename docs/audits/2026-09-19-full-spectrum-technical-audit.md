# KELD Full-Spectrum Technical Audit — 2026-09-19

> **Historical snapshot.** This report evaluates pinned revisions. It is an AI-assisted
> maintainer technical audit, not a third-party certification. Product changes after the
> audited revisions do not change this report's conclusions; a later state requires a
> new dated audit.

## A. Audit identity

| Field | Value |
|---|---|
| Audit date | 2026-09-19 |
| KELD revision | [`0ea0780bb574ad242e9f1105fa4af5842872bad3`](https://github.com/gyldlab/keld/commit/0ea0780bb574ad242e9f1105fa4af5842872bad3) |
| KELD Benches revision | [`da36dd9b26a0f4134d53899818a877b9e2f0620b`](https://github.com/gyldlab/keld-benches/commit/da36dd9b26a0f4134d53899818a877b9e2f0620b) |
| Product maturity stated by project | Pre-alpha |
| Audit mode | Read-only inspection plus isolated verification |
| Public evidence rule | Every published conclusion below is supportable without private repository or work-management access |

The audit covered the public KELD source tree, architecture and status documentation,
tests, selected Git history and CI evidence, the public benchmark repository, and
authoritative upstream documentation where platform semantics mattered. Open pull
requests were inspected only as context and are **not** counted as landed capability.
Upstream platform facts used below are frozen in a [pinned audit-time receipt](evidence/upstream/2026-09-19-platform-semantics.json); live upstream URLs are convenience links, not the historical evidence anchor.

The audit did not attempt to prove the absence of defects. No S0 or S1 finding was
established by this audit; that statement is **not** a claim that no S0/S1 issue exists.

## B. Executive state

At `0ea0780bb574`, KELD has a functioning pre-alpha desktop demonstration architecture,
substantial IPC and lifecycle engineering, and materially narrower product capability
than its long-term framework vision.

The strongest demonstrated areas are authenticated application-link IPC, the TypeScript
transport's framing and receiver validation, lifecycle/output handling, explicit
rejection of reserved-but-unimplemented CLI verbs, host-owned developer staging, and
parts of the Linux benchmark methodology.

The largest current gaps are not cosmetic. General Electron migration, the renderer
bridge, broad native services, cross-platform strict authority containment, application
packaging/signing/updates, and broad real-application compatibility are not demonstrated
as production-ready capabilities in the audited revision.

## C. Surface ledger

The project's own generated status ledger is the primary current/target inventory:
[`docs/engineering/product-status.md`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/docs/engineering/product-status.md).

| Surface | Audit status | Evidence boundary |
|---|---|---|
| Rust host / app session | Partially demonstrated | No-flag sessions and lifecycle slices exist; destination role/privileged dispatch remains incomplete. |
| Bun TypeScript main process | Partially demonstrated | Supervised primary process and authenticated app link exist; full role family and strict admission are incomplete. |
| System webviews | Partially demonstrated | WKWebView, WebView2, WebKitGTK backends exist; renderer bridge and identity-bound persistence remain incomplete. |
| KIPC | Demonstrated for current app-link slice | Framing, HELLO authentication, Unix bootstrap, Windows named pipe, TS transport and tests exist. |
| Permission guard | Partially demonstrated | Parsing, verified loading and guard-before-handler dispatch exist; production native-service coverage remains incomplete. |
| Native service broker | Prototype / partial | Scoped filesystem broker is guard-checked and wire-tested but is not wired into the ordinary production host path. |
| Electron compatibility | Partial, narrow | Lifecycle facade exists; `BrowserWindow`, `ipcMain`, broader modules and automatic migration do not. |
| CLI | Partial | `create`, `dev`, `doctor`, MCP and diagnostics exist; `build`, `migrate`, `gen`, `ext` are explicitly reserved. |
| Packager | Skeleton | Format contract only. |
| Updater | Skeleton | Channel contract only. |
| Renderer-to-host bridge | Planned | General bridge is explicitly incomplete. |
| Shipping installers/signing | Planned | Not available in audited revision. |

## D. Architecture reality map

| Boundary | Audited reality | Destination gap |
|---|---|---|
| Native application owner | Rust host owns window/session lifecycle. | Broader public role API and release artifact chain. |
| Application logic | Bun runs the current TypeScript main process. | Principalized role family and broad Node-shaped compatibility. |
| UI engine | Platform webview backend selected by OS. | General renderer bridge, persistent profile identity, broader platform qualification. |
| Host ↔ Bun IPC | Authenticated KIPC app link. | Generated schema/codegen, channel registry and optional bulk lanes. |
| Privileged authority | Guard primitives and one filesystem broker exist. | Complete broker wiring and equivalent strict containment across supported platforms. |
| Distribution | Source-built evaluation flow. | `keld build`, installers, signing/notarization and signed updates. |

A destination described in architecture is not evidence that the current product
implements it.

## E. Claim matrix

| Claim | Audit classification |
|---|---|
| Rust host owns the current native app session | Demonstrated |
| Bun can serve as the current TypeScript primary process | Partially demonstrated |
| KELD uses system webview backends on the current three OS paths | Partially demonstrated |
| Authenticated, validated app-link IPC exists | Demonstrated |
| Default-deny privileged model is fully wired end-to-end | Partially demonstrated |
| Three-platform zero-ambient-authority execution | Partially demonstrated |
| Electron apps can currently migrate without a full rewrite | Planned |
| General native KELD app authoring is complete | Prototype only |
| Application backend can generally be selected as Rust or TypeScript | Planned |
| General renderer IPC/codegen is available | Planned |
| Steady-state IPC is allocation-free | Contradicted by current implementation |
| Persistent webview storage is isolated by authenticated app identity | Unverified; Windows current path is shared |
| Packaging, signing and signed updating are available | Planned |
| Low-latency IPC has useful evidence | Partially demonstrated |
| Small full-product memory/package footprint is proven | Unverified |
| Broad desktop-framework compatibility parity is proven | Partially demonstrated at most |
| KELD is proven superior to established desktop frameworks | Unverified |

## F. Findings register

| ID | Severity | Confidence | Finding |
|---|---|---|---|
| F-01 | S2 | Confirmed | Developer staging is a bounded demo stage, not a general application staging/permissions pipeline. |
| F-02 | S2 | High | Windows WebView2 storage uses a shared `dev.keld` user-data namespace rather than a demonstrated per-app namespace. |
| F-03 | S2 | High | Filesystem authorization is lexical and is not bound to the opened filesystem object. |
| F-04 | S2 | High | Equivalent shipping authority containment is not established across Linux, macOS and Windows. |
| F-05 | S3 | Confirmed | `glib 0.18.5` is present in the dependency graph and is in the affected range of RUSTSEC-2024-0429; affected-function reachability is unverified. |
| F-06 | S3 | Confirmed | Agent/path invariant says steady-state IPC avoids allocation, while current codecs/link/TS writer allocate. |
| F-07 | S3 | Confirmed | Architecture memory-budget census and current Linux `MEM-IDLE` scored denominator differ. |
| F-08 | S3 | High | TypeScript outbound write serialization lacks an aggregate admission bound and does not include queue wait in the per-write deadline. |
| F-09 | S3 | High | `doctor --json` output and process status are derived from two separate finding executions. |
| F-10 | S3 | High | Windows WebView2 COM initialization result/ownership is not explicitly balanced in the inspected constructor. |

### F-01 — Developer staging is not a general application pipeline

`crates/keld-cli/src/boot.rs` stages selected app inputs and writes the permissions file
from the constant `PERMISSIONS_BYTES = b"{}\n"`. The boot descriptor hashes that
generated empty policy rather than staging an arbitrary application-supplied permissions
manifest.

Evidence:
- [`crates/keld-cli/src/boot.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-cli/src/boot.rs)
- [`README.md`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/README.md) — the current scaffold is intentionally self-contained and has no package dependencies.

**Impact.** The current hello/developer slice is bounded and understandable, but it must
not be presented as proof of a general dependency/permission staging pipeline.

### F-02 — Windows WebView2 persistent storage is shared

The WebView2 backend defines `PROFILE_IDENTIFIER` as `dev.keld` and derives its user-data
directory from that constant. The current source comment also states that config
identifier plumbing is not built yet.

Evidence:
- [`crates/keld-wv/src/webview2/mod.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-wv/src/webview2/mod.rs)
- [Pinned audit-time upstream receipt](evidence/upstream/2026-09-19-platform-semantics.json) (`microsoft-webview2-udf`)
- [Microsoft WebView2 user-data-folder guidance — live source](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/user-data-folder)

Microsoft documents the user data folder as the location for browser data such as
cookies, permissions and cache, and supports sharing a UDF across applications. A
constant KELD namespace therefore does not demonstrate per-app persistent-state
partitioning.

### F-03 — Filesystem authorization is not object-bound

The filesystem broker dispatches a guard decision using a path string and then performs
`std::fs::read(path)` or `std::fs::write(path, bytes)`. Its own source documents the v0
path matcher as literal with no symlink resolution.

Evidence:
- [`crates/keld-native/src/fs.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-native/src/fs.rs)

**Impact.** A lexical authorization decision is separated from the later filesystem
object resolution, leaving symlink/TOCTOU classes of risk if this broker is exposed to
untrusted paths. Reads also materialize the whole file before the transport-size
boundary. The audit did **not** demonstrate a working exploit, and the audited ordinary
production host path does not yet wire this broker, so this is a latent enforcement gap
rather than a claim of an exposed production exploit.

### F-04 — Authority containment is platform-asymmetric

The current Linux no-flag primary consumes the strict Linux preparation path. macOS has
guardian/lifecycle hardening and Windows has lifecycle/named-pipe protections, but this
audit did not establish equivalent strict ordinary-product containment for the Bun
primary on those two platforms.

Evidence:
- [`crates/keld-runtime/src/linux_strict.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-runtime/src/linux_strict.rs)
- [`crates/keld-core/src/app_session.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-core/src/app_session.rs)
- [`docs/engineering/product-status.md`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/docs/engineering/product-status.md)

This finding is about **unproven equivalence**, not a claim that the macOS or Windows
lifecycle mechanisms provide no protection.

### F-05 — `glib` advisory exposure

The audited lockfile contains `glib 0.18.5`, and the GTK/WebKitGTK dependency path makes
it part of the Linux graph. RustSec advisory RUSTSEC-2024-0429 covers `glib` versions
from 0.15.0 through versions before 0.20.0 for unsound `VariantStrIter` methods.

Evidence:
- [`Cargo.lock`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/Cargo.lock)
- [Pinned audit-time upstream receipt](evidence/upstream/2026-09-19-platform-semantics.json) (`rustsec-2024-0429`)
- [RUSTSEC-2024-0429 — live source](https://rustsec.org/advisories/RUSTSEC-2024-0429.html)

The audit established dependency exposure, **not** that KELD executes an affected method
on a reachable hostile input path.

### F-06 — Allocation-free IPC instruction contradicts code

The nearest IPC instruction forbids steady-state allocation, but the audited
implementation uses `postcard::to_allocvec`, allocates a payload buffer in the Rust link,
and creates/copies into a combined `Uint8Array` for a TypeScript write.

Evidence:
- [`crates/keld-ipc/AGENTS.md`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-ipc/AGENTS.md)
- [`crates/keld-ipc/src/codec.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-ipc/src/codec.rs)
- [`crates/keld-ipc/src/link.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-ipc/src/link.rs)
- [`packages/@keld/kipc/src/transport.ts`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/packages/%40keld/kipc/src/transport.ts)

This is a contract/invariant contradiction. The audit does not claim these allocations
alone create a measured user-visible regression.

### F-07 — Memory budget and benchmark score use different censuses

Architecture 01 defines the idle-RSS budget as KELD-owned host plus supervised family,
excluding engine helpers and development-loop helpers. The audited Linux benchmark
preserves the existing scored `MEM-IDLE` denominator as staged `keld-host` RSS and emits
CLI/Bun/KELD-owned/tree RSS as diagnostics.

Evidence:
- [`docs/architecture/01-overview.md`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/docs/architecture/01-overview.md)
- [`linux/bench/harness.py`](https://github.com/gyldlab/keld-benches/blob/da36dd9b26a0f4134d53899818a877b9e2f0620b/linux/bench/harness.py)
- [Wayland product memory raw result](https://github.com/gyldlab/keld-benches/blob/da36dd9b26a0f4134d53899818a877b9e2f0620b/linux/bench/results/mem-idle/2026-09-18.kel90-linux-product-wayland-memory-30.fresh-process.json)

The benchmark is still useful diagnostic evidence, but the score cannot be treated as a
direct pass/fail of the architecture's full KELD-owned memory census until those
denominators converge.

### F-08 — Outbound TypeScript queue has no aggregate admission bound

The TypeScript transport serializes writes through a promise chain and applies a
per-payload bound. The audit did not find an aggregate queued-byte or queued-frame
admission limit. The write deadline starts when the queued operation reaches the actual
write function, so time already spent waiting in the queue is outside that deadline.

Evidence:
- [`packages/@keld/kipc/src/transport.ts`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/packages/%40keld/kipc/src/transport.ts)

A sustained-pressure runtime reproduction was not run, so the finding is a
high-confidence static backpressure/liveness limitation rather than a measured
exhaustion incident.

### F-09 — `doctor --json` runs findings twice

`findings_json(project_root)` serializes `run_findings(project_root)`. The CLI then calls
`run_findings(project_root)` again to decide whether to exit successfully.

Evidence:
- [`crates/keld-cli/src/doctor.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-cli/src/doctor.rs)
- [`crates/keld-cli/src/main.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-cli/src/main.rs)

A changing environment can therefore make printed JSON and exit status describe
different observations. No race was reproduced during this audit.

### F-10 — Windows COM initialization ownership is not explicit

The inspected WebView2 constructor calls `CoInitializeEx` and discards its result. The
module search did not establish a balancing `CoUninitialize` for a successful constructor
initialization.

Evidence:
- [`crates/keld-wv/src/webview2/mod.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-wv/src/webview2/mod.rs)
- [Pinned audit-time upstream receipt](evidence/upstream/2026-09-19-platform-semantics.json) (`microsoft-couninitialize`, `microsoft-com-initialization`)
- [Microsoft `CoUninitialize` documentation — live source](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-couninitialize)
- [Microsoft COM initialization guidance — live source](https://learn.microsoft.com/en-us/windows/win32/learnwin32/initializing-the-com-library)

Microsoft requires each successful COM initialization on a thread, including the
`S_FALSE` case, to be balanced by a corresponding uninitialization. This audit did not
run a live Windows reproduction for the constructor path.

## G. Security

### Identity and authentication

The current app link has meaningful security engineering: HELLO authentication,
session-scoped bootstrap material and a current-user-protected Windows named pipe are
implemented for the audited slice.

### Authorization

Manifest parsing, verified startup loading and guard-before-handler primitives are real.
An older concern that the host never loads a permissions file is **not applicable** to
the audited revision: current `app_session` code calls verified manifest loading before
running the app session.

Authorization is nevertheless incomplete as a product boundary because the developer
stage emits an empty policy and broad production native-service dispatch is not wired.

### OS containment

Linux strict primary preparation is the strongest integrated containment slice in this
revision. Equivalent ordinary-product containment on macOS and Windows was not
established by this audit. Authenticated IPC and process cleanup are valuable controls,
but they are not substitutes for restricting ambient filesystem/network/process
authority.

### Lifecycle and revocation

The host/runtime contains explicit cleanup, recovery, generation and parent-death
mechanisms. On macOS, the guardian reopens and checks the entry identity before launch,
but the inspected flow still ultimately launches Bun by path after dropping the reopened
handle, leaving a residual same-user replacement window documented by the implementation
itself.

## H. Compatibility

The current `@keld/electron` package is intentionally narrow.

| Surface | Audited result |
|---|---|
| `app.whenReady()` / lifecycle | Implemented for current facade |
| `window-all-closed` | Implemented with KELD-specific tested semantics |
| `app.quit()` | Deliberate divergence: returns `Promise<void>` |
| Listener failures | Deliberate isolation behavior differs from Electron EventEmitter propagation |
| `BrowserWindow` | Not implemented |
| `ipcMain` and broader Electron modules | Not implemented |
| `keld migrate` | Reserved, not implemented |
| General Node/N-API/NAN/node-pty parity | Unverified |
| VS Code-class migration | Unverified |

Evidence:
- [`packages/@keld/electron/src/index.ts`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/packages/%40keld/electron/src/index.ts)
- [`crates/keld-cli/src/verb.rs`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/crates/keld-cli/src/verb.rs)
- [`docs/engineering/compat-scoreboard.md`](https://github.com/gyldlab/keld/blob/0ea0780bb574ad242e9f1105fa4af5842872bad3/docs/engineering/compat-scoreboard.md)

The long-term “move beyond Electron without rewriting application core logic” direction
remains a product goal, not a demonstrated broad migration result at this revision.

## I. Performance

The public benchmark repository has materially stronger methodology than a collection of
headline numbers: it records provenance, process census, workload identity, cache class,
publication reasons and raw samples, and has tests around statistical and lifecycle
behavior.

### Recomputed Linux product evidence

From the audited public raw artifacts:

| Metric | Audited diagnostic result | Publication state |
|---|---:|---|
| Wayland `keld dev` paint opportunity, fresh process | median **533.646 ms**, 30 valid samples; audit bootstrap CI ≈ **524.703–543.409 ms** | Not publication-eligible |
| Wayland staged host RSS, fresh process | median **179,710 KiB** | Not publication-eligible |
| Wayland total descendant-tree RSS diagnostic | median **494,352 KiB** | Diagnostic |
| Wayland total private-dirty diagnostic | median **75,978 KiB** | Diagnostic |

The raw product paint and memory documents explicitly mark publication as ineligible
because thermal state was not independently verified, no paired arm is present, and the
measurement is a developer-flow (`keld dev`) scope rather than packaged-app startup.

Evidence:
- [Wayland product paint raw result](https://github.com/gyldlab/keld-benches/blob/da36dd9b26a0f4134d53899818a877b9e2f0620b/linux/bench/results/paint-opportunity/2026-09-18.kel90-linux-product-wayland-30.fresh-process.json)
- [Wayland product memory raw result](https://github.com/gyldlab/keld-benches/blob/da36dd9b26a0f4134d53899818a877b9e2f0620b/linux/bench/results/mem-idle/2026-09-18.kel90-linux-product-wayland-memory-30.fresh-process.json)
- [`HARNESS-CONTRACT.md`](https://github.com/gyldlab/keld-benches/blob/da36dd9b26a0f4134d53899818a877b9e2f0620b/HARNESS-CONTRACT.md)

### Bun-client versus Rust-library IPC diagnostic

The recorded Linux paired diagnostic at the audited bench revision reports:

| Cache class / payload | Rust library p99 | Bun product-client p99 |
|---|---:|---:|
| Fresh / 6-byte | 11.796 µs | 30.567 µs |
| Fresh / 1,024-byte | 13.324 µs | 40.348 µs |
| Warm / 6-byte | 11.583 µs | 29.177 µs |
| Warm / 1,024-byte | 13.340 µs | 38.305 µs |

These arms do not isolate a pure “Bun tax”; they compare a product client path against a
Rust library floor. They are useful diagnostics, not proof that KELD as a full framework
is faster than another framework.

The README's public macOS 9.375/10.25 µs values are separately pinned Rust↔Rust KIPC
library measurements and are correctly described as such.

## J. Cross-platform reality

| Capability | macOS | Windows | Linux |
|---|---|---|---|
| Native system-webview backend | Present | Present | Present |
| No-flag product session slice | Present | Present | Present for qualified Ubuntu/Debian x86_64 Wayland slice |
| Authenticated app link | Present | Present | Present |
| Strict ordinary-primary containment established in product path | Not established by this audit | Not established by this audit | Present for current strict primary slice |
| Per-app persistent webview identity | Unverified | Shared constant profile in audited source | Unverified |
| General renderer bridge | Planned | Planned | Planned |
| Production packaging/signing/update | Planned | Planned | Planned |

A fresh real-device GUI/signing campaign on all three platforms was outside this audit.
Historical captures remain evidence only for the revisions and environments they record.

## K. Testing and CI

The audit ran isolated verification against the audited KELD/bench source rather than
modifying the audited working trees.

| Verification | Result |
|---|---:|
| Rust library tests across guard/ipc/native/compat/runtime/core | **353 passed** |
| Rust ignored fixture/helper tests | 3 ignored |
| TypeScript `@keld/kipc` + `@keld/electron` tests | **111 passed, 0 failed** |
| TypeScript expectations | 2,209 |
| Linux benchmark harness tests in an exact Git checkout | **38 passed** |
| Benchmark schema check | Passed |
| CI-required contract self-test | Passed |
| CI change-router contract in an exact Git checkout | Passed |

The GitHub Actions run associated with the audited KELD main commit completed
successfully: [run 35276772893](https://github.com/gyldlab/keld/actions/runs/35276772893).
That commit was documentation-scoped, so routed Rust runtime jobs were skipped. A green
docs-only run must not be represented as a fresh three-platform runtime test matrix.

## L. Dependencies and upstream boundary

KELD mostly relies on normal Cargo/npm/upstream platform dependencies rather than a
tracked vendor tree. The audit did not establish a tracked Git submodule or vendored
third-party source tree in the inspected product topology.

The `glib` advisory in F-05 is the clearest confirmed dependency exposure from this
audit. The repository's dependency/license policy and automated checks are useful, but
this audit is **not** a complete SBOM or legal-license audit.

## M. Documentation and drift

The main README is a strength: it explicitly calls KELD pre-alpha, says it is not yet a
drop-in Electron replacement or production distribution toolchain, names missing
`BrowserWindow`/`ipcMain`/migration capability, and scopes benchmark claims.

Material drift still exists:
- the allocation-free IPC instruction does not match current code (F-06);
- the architecture memory census does not match the currently scored Linux benchmark
  denominator (F-07).

Open PR descriptions and future specs were not promoted into current product claims.

## N. Transitional, dead and deliberately reserved surfaces

The audit found explicit transitional behavior rather than hidden command theater:
- `build`, `migrate`, `gen`, and `ext` are reserved and return a typed not-implemented
  diagnostic;
- `keld-pack` and `keld-update` are skeleton contracts, not disguised finished systems;
- the general renderer bridge is documented as incomplete.

Two hypotheses from earlier inspection were corrected during this audit rather than
retained as findings:
1. **Permission loading:** current host session code does load a verified permission
   manifest; the remaining issue is the empty developer-stage policy and incomplete
   privileged routing, not total absence of loading.
2. **Linux GPU safe mode:** current WebKitGTK preparation uses an exact-self re-exec path
   to add the environment override and fails closed when preparation is missing; the
   earlier unsafe-live-environment-mutation suspicion does not apply to this revision.

## O. Strengths

- Public-facing status language is unusually explicit for a pre-alpha framework.
- Reserved CLI functionality fails explicitly instead of pretending to exist.
- The app-link has authentication and meaningful negative-test coverage.
- TypeScript framing/receiver tests cover malformed and poisoned-path behavior.
- Host/runtime lifecycle work includes recovery and cleanup rather than only happy-path
  window creation.
- Linux benchmark artifacts carry provenance and publication-blocking reasons rather
  than automatically turning every measurement into a marketing claim.
- Product-status documentation distinguishes current, target and evidence.

## P. Unknowns and non-claims

This audit does **not** establish:
- a fresh Windows real-device acceptance result for the inspected revision;
- a fresh macOS signing/notarization/sandbox acceptance campaign;
- broad real-world Electron application migration success;
- general VS Code extension-host compatibility;
- general native-addon compatibility;
- long-running production workload stability;
- complete release/update-chain security;
- complete unsafe-code or supply-chain reachability analysis;
- a complete SBOM/legal audit;
- superiority over Electron, Tauri, Wails, Dioxus or other frameworks;
- absence of undiscovered critical vulnerabilities.

## Q. Change versus prior public audit

This is the first report in KELD's canonical public audit registry, so there is no prior
public audit snapshot to score as improved/regressed/unchanged.

During the audit itself, stale hypotheses were actively refuted or narrowed rather than
carried forward when current source disagreed. Future audits should compare against this
dated snapshot and classify each material finding as fixed, improved, unchanged,
regressed, refuted, or superseded.

## R. Final alignment

KELD's audited implementation is aligned with its **pre-alpha evaluation** description:
there is enough real host, webview, Bun, IPC, lifecycle and security machinery to
evaluate and extend.

It is not yet aligned with the strongest form of its destination promise. The largest
distance remains in broad migration compatibility, renderer/native API completeness,
cross-platform authority confinement, profile isolation, packaging/signing/updating, and
full-product performance qualification.

The correct public conclusion for this revision is therefore neither “only a prototype
with no substance” nor “an Electron replacement already ready for production.” It is a
substantial pre-alpha framework foundation whose strongest claims should remain bounded
to the evidence above.

## Errata

No errata at publication.
