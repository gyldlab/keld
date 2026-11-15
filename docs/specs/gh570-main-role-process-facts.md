# Spec: Electron-shaped `process.*` facts for the Bun main role
- Status: draft
- Tracker: GH-570
- Owner: @0monish
- Updated: 2026-11-15
- Pin: 7feedd409b668280e83062eb8d8bc5a547611fa7

**Scoped owner decision (2026-11-15): T-1 approved. Overall status remains draft because T-2/T-3 are unapproved.**

The owner approved T-1 at reviewed head `7feedd409b668280e83062eb8d8bc5a547611fa7`
after independent adversarial review: Option A, the package-bound version, native
property descriptors and the existing single installer. [Decision record](https://github.com/gyldlab/keld/pull/1363#discussion_r4499768681).
This is not approval of T-2/T-3, native or installed acceptance, the whole PR, merge,
or a security/provenance bypass. The PR remains draft/do-not-merge. T-2/T-3 text below
is a deferred proposal, not an approved implementation contract.

## 1. Goal & non-goals

drawio-desktop's main (pinned `src/main/electron.js`) reads `process.argv` at module
top level (`:76`, `:79`, `:164`), decides how to slice it by `process.defaultApp`
(`:1056-1062`: `argv.unshift(null)` unless `process.defaultApp == true`, then
`args.js:191-199` takes `argv.slice(2).filter(t => t != null)` as the tokens), and its
dependencies read `process.type` while they evaluate (electron-log 5.4.4
[electron-log 5.4.4 source](https://github.com/megahertz/electron-log/blob/v5.4.4/src/index.js#L5-L8) selects its main or renderer module from it), `process.resourcesPath`
(electron-updater's app adapter joins `app-update.yml` onto it when packaged) and
`process.versions.electron`. Today `@keld/electron` installs only `process.type` and
`process.versions.electron`, at facade import (`packages/@keld/electron/src/index.ts`).
`process.defaultApp` and `process.resourcesPath` are absent, so a draw.io-shaped main
treats the Bun entry path as its first file argument. This spec makes one module the
owner of every main-role `process.*` value Keld states, fixes the `(argv, defaultApp)`
atom, keeps `versions.electron` honest, keeps the renderer/store flags `undefined`
rather than `false`, defines the `resourcesPath` value contract, and proposes the
pre-entry installation moment as a KEL-75 role-spawn amendment for approval.

Observable outcome: after `import "electron"` in the Bun main role, a draw.io-shaped
main's `argv.slice(defaultApp ? 2 : 1)` equals exactly the user arguments the launcher
appended after the entry, `process.type === "browser"`, `process.versions.electron`
is the Keld shim constant, and `mas`, `windowsStore`, `sandboxed`, `contextIsolated`
and `isMainFrame` are not own properties of `process`.

Non-goals:

- Payload content, delivery, fail-closed behaviour and the `app.*` getters (#452,
  F01-T4). This spec consumes that payload for `resourcesPath` (T-3) and asserts no
  `app.*` observable.
- The renderer/preload `process` object (#484, F07-T1); `process.noAsar` (F01-T6);
  diagnostic process methods (F08-T2); `process.parentPort` (F04-T5).
- Module specifier resolution and the package exports map (#502, X03-T1).
- Any Host (`crates/keld-host`) or `keld-cli` change in T-1. T-2 names the exact change
  and stops for approval.
- `process.versions.chrome`: absent in the main role and recorded ✘ (there is no
  Chromium in the Bun role; inventing a version string would be a lie).
- The installed-launch half of the argv cell (KEL-141, Backlog) and Windows/Linux
  acceptance: documented gaps. `process.windowsStore` stays `undefined` because no
  Keld MSIX mode exists in `keld-pack` or `keld-update`.

## 2. Spec refs

- `docs/architecture/04-electron-compat.md` §3 (shim layers; the v0 paragraph names
  `process.versions.electron = "0.0.1"` and `process.type = "browser"`). This spec adds
  `process.defaultApp` to that paragraph in the same PR; no deviation.
- `docs/architecture/04-electron-compat.md` §4 and `docs/engineering/compat-scoreboard.md`
  (per-API ✔/▲/✘ rows). The rows this spec adds are listed in §4.
- `docs/specs/kel75-principalized-bun-child-roles.md` § Role declaration and identity:
  `KELD_APP_LINK` is the sole child bootstrap variable and the entry arrives through the
  Host's trusted bundle resolution. T-1 changes nothing there. T-2 is an amendment to the
  trusted Bun command (a second Host-selected `--preload`), recorded as such.
- `docs/specs/kel96-no-flag-host-boot.md` (closed schema-v1 `keld.boot.json` fields):
  T-2 must not add a descriptor field without a KEL-96 schema decision (§10 Q-2).
- `docs/specs/gh532-first-proof-evidence-rules.md` rule 3 (red-until-implemented cells)
  and `docs/specs/gh566-corpus-manifest-owner.md` (corpus owner). The cells for this
  ticket are #569's `electron-module-process-v1` corpus (PRs #1294 and #1323, not on
  `origin/main` at `b72e4c19`); this spec adds no second corpus.
- Electron v44.4.5 oracle: `docs/api/process.md` (`process.defaultApp`, `process.type`,
  `process.resourcesPath`, `process.versions.electron`, the four `undefined`-flags),
  [Electron defaultApp definition](https://github.com/electron/electron/blob/v44.4.5/default_app/main.ts#L92-L96) (the `defaultApp` descriptor), and
  `shell/common/node_bindings.cc:933,1231,1237` (`type`, `resourcesPath` and
  `versions.electron` are set read-only by the runtime, not by application code).

## 3. Acceptance criteria


### AC-1: Argument slicing
- **Given:** A Bun main role and the draw.io-shaped argument parser.
- **When:** The entry imports electron and parses its launch arguments.
- **Then:** Given the Bun main role has imported `electron`, when a draw.io-shaped main applies
  `if (process.defaultApp != true) argv.unshift(null)` and then
  `argv.slice(2).filter(t => t != null)`, then the tokens equal exactly the user
  arguments the launcher appended after the entry; with no user arguments (the
  `keld dev` case, which refuses every argument with `KELD-CLI-044`) the tokens are
  `[]`.
- **Negative control:**
  - **Single fault:** Omit defaultApp installation.
  - **At-risk precondition:** The launcher supplies an entry path and no user arguments.
  - **Expected RED:** AC-1 observes the entry path instead of an empty user list.

### AC-2: Guardian argv
- **Given:** The macOS guardian command shape described below.
- **When:** Bun evaluates the entry.
- **Then:** Given the role was spawned with the macOS guardian's command shape
  (`bun --define=… [--preload=…] run <entry> …user`,
  `crates/keld-host/src/main.rs::guardian_bun_command`), when the entry reads
  `process.argv`, then `argv[0]` is the Bun executable, `argv[1]` is the entry path,
  `argv.slice(2)` is the user argument list and the runtime flags appear only in
  `process.execArgv`.
- **Negative control:**
  - **Single fault:** Insert a runtime flag into process.argv.
  - **At-risk precondition:** The launch includes the guardian define flag.
  - **Expected RED:** AC-2 detects a flag in the application argument list.

### AC-3: defaultApp descriptor
- **Given:** An ordinary Bun main process.
- **When:** The facade installs its facts.
- **Then:** After facade import, `Object.getOwnPropertyDescriptor(process, "defaultApp")` is
  `{ value: true, writable: false, enumerable: true, configurable: false }`, the
  descriptor Electron's `default_app/main.ts` installs.
- **Negative control:**
  - **Single fault:** Make defaultApp configurable.
  - **At-risk precondition:** The descriptor is inspected after installation.
  - **Expected RED:** AC-3 descriptor equality fails.

### AC-4: Native facts and honest version
- **Given:** An ordinary Bun main process and the pinned shim package.
- **When:** The facade installs type and version facts.
- **Then:** After facade import, `process.type === "browser"` and
  `process.versions.electron === ELECTRON_VERSION_SHIM`, both read-only and
  configurable (Electron v44.4.5 native `SetReadOnly`); `ELECTRON_VERSION_SHIM` equals the `@keld/electron` package
  version, is not `44.4.5` (the pinned oracle release), and is outside the npm
  `electron` release list (the full-list predicate is #569's cell
  `process.versions.electron.honesty`; this branch proves the constant against the
  pinned release and the package version).
- **Negative control:**
  - **Single fault:** Use the pinned Electron release as the shim version.
  - **At-risk precondition:** The installed version is compared with the package and pinned oracle.
  - **Expected RED:** AC-4 honesty assertions fail.

### AC-5: Absent flags
- **Given:** An ordinary Bun main process.
- **When:** The facade finishes installation.
- **Then:** After facade import, for each of `mas`, `windowsStore`, `sandboxed`,
  `contextIsolated`, `isMainFrame`: `Object.hasOwn(process, name) === false` and
  `typeof process[name] === "undefined"`; `Object.hasOwn(process.versions, "chrome")`
  is `false`.
- **Negative control:**
  - **Single fault:** Define sandboxed=false.
  - **At-risk precondition:** The test inspects own properties after installation.
  - **Expected RED:** AC-5 fails the absence census.

### AC-6: Idempotence
- **Given:** A target with facts installed by this module.
- **When:** The installer runs again.
- **Then:** Installing twice (a pre-entry preload followed by the facade import) throws nothing
  and leaves every descriptor byte-identical.
- **Negative control:**
  - **Single fault:** Change a descriptor during the second install.
  - **At-risk precondition:** The first install already matches the native descriptor.
  - **Expected RED:** AC-6 before/after descriptor equality fails.

### AC-7: Preflight refusal
- **Given:** Ordinary targets with the conflict cases below.
- **When:** The installer plans and applies facts.
- **Then:** Error case: given a foreign definition of `type`, `defaultApp` or `versions.electron`
  with a different value (configurable or not), when the installer runs, then it throws
  an `Error` whose message starts with `KELD-COMPAT-014:`, names the property and the
  fix (remove the second owner), and defines no fact on that target. A foreign
  *configurable* definition with the *same* value is adopted with Electron's
  descriptor, not refused. A same-value, non-configurable `type` or
  `versions.electron` is refused before any write because its configurability
  cannot be restored; a compatible non-configurable `defaultApp` is allowed.
  On ordinary objects, an absent fact whose owner is non-extensible also refuses
  before any write with `KELD-COMPAT-014`, naming the property and directing the
  caller to install the facts before making their owners non-extensible. Compatible
  existing facts on non-extensible owners remain adoptable; arbitrary Proxy traps
  are outside the no-partial-write guarantee.
- **Negative control:**
  - **Single fault:** Remove the absent-owner extensibility check.
  - **At-risk precondition:** versions is non-extensible and lacks electron.
  - **Expected RED:** AC-7 observes a TypeError or partial writes rather than typed refusal.

### AC-8: Preload readiness
- **Given:** A child with the independent early dependency fixture.
- **When:** The child starts with or without the facts preload.
- **Then:** Given a Bun role launched with `--preload=<a preload that imports only
  process-facts and installs>`, when a dependency shaped like electron-log evaluates
  before the entry imports `electron`, then it observes `process.type === "browser"`
  and `process.defaultApp === true`; without the preload it observes `undefined` for
  both while the facade import still installs them afterwards. T-1 proves the module is
  preload-safe; T-2 proves the Host wiring.
- **Negative control:**
  - **Single fault:** Remove the install call from the preload.
  - **At-risk precondition:** The dependency reads process.type before the facade import.
  - **Expected RED:** AC-8 reports unknown mode rather than main.

### AC-9: Deferred Host wiring
- **Given:** The real macOS staged Host launch and the existing #569 cells.
- **When:** T-2 is approved, implemented and exercised.
- **Then:** (T-2) Under `keld dev` on macOS the #569 cells `process.early.type` and
  `process.argv.default-app.empty-user-args` flip from `fail` to `pass` in the T-2
  diff, observed by `crates/keld-host/tests/no_flag_macos.rs`
  (`module_process::shipping_role_early_dependency_reports_process_facts`).
- **Negative control:**
  - **Single fault:** Install facts only at facade import.
  - **At-risk precondition:** The dependency reads the facts before importing electron.
  - **Expected RED:** AC-9 early-process cells remain failed.

### AC-10: Deferred Host path
- **Given:** A T-3 implementation using the #452 session-facts payload.
- **When:** The main role observes resourcesPath.
- **Then:** (T-3, after #452) `process.resourcesPath` is a string equal to the Host-staged launch
  root carried by the session-facts payload, never a path containing `app.asar`, and
  never derived from `__dirname`, `process.cwd()` or an environment variable; the #569
  cell `process.resources-path.string` flips to `pass` in that diff. Until then the
  property is absent and that cell stays `fail` naming #452.
- **Negative control:**
  - **Single fault:** Replace the payload path with process.cwd().
  - **At-risk precondition:** The role changes cwd away from the staged launch root.
  - **Expected RED:** AC-10 exact payload-path equality fails.

## 4. Design

### D-1: First-principles and reuse decision

Ownership facts. The Host owns the spawn command and therefore the argv shape
(`guardian_bun_command`: `bun --define=process.env.KELD_NATIVE_WINDOW_FACADE:"<0|1>"
[--preload=<root>/src/kipc-transport.ts] run <root>/<entry>`, cwd `<root>`,
`KELD_APP_LINK` set, `BUN_OPTIONS` removed). Bun owns `process`. `@keld/electron` owns
the Electron-shaped values. Measured on Bun 1.4.4 with that exact flag shape:
`process.argv` is `[<bun>, <absolute entry>, ...user]`, the two runtime flags are in
`process.execArgv` only, and a `--preload` module sees the same `process.argv` as the
entry. `keld dev` refuses every argument (`KELD-CLI-044`), so the user list is empty
under `keld dev` today; forwarding OS user arguments is #452's contract.

Trust. A `process.*` value mints no authority: `defaultApp` is a parsing hint,
`versions.electron` a version string, `resourcesPath` a path the Host already staged.
Nothing here widens a grant or reads an environment variable.

Lifecycle. Facts are installed once per role process, non-writable and enumerable.
`process.type` and `process.versions.electron` are configurable because Electron
uses `SetReadOnly` without `DontDelete` ([Electron SetReadOnly](https://github.com/electron/electron/blob/v44.4.5/shell/common/gin_helper/dictionary.h#L173-L192);
`node_bindings.cc:933,1237`). Only `defaultApp` is non-configurable
([Electron defaultApp definition](https://github.com/electron/electron/blob/v44.4.5/default_app/main.ts#L92-L96)). A second owner is a defect and surfaces as
`KELD-COMPAT-014`, never as a silent overwrite or a bare `TypeError`.

Failure. The installer does no I/O and opens no link, so it cannot fail on the
transport. Planning refuses conflicting descriptors or an absent fact on an ordinary
non-extensible owner with `KELD-COMPAT-014` before any fact is written. Existing
compatible facts can still be adopted on sealed owners. Arbitrary Proxy traps can
throw or mutate during inspection or definition; transactional installation on those
targets is not promised.

The `(argv, defaultApp)` atom — Option A (selected). Keep Bun's `process.argv`
untouched and define `defaultApp = true`. Electron documents `defaultApp` as `true`
"when the app is started by being passed as parameter to the default Electron
executable … even if the app is packaged", existing "to determine how many arguments
will need to be sliced off from `process.argv`". The Host runs the role in exactly that
shape, in `keld dev` and in the installed launch alike (the prebuilt Host still runs
`bun … run <entry>`), so Option A is true under both. The only `defaultApp` read in the
pinned draw.io main is the slicing decision (`:1059`); the issue's falsifier (a branch on
`defaultApp` for anything else) did not fire.

Rejected alternatives:

- Option B (rewrite `process.argv` to `[exe, ...user]`, leave `defaultApp` undefined).
  Rejected: it mutates a Node-owned array at an order-dependent moment, breaks
  `process.argv[1]` for every Node/Bun consumer that expects the script path, and buys
  nothing because the measured shape already matches `electron .`.
- Keeping the inline shim in `index.ts`. Rejected: no single owner a pre-entry preload
  can import, because `index.ts` pulls in `app` → `@keld/api` → the transport.
- Installing the facts from the already-preloaded transport sidecar
  (`src/kipc-transport.ts`). Rejected: Electron-isms in kipc (`crates/keld-compat/AGENTS.md`),
  the file is stamped and generated, and the dependency direction would invert.
- A throwing getter for `resourcesPath`. Rejected: dependency code calls
  `path.join(process.resourcesPath, …)` and probes `typeof`; a throwing property would
  crash roles that never needed the path. Absence is today's honest observable and
  matches the red cell.
- Deriving `resourcesPath` from `process.cwd()` or the entry directory. Rejected by the
  issue (no path heuristic); cwd is app-mutable and the value would be order-dependent.
- Reading the package version at runtime for `versions.electron`. Rejected: runtime
  file I/O at import for a constant; the test instead asserts the constant equals the
  package version, keeping one source of truth under test.

Compatibility fallback: facade-import installation is the published contract today
("`import "electron"` is enough to observe them") and remains in T-1. T-2 adds the earlier
moment through the same module; the facade import then finds identical descriptors and
does nothing (AC-6).

`resourcesPath` value contract (T-3). Electron's `process.resourcesPath` is the runtime's
resources directory, set natively (`node_bindings.cc:1231`): `Electron.app/Contents/
Resources` for `electron .`, `<App>.app/Contents/Resources` when packaged; packagers put
extra resources there and electron-updater reads `app-update.yml` from it when packaged.
Keld's honest equivalent is the launch root the Host staged and runs the role from:
under `keld dev`, `<project>/.keld/dev/<nonce>` (the directory holding `keld-host`,
`keld.boot.json` and the staged `src/`), not the developer's source tree; for the
installed launch, the bundle's resources directory as KEL-141/KEL-53 define it. The
value MUST NOT contain `app.asar` (Keld ships no asar), MUST be Host-minted (the #452
payload), and MUST NOT be derived in the role from `__dirname`, `process.cwd()` or an
environment variable.

### D-2: New/changed types & channels

```ts
// packages/@keld/electron/src/process-facts.ts (T-1)
export const ELECTRON_VERSION_SHIM = "0.0.1";
export const MAIN_PROCESS_TYPE = "browser";
/** Electron leaves these `undefined` in the main process; Keld never defines them. */
export const MAIN_ROLE_UNDEFINED_FLAGS: readonly [
  "mas", "windowsStore", "sandboxed", "contextIsolated", "isMainFrame",
];
/** `process` satisfies this; tests pass a plain object. */
export interface ProcessFactsTarget {
  readonly versions: object;
}
/** Installs `type`, `defaultApp` and `versions.electron`; idempotent; KELD-COMPAT-014 on incompatible descriptors or absent facts on non-extensible owners. */
export function installMainProcessFacts(target?: ProcessFactsTarget): void;
```

`index.ts` keeps its export set (`app`, `BrowserWindow`, `ELECTRON_VERSION_SHIM`,
`isCallError`, types) and replaces its inline shim block with
`import { ELECTRON_VERSION_SHIM, installMainProcessFacts } from "./process-facts"` plus
one `installMainProcessFacts()` call; the line `export { app, BrowserWindow };` stays
byte-identical because #569's export-mutation probe matches it.

T-3 adds one optional field to the installer input, filled from the #452 payload reader:

```ts
export interface MainRoleProcessFacts {
  /** Host-staged launch root from the session-facts payload (#452). */
  readonly resourcesPath?: string;
}
export function installMainProcessFacts(target?: ProcessFactsTarget, facts?: MainRoleProcessFacts): void;
```

- Capabilities required; manifest changes (spec 03): none.
- Wire/protocol changes (spec 02): none. T-2 changes no frame; it changes the Bun command
  line the Host builds (KEL-75 amendment, public API gate).
- Platform notes: the `bun … run <entry>` argv shape is the same on macOS, Windows and
  Linux, but only the macOS guardian path was read and measured; Windows
  (`windows_job`) and Linux (`linux_strict`, bubblewrap launcher) spawn paths are
  documented gaps for AC-2. `windowsStore` and `mas` stay `undefined` on every OS.
- Runtime seam: T-1 none (same execution owner, grants, crash domain; the facade import
  is the capture moment). T-2: the Host adds a second preload before the entry; no new OS
  grant, same process and crash domain, capture moment moves to spawn.
- Migration unit: callers of `ELECTRON_VERSION_SHIM` (re-exported unchanged);
  `fixtures/shim_env.ts` and `fixtures/lifecycle.ts` markers unchanged; #569's probes
  copy `electron/src` wholesale and keep working; no persisted state; no temporary
  adapter.

### D-3: Deferred T-2 pre-entry installation (KEL-75 role-spawn amendment, needs approval)

Proposal: `keld dev` (`crates/keld-cli/src/boot.rs`) stages a Keld-owned bootstrap
preload beside `src/kipc-transport.ts` under a fixed staged name, and the guardian
(`crates/keld-host/src/main.rs::guardian_bun_command`) passes it as a second
`--preload=` after the transport sidecar, with the same regular-file checks the sidecar
already gets. The preload imports only `process-facts` and calls
`installMainProcessFacts()`: no module resolution of application code (#502 owns
resolution), no second link or inherited handle, no source other than the payload. The
transport sidecar precedent (fixed staged filename, no descriptor field) is the smallest
shape; whether a `keld.boot.json` field is needed is §10 Q-2. Falsifier for the moment:
the electron-log-first fixture (AC-8) observing `undefined` in a real staged role.

## 5. Boundaries

- Implement in (T-1): `packages/@keld/electron/src/process-facts.ts`,
  `packages/@keld/electron/src/process-facts.test.ts`,
  `packages/@keld/electron/src/index.ts` (the shim block only; export names unchanged),
  `packages/@keld/electron/fixtures/process_facts_*.ts`,
  `docs/architecture/04-electron-compat.md` (§3 v0 paragraph, one sentence),
  `docs/engineering/compat-scoreboard.md` (process rows), regenerated `llms-full.txt`.
- Must not touch (T-1): `crates/*`, `packages/@keld/kipc/src/transport.ts`,
  `packages/@keld/api/*`, `crates/keld-compat/fixtures/*` and
  `tests/support/corpus_manifest.rs` (the #569 corpus and registry belong to PRs
  #1294/#1323), the `index.ts` export list, workspace `Cargo.toml`.
- T-2 touches `crates/keld-host/src/main.rs` and `crates/keld-cli/src/boot.rs` only
  after this spec's T-2 text is approved; T-3 touches the #452 payload reader only after
  #452 lands.

## 6. Tasks

### T-1: Single process-facts installer
- **Blocked by:** none

Implement Option A, Electron-matching descriptors, KELD-COMPAT-014, red-first tests,
fixtures, scoreboard rows and the architecture sentence. T-1 design was approved
on 2026-11-15. Implementation remains on this draft branch, unmerged.
Approval does not certify CI or native acceptance. Verify AC-1 through AC-8.

### T-2: Deferred pre-entry installation
- **Blocked by:** Q-2, B-1

After approval, implement the Host-staged second preload, KEL-75 amendment,
CLI staging and guardian command. After #569 lands, flip its early-type and
empty-user-args cells. Verify AC-9 with the real staged launch.

### T-3: Deferred authoritative resourcesPath
- **Blocked by:** Q-3, B-2

After #452 payload delivery, consume resourcesPath and flip the existing
process.resources-path.string cell. Verify AC-10.

## 7. Test plan & evidence

| AC | Test | Oracle |
|---|---|---|
| 1, 2 | `process-facts.test.ts` spawns `fixtures/process_facts_drawio_shape.ts` with the guardian flag shape, once with `gh570-alpha --disable-update` and once with no arguments | exact stdout lines: `GH570_DEFAULT_APP=true`, `GH570_EXEC_ARGV=[…define…]`, `GH570_ARGV1_IS_ENTRY=true`, `GH570_USER_ARGS=<the launcher's own list>`; exit 0 |
| 3, 4 | descriptor equality in-process after `import "./index"`; `ELECTRON_VERSION_SHIM` vs `package.json` version and vs `44.4.5` | `Object.getOwnPropertyDescriptor` values; file bytes |
| 5 | in-process `Object.hasOwn`/`typeof` over `MAIN_ROLE_UNDEFINED_FLAGS` and `versions.chrome`; `resourcesPath` absent today (T-3 inverts this case) | property census |
| 6 | second `installMainProcessFacts()` call; descriptors deep-equal before/after | no throw, equal descriptors |
| 7 | plain-object targets with foreign `type: "renderer"` (configurable and not), foreign `versions.electron: "44.4.5"`, same-value configurable and non-configurable `type`/`versions.electron`, and non-extensible compatible owners | `KELD-COMPAT-014:` on refusal without partial writes; adopt same-value configurable facts with native descriptors; permit compatible `defaultApp` |
| 8 | spawn `fixtures/process_facts_early_entry.ts` with and without `--preload=./process_facts_preload.ts` | `GH570_EARLY mode=main defaultApp=true facadeType=browser` with the preload; `mode=unknown defaultApp=undefined facadeType=browser` without |
| 9 | T-2: #569 `crates/keld-host/tests/no_flag_macos.rs` exact case, real macOS | the two cells flip in the corpus manifest |
| 10 | T-3: #569 `process.resources-path.string` plus a Keld rule test that the value has no `app.asar` segment and equals the payload field | corpus cell plus exact string |

Negative controls (one mutation each): defining `defaultApp` as `undefined` or dropping
the define fails AC-1–3; defining `sandboxed = false` fails AC-5; setting
`ELECTRON_VERSION_SHIM = "44.4.5"` fails AC-4 (and #569's honesty predicate); removing
the preload's import fails AC-8; moving installation back behind the facade's kipc
imports is invisible to T-1 but fails #569's `process.early.type` in T-2.

Anti-flake: fixtures run in child processes through `Bun.spawnSync` with a kill-switch
timeout only (no sleeps), cwd `packages/@keld/electron/fixtures` (its `tsconfig.json`
paths resolve `electron`), `KELD_APP_LINK` removed so no link is attempted; no ports;
no temporary files. `bun test` shares one module cache, so in-process cases read the
descriptors the single facade import produced.

### E-1: Isolated T1 focused tests
- **Run:** `bun test src/process-facts.test.ts` in the Electron package.
- **n:** 28 passed, 0 failed, 75 assertions.
- **Environment:** Linux cloud, Bun 1.4.4, exact Pin package snapshot.
- **Limits:** This does not prove native Host, installed bundles or T-2/T-3 acceptance.

## 8. Review gates triggered

- public API: yes — `process.defaultApp` becomes an observable contract of the Bun
  main role (no new exported names from `index.ts`); T-2 changes the trusted Bun command
  line (KEL-75).
- unsafe: none. permission model: none (no grant, no environment read). dependency
  addition: none. wire protocol: none.

## 9. Perf impact

none — three `Object.defineProperty` calls at import; no steady-state work, no I/O.

## 10. Open questions, blockers & unknowns

### Q-1: Approved for T-1: Option A for the `(argv, defaultApp)` atom (Bun argv kept,
  `defaultApp = true`, `execPath` keeps Node semantics) over Option B.
### Q-2: Deferred, not approved. T-2 mechanism: a second Host-selected `--preload` of a Keld-owned bootstrap staged
  by `keld dev` under a fixed name (transport-sidecar precedent, no `keld.boot.json`
  field) versus a bootstrap entry that imports the app entry. Who writes the KEL-75
  amendment text, and does KEL-96's closed schema v1 need a field?
### Q-3: Deferred, not approved. `resourcesPath` under `keld dev` is the launch root `<project>/.keld/dev/<nonce>`;
  the installed value follows the KEL-141/KEL-53 bundle layout — confirm with those
  owners before T-3.
### Q-4: Approved for T-1: `type` and `versions.electron` are configurable but non-writable,
  matching Electron's native `SetReadOnly`; only `defaultApp` is
  non-configurable. Any consumer that deletes/redefines the native facts?
  No such consumer was found in this repository. This corrects the earlier
  proposed non-configurable behavior; no T-2/T-3 approval is implied.
### Q-5: Approved for the current T-1 scope: `ELECTRON_VERSION_SHIM` stays the `@keld/electron` package version (`0.0.1`,
  outside the npm `electron` list, which starts at `0.1.0`); the test binds the two.
  No speculative future-version policy is approved here; #569's bounded-list
  honesty check remains separate evidence.
### Q-6: Keep T-1 on the existing draft branch, preserving commit history; no new
  spec-only PR or dropped implementation commits. This decision does not authorize merge.

### B-1: Deferred pre-entry integration
- **Status:** open
- **Owner:** GH-570 Host integration owner, coordinated with the #569 corpus owner.
- **Impact:** T-2 and AC-9 cannot be accepted from facade-import tests.
- **Check:** Resolve Q-2, land the #569 cells, then run the real staged macOS early-dependency test.

### B-2: Deferred resourcesPath integration
- **Status:** open
- **Owner:** GH-570 integrator with #452 and installed-bundle owners.
- **Impact:** T-3 and AC-10 remain deferred. No authoritative resourcesPath is installed by T-1.
- **Check:** Resolve Q-3 and #452 payload delivery, then verify exact payload/path equality.
