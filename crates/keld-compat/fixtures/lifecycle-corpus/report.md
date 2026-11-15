# Bounded Electron lifecycle evidence report

This report publishes the committed `electron-lifecycle-v0` **showcase** corpus only. It is not a product/median-app Electron compatibility percentage and it does not qualify native windows, webview engines, strict-profile sandboxing, packaging, releases, or adoption.

- Corpus digest: `sha256:badc0aaf3619168927cf464e2dd0006a599b5614a35b84960c59984b18e0e8b2`
- Upstream oracle: Electron 44.3.0 @ `07e460719c75b2ec5ee4893f7d2192ef31c7b8c2`
- PR source head: `fd3c875c59e3cb0b0530166b21013571afd05754`
- Exact GitHub Actions tested merge: `38db257ba2d1f377bd2e24f7bc871faca895c6d5`
- Candidate-A Actions run: [35015086640](https://github.com/gyldlab/keld/actions/runs/35015086640)
- Authority profile: `legacy_sandbox_off` (the CI conformance harness is an ordinary test process, not a product strict-Bun session)
- Harness engine identity: `headless-lifecycle-conformance@38db257ba2d1f377bd2e24f7bc871faca895c6d5` (headless conformance identity; not WKWebView/WebView2/WebKitGTK)

## Platform results

| Platform | Hosted runner | keld-compat batch | Oracle matches | Intentional divergence | Receipt |
| --- | --- | ---: | ---: | ---: | --- |
| macOS aarch64 | `macos-26-arm64` `20260907.0351.1` | 48 passed / 0 skipped | 2/3 | 1 (`app.quit()` return contract) | `sha256:741490530fde052e312e938d6969d31e151da1f74b86e25067ca2a7632200355` |
| Linux x86_64 | `ubuntu-24.04` `20260907.300.1` | 48 passed / 0 skipped | 2/3 | 1 (`app.quit()` return contract) | `sha256:5357cba3cbfe1458b078d815956f15959491a09e95aaa7b03643810c508edc3d` |
| Windows x86_64 | `windows-2025-vs2026` `20260907.229.1` | 49 passed / 0 skipped | 2/3 | 1 (`app.quit()` return contract) | `sha256:eab36c9eb94a6d0c9974190b8ca70843473d0e5c175c0b68b4079609a4154ec3` |

Each platform is scored separately against the same committed three-cell denominator. Two cells match Electron's pinned lifecycle oracle; the third intentionally records Keld's `app.quit(): Promise<void>` divergence from Electron's `void` return. Counts are published instead of turning this bounded showcase into an overall Electron-compatibility percentage.

## Cell results

| Operation | Oracle | Result | Meaning |
| --- | --- | --- | --- |
| `app.when-ready.host-ready-gate` | `electron-v44.3.0.app.when-ready-initialized` | pass | `app.whenReady()` stays pending until the host lifecycle `Ready` event. |
| `app.window-all-closed.policy` | `electron-v44.3.0.app.window-all-closed-policy` | pass | Listener presence suppresses default quit; removing the last listener restores it. |
| `app.quit.return-contract` | `electron-v44.3.0.app.quit-void` | fail | Intentional divergence: Keld returns `Promise<void>` so typed `KELD-IPC-*` failure remains observable. |

## Reproduction boundary

The machine-readable evidence records under `evidence/` are parsed by the existing KEL-74 owner. Their `evidence_uri` values are SHA-256 digests of the corresponding committed `receipts/*.json` bytes. The receipts bind the exact Actions run/job, tested commit, runner image, architecture, Bun/Rust revisions, corpus digest, and successful mapped-oracle batch.

GitHub Actions tested the pull-request merge commit rather than the PR head directly. Both identities are retained above and in each receipt; neither is rewritten as the later landed squash/merge source.

The lifecycle corpus validator executes the mapped Rust and Bun behavioral oracles and contains negative controls for missing/skipped/comment-only/helper-only cases. A parser-only success is not treated as lifecycle compatibility evidence.
