# Size, RSS, latency, and installer scoreboard

Measured hello (and later app) artifacts against
[`docs/architecture/01-overview.md`](../architecture/01-overview.md) §5 budgets
and competitor hellos. Not the Electron API scoreboard
([`compat-scoreboard.md`](./compat-scoreboard.md)).

KEL-25 DoD until `keld-pack` and `bench/` exist. Append rows; do not invent
numbers, a fifth unique, a `bench/` crate, or CI that does not run.
Competitor apps live in [`gyldlab/keld-benches`](https://github.com/gyldlab/keld-benches),
not this monorepo.

**Read latency and memory as different tables.** Architecture 01 §5 budgets
cold start → first paint (≤ 300 ms) and idle RSS (≤ 90 MB) separately. A
memory figure is never a paint pass/fail. A paint-opportunity proxy is never
an idle-RSS pass/fail.

## What is not first paint

| Signal | What it actually is | Use |
|---|---|---|
| Idle RSS / coalition RSS / `ps -o rss=` | Memory after the window is up | Memory tables only |
| Titled `HWND` / window-visible | Presentation policy (when the framework shows a window) | Weak Windows diagnostic; not architecture 01 §5 |
| [gyldlab/keld#10](https://github.com/gyldlab/keld/pull/10) `PageLoadEvent::Finished` | wry navigation completion; clock starts after process launch | **Wrong paint oracle. Do not merge as first paint.** |
| Debug `just hello` (~5.5M / host RSS) | Debug host | Never a `vs` cell |
| Traced-arm beacon or `webview_built` | KEL-64 attribution (AC4: scores from trace-disabled arms only) | Attribution table only; not a published score |
| Hopp / Youngju / blog citations | Other machines, other protocols | **Do not** fill `vs` cells |
| Electrobun launcher 72,032 KB @ `0308d55` | Incomplete RSS (Bun child unmatched; WebKit GPU/WebContent not up) | **Not** a vs-WK memory claim |
| Chromium `.app` / zip disk | Shipped engine bytes | **Not** a vs-WK paint or vs-WK host-Mach-O cell |

The architecture 01 §5 paint metric is **cold start → first paint**. On macOS
KEL-64 that is an **untraced** external double-rAF image beacon (paint-opportunity
**proxy**, not compositor completion or display scanout). On Windows it is the
committed `windows/bench` image-beacon harness (WebView2 / Chromium-derived;
not comparable to macOS WK numbers).

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
Until that gate exists, the latency rows report measurements and attribution;
this scoreboard does not publish a ≤ 300 ms pass/fail label.

| Metric | Budget | Electron lead (KEL-11) | Phase 2 hello |
|---|---|---|---|
| Installer (runtime = bun) | ≤ 20 MB | 85–150 MB | N/A — Bun not packed |
| Installer (runtime = none) | ≤ 6 MB | — | N/A — no `.app` / DMG |
| Cold start → first paint | ≤ 300 ms | 1–3 s | **macOS WK, untraced double-rAF proxy:** last recorded `--publish` median **342.911 ms** (Keld `5ba4672`, benches recipe [`9e7c83d`](https://github.com/gyldlab/keld-benches/commit/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b); raw JSON not in keld-benches git). **Not** traced-arm beacon 352.211 ms. **Not** gyldlab/keld#10 `PageLoadEvent::Finished`. **Not** RSS. **Windows WebView2:** **472 ms** (2026-08-15 direct-COM, [`windows-first-paint-kel65-direct-com.json`](https://github.com/gyldlab/keld-benches/blob/686d1ab8ed57fc3b96b1a828d20bcf07adfab86a/windows/bench/windows-first-paint-kel65-direct-com.json) @ [`686d1ab`](https://github.com/gyldlab/keld-benches/commit/686d1ab8ed57fc3b96b1a828d20bcf07adfab86a)); architecture 01 §5 records the Chromium controller-creation floor. Measurement only, not a scoreboard pass/fail. |
| Idle RSS, 1 window (sum of keld processes) | ≤ 90 MB | 150–300 MB | **72.6–77.8 MiB** host-only @ Keld `b93ebb6` (under budget; no Bun; WebKit XPCs excluded). Coalition (host+WebKit helpers) is a different column — see Memory |
| kipc small-message p99 | ≤ 100 µs | ~ms-class | echo exists; not bench'd |
| kipc bulk (shm) | ≥ 1 GB/s | n/a | no shm |
| Update patch, 1-line JS | ≤ 50 KB | full installer | no updater |
| `keld dev` cold → window | ≤ 2 s | — | not measured here |

KEL-11 freeze (survey, not this Mac): Electron 85–150 MB / 150–300 MB RSS / 1–3 s;
Tauri 2.5–10 MB / 30–80 MB / 0.2–0.8 s. This-Mac Release hellos (2026-08-14)
fill `vs` Electron/Tauri cells in the **disk and RSS** tables only. Do not mix
Chromium vs WK. Do not mix disk with paint.

## Fairness rules

| Rule | Detail |
|---|---|
| Release only | `just hello` is debug (~5.5M). Never put debug in a `vs` cell. |
| Split lanes | host / runtime / engine-in-bundle / wrapping — never blend. |
| Same engine class | Do not mix WKWebView / Chromium / Skia in one `vs` cell. |
| Host ≠ product | `keld-host --hello` does **not** spawn Bun; no `.app`/DMG. Hello-without-Bun is a host-lane diagnostic. |
| RSS lines | Main-process `ps` and engine helpers (`com.apple.WebKit.*` or Chromium GPU/utility) stay separate. |
| Latency ≠ memory | Untraced double-rAF and idle RSS never share a `vs` cell. Coalition RSS is not paint. |
| Same-protocol `vs` | Same Mac, official Release packages. Blog/Hopp/Youngju citations do **not** fill `vs` cells. |
| Native floor | Swift AppKit/SwiftUI + WKWebView + same `HELLO_HTML`. Not AppKit TextKit / NSTextView. |
| Electrobun RSS | Launcher-only sample is **incomplete** (Bun child unmatched; WebKit GPU/WebContent not up). Not a vs-WK claim. |
| Cite SHA + path | keld-benches rows MUST name an OS-qualified fixture path **and** an immutable commit. `/tmp` JSON is not an immutable cite. |

---

## Latency — untraced double-rAF (not compositor)

**Machine (macOS):** Apple M4, macOS 26.5.1 (25F80), darwin/arm64.

### macOS Keld / Tauri — historical last recorded untraced `--publish` (raw JSON gap)

KEL-64 Swift oracle: one external monotonic clock armed before spawn; loopback
port 0; unique nonce; canonical HTML; nested double-rAF `<img>` beacon on a
focused visible document. **Proxy, not compositor completion.**

AC4
[`c3030d5`](https://github.com/gyldlab/keld-benches/commit/c3030d579b606ac00d3ba9e0fba8f57987a41baa):
reported scores come only from **trace-disabled** arms. The 2026-08-16
interleaved run predates the trace seam, so every arm was untraced.

These are historical last-recorded `--publish` values, not a newly published
score. The missing raw JSON is explicit in the final column and below.

| Arm | Samples | Double-rAF proxy median / p90 | Recipe SHA | Keld source | Raw JSON |
|---|---:|---:|---|---|---|
| **Keld** adapter `--hello` | 11/11 | **342.911 / 393.103 ms** | [`9e7c83d`](https://github.com/gyldlab/keld-benches/commit/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b) | `5ba4672` | **Gap:** not in keld-benches git. Previously recorded on Keld [`4b70435`](https://github.com/gyldlab/keld/commit/4b7043505b84c488aa6213e59ae4c198c01d04e8) / Linear KEL-64 from `/tmp/keld-vs-tauri-20260816d.json` (`--publish` eligible at the time). |
| **Tauri** 2.11.5 | 11/11 | **346.034 / 353.070 ms** | same | n/a (Tauri recipe `9e7c83d`) | same gap |

Same-recipe Keld-only and Tauri-only `--publish` disk/RSS live in the
[Memory](#memory--idle-rss-not-paint) and [Disk](#disk--installer-not-paint)
tables (Keld `dc5dea2` coalition **199,568 KiB**; Tauri-only double-rAF
**378.758 / 749.492 ms** is a different session from the interleaved row
above — not a vs-Electron paint cell).

**Committed untraced sample after AC4:** none. keld-benches
[`MEASUREMENTS.md`](https://github.com/gyldlab/keld-benches/blob/aae2e12f998ff47805eed38083c624525d87b9a8/MEASUREMENTS.md)
@ [`aae2e12`](https://github.com/gyldlab/keld-benches/commit/aae2e12f998ff47805eed38083c624525d87b9a8)
records the traced attribution decision, not an untraced `--publish` JSON.
The last untraced `--publish` JSON lived at `/tmp/keld-vs-tauri-20260816d.json`
and is **not** in keld-benches. Do not invent a replacement median.

### macOS traced attribution (not a score)

Paired diagnostic, n=11 traced + 11 untraced, Finder-anchored. The **traced**
lane is attribution. File + SHA:

[`macos/keld/hello/kel64-startup-attribution.json`](https://github.com/gyldlab/keld-benches/blob/aae2e12f998ff47805eed38083c624525d87b9a8/macos/keld/hello/kel64-startup-attribution.json)
@ [`aae2e12`](https://github.com/gyldlab/keld-benches/commit/aae2e12f998ff47805eed38083c624525d87b9a8)
(`evidenceKind`: `spec-section-7-paired-diagnostic-not-a-published-score`).
Keld source `59e0987696d874f7f12120d5fc1a7fabe5b79aa7`, recipe
[`258756c`](https://github.com/gyldlab/keld-benches/commit/258756c051fb951590d69d16fbad96f85c605d8b).
Also in `MEASUREMENTS.md` at the same SHA.

| Attribution (traced arm; **not** a score) | Median / p90 | Bounds |
|---|---:|---|
| Keld `webview_built` construction | **149.031 / 168.192 ms** | wry/WebKit webview-construction boundary |
| External valid double-rAF beacon | **352.211 / 392.408 ms** | End-to-end contract on the **traced** arm; not compositor completion; **not** the untraced score |
| Residual after construction | **197.656 / 215.243 ms** | WebKit process/navigation/canonical-page scheduling |

Limitation `external_webkit_scheduling`. Construction does not explain the p90
tail. **No product `keld-wv` window-build optimisation** from this slice.
AC1 [`a5b517c`](https://github.com/gyldlab/keld-benches/commit/a5b517c281da5555dcf7d310151af105ce589d02),
AC4 [`c3030d5`](https://github.com/gyldlab/keld-benches/commit/c3030d579b606ac00d3ba9e0fba8f57987a41baa),
AC6 [`aae2e12`](https://github.com/gyldlab/keld-benches/commit/aae2e12f998ff47805eed38083c624525d87b9a8).

### macOS diagnostic reruns (not publish-eligible)

Valid samples under the same beacon contract, **not** publication-eligible
(dirty public Wails `Assets.car` and unbound competitor provenance). Raw JSON
lived under `/tmp` and is **not** in keld-benches git. Fixture/harness
[`495ae15`](https://github.com/gyldlab/keld-benches/commit/495ae156604a14e33b485ca4a6c5e967c2be35ba).
**Not** a warmed-session league table. **Do not** mix the Chromium row into a
WK `vs` cell.

WKWebView-class arms (2026-08-17 sequential, Finder-anchored):

| Framework | Valid samples | Double-rAF median / p90 | Coalition RSS median / p90 (memory, not paint) |
|---|---:|---:|---:|
| Keld adapter (`--hello`) | 11/11 | **351.717 / 765.689 ms** | **200,464 / 201,024 KiB** |
| Tauri 2.11.5 | 11/11 | **361.355 / 732.112 ms** | **204,800 / 205,280 KiB** |
| Wails v3.0.0-beta.8 | 11/11 | **341.356 / 366.036 ms** | **207,136 / 207,648 KiB** |
| Neutralino 6.9.0 (stock) | 0/11 | not reportable | not reportable — window key without WKWebView first responder |
| Neutralino 6.9.0 + local focus patch | 11/11 | **595.975 / 625.097 ms** | **206,976 / 207,216 KiB** — patched-runtime proof, not stock |
| Electrobun 1.18.1 | 11/11 | **709.551 / 732.686 ms** | **311,312 / 311,840 KiB** |

Chromium-class arms (same session, **different engine** — not vs-WK):

| Framework | Valid samples | Double-rAF median / p90 | Coalition RSS median / p90 |
|---|---:|---:|---:|
| Electron 43.4.0 | 11/11 | **276.784 / 295.568 ms** | **373,104 / 373,296 KiB** |
| NW.js 0.114.1 | 11/11 | **425.380 / 442.879 ms** | **682,304 / 682,848 KiB** |

Earlier 2026-08-16 competitor extension @ recipe
[`91fd3e6`](https://github.com/gyldlab/keld-benches/commit/91fd3e6): Wails
11/11, median **340.984 / 353.263 ms**, coalition **206,976 / 207,776 KiB**.
NW.js / stock Neutralino / Electrobun / Electron were fail-closed that day
(not slow scores). `/tmp` JSON; not in git.

### Windows first paint (WebView2 — not macOS WK)

Current session for architecture 01 §5 on Windows: 2026-08-15 direct-COM A/B,
median of 7, committed JSON.

| Stack | first paint | Cite |
|---|---:|---|
| **Keld** `39be9cc` direct COM | **472 ms** | [`windows-first-paint-kel65-direct-com.json`](https://github.com/gyldlab/keld-benches/blob/686d1ab8ed57fc3b96b1a828d20bcf07adfab86a/windows/bench/windows-first-paint-kel65-direct-com.json) @ [`686d1ab`](https://github.com/gyldlab/keld-benches/commit/686d1ab8ed57fc3b96b1a828d20bcf07adfab86a); also `MEASUREMENTS.md` |
| Tauri 2.11.5 (same session) | 483 ms | same files |
| Electron 43.4.0 (same session) | 278 ms | same files |
| Keld wry baseline `137633f` | 467 ms | [`windows-first-paint-kel65-baseline.json`](https://github.com/gyldlab/keld-benches/blob/686d1ab8ed57fc3b96b1a828d20bcf07adfab86a/windows/bench/windows-first-paint-kel65-baseline.json) |

This is a measurement row, not a ≤ 300 ms pass/fail label. Architecture 01 §5
records the WebView2 controller-creation floor. Titled `HWND` is **not** this
metric. Historical Windows sessions (906 ms ad-hoc, 1,289 ms, 590 ms) are superseded; see
[Windows measured rows](#windows-measured-rows-2026-08-13) below. Do not
compare Windows WebView2 paint to macOS WK paint.

---

## Memory — idle RSS (not paint)

**Machine:** Apple M4, macOS 26.5.1 (25F80), darwin/arm64.

### Host-only `ps` (Phase 2 hello)

Keld `b93ebb6e0fb557b20ae312f155f3a33713212ccf`, release `keld-host --hello`,
no Bun. Method: `ps -o rss=` after the window is up. WebKit XPCs (`ppid=1`)
are **not** in the host figure.

| Stack | Main RSS | Helpers (not in main) | vs ≤ 90 MB | Cite |
|---|---:|---|---|---|
| **Keld** host-only | **79,616 then 74,304 KiB (72.6–77.8 MiB)** | WebKit XPCs ~70 MiB extra (`phys_footprint` autopsy) | **under** (host-only; XPCs excluded; no Bun) | this repo @ `b93ebb6` |
| Keld debug `just hello` | 73,184 then 70,752 KiB | — | not a `vs` | debug |
| Tauri 2.11.5 main | **102,896 KB (~100.5 MiB)** | WebKit XPCs **80,560 KB** | n/a (not a keld process sum) | [`MEASUREMENTS.md`](https://github.com/gyldlab/keld-benches/blob/0308d55f628797067985247000b30b43ea00cba1/MEASUREMENTS.md) @ [`0308d55`](https://github.com/gyldlab/keld-benches/commit/0308d55f628797067985247000b30b43ea00cba1) [`macos/tauri/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/tauri/hello) |
| Wails v3.0.0-beta.8 main | **95,648 KB (~93.4 MiB)** | WebKit XPCs **73,760 KB** | n/a | same SHA [`macos/wails/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/wails/hello) |
| Neutralino 6.9.0 main | **86,336 KB (~84.3 MiB)** | WebKit XPCs **81,200 KB** | n/a | same SHA [`macos/neutralino/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/neutralino/hello) |
| SwiftUI + WK main | **101,168 KB (~98.8 MiB)** | WebKit XPCs extra | n/a | same-day native floor; benches [`macos/swift/swiftui-wk`](https://github.com/gyldlab/keld-benches/tree/646be7972706158ff744a1cb33547eddfe127445/macos/swift/swiftui-wk) @ [`646be79`](https://github.com/gyldlab/keld-benches/commit/646be7972706158ff744a1cb33547eddfe127445) |
| AppKit + WK main | **97,344 KB (~95.1 MiB)** | WebKit XPCs extra | n/a | [`macos/swift/appkit-wk`](https://github.com/gyldlab/keld-benches/tree/646be7972706158ff744a1cb33547eddfe127445/macos/swift/appkit-wk) @ `646be79` |
| Electron 43.4.0 main | **138,064 KB (~134.8 MiB)** | GPU 79,760 + util 40,384 + renderer 84,640 KB | **not same engine** | [`macos/electron/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electron/hello) @ `0308d55` |
| NW.js 0.114.1 main | **205,776 KB (~201 MiB)** | Chromium helpers **386,880 KB** | **not same engine** | [`macos/nwjs/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/nwjs/hello) @ `0308d55` |
| Electrobun 1.18.1 launcher | **72,032 KB** | Networking 17,328 KB only; Bun + GPU/WebContent **not up** | **incomplete — not vs-WK** | [`macos/electrobun/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electrobun/hello) @ `0308d55` |

`0308d55` competitor rows are **disk/RSS (memory), not paint.** Swift RSS used
`WKNavigationDelegate.didFinish` as a settle signal for `ps`; that is still
not KEL-64 first paint.

### Coalition RSS (KEL-64, host + WebKit helpers)

Different contract from host-only `ps`. Not comparable to 72.6–77.8 MiB.
Not paint.

| Arm | Samples | Coalition RSS median (min–max) | Recipe |
|---|---:|---:|---|
| Keld adapter (Keld-only `--publish`) | 11/11 | **199,568 KiB** (196,080–200,368) | `9e7c83d`, Keld `dc5dea2` |
| Keld adapter (interleaved `--publish`) | 11/11 | **199,968 KiB** (196,496–200,480) | `9e7c83d`, Keld `5ba4672` |
| Tauri-only `--publish` | 11/11 | **204,624 KiB** (203,744–207,840) | `9e7c83d` |
| Tauri interleaved `--publish` | 11/11 | **205,024 KiB** (199,264–206,032) | `9e7c83d` |

Raw JSON for these coalition rows was `/tmp`; not in keld-benches git. Recorded
on Keld [`4b70435`](https://github.com/gyldlab/keld/commit/4b7043505b84c488aa6213e59ae4c198c01d04e8).

---

## Disk / installer (not paint)

Same machine as the RSS table. Competitor hellos 2026-08-14,
[`0308d55`](https://github.com/gyldlab/keld-benches/commit/0308d55f628797067985247000b30b43ea00cba1)
[`MEASUREMENTS.md`](https://github.com/gyldlab/keld-benches/blob/0308d55f628797067985247000b30b43ea00cba1/MEASUREMENTS.md).
Keld host `b93ebb6`. Electron/NW.js = Chromium **disk** ceiling; Native Swift =
WK wrapping floor. Flutter = Skia (do not put on the WK floor).

| Stack | Lane | Disk / bytes | Engine | Runtime | Conf. | Cite |
|---|---|---|---|---|---|---|
| **Keld** `b93ebb6` | **host** Mach-O | **1,010,464 B (987K)** | system WKWebView | **none** | **high** | `cargo build --release -p keld-host`; `stat`. No `.app` / DMG. |
| Keld | host | 5,793,368 B | same | none | high | **debug** `just hello` — not for `vs`. |
| Keld | cli | 2,531,600 B | n/a | n/a | high | `keld` binary — devtools, not an installer. |
| Keld | create hello source | 1,316 B | n/a | n/a | high | embedded template; no `node_modules`. |
| Keld KEL-64 adapter `.app` | wrapping diagnostic | **1,043,880 B** file sum / 1,024K `du` | system WKWebView | none | high | oracle artifact @ `9e7c83d`; no Bun. |
| **Electron** 43.4.0 | zip + wrapping | zip **122,121,746 B**; `.app` **288,448,512 B** | Chromium (in zip) | Node (in zip) | **high** | [`macos/electron/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electron/hello) @ `0308d55`. Forge did not write `out/`; weighed official zip via ditto. **Not** `electron .`. **Not** a vs-WK paint cell. |
| **Tauri** 2.11.5 | wrapping | `.app` **8,265,728 B**; exe **8,153,472 B**; DMG **2,910,772 B** | system WKWebView | none | **high** | [`macos/tauri/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/tauri/hello) @ `0308d55`. KEL-64 recipe artifact **8,292,272 B** file sum @ `9e7c83d`. |
| **Wails** v3.0.0-beta.8 | wrapping | `.app` **9,818,112 B**; exe **8,271,424 B**; UDZO **5,320,599 B** | system WKWebView | none (Go in binary) | **high** | [`macos/wails/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/wails/hello) @ `0308d55`. |
| **Neutralino** 6.9.0 | wrapping | wrapped `.app` **2,953,216 B**; UDZO **1,322,015 B** | system WKWebView | none | **high** | [`macos/neutralino/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/neutralino/hello) @ `0308d55`. `--macos-bundle` only renames the Mach-O. |
| **NW.js** 0.114.1 | zip + wrapping | zip **169,495,010 B**; `.app` **410,271,744 B** | Chromium | Node | **high** | [`macos/nwjs/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/nwjs/hello) @ `0308d55`. Runtime zip not in git. |
| **Electrobun** 1.18.1 | wrapping vs runtime | wrapped `.app` **18,710,528 B**; zstd **18,514,771 B**; extracted **42,360,832 B** (Bun **32,287,232 B**) | system webview | **Bun** | high (disk) | [`macos/electrobun/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electrobun/hello) @ `0308d55`. `bundleCEF: false`. |
| **Native Swift** SwiftUI + WK | host + wrapping | **96K** `du` / **92,740 B** file sum; exe **89,696 B**; DMG **31,655 B** | WKWebView | none | **high** | `swiftc -O`; [`macos/swift/swiftui-wk`](https://github.com/gyldlab/keld-benches/tree/646be7972706158ff744a1cb33547eddfe127445/macos/swift/swiftui-wk) @ `646be79`. |
| **Native Swift** AppKit + WK | host + wrapping | **88K** `du` / **80,976 B** file sum; exe **77,936 B**; DMG **29,774 B** | WKWebView | none | **high** | Closest floor to wry NSWindow+WKWebView. [`macos/swift/appkit-wk`](https://github.com/gyldlab/keld-benches/tree/646be7972706158ff744a1cb33547eddfe127445/macos/swift/appkit-wk) @ `646be79`. |
| Flutter macOS | — | unmeasured (survey) | **Skia — not a webview** | Dart | low | Do not put on the WKWebView floor. |

Host-lane `vs` (exe-to-exe, not wrapping, not Chromium zip): Keld 1,010,464 B
vs Swift AppKit 77,936 B vs SwiftUI 89,696 B. Tauri `.app` 8,265,728 B and
Electron `.app` 288,448,512 B are **other lanes**.

**Do not misread:**

| Trap | Why it lies |
|---|---|
| Bare Mach-O vs `.app` vs zip vs zstd/DMG | Different lanes. |
| Host-only RSS vs engine-inclusive / coalition | Keld 72.6–77.8 MiB is host process only; KEL-64 coalition ~200,000 KiB includes WebKit helpers. |
| RSS as ≤ 300 ms paint | Memory is not paint. |
| Vendor RSS that omits WebKit | Wails vendor ~10 MB is Go-side; this-Mac Wails main is 95,648 KB plus 73,760 KB XPCs. |
| Compressed download vs unpacked `.app` | Electrobun zstd 18,514,771 vs wrapped 18,710,528 vs extracted 42,360,832; Electron zip 122,121,746 vs `.app` 288,448,512. |
| Tiny Swift `.app` vs future Keld installer | 88–96K because WebKit is system. Packed Keld adds Bun. |
| Swift exe vs Keld Mach-O vs Swift `.app` | Compare exe-to-exe (89,696 / 77,936 vs 1,010,464). |
| Electrobun launcher RSS vs Tauri/Wails | 72,032 KB is incomplete; Bun child and WebKit GPU/WebContent were not up. |
| Neutralino `--macos-bundle` vs wrapped `.app` | Official dist name is a renamed Mach-O. Weighed wrapped `.app` 2,953,216 B. |
| Chromium disk vs WK paint | Different metrics and different engines. |

### Still waiting

| Item | Why |
|---|---|
| Keld `.app` / DMG | `keld-pack` has no authoring code |
| Bun lane in installer | Not packed; this-Mac `bun` 1.3.14 = 63,096,576 B extracted; gzip-9 = 23,548,666; zstd-19 = 16,838,595 |
| Electrobun complete RSS @ `0308d55` | Launcher 72,032 KB only; Bun child + WebKit GPU/WebContent not enumerated |
| Committed untraced macOS `--publish` JSON | Last untraced median 342.911 ms is a Keld-scoreboard/Linear record; not a file in keld-benches after AC4 |
| First-paint measurement evidence | Windows **472 ms** (JSON @ `686d1ab`). macOS last recorded untraced proxy **342.911 ms**; traced beacon **352.211 ms** is not that score. Construction ~149 ms vs traced beacon ~352 ms is residual WebKit, not a `keld-wv` build target. Do not use gyldlab/keld#10 `PageLoadEvent::Finished`. Linux has a live backend and an Xvfb/`xdotool` attachment smoke after the `default_vbox` fix, but no first-paint measurement. No ≤ 300 ms pass/fail label until the gate exists. |
| kipc p99 / shm / update patch | No shm, no updater, no `bench/` |
| CI > 5% regression gate | Architecture 01 §5 once `bench/` lands (KEL-39) |
| Windows / Linux competitor hellos | keld-benches stubs; not this machine |
| Six-framework notes-app bench | Guard-on-IPC (KEL-69) and host `fs.read`/`fs.write` (KEL-71) landed, but the separate bench epic has not started. |

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
| 2 | Idle RSS vs Swift ~95 MiB / Tauri 102,896 KB / Wails 95,648 KB / Neutralino 86,336 KB (WK mains); Electron 138,064 KB Chromium main | **can win with work** | Host-only 72.6–77.8 MiB under those WK mains and ≤90 MB — not the product (no Bun, no XPCs). **Not** a vs-Electron claim. **Not** a first-paint claim. Electrobun 72,032 KB launcher is incomplete. Insufficient headroom vs the reported ~22 MiB Bun floor; measure host+Bun before claiming this lane can win. |
| 3 | Installer no-Bun (≤6 MB) vs Tauri / Neutralino | **can win with work** vs Tauri | Host already 987K. Pack `.app`/DMG vs this-Mac Tauri `.app` 8,265,728 / DMG 2,910,772. **Cannot** claim smallest shell vs Swift 88K / Neutralino wrapped `.app` 2,953,216. |
| 4 | Installer **with Bun** (≤20 MB) vs Electrobun / Electron | **can win with work** vs Electron | gzip-9 Bun alone is over 20 MB; zstd-19 = 16,838,595 for Bun alone — full installer size is unmeasured. This-Mac Electrobun zstd 18,514,771 (extracted 42,360,832; bundled Bun 32,287,232) is the compressed Bun-class ceiling to beat once packed. Electron zip 122,121,746 / `.app` 288,448,512. |
| 5 | Cold start first paint (architecture target ≤300 ms) | **measurement only — no current gate** | Windows JSON @ `686d1ab`: Keld **472 ms** vs Tauri 483; Electron 278; floor is Chromium boot inside `CreateCoreWebView2Controller`. macOS: KEL-64 **untraced** double-rAF proxy **342.911 ms** (recipe `9e7c83d`; JSON not in benches git). Traced construction **149.031 ms** vs traced beacon **352.211 ms** @ `aae2e12` is residual WebKit (`external_webkit_scheduling`), **not** a paint score and **not** a `keld-wv` rewrite. Do not use gyldlab/keld#10 `PageLoadEvent::Finished`. Do not use RSS. |
| 6 | Chromium-complete web platform **and** beat Electron on disk | **cannot win honestly** | Chromium *is* the disk (this-Mac Electron `.app` 288,448,512; NW.js `.app` 410,271,744). Spec 01 §6: no CEF-by-default. |
| 7 | Default-deny / crash isolation / zero ambient OS authority | **can win with work** | The product bet (architecture 01 §1). Specified, not enforced on hello. |
| 8 | Flutter Skia hello | **cannot win honestly** | Different engine. Spec 01 §6: not a UI toolkit. |

Zero lanes are **already winning** as a public claim. Host-only RSS vs Swift/Tauri
is a labeled diagnostic, not a launch tweet.

**Spec tension:** architecture 01 §5 ≤20 MB with Bun vs architecture 06 §1
compressed Bun ~25–35 MB — the 20 MB budget is a **compressor choice** (zstd vs
UDZO), not a host-trim project.

**Refuse:** CEF-by-default; drop Bun to win installer; count hello-without-Bun as
the product; mix WK/Chromium/Skia in one `vs`; debug 5.5M in `vs`; time-to-RSS,
time-to-titled-`HWND`, or wry `PageLoadEvent::Finished` (gyldlab/keld#10) as first
paint; fill `vs` from blog citations; treat Electrobun launcher RSS as vs-WK;
fake `bench/` CI; treat traced-arm construction or traced-arm beacon as a paint
score; claim ≤300 ms pass/fail from RSS or from PR #10.

## Related

Fixtures are public and OS-first (`{macos|windows|linux}/<framework>/...`).
Cite the path **and** an immutable SHA — not only `main`. Not in this monorepo.

- Competitor hellos (disk/RSS, **not** paint) @ [`0308d55`](https://github.com/gyldlab/keld-benches/commit/0308d55f628797067985247000b30b43ea00cba1) ([MEASUREMENTS.md](https://github.com/gyldlab/keld-benches/blob/0308d55f628797067985247000b30b43ea00cba1/MEASUREMENTS.md)): [`macos/electron/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electron/hello), [`macos/tauri/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/tauri/hello), [`macos/neutralino/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/neutralino/hello), [`macos/wails/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/wails/hello), [`macos/nwjs/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/nwjs/hello), [`macos/electrobun/hello`](https://github.com/gyldlab/keld-benches/tree/0308d55f628797067985247000b30b43ea00cba1/macos/electrobun/hello)
- KEL-64 macOS oracle + recipes @ [`9e7c83d`](https://github.com/gyldlab/keld-benches/commit/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b): [`macos/harness`](https://github.com/gyldlab/keld-benches/tree/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b/macos/harness), [`macos/keld/hello`](https://github.com/gyldlab/keld-benches/tree/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b/macos/keld/hello), [`macos/tauri/hello`](https://github.com/gyldlab/keld-benches/tree/9e7c83d1a5c94a790b2e3ed0a89855e3aed4ab9b/macos/tauri/hello)
- KEL-64 attribution fixture [`macos/keld/hello`](https://github.com/gyldlab/keld-benches/tree/aae2e12f998ff47805eed38083c624525d87b9a8/macos/keld/hello): AC1 [`a5b517c`](https://github.com/gyldlab/keld-benches/commit/a5b517c281da5555dcf7d310151af105ce589d02), AC4 [`c3030d5`](https://github.com/gyldlab/keld-benches/commit/c3030d579b606ac00d3ba9e0fba8f57987a41baa), AC6 [`aae2e12`](https://github.com/gyldlab/keld-benches/commit/aae2e12f998ff47805eed38083c624525d87b9a8); JSON [`kel64-startup-attribution.json`](https://github.com/gyldlab/keld-benches/blob/aae2e12f998ff47805eed38083c624525d87b9a8/macos/keld/hello/kel64-startup-attribution.json)
- Native Swift @ [`646be79`](https://github.com/gyldlab/keld-benches/commit/646be7972706158ff744a1cb33547eddfe127445): [`macos/swift/appkit-wk`](https://github.com/gyldlab/keld-benches/tree/646be7972706158ff744a1cb33547eddfe127445/macos/swift/appkit-wk), [`macos/swift/swiftui-wk`](https://github.com/gyldlab/keld-benches/tree/646be7972706158ff744a1cb33547eddfe127445/macos/swift/swiftui-wk)
- Windows first-paint JSON @ [`686d1ab`](https://github.com/gyldlab/keld-benches/commit/686d1ab8ed57fc3b96b1a828d20bcf07adfab86a) `windows/bench/`
- Budgets: [`docs/architecture/01-overview.md`](../architecture/01-overview.md) §5
- Packing / Bun size: [`docs/architecture/06-runtime-and-tooling.md`](../architecture/06-runtime-and-tooling.md) §1, §3
- Four uniques / parked `bench/`: [`decisions.md`](./decisions.md) §1, §11
- Electron API scores: [`compat-scoreboard.md`](./compat-scoreboard.md)
- Linear: KEL-11, KEL-25, KEL-26, KEL-39, KEL-64


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

| Stack | first paint (budgeted metric) | titled `HWND` (weak) | Historical relation to ≤ 300 ms target |
|---|---|---|---|
| **Keld** `keld-host --hello` | **906 ms** | 433 ms | 906 ms recorded; no current gate |
| Tauri 2.11.5 | **504 ms** | 32 ms | 504 ms recorded; no current gate |
| Electron 43.4.0 | **not measured** | 125 ms | — |

Raw first-paint runs (ms): Keld 906 / 943 / 867 / 857 / 977 · Tauri 504 / 568 /
464 / 500 / 510. Full method and per-arm notes:
[`keld-benches@f0d042d` MEASUREMENTS.md](https://github.com/gyldlab/keld-benches/blob/f0d042dea36f99b448a01be03b679a07eb9e4c80/MEASUREMENTS.md).

These dated values are retained for provenance, not as a current ≤ 300 ms
pass/fail label. The later committed direct-COM session supersedes them.

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
| Historical 2026-08-14 relation to the ≤ 300 ms target | Keld **906 ms**, Tauri **504 ms**. This dated row is superseded by the 2026-08-15 JSON median **472 ms** and is not the current scoreboard pass/fail label. |
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
| Historical relation to the ≤ 300 ms target | Keld **1,289 ms**, Tauri **688 ms**, Electron **444 ms**. Retained as a superseded session, not a current pass/fail label. |
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

Harness at [`windows/bench/`](https://github.com/gyldlab/keld-benches/tree/39bab061951283901023a34e885de41d432e3483/windows/bench)
@ [`39bab06`](https://github.com/gyldlab/keld-benches/commit/39bab061951283901023a34e885de41d432e3483)
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
| Historical relation to the ≤ 300 ms target | Keld **590 ms**, Tauri **596 ms**, Electron **395 ms**. The remaining Keld cost is `WebViewBuilder::build` (~550 ms: WebView2 environment + controller creation), which is engine-inherent on this path. Measurement only; no current pass/fail label. Tracked on KEL-62. |
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
| Historical relation to the ≤ 300 ms target | Direct COM **472 ms**, wry baseline **467 ms**. Measurement only; no current pass/fail label. The floor is Chromium process boot inside controller creation. Supported levers per Microsoft: early env creation (already done, 3–6 ms), hidden-webview prewarm + `put_ParentWindow` reparent (a memory-for-latency trade that fits real apps with init work, not the hello bench). Tracked on KEL-62. |
