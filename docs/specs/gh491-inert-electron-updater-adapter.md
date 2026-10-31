# Spec: inert `electron-updater` adapter for migrated apps (F09-T1, GH-491)

Status: draft
Linear: GH-491 (#517 tracker of record; KEL-127 reference) · Owner: @0monish · Updated: 2026-10-31

Source basis: `origin/main b72e4c19`. Upstream oracle: electron-updater **6.8.9**, the npm
artifact drawio-desktop's lockfile pins by integrity
(`sha512-ZhVxM9iGONUpZGI1FxdMRgJjUFXi7AYGVa5PwKlO1tV1/4zDxQmfKpXOHVztKrd6L9rLcFjERvi1Mf2vxyTkig==`
at the #589 pin). On 2026-10-31 the registry tarball hashed to exactly that integrity and the
ten files cited below byte-matched the restored `node_modules/electron-updater` copy. Line
numbers are into that artifact's `out/*.js`. Consumer facts are from the pinned draw.io main
process, `src/main/electron.js` at the #589 pin. This draft authorizes no merge; the
implementation on its branch is review material until the exact text is approved.

## 1. Goal & non-goals

drawio-desktop's main process does `import elecUpPkg from 'electron-updater'; const {autoUpdater}
= elecUpPkg` (`electron.js:17-18`) and, before any guard, assigns `autoUpdater.logger`, writes
through it (`:81-83`), sets `autoDownload`/`autoInstallOnAppQuit` (`:85-86`), registers six
listeners at module top level (`:2228-2355`) and calls `setFeedURL` inside `whenReady`
(`:2078-2083`) outside its `disableUpdate` guard. Real electron-updater needs Electron's `net`
session, a feed and a host updater; none exists under Keld, and a throwing or missing member
sends draw.io's `safeUpdaterCall` (`:124-141`) to its "Update Error" dialog. This spec defines a
Keld-owned package, **`@keld/electron-updater`**, that the migrated manifest resolves for the
`electron-updater` specifier (X02-T3 / #499 A10 own that edit). It exposes the `AppUpdater`
subset draw.io touches, evaluates at import with no app-link frame and no I/O, mirrors
electron-updater 6.8.9 wherever that can be honoured without a host updater, and never
fabricates an updater outcome: a check that would need a feed emits one typed `error` and
rejects.

Non-goals (owners in parentheses):

- Any host update channel, feed contact, download, staging, install or relaunch (F09-T5;
  real updater authority is KEL-53, architecture 06 §4a).
- The Electron-core `autoUpdater` facade on `@keld/electron` (F09-T4, parked).
- Declaring or delivering `DRAWIO_DISABLE_UPDATE=true` in the role environment (F01-T4 / #452);
  minting `app.isPackaged` (#452, KEL-141 for the installed half).
- The #580 conformance cells in `crates/keld-compat` (separate owner; this change edits nothing
  there) and the draw.io product-denominator cells (X02-T4).
- Comparing the `setFeedURL` argument with any build config (F09-A6 closed: no schema, no
  consumer); feed-base reporting (F09-A2 closed: park).
- Upstream members draw.io never touches (`channel`, `allowPrerelease`, `allowDowngrade`,
  `requestHeaders`, `netSession`, `signals`, `isUpdaterActive`, `forceDevUpdateConfig`,
  `currentVersion`, the platform updater classes, `Provider`).

## 2. Spec refs

- `docs/architecture/04-electron-compat.md` §3 (module alias layer: the app's `electron`
  resolves to `@keld/electron`; this package is the same layer for the `electron-updater`
  specifier) and §7 (the updater bridge: the feed is host-declared; apps do not own update
  channels under Keld). This is the ▲ this package records for `setFeedURL`.
- `docs/architecture/06-runtime-and-tooling.md` §4a (update authority stays with the host and
  KEL-53; "`autoUpdater` compatibility … remain target behavior").
- `docs/architecture/07-agent-experience.md` §2 (typed `KELD-*` errors with a fix in the
  message; `docs/engineering/keld-error-codes.md` is the registry).
- Deviation from the architecture docs: none. `No boundary change`: no handle ownership, crash
  ownership or principal minting changes; the package makes no host call.

## 3. Acceptance criteria (binary, each becomes a test)

Upstream citations are electron-updater 6.8.9 `out/AppUpdater.js` unless named otherwise.

1. Given a Bun 1.4.4 role, when `import pkg from "@keld/electron-updater"` runs and
   `const {autoUpdater} = pkg` follows, then `autoUpdater` is an `EventEmitter` and the same
   object as the named export `autoUpdater`, and `pkg.AppUpdater`/`pkg.NoOpLogger` exist
   (upstream `out/main.js` module object).
2. Given a probe that refuses every socket, Worker, child process and file write (`fetch`,
   `Bun.connect`/`listen`/`udpSocket`/`spawn`/`spawnSync`/`write`, `Worker`, `WebSocket`,
   `node:net`/`tls`/`http`/`https`/`dgram`/`child_process`/`worker_threads`, `node:fs` writes),
   when the package is imported and the draw.io sequence of criterion 10 is replayed, then the
   probe records zero calls. The probe's own self-check records a real `fetch`, `net.connect`
   and `fs.writeFileSync`, so an inert probe cannot pass this criterion.
3. Given an object `log`, when `autoUpdater.logger = log`, then `autoUpdater.logger === log`,
   so `autoUpdater.logger.transports.file.level = 'error'` (`electron.js:82`) lands on `log`.
   Given `autoUpdater.logger = null`, then the getter returns a `NoOpLogger` instance, not
   `null` (`:62-67`). The default logger is `console` (`:179`).
4. `autoDownload`, `autoInstallOnAppQuit` and `autoRunAppAfterInstall` default to `true`
   (`:109-119`), read back whatever is assigned, and emit nothing.
5. Given draw.io's options `{provider: 'github', repo: 'drawio-desktop', owner: 'jgraph'}` with
   and without `channel` (`:2078-2083`), and given a plain string, when `setFeedURL` is called,
   then it returns `undefined`, throws nothing, emits nothing, opens no socket and writes no
   file (criterion 2), and `getFeedURL()` is unchanged. ▲ Upstream builds a provider client
   (`:234-252`); Keld ignores the app feed because the feed is host-declared (§2).
6. `getFeedURL()` returns exactly `"Deprecated. Do not use it."` (`:227-229`).
7. Given the application facts report not packaged, when `checkForUpdates()` is called, then it
   resolves `null`, emits no event and logs upstream's skip line through the assigned logger
   (`:277-284`). Today the Electron facade has no `isPackaged`, which reads as not packaged
   exactly as upstream's `=== true` comparison does (`out/ElectronAppAdapter.js`
   `get isPackaged`); `keld dev` is unpackaged (#452).
8. Error case: given the application facts report packaged and no host updater exists, when
   `checkForUpdates()` is called, then exactly one `error` event fires carrying an `Error`
   whose `code` is `KELD-COMPAT-050` (a registry code outside `KELD-COMPAT-001..009` and not a
   `KELD-UPDATE-*` host code), whose message starts with that code and states the fix, with
   upstream's second argument shape `Cannot check for updates: …` (`:265-268`); then the
   returned promise rejects with the identical `Error` object. `checking-for-update` never
   fires, no feed is contacted. `checkForUpdatesAndNotify()` has the same two outcomes
   (`:286-300`).
9. `downloadUpdate()` before any offer dispatches one `error` with message
   `Please check update first` and rejects with the same `Error` (`:435-441`, `dispatchError`
   `:476-478`). `quitAndInstall()` with nothing staged logs `Install on explicit quitAndInstall`,
   dispatches one `error` with message `No update filepath provided, can't quit and install`,
   and never calls the application's `quit` (`out/BaseUpdater.js:13-26`, `:42-53`); a second
   call does the same again.
10. Given a fixture that replays the PANEL-P3 sequence (`electron.js:74-86` configuration,
    `:2228-2355` listeners, then the `whenReady` tail `:2078-2083` `setFeedURL` and the
    `:2092-2100` guarded boot check) with the variable set or cleared explicitly in the child's
    environment, never inherited:
    - with `DRAWIO_DISABLE_UPDATE=true`: zero `checkForUpdates` calls, zero `error` events,
      zero synchronous throws or rejections observed by the `safeUpdaterCall` replica;
    - with the variable absent and the facts unpackaged: one call resolving `null`, zero events;
    - with the variable absent, the facts packaged and no host updater: one call, exactly one
      `error` with `code` `KELD-COMPAT-050`, one rejection reaching the replica's catch (what
      draw.io shows as its Update Error dialog: the honest outcome), zero other events.
11. Across every case above, `update-available`, `update-not-available`, `download-progress`,
    `update-downloaded`, `checking-for-update`, `login`, `update-cancelled` and
    `appimage-filename-updated` are never emitted.
12. `cargo test -p keld-cli --test error_registry` passes with `KELD-COMPAT-050` registered and
    emitted only from `packages/@keld/electron-updater/src`; removing either the heading or the
    emitter fails it.
13. Emitting `error` with no application listener does not throw from the emitter: the
    instance's own first `error` listener logs `Error: <stack>` through the assigned logger
    (`:198-200`).

## 4. Design

### 4.1 First-principles and reuse decision

- Ownership: the packaged fact is owned by F01-T4 (#452) and surfaced as `app.isPackaged` on
  `@keld/electron`; this package reads it and owns no second notion of "packaged" (no
  environment variable, no file). The `DRAWIO_DISABLE_UPDATE` guard is draw.io's and the
  variable's delivery is #452's; the adapter never reads it. The feed is host-declared
  (architecture 04 §7, KEL-53); the app's feed argument is discarded.
- Process and trust: the package runs inside the app's Bun role with that role's authority and
  adds no grant, host call, socket, file or child process. Nothing it does can change another
  process's state; the only outputs are `EventEmitter` events, promise settlements and lines
  through the application-assigned logger.
- Lifecycle and failure: one `EventEmitter` is constructed at import; there is no steady-state
  work, timer or queue. Every refusal is the same shape upstream uses (one `error`, then the
  promise rejects), so draw.io's existing handlers (`:124-141`, `:2228`) stay the owners of the
  user-visible consequence. An `error` listener that throws propagates to the caller exactly as
  under upstream's `EventEmitter`.
- Existing options evaluated:
  - Real electron-updater 6.8.9: needs Electron's `net`/`session`, a feed and a platform
    installer path; under Keld it would contact `api.github.com` at the first check and fail at
    install. Rejected: there is no host updater path, and contacting a feed from an app role
    violates the host-owned update authority.
  - The Electron-core `autoUpdater` facade (F09-T4) or a bridge to `keld-update`: zero corpus
    demand for the core API and no host update channel yet (F09-T5). Rejected for this slice;
    the `checkForUpdates` refusal is the seam F09-T5 replaces.
  - `@keld/electron` gaining an `electron-updater` sub-path: the specifier is a separate
    package in every consumer's manifest, and the single-resolver rule (X02-A1) makes a
    dependency replacement the one mechanism; a sub-path would be a second resolver. Rejected.
- Reuse: `node:events` for the emitter (as `@keld/electron/src/browser-window.ts` does);
  `@keld/kipc`'s `KeldCallError` shape (`Error & { code }`) and `isCallError` predicate for the
  typed refusal, with the registry's `"<code>: <detail>"` message form (`linkError`); sibling
  packages reached by relative source path exactly as `@keld/electron` reaches `@keld/api`;
  the `docs/engineering/keld-error-codes.md` registry held by `crates/keld-cli/tests/error_registry.rs`.
- Compatibility fallback: `getFeedURL()` keeps upstream's deprecated literal; `setFeedURL`
  keeps upstream's `void` no-throw contract; both match cells of criterion 9 keep upstream's
  exact message text (a `KELD-*` code there would itself be a divergence).
- Performance: none claimed; import constructs one `EventEmitter`.

### 4.2 Decisions (each with the alternative rejected and its falsifier)

- D1 **Name and path.** `@keld/electron-updater` at `packages/@keld/electron-updater`,
  following the root naming rule (`@keld/*`) and naming the specifier it stands in for.
  Alternative `@keld/updater-compat` rejected: it does not name the replaced package, which
  the X02-T3 manifest reviewer must recognise. Falsifier: the migrated manifest's
  `electron-updater` entry cannot name the package.
- D2 **Reaching the facade.** `src/electron-app.ts` imports `app` from
  `../../electron/src/index.ts` statically, as upstream's `ElectronAppAdapter` does
  `require("electron").app` when the updater is constructed. Alternatives: (a) a lazy dynamic
  import, rejected because upstream's gate is synchronous and laziness would only hide a facade
  import-time regression that the app's own `import "electron"` hits first; (b) `import "electron"`
  through the app's resolver, rejected because the repository's sibling convention is relative
  source paths and a symlinked checkout cannot resolve `electron` from inside the repository.
  Edge: this makes the facade's import-time purity part of criterion 2; today the transport
  connects only inside functions (`packages/@keld/kipc/src/transport.ts:1287`, `:2600`) and its
  Worker self-entry is guarded by `!isMainThread` (`:3760`). Falsifier: the import probe records
  a socket or Worker.
- D3 **Injectable application facts.** `AppUpdater`'s constructor takes `(options, app:
  AppAdapter)` with `AppAdapter = { readonly isPackaged: boolean; quit(): void | Promise<void> }`,
  the two upstream `AppAdapter` members this slice can observe (upstream's constructor is
  `(options, app?)`). The module `autoUpdater` is constructed with the facade-backed adapter.
  Alternative, a module-level stub of the facade in tests, rejected: `bun test` shares one module
  cache across files, so a stub leaks (learnings 2026-08-18). Falsifier: the no-quit test cannot
  observe a quit.
- D4 **One new code.** `KELD-COMPAT-050` for the refused check, emitted only from
  `packages/@keld/electron-updater/src/error.ts`, registered with crate `@keld/electron-updater`.
  The match cells of criterion 9 keep upstream's plain `Error` text. The number is taken from the
  middle of the free range (`013..999`) because parallel agents allocate from both ends this
  week; the registry's "next free number" step is therefore not followed and is raised in §10.
  Falsifier: reusing `KELD-UPDATE-*` or `KELD-COMPAT-001..009` fails the provenance test.
- D5 **No pretence.** The adapter never logs upstream's `Checking for update` line, never emits
  `checking-for-update`, and does not deduplicate concurrent checks (upstream's
  `checkForUpdatesPromise`, `:257-262`): each call refuses independently. draw.io checks at most
  once per boot and once per menu action. Falsifier: a `checking-for-update` listener fires.
- D6 **`setFeedURL` is silent.** No log line, no stored field, no thrown error. A logger line
  was rejected because a throwing application logger would turn `setFeedURL` into the throw
  the issue's negative control forbids. The ▲ is recorded by #580's cell, not by runtime output.
- D7 **Upstream's first `error` listener is mirrored** (criterion 13) so a refusal with no app
  listener is logged, not thrown by `EventEmitter`.
- D8 **Registry holding of a TypeScript emitter.** `SCAN_REL` in
  `crates/keld-cli/tests/error_registry.rs` gains `packages/@keld/electron-updater/src`, so the
  heading and the emitter are checked in both directions; a Bun test additionally reads the
  registry file and asserts the heading exists, an oracle independent of the Rust test.

### 4.3 Types and channels (TypeScript, no `any` in the public API)

```ts
// packages/@keld/electron-updater/src/types.ts (shapes from electron-updater 6.8.9 and
// builder-util-runtime 9.7.0 typings; upstream `any` is `unknown` here)
export interface Logger { info(m?: unknown): void; warn(m?: unknown): void; error(m?: unknown): void; debug?(m: string): void }
export interface UpdateInfo { readonly version: string; readonly files: UpdateFileInfo[]; readonly path: string; readonly sha512: string; releaseName?: string | null; releaseNotes?: string | ReleaseNoteInfo[] | null; releaseDate: string; readonly stagingPercentage?: number; readonly minimumSystemVersion?: string }
export interface UpdateCheckResult { readonly isUpdateAvailable: boolean; readonly updateInfo: UpdateInfo; readonly downloadPromise?: Promise<string[]> | null; readonly versionInfo: UpdateInfo }
export type AppUpdaterEventMap = {
  error: [error: Error, message?: string];
  login: [info: LoginAuthInfo, callback: LoginCallback];
  "checking-for-update": [];
  "update-not-available": [info: UpdateInfo];
  "update-available": [info: UpdateInfo];
  "update-downloaded": [event: UpdateDownloadedEvent];
  "download-progress": [info: ProgressInfo];
  "update-cancelled": [info: UpdateInfo];
  "appimage-filename-updated": [path: string];
};
export interface PublishConfiguration { readonly provider: PublishProvider; readonly channel?: string | null; readonly url?: string | null; readonly owner?: string | null; readonly repo?: string | null; readonly [option: string]: unknown }
export type FeedOptions = string | PublishConfiguration;
export interface AppAdapter { readonly isPackaged: boolean; quit(): void | Promise<void> }

// packages/@keld/electron-updater/src/app-updater.ts
export class NoOpLogger implements Logger { info(): void; warn(): void; error(): void }
export class AppUpdater extends EventEmitter<AppUpdaterEventMap> {
  autoDownload: boolean;            // true
  autoInstallOnAppQuit: boolean;    // true
  autoRunAppAfterInstall: boolean;  // true
  constructor(options: FeedOptions | null | undefined, app: AppAdapter);
  get logger(): Logger | null;      // returns the stored object itself
  set logger(value: Logger | null); // null installs a NoOpLogger
  getFeedURL(): string | null | undefined;      // "Deprecated. Do not use it."
  setFeedURL(options: FeedOptions): void;       // ▲ no effect, never throws
  checkForUpdates(): Promise<UpdateCheckResult | null>;
  checkForUpdatesAndNotify(downloadNotification?: DownloadNotification): Promise<UpdateCheckResult | null>;
  downloadUpdate(): Promise<string[]>;          // rejects "Please check update first"
  quitAndInstall(isSilent?: boolean, isForceRunAfter?: boolean): void; // one error, no quit
  protected dispatchError(e: Error): void;      // emit("error", e, (e.stack || e).toString())
}

// packages/@keld/electron-updater/src/error.ts
export const UPDATE_CHECK_REFUSED = "KELD-COMPAT-050";
export function updateCheckRefused(detail: string): KeldCallError; // Error & { code }

// packages/@keld/electron-updater/src/index.ts
export { AppUpdater, NoOpLogger }; export const autoUpdater: AppUpdater;
export default { autoUpdater, AppUpdater, NoOpLogger };
```

- Capabilities required; manifest changes (spec 03): none.
- Wire/protocol changes (spec 02): none.
- Platform notes: Bun-only and platform-independent; the CI Bun lane runs the suite on
  Ubuntu, macOS and Windows; recorded first on macOS (first-proof lane). No behaviour differs
  by OS because no OS facility is used.
- Runtime seam: none (`No boundary change`).
- Migration unit: callers are draw.io's 16 `autoUpdater.` sites (`electron.js:18`, `:81-86`,
  `:1901`, `:2078`, `:2099`, `:2228`, `:2230`, `:2264`, `:2279`, `:2329`, `:2350`, `:2355`,
  `:2360`, `:2389`); the generated/persisted contract is the migrated `package.json`
  `electron-updater` entry (X02-T3 / #499 A10, row `package.json`), owned there. Temporary
  adapter: none. Permanent compat facade retained: this package; F09-T5 replaces only its
  `checkForUpdates` refusal with the host channel.

## 5. Boundaries

- Implement in: `packages/@keld/electron-updater/{package.json,tsconfig.json,bun.lock,src/**,fixtures/**}`;
  `crates/keld-cli/tests/error_registry.rs` (one `SCAN_REL` entry); `docs/engineering/keld-error-codes.md`
  (one heading; the scanned-tree sentence); `llms-full.txt` (regenerated); `justfile` (`typescript`
  recipe, three lines).
- Must not touch: `crates/keld-compat/**` (#580 owns the cells), `packages/@keld/electron/**`,
  `packages/@keld/api/**`, `packages/@keld/kipc/**`, `crates/keld-update/**`, workspace
  `Cargo.toml`, `.github/workflows/**`, `tools/ci_changes.sh` (it discovers Bun suites from
  `packages/**/package.json` plus a test file; no edit is needed), the draw.io pin.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 This spec (`Status: draft`), opened as a DRAFT PR and stopped for exact-text approval.
- [ ] T2 Package `@keld/electron-updater` with the registry entry, `SCAN_REL`, `justfile` wiring and
      regenerated `llms-full.txt`; Bun tests and fixtures for criteria 1-13 (same PR, behind T1;
      merge only at `Status: approved`).
- [ ] T3 (#580, separate owner) flips the eight F09-T3 cells from their recorded expected `fail`
      to `pass`/▲ against the test names in §7 once this package lands.
- [ ] T4 (X02-T3 / #499, separate owner) points the migrated manifest's `electron-updater` entry
      at this package under the #502 resolver.

## 7. Test plan

All tests run with `env -u CLAUDECODE bun test` in `packages/@keld/electron-updater` (Bun 1.4.4)
and are discovered by the CI router as a Bun suite root. Test names begin with an operation id
of the form `electron-updater.<member>.<observable>` so #580's cells can map to them.

| Criterion | Test (file: name prefix) | Oracle | Negative control (the one mutation that must fail) |
|---|---|---|---|
| 1 | `import-shape.test.ts`: `electron-updater.import.default-destructure` | upstream `out/main.js` module object; `EventEmitter` prototype | drop the default export |
| 2 | `replay.test.ts`: `electron-updater.import.zero-io`, `electron-updater.probe.self-check`, every replay case | refusing probe preload (`fixtures/probe_preload.ts`) counts; self-check proves it sees `fetch`, `net.connect`, `fs.writeFileSync` | add `fetch(...)` to `setFeedURL` |
| 3 | `app-updater.test.ts`: `electron-updater.logger.identity` | strict equality; property write lands | return a wrapper from the getter |
| 4 | `app-updater.test.ts`: `electron-updater.flags.plain-booleans` | field values; zero events | a setter with a side effect |
| 5, 6 | `app-updater.test.ts`: `electron-updater.set-feed-url.no-throw-no-effect`, `electron-updater.get-feed-url.deprecated-literal` | `undefined` return; exact literal; zero events; probe in replay | `throw` inside `setFeedURL` |
| 7 | `app-updater.test.ts`: `electron-updater.check-for-updates.unpackaged-null`, `electron-updater.check-for-updates.facade-today-unpackaged` | `null`; zero events; upstream skip line | remove the gate (packaged path taken) |
| 8 | `app-updater.test.ts`: `electron-updater.check-for-updates.packaged-no-host-updater-error`, `electron-updater.error-code.provenance`, `electron-updater.check-for-updates-and-notify.same-outcome` | one `error`; `isCallError`; `code === KELD-COMPAT-050`; identical rejection; registry heading present | resolve `null` when packaged; reuse `KELD-UPDATE-001` |
| 9 | `app-updater.test.ts`: `electron-updater.download-update.check-first`, `electron-updater.quit-and-install.nothing-staged-no-quit` | upstream message text; one `error`; `quit` count 0 | call `app.quit()` from `quitAndInstall` |
| 10 | `replay.test.ts`: `electron-updater.replay.env-disabled`, `electron-updater.replay.env-absent-unpackaged`, `electron-updater.replay.env-absent-packaged-no-host-updater` | fixture `GH491_RESULT` line: call count, events, replica catch; probe counts | make `setFeedURL` throw (env-disabled replica records a sync throw) |
| 11 | `app-updater.test.ts`: `electron-updater.never-fabricate`; every replay case | listeners for all nine names count zero | emit `update-not-available` from `checkForUpdates` |
| 12 | `cargo test -p keld-cli --test error_registry` | registry ↔ scanned emitters, both directions | delete the heading |
| 13 | `app-updater.test.ts`: `electron-updater.error-event.no-app-listener-logs` | no synchronous throw; logger receives `Error: …` | remove the constructor's `error` listener |

Anti-flake: fixtures are spawned with `Bun.spawnSync` and a kill-switch timeout only; they
await `setImmediate` to let events flush and exit by themselves. The environment is built
explicitly (`DRAWIO_DISABLE_UPDATE` set or deleted per case). No ports, no files, no timing
assertions. The facade-today case (criterion 7) is pinned to the current facade: when #452 mints
`isPackaged`, `bun test` still runs unpackaged, so the case keeps passing; an installed-root run
is X02-T4's cell, not this suite's.

## 8. Review gates triggered

- unsafe: none.
- public API: **yes** (new package `@keld/electron-updater`: `autoUpdater`, `AppUpdater`,
  `NoOpLogger`, the types above, and one new registry code).
- permission model: none (no grant, no host call).
- dependency addition: none (Keld-owned package; its two devDependencies, `@types/bun 1.4.0`
  and `typescript 7.0.2`, are the siblings' exact pins; no runtime dependency).
- wire protocol: none.

## 9. Perf impact

none. Import constructs one `EventEmitter`; no steady-state allocation, timer or I/O. No
architecture 01 §5 budget moves.

## 10. Open questions

1. Code number: `KELD-COMPAT-050` deliberately skips the registry's "next free number" step so
   parallel allocations this week do not collide. Owner choice: keep `050`, or renumber to the
   next free number at merge (three literals: `src/error.ts`, the registry heading, this spec).
2. Public seam: `AppUpdater`'s constructor requires an `AppAdapter`. Upstream's is protected with
   an optional `app`. Keep the explicit seam (recommended: it is what makes the no-quit and
   packaged contracts observable without module stubs), or hide it behind a facade default.
3. Product-status ledger: a `package.keld-electron-updater` row needs an id mapping in
   `tools/product_status.rs` (`expected_package_owner`) plus a TSV row and regeneration. Owner
   choice: add the row in this PR (tool edit), fold the package into `package.keld-electron`'s
   note, or defer until the package is consumed by the migrated manifest.
4. Approval of this exact text (`Status: approved`) before the implementation commits on this
   branch may merge.
