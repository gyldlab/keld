# Size, RSS, and installer scoreboard

Measured hello (and later app) artifacts against
[`docs/architecture/01-overview.md`](../architecture/01-overview.md) §5 budgets
and competitor hellos. Not the Electron API scoreboard
([`compat-scoreboard.md`](./compat-scoreboard.md)).

KEL-25 DoD until `keld-pack` and `bench/` exist. Append rows; do not invent
numbers, a fifth unique, a `bench/` crate, or CI that does not run.
Competitor apps live in [`gyldlab/keld-benches`](https://github.com/gyldlab/keld-benches),
not this monorepo.

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
| Cold start → first paint | ≤ 300 ms | 1–3 s | macOS **unmeasured**; Windows **472 ms** (2026-08-15 session, direct-COM backend) — ~1.6x over, floor is Chromium boot in controller creation |
| Idle RSS, 1 window (sum of keld processes) | ≤ 90 MB | 150–300 MB | **72.6–77.8 MiB** host-only (under budget; no Bun; WebKit XPCs excluded) |
| kipc small-message p99 | ≤ 100 µs | ~ms-class | echo exists; not bench'd |
| kipc bulk (shm) | ≥ 1 GB/s | n/a | no shm |
| Update patch, 1-line JS | ≤ 50 KB | full installer | no updater |
| `keld dev` cold → window | ≤ 2 s | — | not measured here |

KEL-11 freeze (survey, not this Mac): Electron 85–150 MB / 150–300 MB RSS / 1–3 s;
Tauri 2.5–10 MB / 30–80 MB / 0.2–0.8 s. This-Mac Release hellos (2026-08-14)
fill `vs` Electron/Tauri cells and the landscape table. Do not mix Chromium vs WK.

## Fairness rules

| Rule | Detail |
|---|---|
| Release only | `just hello` is debug (~5.5M). Never put debug in a `vs` cell. |
| Split lanes | host / runtime / engine-in-bundle / wrapping — never blend. |
| Same engine class | Do not mix WKWebView / Chromium / Skia in one `vs` cell. |
| Host ≠ product | `keld-host --hello` does **not** spawn Bun; no `.app`/DMG. Hello-without-Bun is a host-lane diagnostic. |
| RSS lines | Main-process `ps` and engine helpers (`com.apple.WebKit.*` or Chromium GPU/utility) stay separate. |
| Same-protocol `vs` | Same Mac, official Release packages. Blog/Hopp/Youngju citations do **not** fill `vs` cells. |
| Native floor | Swift AppKit/SwiftUI + WKWebView + same `HELLO_HTML`. Not AppKit TextKit / NSTextView. |
| Electrobun RSS | Launcher-only sample is **incomplete** (Bun child unmatched; WebKit GPU/WebContent not up). Not a vs-WK claim. |

## Competitor landscape (this Mac)

**Machine:** Apple M4, macOS 26.5.1 (25F80), darwin/arm64. RSS = main process
unless noted. Keld + Native Swift: 2026-08-13 (`b93ebb6`). Competitor hellos:
2026-08-14, fixtures
[`gyldlab/keld-benches@0308d55`](https://github.com/gyldlab/keld-benches/commit/0308d55f628797067985247000b30b43ea00cba1).
Electron/NW.js = Chromium ceiling; Native Swift = WK floor. Flutter = Skia
(one-line caveat). Electrobun RSS is **not** a vs-WK number.

| Stack | Lane | Disk / bytes | Idle RSS | Engine | Runtime | Conf. | Cite |
|---|---|---|---|---|---|---|---|
| **Keld** `b93ebb6` | **host** Mach-O | **1,010,464 B (987K)** | **72.6–77.8 MiB** (host; WebKit helpers not in figure) | system WKWebView | **none** | **high** | Measured rows. No `.app` / DMG. |
| Keld | host | 5.5M | — | same | none | high | **debug** `just hello` — not for `vs`. |
| **Electron** 43.4.0 | zip + wrapping | zip **122,121,746 B**; `.app` **288,448,512 B** | **138,064 KB (~134.8 MiB)** main; GPU 79,760 + util 40,384 + renderer 84,640 KB | Chromium (in zip) | Node (in zip) | **high** | [`macos/electron/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electron/hello) @ `0308d55`. Forge did not write `out/`; weighed official zip via ditto. Not `electron .`. |
| **Tauri** 2.11.5 | wrapping | `.app` **8,265,728 B**; exe **8,153,472 B**; DMG **2,910,772 B** | **102,896 KB (~100.5 MiB)**; WebKit XPCs 80,560 KB | system WKWebView | none | **high** | [`macos/tauri/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/tauri/hello) @ `0308d55`. vanilla JS `tauri build`. |
| **Wails** v3.0.0-beta.8 | wrapping | `.app` **9,818,112 B**; exe **8,271,424 B**; UDZO **5,320,599 B** | **95,648 KB (~93.4 MiB)**; WebKit XPCs 73,760 KB | system WKWebView | none (Go in binary) | **high** | [`macos/wails/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/wails/hello) @ `0308d55`. |
| **Neutralino** 6.9.0 | wrapping | wrapped `.app` **2,953,216 B**; UDZO **1,322,015 B** | **86,336 KB (~84.3 MiB)**; WebKit XPCs 81,200 KB | system WKWebView | none | **high** | [`macos/neutralino/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/neutralino/hello) @ `0308d55`. `neu create` failed; official minimal clone. `--macos-bundle` only renames the Mach-O. |
| **NW.js** 0.114.1 | zip + wrapping | zip **169,495,010 B**; `.app` **410,271,744 B** | **205,776 KB (~201 MiB)**; Chromium helpers 386,880 KB | Chromium | Node | **high** | [`macos/nwjs/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/nwjs/hello) @ `0308d55`. Runtime zip not in git. |
| **Electrobun** 1.18.1 | wrapping vs runtime | wrapped `.app` **18,710,528 B**; zstd **18,514,771 B**; extracted **42,360,832 B** (Bun **32,287,232 B**) | launcher **72,032 KB** — **incomplete** | system webview | **Bun** | high (disk) / **low (RSS)** | [`macos/electrobun/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electrobun/hello) @ `0308d55`. `bundleCEF: false`. Not a vs-WK RSS claim. |
| **Native Swift** SwiftUI + WK | host + wrapping | **96K** `du` / **92,740 B** file sum; exe **89,696 B**; DMG **31,655 B** | **101,168 KB (~98.8 MiB)** | WKWebView | none | **high** | Measured this Mac, `swiftc -O`. |
| **Native Swift** AppKit + WK | host + wrapping | **88K** `du` / **80,976 B** file sum; exe **77,936 B**; DMG **29,774 B** | **97,344 KB (~95.1 MiB)** | WKWebView | none | **high** | Closest floor to wry NSWindow+WKWebView. |
| Flutter macOS | — | unmeasured (survey) | unmeasured (survey) | **Skia — not a webview** | Dart | low | Different engine; do not put on the WKWebView floor. No same-protocol cite for this Mac. |

**Do not misread:**

| Trap | Why it lies |
|---|---|
| Bare Mach-O vs `.app` vs zip vs zstd/DMG | Different lanes. |
| Host-only RSS vs engine-inclusive | Keld 72.6–77.8 MiB is host process only. |
| Vendor RSS that omits WebKit | Wails vendor ~10 MB is Go-side; this-Mac Wails main is 95,648 KB plus 73,760 KB XPCs. |
| Compressed download vs unpacked `.app` | Electrobun zstd 18,514,771 vs wrapped 18,710,528 vs extracted 42,360,832; Electron zip 122,121,746 vs `.app` 288,448,512. |
| Tiny Swift `.app` vs future Keld installer | 88–96K because WebKit is system. Packed Keld adds Bun. |
| Swift exe vs Keld Mach-O vs Swift `.app` | Compare exe-to-exe (89,696 / 77,936 vs 1,010,464). |
| Electrobun launcher RSS vs Tauri/Wails | 72,032 KB is incomplete; Bun child and WebKit GPU/WebContent were not up. |
| Neutralino `--macos-bundle` vs wrapped `.app` | Official dist name is a renamed Mach-O. Weighed wrapped `.app` 2,953,216 B. |

## Measured rows (2026-08-13)

**Host:** Apple M4, macOS 26.5.1 (Build 25F80), Darwin 25.5.0, rustc 1.93.0.
**SHA:** `b93ebb6e0fb557b20ae312f155f3a33713212ccf` (`origin/main` at measure).
**Build:** `cargo build --release -p keld-host -p keld-cli`.
**Engine:** system WKWebView (wry+tao). **Not Chromium.** `keld-host --hello` does **not** spawn Bun.
**First paint:** not instrumented. Do not treat RSS-ready as ≤ 300 ms.
**Native Swift:** Xcode 26.5 / Swift 6.3.2, `swiftc -O`, `arm64-macos14.0`, ad-hoc, no sandbox; artifacts `/tmp/keld-native-swift-hello`; same `HELLO_HTML` as `crates/keld-wv/src/hello/mod.rs`.
**Competitor `vs`:** 2026-08-14, same Mac; fixtures `0308d55f628797067985247000b30b43ea00cba1`.

| Phase | Date | SHA | OS+arch | Stack | Artifact | Config | Bytes | Spec budget | vs Electron | vs Tauri | vs SwiftUI+WK | vs AppKit+WK | Notes | Method |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Phase 2 hello | 2026-08-13 | `b93ebb6e0fb557b20ae312f155f3a33713212ccf` | darwin/arm64 | keld | host | release | 1,010,464 | n/a (not an installer; runtimeless budget ≤ 6 MB) | zip 122,121,746 B; `.app` 288,448,512 B (Chromium+Node — **not host lane**) | exe 8,153,472 B; `.app` 8,265,728 B (wrapping — **not vs host Mach-O**) | 89,696 B exe (not vs 96K `.app`) | 77,936 B exe (not vs 88K `.app`) | Mach-O arm64; system WebKit; no `.app`; bundled dylibs **0**; no Bun | `cargo build --release -p keld-host`; `stat` byte size |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | host | debug | 5,793,368 | n/a | — | — | — | — | `just hello` is **debug**. Not for `vs` | `just hello` / debug `keld-host` |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | cli | release | 2,531,600 | n/a (devtools, not app installer) | — | — | n/a | n/a | `keld` binary | `cargo build --release -p keld-cli` |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | create hello source | — | 1,316 | n/a | — | — | n/a | n/a | File bytes; no `node_modules` | sum of embedded hello template |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | rss | release | 79,616 then 74,304 KiB (72.6–77.8 MiB) | ≤ 90 MB idle | 138,064 KB (~134.8 MiB) Chromium main — **not same engine** | 102,896 KB (~100.5 MiB) WK main; Keld host-only **lower** (no Bun; XPCs excluded) | 101,168 KB (~98.8 MiB); Keld **lower** | 97,344 KB (~95.1 MiB); Keld **lower** | **Under budget.** Host-only; no Bun; WebKit XPCs excluded | `keld-host --hello` release; `ps -o rss=` after paint |
| KEL-64 app oracle | 2026-08-16 | [`9e7c83d1`](https://github.com/gyldlab/keld-benches/commit/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b) | darwin/arm64 | **keld adapter + WKWebView** | `.app` | release; Keld source `dc5dea2` | **1,043,880 B** file sum / 1,024K `du` | n/a (oracle artifact; no Bun) | — | — | — | — | **11/11 publish-eligible samples.** Coalition RSS median **199,568 KiB** (min 196,080; max 200,368), including host + WebKit helpers; not comparable to the host-only row above. | KEL-64 Swift oracle `--publish`; exact source/recipe commits and byte-bound harness |
| KEL-64 app oracle | 2026-08-16 | [`9e7c83d1`](https://github.com/gyldlab/keld-benches/commit/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b) | darwin/arm64 | **tauri 2.11.5 + WKWebView** | `.app` | release; Tauri source/recipe `9e7c83d1` | **8,292,272 B** file sum / 8,108K `du` | n/a (oracle artifact; no Bun) | — | — | — | — | **11/11 publish-eligible samples.** Coalition RSS median **204,624 KiB** (min 203,744; max 207,840), including host + WebKit helpers; Tauri-only run, not the aborted mixed handoff run. Double-rAF proxy median **378.758 ms**, p90 **749.492 ms**. | KEL-64 Swift oracle `--publish`; exact source/recipe commits and byte-bound harness |
| KEL-64 interleaved oracle | 2026-08-16 | [`9e7c83d1`](https://github.com/gyldlab/keld-benches/commit/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b) | darwin/arm64 | **keld adapter + WKWebView** | `.app` | release; Keld source `5ba4672` | **1,043,880 B** file sum / 1,024K `du` | n/a | — | — | — | — | **11/11 in one 22-arm publish-eligible interleaving.** Coalition RSS median **199,968 KiB** (min 196,496; max 200,480), including host + WebKit helpers. Double-rAF proxy median **342.911 ms**, p90 **393.103 ms**. | KEL-64 Swift oracle `--publish`; `/tmp/keld-vs-tauri-20260816d.json`; exact source/recipe commits |
| KEL-64 interleaved oracle | 2026-08-16 | [`9e7c83d1`](https://github.com/gyldlab/keld-benches/commit/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b) | darwin/arm64 | **tauri 2.11.5 + WKWebView** | `.app` | release; Tauri source/recipe `9e7c83d1` | **8,292,272 B** file sum / 8,108K `du` | n/a | — | — | — | — | **11/11 in one 22-arm publish-eligible interleaving.** Coalition RSS median **205,024 KiB** (min 199,264; max 206,032), including host + WebKit helpers. Double-rAF proxy median **346.034 ms**, p90 **353.070 ms**. | KEL-64 Swift oracle `--publish`; `/tmp/keld-vs-tauri-20260816d.json`; exact source/recipe commits |

### KEL-64 competitor extension (same oracle, 2026-08-16)

These runs use the same canonical `hello.html`, loopback beacon, 11-arm
foreground session, exact-anchor restoration, and coalition RSS accounting. They
are intentionally kept separate from the publish-eligible Keld/Tauri rows until
competitor fixture provenance is bound into the publication schema.

| Framework | Samples | Double-rAF proxy | Coalition RSS | Result | Evidence |
|---|---:|---:|---:|---|---|
| Wails v3.0.0-beta.8 | **11/11** | median **340.984 ms**, p90 **353.263 ms** | median **206,976 KiB**, p90 **207,776 KiB** | Diagnostic run complete; foreground/session proof and cleanup passed. Publication remained false only for repository/harness/fixture provenance reasons. | `/tmp/wails-kel64-oracle-20260816.json`; fixture recipe [`e40b5c7`](https://github.com/gyldlab/keld-benches/commit/e40b5c7) |
| NW.js 0.114.1 | **1/11** | not reportable | not reportable | Fail-closed: round 2 lost foreground to a foreign process before beacon; no 11-sample metric. | `/tmp/nwjs-kel64-oracle-20260816.json`; fixture recipe [`e40b5c7`](https://github.com/gyldlab/keld-benches/commit/e40b5c7) |
| Neutralino 6.9.0 | **0/11** | not reportable | not reportable | Fail-closed: canonical HTML served, but beacon was rejected as hidden/unfocused and the sample timed out. | `/tmp/neutralino-kel64-oracle-20260816.json`; fixture recipe [`e40b5c7`](https://github.com/gyldlab/keld-benches/commit/e40b5c7) |
| Electrobun 1.18.1 | **0/11** | not reportable | not reportable | Fail-closed: HTML served, but no valid focused/visible beacon; cleanup drained the app. | `/tmp/keld-electrobun-kel64-*.json`; fixture recipe [`e40b5c7`](https://github.com/gyldlab/keld-benches/commit/e40b5c7) |

The failed rows are not “slow scores.” They are proof that this Mac/fixture
launch path did not satisfy the measurement contract, so publishing a number
would be misleading. Electron 43.4.0 likewise remained launch-ownership
unresolved (`foreground_target_generation_unavailable`) and has no metric row.
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | rss | debug | 73,184 then 70,752 KiB (~70–73 MiB) | ≤ 90 MB | — | — | — | — | Host-only; KEL-26 GUI pass | `just hello` / debug |
| Phase 2 hello | 2026-08-13 | `b93ebb6` | darwin/arm64 | keld | dmg / .app | — | N/A | ≤ 6 MB runtimeless / ≤ 20 MB w/ Bun | zip 122,121,746 B; `.app` 288,448,512 B; Keld wrapping N/A | `.app` 8,265,728 B; DMG 2,910,772 B; Keld wrapping N/A | 96K `.app` / 31,655 B UDZO | 88K `.app` / 29,774 B UDZO | `keld-pack` is a `Format` enum. Tiny Swift `.app` ≠ packed Keld | n/a |
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
| Electrobun complete RSS | Launcher 72,032 KB only; Bun child + WebKit GPU/WebContent not enumerated |
| First paint ≤ 300 ms | **Windows: 472 ms (2026-08-15 controlled session), ~1.6x over budget** — floor is Chromium boot inside controller creation (see the direct-COM A/B section). macOS/Linux: still not instrumented. |
| kipc p99 / shm / update patch | No shm, no updater, no `bench/` |
| CI > 5% regression gate | Architecture 01 §5 once `bench/` lands (KEL-39) |
| Windows / Linux competitor hellos | keld-benches stubs; not this machine |

## Byte / RSS autopsy

Same SHA (`b93ebb6`). `nm` split used an unstripped rebuild; shipped host is
`strip = "symbols"`. `bloaty` / `cargo-bloat` were not installed.

**987K vs 78K is static Rust + tao/wry + URL, not Keld logic.** Own crates are
~2% of `__text`. A future Bun-packed installer may be dominated by Bun; no Keld
installer is measured yet.

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
| 2 | Idle RSS vs Swift ~95 MiB / Tauri 102,896 KB / Wails 95,648 KB / Neutralino 86,336 KB (WK mains); Electron 138,064 KB Chromium main | **can win with work** | Host-only 72.6–77.8 MiB under those WK mains and ≤90 MB — not the product (no Bun, no XPCs). **Not** a vs-Electron claim. Electrobun 72,032 KB launcher is incomplete. Insufficient headroom vs the reported ~22 MiB Bun floor; measure host+Bun before claiming this lane can win. |
| 3 | Installer no-Bun (≤6 MB) vs Tauri / Neutralino | **can win with work** vs Tauri | Host already 987K. Pack `.app`/DMG vs this-Mac Tauri `.app` 8,265,728 / DMG 2,910,772. **Cannot** claim smallest shell vs Swift 88K / Neutralino wrapped `.app` 2,953,216. |
| 4 | Installer **with Bun** (≤20 MB) vs Electrobun / Electron | **can win with work** vs Electron | gzip-9 Bun alone is over 20 MB; zstd-19 = 16,838,595 for Bun alone — full installer size is unmeasured. This-Mac Electrobun zstd 18,514,771 (extracted 42,360,832; bundled Bun 32,287,232) is the compressed Bun-class ceiling to beat once packed. Electron zip 122,121,746 / `.app` 288,448,512. |
| 5 | Cold start first paint (≤300 ms) | **can win with work** — **fastest WebView2 arm, budget still missed** | Latest controlled session (Windows, 2026-08-15): Keld **472 ms** ahead of Tauri 483/506 in both same-session runs; Electron 278 leads absolutes. Budget missed ~1.6x; the floor is Chromium boot inside `CreateCoreWebView2Controller` (Microsoft-confirmed not app-reducible). macOS still uninstrumented. See "Windows first paint on the direct-COM backend". |
| 6 | Chromium-complete web platform **and** beat Electron on disk | **cannot win honestly** | Chromium *is* the disk (this-Mac Electron `.app` 288,448,512; NW.js `.app` 410,271,744). Spec 01 §6: no CEF-by-default. |
| 7 | Default-deny / crash isolation / zero ambient OS authority | **can win with work** | The product bet (architecture 01 §1). Specified, not enforced on hello. |
| 8 | Flutter Skia hello | **cannot win honestly** | Different engine. Spec 01 §6: not a UI toolkit. |

Zero lanes are **already winning** as a public claim. Host-only RSS vs Swift/Tauri
is a labeled diagnostic, not a launch tweet.

**Spec tension:** architecture 01 §5 ≤20 MB with Bun vs architecture 06 §1
compressed Bun ~25–35 MB — the 20 MB budget is a **compressor choice** (zstd vs
UDZO), not a host-trim project.

**Refuse:** CEF-by-default; drop Bun to win installer; count hello-without-Bun as
the product; mix WK/Chromium/Skia in one `vs`; debug 5.5M in `vs`; time-to-RSS or
time-to-titled-`HWND` as first paint; fill `vs` from blog citations; treat
Electrobun launcher RSS as vs-WK; fake `bench/` CI.

## Related

Fixtures are public and OS-first (`{macos|windows|linux}/<framework>/...`).
Cite the path **and** an immutable SHA — not only `main`. Not in this monorepo.

- Competitor hellos @ [`0308d55`](https://github.com/gyldlab/keld-benches/commit/0308d55f628797067985247000b30b43ea00cba1) ([MEASUREMENTS.md](https://github.com/gyldlab/keld-benches/blob/0308d55f628797067985247000b30b43ea00cba1/MEASUREMENTS.md)): [`macos/electron/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electron/hello), [`macos/tauri/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/tauri/hello), [`macos/neutralino/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/neutralino/hello), [`macos/wails/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/wails/hello), [`macos/nwjs/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/nwjs/hello), [`macos/electrobun/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electrobun/hello)
- KEL-64 macOS oracle + recipes @ [`9e7c83d`](https://github.com/gyldlab/keld-benches/commit/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b): [`macos/harness`](https://github.com/gyldlab/keld-benches/tree/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b/macos/harness), [`macos/keld/hello`](https://github.com/gyldlab/keld-benches/tree/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b/macos/keld/hello), [`macos/tauri/hello`](https://github.com/gyldlab/keld-benches/tree/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b/macos/tauri/hello)
- Native Swift @ [`646be79`](https://github.com/gyldlab/keld-benches/commit/646be7972706158ff744a1cb33547eddfe127445): [`macos/swift/appkit-wk`](https://github.com/gyldlab/keld-benches/tree/646be7972706158ff744a1cb33547eddfe127445/macos/swift/appkit-wk), [`macos/swift/swiftui-wk`](https://github.com/gyldlab/keld-benches/tree/646be7972706158ff744a1cb33547eddfe127445/macos/swift/swiftui-wk)
- Budgets: [`docs/architecture/01-overview.md`](../architecture/01-overview.md) §5
- Packing / Bun size: [`docs/architecture/06-runtime-and-tooling.md`](../architecture/06-runtime-and-tooling.md) §1, §3
- Four uniques / parked `bench/`: [`decisions.md`](./decisions.md) §1, §11
- Electron API scores: [`compat-scoreboard.md`](./compat-scoreboard.md)
- Linear: KEL-11, KEL-25, KEL-26, KEL-39


## Windows measured rows (2026-08-13)

**Machine:** Windows 11 Home Single Language 10.0.26200, x64.
**Engine:** WebView2 Evergreen **151.0.4129.78** (system webview on Windows is
Chromium-derived — the macOS WK-vs-Chromium lane split does **not** transfer).
**Build:** `cargo build --release -p keld-host -p keld-cli`, rustc
1.93.0-x86_64-pc-windows-msvc.
**SHA:** `44d23b2` (`agent/kel-27-window-windows-via-webview2`) — the immutable
commit these binaries were built from; the branch name alone would not stay
reproducible once it advances.
**wry:** 0.56.1 on both platforms. Keld's Windows arm was first measured on
0.55.1 and **re-measured** after KEL-59 bumped macOS to 0.56.1, because pinning
Windows to 0.55.1 would have reintroduced its camera/mic auto-grant. The bump
cut Keld startup from 657 ms to ~205 ms — the older figure is superseded.
**Competitor fixtures:**
[`gyldlab/keld-benches@f54a3c4`](https://github.com/gyldlab/keld-benches/commit/f54a3c406f861d9f55d2c1518fde89f75e817bf5)
under `windows/<framework>/hello`. Release builds, median of 3.
**Session drift:** Keld and Tauri were re-measured together after the wry bump; Electron, Wails, Neutralino and NW.js startup figures are from the earlier session. Absolute `window-visible` numbers drift with cache warmth (Tauri moved 61 -> 31 ms on identical bits), so **compare ratios within a session**, not absolutes across them.
**RSS method:** `WorkingSet64` 4 s after the window appears; helpers are the
recursive descendant tree of our own PID only (a global `msedgewebview2` sweep
counts 6 unrelated WebView2 processes this machine idles with). Main and helper
RSS stay separate, as in the macOS rows. `window-visible` = first titled `HWND`;
**not** first paint, not comparable to the macOS rows, and — see
[Time to first paint](#time-to-first-paint-2026-08-14-median-of-5) — **not
comparable across frameworks either**. Kept for continuity only.

| Stack | Version | Binary | Installer | window-visible (weak) | Main RSS | Helper RSS | Procs | Fixture |
|---|---|---|---|---|---|---|---|---|
| **Keld** `keld-host --hello` | `44d23b2` | **625,152 B** | none (`keld-pack` is a `Format` enum) | ~205 ms | **21,880 KB** | 333,416 KB | 7 | this repo |
| Keld `keld.exe` CLI | same | 2,445,312 B | n/a | — | — | — | — | this repo |
| Tauri | 2.11.5 | 8,634,880 B | MSI 2,846,720 B; NSIS 1,828,010 B | **~31 ms** | 26,700 KB | 337,732 KB | 7 | [`windows/tauri/hello`](https://github.com/gyldlab/keld-benches/tree/f54a3c406f861d9f55d2c1518fde89f75e817bf5/windows/tauri/hello) |
| Neutralino | 6.9.0 | 2,490,880 B | zip 8,291,997 B | 566 ms | 26,548 KB | 351,844 KB | 7 | [`windows/neutralino/hello`](https://github.com/gyldlab/keld-benches/tree/f54a3c406f861d9f55d2c1518fde89f75e817bf5/windows/neutralino/hello) |
| Wails | v3.0.0-beta.8 | 10,295,296 B | none from `wails3 build` | 766 ms | 30,264 KB | 344,884 KB | 7 | [`windows/wails/hello`](https://github.com/gyldlab/keld-benches/tree/f54a3c406f861d9f55d2c1518fde89f75e817bf5/windows/wails/hello) |
| Electron | 43.4.0 | forge `package` dir | not made | 185 ms | 89,500 KB | **215,536 KB** | **4** | [`windows/electron/hello`](https://github.com/gyldlab/keld-benches/tree/f54a3c406f861d9f55d2c1518fde89f75e817bf5/windows/electron/hello) |
| NW.js | 0.114.1 | runtime zip 209,290,666 B | unpacked 552,990,288 B | 926 ms | 143,456 KB | 248,940 KB | 6 | [`windows/nwjs/hello`](https://github.com/gyldlab/keld-benches/tree/f54a3c406f861d9f55d2c1518fde89f75e817bf5/windows/nwjs/hello) |
| Electrobun | 1.18.1 | Setup.exe 423,936 B | `.tar.zst` 33,164,123 B | **never opened** | 9,396 KB | 553,416 KB | 8 | [`windows/electrobun/hello`](https://github.com/gyldlab/keld-benches/tree/f54a3c406f861d9f55d2c1518fde89f75e817bf5/windows/electrobun/hello) |

### Time to first paint (2026-08-14, median of 5)

`window-visible` above is a **weak** metric. Frameworks differ in *when* they
present the window relative to webview construction, so a titled `HWND` can
appear before, during, or after the engine has anything to show — the column
compares presentation policy, not speed, and is **not comparable across
frameworks**. It is also not the metric
[`docs/architecture/01-overview.md`](../architecture/01-overview.md) §5 budgets.
That metric is **cold start → first paint ≤ 300 ms**, and this is it.

**Instrumentation — identical for every arm.** Every arm serves byte-identical
hello HTML (M-01). The page fires an image beacon —
`new Image().src = "http://127.0.0.1:45877/painted"` — from inside a double
`requestAnimationFrame`, i.e. after the first frame has been composited. A
single local `HttpListener` timestamps arrival, so **every arm shares one clock**
and none gets privileged in-process instrumentation the others lack.

**Image beacon, not `fetch()`.** The hello page runs on an opaque origin (wry
`with_html` / WebView2 `NavigateToString`), so `fetch()` is CORS-restricted;
`<img>` is not.

**`document.title` does not work — do not retry it.** Setting the document title
and watching for the native window caption is a dead end in an embedded webview:
the native window title is owned by the framework, not the document. That attempt
failed on **every** arm.

The beacon HTML was injected for the measurement session only and reverted
afterwards. It is **not** in product code, and by construction no committed SHA
reproduces the instrumented binaries.

| Stack | first paint (budgeted metric) | titled `HWND` (weak) | vs ≤ 300 ms budget |
|---|---|---|---|
| **Keld** `keld-host --hello` | **906 ms** | 433 ms | **over** — 3.0x |
| Tauri 2.11.5 | **504 ms** | 32 ms | **over** — 1.7x |
| Electron 43.4.0 | **not measured** | 125 ms | — |

Raw first-paint runs (ms): Keld 906 / 943 / 867 / 857 / 977 · Tauri 504 / 568 /
464 / 500 / 510. Full method and per-arm notes:
[`keld-benches@f0d042d` MEASUREMENTS.md](https://github.com/gyldlab/keld-benches/blob/f0d042dea36f99b448a01be03b679a07eb9e4c80/MEASUREMENTS.md).

**Both arms miss the budget.** Keld 906 ms is 3.0x over ≤ 300 ms; Tauri 504 ms is
1.7x over. Tauri also failing is not a defence — the budget is not graded on a
curve.

**The Electron first paint is missing, not fast.** `electron-forge package` had
already baked `out/` before the fixture HTML was edited, so the packaged app
served a stale copy of the page without the beacon. Reproducing this row needs a
repackage.

Titled-`HWND` medians are not stable across sessions either: Keld's moved
205 -> 433 ms between the 2026-08-13 and 2026-08-14 sessions while Tauri's held
(31 -> 32 ms). Run count differs (3 vs 5) and the instrumented tree carries the
beacon, so read that as session drift rather than a regression — and as one more
reason not to build a claim on that column.

### What these rows do and do not support

| Claim | Supported? |
|---|---|
| Keld has the smallest binary on Windows | **Yes.** 625,152 B is 13.8x under Tauri's exe, 16.5x under Wails'. |
| Keld has the lowest main-process RSS on Windows | **Yes** — 21,880 KB, lowest of every arm that opened a window, and well under the ≤ 90 MB idle budget. |
| Keld starts faster than Tauri | **No.** On the budgeted metric — cold start → first paint — Tauri is **504 ms against Keld's 906 ms: 1.8x against us** (2026-08-14, median of 5). The earlier entry here said 6.6x; that figure came from titled-`HWND` times and was **inflated by a metric artifact** — that column times when a framework chooses to present its window, not when either renders. Fixing the metric shrinks the gap; it does **not** close it. Keld is still ~400 ms behind Tauri. Do not publish a startup claim. See KEL-62. |
| Keld meets the ≤ 300 ms first-paint budget | **No.** 906 ms — 3.0x over architecture 01 §5. Tauri misses it too (504 ms, 1.7x over); that is context, not an excuse. |
| Keld uses less total memory than Electron | **No.** Electron 305,036 KB total beats every WebView2 arm because it runs 4 processes to WebView2's 7. |
| Keld beats Tauri on total RSS | **Not meaningfully** — ~3%, inside run-to-run noise. The ~330 MB helper tier is engine-fixed and near-identical across all WebView2 arms. |
| Electrobun comparison | **No.** Windows `--env=stable` emitted a macOS-shaped bundle and no window opened; the sample is a launcher that never rendered. |
| Installer-to-installer vs Tauri MSI / NW.js zip | **No.** Keld has no installer; `keld-host --hello` is a host-lane diagnostic and does not spawn Bun. |


## Windows first paint, reproducible harness (2026-08-14)

Supersedes the 2026-08-14 ad-hoc first-paint figures above. Those came from a
throwaway fixed-port beacon that was reverted, so they were not reproducible.
Re-measured with the committed harness
[`windows/bench/Measure-FirstPaint.ps1`](https://github.com/gyldlab/keld-benches/blob/e3f65f4/windows/bench/Measure-FirstPaint.ps1)
at [`gyldlab/keld-benches@e3f65f4`](https://github.com/gyldlab/keld-benches/commit/e3f65f4);
raw samples (git SHAs, exe SHA-256, versions, exact command) in
`windows/bench/windows-first-paint.json`. Median of 5, same machine/session.

| Stack | first paint | main RSS | helper RSS | total RSS | procs |
|---|---|---|---|---|---|
| **Electron** 43.4.0 | **444 ms** | 87,088 KB | **217,284 KB** | **304,372 KB** | **4** |
| Tauri 2.11.5 | 688 ms | 24,584 KB | 336,852 KB | 361,436 KB | 7 |
| **Keld** `keld-host --hello` | **1,289 ms** | **19,860 KB** | 337,192 KB | 357,052 KB | 7 |

### What these rows support

| Claim | Supported? |
|---|---|
| Keld has the lowest main-process RSS on Windows | **Yes** — 19,860 KB, lowest of all three, under the ≤ 90 MB idle budget. |
| Keld has the smallest binary | **Yes** — unchanged; 625,152 B, 13.8x under Tauri. |
| Keld starts fast | **No. Keld is the slowest arm measured** — 1,289 ms, 1.87x Tauri on the *same* WebView2 engine and 2.9x Electron. Not an engine cost; it is Keld's own startup path. See KEL-62. |
| Keld meets the ≤ 300 ms cold-start-to-first-paint budget (arch 01 §5) | **No** — 4.3x over. Tauri (2.3x) and Electron (1.5x) also miss it, which is context, not an excuse. |
| Keld uses less total memory than Electron | **No** — Electron's 4-process tree beats both 7-process WebView2 arms. |
| Keld beats Tauri on total RSS | **Not meaningfully** — ~1%, inside noise. The ~337 MB helper tier is engine-fixed. |

Absolutes here run higher than the earlier ad-hoc pass (Keld 906 ms, Tauri
504 ms) on the same machine; the **ratio** is stable at ~1.8x. Compare within a
session, never across.


## Windows first paint after the controller-bounds fix (2026-08-14, median of 7)

Supersedes the table above. Two defects moved every number:

1. **Product (Keld):** wry sets WebView2 controller bounds only from its WM_SIZE
   hook, so Keld's never-resized webview stayed 0x0 and Chromium deferred the
   first composited frame ~640 ms. Fixed on `agent/kel-62-initial-controller-bounds`
   by sizing the controller once at attach.
2. **Harness (all arms):** PowerShell `Start-Process` costs 450-700 ms between
   the call and the child's `main()` (measured by a wall clock printed at child
   entry). Replaced with .NET `Process.Start` (~11-47 ms), and the spawn wall
   time is now recorded per sample.

Harness at [`gyldlab/keld-benches` `windows/bench/`](https://github.com/gyldlab/keld-benches/tree/main/windows/bench)
(raw samples `windows-first-paint.json`, spawn wall clock included per run).

| Stack | first paint | main RSS | helper RSS | procs |
|---|---|---|---|---|
| **Electron** 43.4.0 | **395 ms** | 92,140 KB | 226,908 KB | **4** |
| **Keld** `b935830` | **590 ms** | **23,084 KB** | 351,680 KB | 7 |
| Tauri 2.11.5 | 596 ms | 28,280 KB | 354,316 KB | 7 |

Raw (ms): Keld 759/627/527/573/558/555/590 · Tauri 912/585/596/565/614/584/589 ·
Electron 1165/395/321/333/410/317/372. First run of each arm is cold. 7/7 valid.

### Claims this changes

| Claim | Now |
|---|---|
| "Keld first-paints 1.87x slower than Tauri" | **Retired.** 590 vs 596 ms is a statistical tie on the identical engine. The published gap was the bounds defect plus harness spawn overhead, both fixed. |
| Keld starts fast | **Parity with Tauri, not leadership.** Electron still first-paints ~1.5x faster than both WebView2 arms. |
| ≤ 300 ms first-paint budget (arch 01 §5) | **Still missed** — Keld and Tauri ~2x over, Electron ~1.3x over. The remaining Keld cost is `WebViewBuilder::build` (~550 ms: WebView2 environment + controller creation), which is engine-inherent on this path. Tracked on KEL-62. |
| Keld has the lowest main-process RSS | **Yes** — unchanged (23,084 KB this session; absolutes drift with machine warmth, ordering does not). |

Absolutes in this table are lower than the previous one for every arm because the
spawn overhead left the measurement. Ratios between arms are the durable signal;
never compare absolutes across sessions.

## Windows first paint on the direct-COM backend (2026-08-15, controlled A/B, median of 7)

KEL-65 replaced wry with direct `webview2-com` calls on Windows. Instrumentation
had shown wry blocking the UI thread 96–109 ms injecting its unused `window.ipc`
bridge, predicting ~100 ms of first-paint win. **The controlled same-session A/B
refuted the prediction** — that time overlapped renderer boot and was never on
the paint critical path:

| Stack | Run A (direct COM `39be9cc`) | Run B (wry baseline `137633f`) |
|---|---|---|
| Electron 43.4.0 | 278 ms | 294 ms |
| **Keld** | **472 ms** | **467 ms** |
| Tauri 2.11.5 | 483 ms | 506 ms |

Both runs: [`windows/bench/`](https://github.com/gyldlab/keld-benches/tree/686d1ab8ed57fc3b96b1a828d20bcf07adfab86a/windows/bench)
@ `686d1ab` (raw samples `windows-first-paint-kel65-*.json`). Every arm sits
~110–120 ms below the 2026-08-14 session — session conditions, which is why
absolutes are never compared across sessions.

What the rewrite *did* measurably change:

| Metric | wry backend | direct COM |
|---|---|---|
| `keld-host.exe` (release) | 625,152 B | **484,864 B** (−24%) |
| Effective browser args | `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` | **none** (SmartScreen on, KEL-66) — live cmdline verified |
| SmartScreen cost | — | ON 472 ms vs OFF 466 ms: **free** (noise) |
| UI-thread block before navigate | 96–109 ms (bridge injection) | ~0 (matters for input responsiveness and future multi-webview creation, not paint) |

### Claims this changes

| Claim | Now |
|---|---|
| "Dropping wry's bridge injection speeds first paint ~100 ms" | **Refuted by the A/B.** The blocking wait overlapped renderer boot. First paint is engine-floor-bound: `CreateCoreWebView2Controller` is "the bulk of starting a WebView2 control" (Microsoft, WebView2Feedback #1536) and not reducible from app code; environment creation is 3–6 ms. |
| Keld vs Tauri | **Keld led in both same-session runs** (472 vs 483; 467 vs 506). Margin 11–39 ms is small against noise — claim "consistently ahead this session", not a ratio. |
| Keld ships SmartScreen-disabled (inherited wry default) | **Fixed and free** — no browser args, verified on the live process; 0 measurable startup cost. |
| Smallest Windows binary of the measured stacks | **Strengthened** — 484,864 B vs Tauri 8,634,880 B (17.8x) and the Electron ~120 MB class. |
| ≤ 300 ms first-paint budget (arch 01 §5) | **Still missed** (~1.6x this session). The floor is Chromium process boot inside controller creation. Supported levers per Microsoft: early env creation (already done, 3–6 ms), hidden-webview prewarm + `put_ParentWindow` reparent (a memory-for-latency trade that fits real apps with init work, not the hello bench). Tracked on KEL-62. |
