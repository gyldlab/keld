# Size, RSS, and installer scoreboard

Measured hello (and later app) artifacts against
[`docs/architecture/01-overview.md`](../architecture/01-overview.md) §5 budgets
and competitor hellos. Not the Electron API scoreboard
([`compat-scoreboard.md`](./compat-scoreboard.md)).

KEL-25 DoD until `keld-pack` and `bench/` exist. Append rows; do not invent
numbers, a fifth unique, a `bench/` crate, or CI that does not run.

## Byte lanes

| Lane | What to weigh | Keld Phase 2 hello | Native floor |
|---|---|---|---|
| **host** | Privileged native executable | `keld-host` Mach-O | Swift `.app` executable (SwiftUI+WK or AppKit+WK) |
| **runtime** | JS runtime in the installer | **not packed** (no Bun on hello) | none |
| **engine** | Renderer bytes in the *bundle* | 0 (system WebKit) | 0 (system WebKit) |
| **wrapping** | `.app` / `.dmg` / MSI | **N/A** (`keld-pack` is a `Format` enum) | SwiftUI 96K `.app` / 31,655 B UDZO; AppKit 88K `.app` / 29,774 B UDZO |
| **devtools CLI** | `keld` binary — not an app installer | `keld` | n/a |

`runtime: "none"` budget (≤ 6 MB) = host + wrapping vs Tauri / Native Swift.
`runtime: "bun"` budget (≤ 20 MB) must show Bun as its own lane. Spec 06 quotes
compressed Bun ~25–35 MB — report both if packing exceeds 20 MB.

## Spec budgets (architecture 01 §5)

Hello-world, M-series Mac / mid Windows laptop. **CI does not gate these today.**
Regressions > 5% fail the PR once `bench/` lands (KEL-39 parked `bench/` as YAGNI).

| Metric | Budget | Electron lead (KEL-11) | Phase 2 hello (`b93ebb6`, darwin/arm64) |
|---|---|---|---|
| Installer (runtime = bun) | ≤ 20 MB | 85–150 MB | N/A — Bun not packed |
| Installer (runtime = none) | ≤ 6 MB | — | N/A — no `.app` / DMG |
| Cold start → first paint | ≤ 300 ms | 1–3 s | **unmeasured** (no load-finished on v0) |
| Idle RSS, 1 window (sum of keld processes) | ≤ 90 MB | 150–300 MB | **72.6–77.8 MiB** host-only (under budget; no Bun; WebKit XPCs excluded) |
| kipc small-message p99 | ≤ 100 µs | ~ms-class | echo exists; not bench'd |
| kipc bulk (shm) | ≥ 1 GB/s | n/a | no shm |
| Update patch, 1-line JS | ≤ 50 KB | full installer | no updater |
| `keld dev` cold → window | ≤ 2 s | — | not measured here |

KEL-11 freeze (not same-protocol measurements): Electron hello 85–150 MB /
150–300 MB RSS / 1–3 s; Tauri 2.5–10 MB / 30–80 MB / 0.2–0.8 s. Re-measure
before any public claim.

## Fairness rules

| Rule | Detail |
|---|---|
| Release only | `just hello` is debug (~5.5M). Never put debug in a `vs` cell. |
| Split lanes | host / runtime / engine-in-bundle / wrapping — never blend. |
| Same engine class | Do not mix WKWebView / Chromium / Skia in one `vs` cell. |
| Host ≠ product | `keld-host --hello` does **not** spawn Bun; no `.app`/DMG. Hello-without-Bun is a host-lane diagnostic. |
| RSS lines | Main-process `ps` and engine helpers (`com.apple.WebKit.*` or Chromium GPU/utility) stay separate. |
| Same-protocol `vs` | Same Mac, same day, official Release packages. Citations do **not** fill `vs` cells. |
| Native floor | Swift AppKit/SwiftUI + WKWebView + same `HELLO_HTML`. Not AppKit TextKit / NSTextView. |

## Competitor landscape (cited, 2026-08-13)

Not `vs` columns. Citations + confidence. Native Swift rows are **measured**
this Mac (same day as Keld). Electron is the Chromium ceiling; Native Swift is
the WKWebView floor. Flutter is a different engine class (one-line caveat only).

| Stack | Lane | Disk / bytes | Idle RSS | Engine | Runtime | Conf. | Cite |
|---|---|---|---|---|---|---|---|
| **Keld** `b93ebb6` | **host** Mach-O | **1,010,464 B (987K)** | **72.6–77.8 MiB** (host; WebKit helpers not in figure) | system WKWebView | **none** | **high** | Measured rows. No `.app` / DMG. |
| Keld | host | 5.5M | — | same | none | high | **debug** `just hello` — not for `vs`. |
| **Electron** v43.4.0 | **runtime** zip | **116.5 MB** `electron-v43.4.0-darwin-arm64.zip` | **150–300 MB** typical | Chromium (in zip) | Node (in zip) | high (zip) / med (`.app`, RSS) | [v43.4.0](https://github.com/electron/electron/releases/tag/v43.4.0). Empty `.app` typically 150–250 MB unpacked. |
| **Tauri 2** | **installer** `.app` | **8.6 MiB** Hopp; Deno comparison **~2–10 MB** | **30–100 MB** survey | system WKWebView | none | med | [Hopp](https://www.gethopp.app/blog/tauri-vs-electron); [Deno comparison](https://docs.deno.com/runtime/desktop/comparison/). |
| **Wails** | vendor disk | **~15 MB** | Youngju **60–120 MB** | system webview | none (Go in binary) | med | Vendor [architecture](https://v3.wails.io/concepts/architecture/); Youngju [2026-05-16](https://www.youngju.dev/blog/culture/2026-05-16-cross-platform-desktop-apps-2026-tauri-2-electron-wails-neutralinojs-flutter-desktop-sciter-deep-dive.en). Vendor ~10 MB RSS likely omits WebKit. |
| **Neutralino** | official / survey | **~2 MB** / **~0.5 MB** compressed | Youngju **40–80 MB** | system webview | none | med | [neutralino.js.org](https://neutralino.js.org/); Youngju RSS. |
| **NW.js** | zip expected | **~110–200 MB** | — | Chromium | Node | low–med | Same Chromium class as Electron; not weighed this Mac. |
| **Electrobun** | installer vs `.app` | zstd **~14 MB**; uncompressed mac hello `.app` **~72.6 MB** | — | system webview | **Bun** (~60 MB of uncompressed `.app`) | med | [#373](https://github.com/blackboardsh/electrobun/issues/373); vendor zstd ~14 MB. |
| **Native Swift** SwiftUI + WK | host + wrapping | **96K** `du` / **92,740 B** file sum; exe **89,696 B**; DMG **31,655 B** | **101,168 KB (~98.8 MiB)** | WKWebView | none | **high** | Measured this Mac, `swiftc -O`. |
| **Native Swift** AppKit + WK | host + wrapping | **88K** `du` / **80,976 B** file sum; exe **77,936 B**; DMG **29,774 B** | **97,344 KB (~95.1 MiB)** | WKWebView | none | **high** | Closest floor to wry NSWindow+WKWebView. |
| Flutter macOS | — | ~25–50 MB survey | ~90–150 MB survey | **Skia — not a webview** | Dart | med | Different engine; do not put on the WKWebView floor. |

**Do not misread:**

| Trap | Why it lies |
|---|---|
| Bare Mach-O vs `.app` vs zip vs zstd/DMG | Different lanes. |
| Host-only RSS vs engine-inclusive | Keld 72.6–77.8 MiB is host process only. |
| Vendor RSS that omits WebKit | Wails ~10 MB is Go-side; Youngju 60–120 MB includes engine. |
| Compressed download vs unpacked `.app` | Electrobun ~14 MB zstd vs ~72.6 MB `.app`; Electron 116.5 MB zip vs 150–250 MB `.app`. |
| Tiny Swift `.app` vs future Keld installer | 88–96K because WebKit is system. Packed Keld adds Bun. |
| Swift exe vs Keld Mach-O vs Swift `.app` | Compare exe-to-exe (89,696 / 77,936 vs 1,010,464). |

## Measured rows (2026-08-13)

**Host:** Apple M4, macOS 26.5.1 (Build 25F80), Darwin 25.5.0, rustc 1.93.0.
**SHA:** `b93ebb6e0fb557b20ae312f155f3a33713212ccf` (`origin/main`).
**Build:** `cargo build --release -p keld-host -p keld-cli`.
**Engine:** system WKWebView (wry+tao). **Not Chromium.** `keld-host --hello` does **not** spawn Bun.
**First paint:** not instrumented. Do not treat RSS-ready as ≤ 300 ms.
**Native Swift:** Xcode 26.5 / Swift 6.3.2, `swiftc -O`, `arm64-macos14.0`, ad-hoc, no sandbox; artifacts `/tmp/keld-native-swift-hello`; same `HELLO_HTML` as `crates/keld-wv/src/hello/mod.rs`.

| Phase | Date | SHA | OS+arch | Stack | Artifact | Config | Bytes | Spec budget | vs Electron | vs Tauri | vs SwiftUI+WK | vs AppKit+WK | Notes | Method |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Phase 2 hello | 2026-08-13 | `b93ebb6e0fb557b20ae312f155f3a33713212ccf` | darwin/arm64 | keld | host | release | 1,010,464 | n/a (not an installer; runtimeless budget ≤ 6 MB) | — | — | 89,696 B exe (not vs 96K `.app`) | 77,936 B exe (not vs 88K `.app`) | Mach-O arm64; system WebKit; no `.app`; bundled dylibs **0**; no Bun | `cargo build --release -p keld-host`; `stat` byte size |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | host | debug | 5,793,368 | n/a | — | — | — | — | `just hello` is **debug**. Not for `vs` | `just hello` / debug `keld-host` |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | cli | release | 2,531,600 | n/a (devtools, not app installer) | — | — | n/a | n/a | `keld` binary | `cargo build --release -p keld-cli` |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | create hello source | — | 1,316 | n/a | — | — | n/a | n/a | File bytes; no `node_modules` | sum of embedded hello template |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | rss | release | 79,616 then 74,304 KiB (72.6–77.8 MiB) | ≤ 90 MB idle | — | — | 101,168 KB (~98.8 MiB); Keld **lower** | 97,344 KB (~95.1 MiB); Keld **lower** | **Under budget.** Host-only; no Bun; WebKit XPCs excluded | `keld-host --hello` release; `ps -o rss=` after paint |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | rss | debug | 73,184 then 70,752 KiB (~70–73 MiB) | ≤ 90 MB | — | — | — | — | Host-only; KEL-26 GUI pass | `just hello` / debug |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | dmg / .app | — | N/A | ≤ 6 MB runtimeless / ≤ 20 MB w/ Bun | — | — | 96K `.app` / 31,655 B UDZO | 88K `.app` / 29,774 B UDZO | `keld-pack` is a `Format` enum. Tiny Swift `.app` ≠ packed Keld | n/a |
| Phase 2 hello | 2026-08-13 | — | darwin/arm64 | **swiftui+wk** | .app | `swiftc -O` | 96K `du` / 92,740 B file sum | native floor | — | — | — | — | WKWebView + loadHTMLString | `/tmp/keld-native-swift-hello` |
| Phase 2 hello | 2026-08-13 | — | darwin/arm64 | **swiftui+wk** | host exe | `swiftc -O` | 89,696 | native floor | — | — | — | — | `Contents/MacOS` | same |
| Phase 2 hello | 2026-08-13 | — | darwin/arm64 | **swiftui+wk** | rss | `swiftc -O` | 101,168 KB (~98.8 MiB) | native floor | — | — | — | — | Main process; WebKit XPCs extra | same |
| Phase 2 hello | 2026-08-13 | — | darwin/arm64 | **swiftui+wk** | dmg | UDZO | 31,655 | native installer floor | — | — | — | — | UDZO of `.app` | same |
| Phase 2 hello | 2026-08-13 | — | darwin/arm64 | **appkit+wk** | .app | `swiftc -O` | 88K `du` / 80,976 B file sum | native floor | — | — | — | — | Closest to wry NSWindow+WKWebView | same |
| Phase 2 hello | 2026-08-13 | — | darwin/arm64 | **appkit+wk** | host exe | `swiftc -O` | 77,936 | native floor | — | — | — | — | `Contents/MacOS` | same |
| Phase 2 hello | 2026-08-13 | — | darwin/arm64 | **appkit+wk** | rss | `swiftc -O` | 97,344 KB (~95.1 MiB) | native floor | — | — | — | — | Main process; higher than Keld host-only | same |
| Phase 2 hello | 2026-08-13 | — | darwin/arm64 | **appkit+wk** | dmg | UDZO | 29,774 | native installer floor | — | — | — | — | UDZO of `.app` | same |

Windows / Linux Keld rows: **N/A** until KEL-27 / KEL-28 (slots return `KELD-WV-001`).
Notes-app rows: **later** (guard-on-IPC + host `fs` first).

### Still waiting

| Item | Why |
|---|---|
| Keld `.app` / DMG | `keld-pack` has no authoring code |
| Bun lane in installer | Not packed; this-Mac `bun` 1.3.14 = 63,096,576 B extracted; gzip-9 = 23,548,666; zstd-19 = 16,838,595 |
| Electron / Tauri `vs` cells | Need same-day official-scaffold Release packages on this Mac |
| First paint ≤ 300 ms | Not instrumented |
| kipc p99 / shm / update patch | No shm, no updater, no `bench/` |
| CI > 5% regression gate | Architecture 01 §5 once `bench/` lands (KEL-39) |

## Byte / RSS autopsy

Same SHA (`b93ebb6`). `nm` split used an unstripped rebuild; shipped host is
`strip = "symbols"`. `bloaty` / `cargo-bloat` were not installed.

**987K vs 78K is static Rust + tao/wry + URL, not Keld logic.** Own crates are
~2% of `__text`. Shipped disk is dominated by Bun, not the host Mach-O.

| | Keld release host | Swift AppKit exe | Ratio |
|---|---:|---:|---|
| File | 1,010,464 B | 77,936 B | ~13× |
| `__text` | 645,464 B | 1,816 B | ~355× |

| `__text` owner | Share | Notes |
|---|---:|---|
| Rust `std` / panic backtrace (`gimli`, `addr2line`, `rustc_demangle`) | ~51% | `panic = "abort"` on; default hook still pulls DWARF symbolizer |
| tao + wry + objc2 | ~29% | Full window toolkit |
| `url` / `idna` / `icu_*` | ~15% | wry navigation / `with_html` |
| **`keld_host` + `keld_core` + `keld_wv`** | **~2%** | Hello + error strings |

| Metric | Keld | Swift AppKit | Read as |
|---|---|---|---|
| Host `ps -o rss=` | ~76 MiB | ~89–97 MiB | Shared-library TEXT **counts**. Scoreboard “Keld lower.” |
| Host `phys_footprint` | ~27 MB | ~30 MB | Owned dirty. Gap is accounting, not a WebKit win. |
| WebKit XPCs (WebContent + GPU + Networking) | ~70 MiB | ~70 MiB | `ppid=1`; not in host RSS; not a keld process |

Architecture 01 §5 “sum of keld processes” can pass while engine-inclusive idle
does not. Empty `bun` RSS floor ~22 MiB — host + Bun may miss ≤90 MB before app
JS heap; XPCs still extra.

Real disk cuts (do not destroy a unique): strip panic backtrace stack; stop
compiling wry surfaces hello never uses; `lto = "fat"` is marginal. An objc2
rewrite does **not** reach 78K — Rust `std` stays statically linked
(`decisions.md` §2, §11).

## Win conditions

Product to beat: Electron’s *architecture* (privileged JS in-process + shipped
Chromium). Not Swift’s 78 KB exe. Not Flutter Skia. Not Neutralino’s empty shell.
Four uniques only — no fifth.

| # | Lane | Score | Why |
|---|---|---|---|
| 1 | Host Mach-O vs Swift AppKit+WK (77,936 B / 88K `.app`) | **cannot win honestly** | Swift dylibs OS frameworks; Rust statically links libstd. 987K vs 78K is that fact. |
| 2 | Idle RSS vs Swift ~95 MiB / Electron 150–300 / Tauri survey | **can win with work** | Host-only 72.6–77.8 MiB under Swift main and ≤90 MB — not the product (no Bun, no XPCs). ~12 MB headroom for Bun. |
| 3 | Installer no-Bun (≤6 MB) vs Tauri / Neutralino | **can win with work** vs Tauri | Host already 987K. Pack `.app`/DMG → Tauri’s 2.5–10 MB band. **Cannot** claim smallest shell vs Swift 88K / Neutralino ~2 MB. |
| 4 | Installer **with Bun** (≤20 MB) vs Electrobun / Electron | **can win with work** vs Electron | gzip-9 Bun alone is over 20 MB; zstd-19 = ~16.1 MB (budget legal if artifact is zstd). Electrobun ~14 MB zstd is the compressed ceiling. |
| 5 | Cold start first paint (≤300 ms) | **can win with work** | Unmeasured. WK class can beat Electron 1–3 s. |
| 6 | Chromium-complete web platform **and** beat Electron on disk | **cannot win honestly** | Chromium *is* the disk. Spec 01 §6: no CEF-by-default. |
| 7 | Default-deny / crash isolation / zero ambient OS authority | **can win with work** | The product bet (architecture 01 §1). Specified, not enforced on hello. |
| 8 | Flutter Skia hello | **cannot win honestly** | Different engine. Spec 01 §6: not a UI toolkit. |

Zero lanes are **already winning** as a public claim. Host-only RSS vs Swift is a
labeled diagnostic, not a launch tweet.

**Spec tension:** architecture 01 §5 ≤20 MB with Bun vs architecture 06 §1
compressed Bun ~25–35 MB — the 20 MB budget is a **compressor choice** (zstd vs
UDZO), not a host-trim project.

**Refuse:** CEF-by-default; drop Bun to win installer; count hello-without-Bun as
the product; mix WK/Chromium/Skia in one `vs`; debug 5.5M in `vs`; time-to-RSS as
first paint; fill `vs` from blog citations; fake `bench/` CI.

## Related

- Fixture home (public): [gyldlab/keld-benches](https://github.com/gyldlab/keld-benches) — hello / installer / RSS apps for Swift and competitors; not in this monorepo
- Budgets: [`docs/architecture/01-overview.md`](../architecture/01-overview.md) §5
- Packing / Bun size: [`docs/architecture/06-runtime-and-tooling.md`](../architecture/06-runtime-and-tooling.md) §1, §3
- Four uniques / parked `bench/`: [`decisions.md`](./decisions.md) §1, §11
- Electron API scores: [`compat-scoreboard.md`](./compat-scoreboard.md)
- Linear: KEL-11, KEL-25, KEL-26, KEL-39
