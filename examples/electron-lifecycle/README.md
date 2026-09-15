# Electron lifecycle example

This is Keld's current **source-conformance** example for the implemented Electron-compatible `app` lifecycle slice. It is intentionally small: it demonstrates `import { app } from "electron"`, host-backed readiness, `window-all-closed`, and the current `app.quit()` contract without implying that the rest of Electron is implemented.

The executable application source already lives at [`packages/@keld/electron/fixtures/lifecycle.ts`](../../packages/@keld/electron/fixtures/lifecycle.ts). The authenticated host-side harness is [`crates/keld-compat/tests/electron_lifecycle.rs`](../../crates/keld-compat/tests/electron_lifecycle.rs). This directory is the discoverability and explanation layer; it does not copy either implementation.

## Run it from a source checkout

Prerequisites are the same Rust toolchain and Bun runtime described in the [source-build onboarding guide](../../docs/onboarding/README.md). From the Keld repository root, run:

```bash
bun --version
bun --revision
cargo test --locked -p keld-compat --test electron_lifecycle \
  when_ready_does_not_resolve_before_host_ready_event -- --nocapture
```

That test launches the TypeScript fixture under Bun, gives it an authenticated app-link, completes the real lifecycle HELLO, and drives the existing `keld_core::LifecycleSession`. The test owns its temporary session credential. Do **not** create, paste, or hard-code a `KELD_APP_LINK` to run the fixture directly.

A successful run proves the bounded lifecycle contract exercised by that test. It is not a native-window acceptance run and it is not evidence for unrelated Electron APIs.

## The application code

The fixture uses the same import shape an Electron main process uses:

```ts
import { app } from "electron";

app.on("window-all-closed", () => {
  void app.quit();
});

await app.whenReady();
```

The fixture contains extra markers and a deliberately throwing `ready` listener because it is also a regression oracle. Those test details prove that listener failure does not skip the remaining listener or break the lifecycle read loop.

The adjacent [`tsconfig.json`](../../packages/@keld/electron/fixtures/tsconfig.json) is the current v0 runtime alias owner for this fixture: Bun resolves `electron` to the repository's [`@keld/electron`](../../packages/@keld/electron/src/index.ts) package through `compilerOptions.paths`. The package then uses the canonical [`@keld/kipc`](../../packages/@keld/kipc/src/transport.ts) transport through its lifecycle adapter. There is one transport and one authenticated app-link; this example adds neither.

## Observable sequence

The conformance harness requires the following behavior:

1. the Bun main process starts and records `KEL72_WAITING`;
2. `app.whenReady()` remains pending after authenticated connection establishment;
3. only after the host sends lifecycle `Ready` do the `ready` listener and `app.whenReady()` complete;
4. the application does not synthesize `window-all-closed` itself;
5. after the host reports its last window closed, the registered `window-all-closed` listener runs;
6. `app.quit()` sends the real lifecycle Quit call and the host session ends successfully.

The negative control is explicit in [`electron_lifecycle.rs`](../../crates/keld-compat/tests/electron_lifecycle.rs): replacing `whenReady()` with an immediate `Promise.resolve()` makes the named readiness test fail before the host signals `Ready`.

The versioned bounded denominator and immutable upstream oracle are in [`crates/keld-compat/fixtures/lifecycle-corpus/`](../../crates/keld-compat/fixtures/lifecycle-corpus/). They keep this example separate from any future median-app/product compatibility percentage.

## Current compatibility boundary

Implemented here:

- `app.whenReady()` waits for the host lifecycle `Ready` event;
- `app.isReady()` reflects that host readiness;
- `window-all-closed` is emitted from the host `LastWindowClosed` lifecycle event;
- with no `window-all-closed` listener, Keld follows Electron's default-quit behavior;
- `removeListener` / `off` restore that default after the last listener is removed;
- `process.type` is shimmed to `browser` in this main-process package;
- `process.versions.electron` is Keld's documented shim value, not a claim that Keld embeds Electron.

Intentional divergence: Electron documents `app.quit()` as returning `void`. Keld currently returns `Promise<void>` so a failed Quit call remains observable as a typed `KELD-IPC-*` transport failure. The compatibility scoreboard records that difference; the example must not hide it.

Not demonstrated or claimed here: `BrowserWindow`, `ipcMain`, `ipcRenderer`, preload/`contextBridge`, `dialog`, menus/tray, migration, `keld build`, installers, or broad Electron compatibility.

## Why this is not run as `keld dev` yet

The current no-flag dev boot stage is deliberately self-contained. It stages the configured entry and renderer plus the optional canonical `src/kipc-transport.ts`. The current Electron runtime alias, however, depends on project `tsconfig.json` paths resolving to the repository's `@keld/electron` source package. That alias/package tree is not part of the current staged-app contract.

This example therefore uses the existing conformance harness instead of copying `@keld/electron` or creating another transport just to make a demo appear product-live. Staging the Electron alias/package for arbitrary `keld dev` applications is separate boot/compat integration work. Until that lands and is qualified, this example remains source-conformance only.

For the current native-window source-build demo, use the [onboarding quick-start](../../docs/onboarding/README.md). Its generated main process speaks the existing KIPC app-link directly and is not an Electron-migration example.

## Native Close evidence is separate

The harness above calls the host lifecycle state machine directly; it does **not** prove that a physical Close button works on every desktop environment.

Current public native-Close evidence is source- and platform-qualified separately: the stock lifecycle correction landed in [PR #228](https://github.com/gyldlab/keld/pull/228); the onboarding guide links the accepted Windows 11 x64 and Ubuntu 26.04.1 GNOME Wayland captures. macOS native Close, X11, other Linux distributions/architectures, strict/release profiles, and release packaging remain unproved unless their own evidence says otherwise.

## Related owners

- [`docs/architecture/04-electron-compat.md`](../../docs/architecture/04-electron-compat.md) — Electron-compatibility contract and current/target boundary.
- [`packages/@keld/electron/src/app.ts`](../../packages/@keld/electron/src/app.ts) — current `app` shim behavior.
- [`packages/@keld/electron/src/link.ts`](../../packages/@keld/electron/src/link.ts) — lifecycle adapter over canonical KIPC transport.
- [`crates/keld-compat/tests/electron_lifecycle.rs`](../../crates/keld-compat/tests/electron_lifecycle.rs) — behavioral conformance owner.
- [`crates/keld-compat/fixtures/lifecycle-corpus/`](../../crates/keld-compat/fixtures/lifecycle-corpus/) — bounded measurable lifecycle corpus.

If making this example runnable through the staged `keld dev` path requires a new public API, transport, or boot contract, that change belongs to its existing architecture owner and must not be hidden inside this example.