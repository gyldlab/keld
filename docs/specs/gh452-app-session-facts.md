# Spec: `app` session facts — fail-closed facade half and Host-minted delivery (GH-452)

Status: draft
Linear: GH-452 (#517) · Owner: @0monish · Updated: 2026-11-01
Disposition: **DRAFT / DO NOT MERGE.** Implementation commits for T1 ride behind this
spec in the same draft PR for review only. Exact-text approval of this document is the
gate for merging T1; T2 and T3 additionally need the KEL-96 schema owner and the KEL-75
spawn owner decisions recorded in §10.

## 1. Goal & non-goals

drawio-desktop's main process reads `app.getPath('userData')` and `app.getVersion()` at
import (`new Store()` at `electron.js:30`, inside a try/catch), `app.getPath('userData')`
at `:58` and `:3796`, `app.getVersion()` at `:70`, `app.getLocale()` at `:767`, `app.name`
at `:2020`/`:2023`, `app.getPath('documents')` at `:4175`, and
`process.env.DRAWIO_DISABLE_UPDATE` at `:75` (pinned desktop source, GH499 §table).
Electron derives these from the loaded application's `package.json`, from Chromium's
locale and from the packaging state. Under Keld they are **session facts** owned by the
Host (issue #452): the Host owns the launch root, the compiled descriptor `name`, every
filesystem path grant and the webview engine whose locale the renderer sees.

Outcome of this spec: the `app` facade exposes the seven members Electron code calls
(`getPath`, `getVersion`, `getName`, `name`, `getAppPath`, `getLocale`, `isPackaged`),
and each one fails closed with a typed `KELD-COMPAT-040` error naming the missing fact
and this issue whenever the Host has delivered no session facts, which at
`origin/main` `b72e4c19` is every launch. `getPath` additionally rejects a non-string or
undocumented name with `KELD-COMPAT-041`; `name` and `isPackaged` are non-enumerable
accessors as in Electron, and assigning `app.name` (Electron's `setName`, a non-goal) is
refused with `KELD-COMPAT-042`. No value is derived in the Bun role from `HOME`,
`APPDATA`, `XDG_*`, `LANG`/`LC_*`, `argv`, `cwd`, `Intl` or any `package.json`, and an
observer with a recording `process.env` Proxy proves it. The Host-minted delivery (T2) and the declared role environment (T3) are
specified here but blocked on owner decisions.

Non-goals:

- Minting any session-fact value in the Bun role (the whole point of T1 is that it does
  not); `process.*` observables (#570); the module exports map (#565); single-instance
  lock (#453); `setPath`/`setAppLogsPath` (F01-T11); strict-profile `userData` (F01-T9);
  `getSystemLocale`/`getPreferredSystemLanguages`/`setName`; filesystem authority over
  any returned path (keld-guard owns grants); the `isPackaged` true half (KEL-141).

## 2. Spec refs

- `docs/architecture/04-electron-compat.md` §3 (the `app` facade delegates to the
  generic API; Electron names and listener behaviour live in `@keld/electron`).
- `docs/architecture/03-security.md` §1 (principals are Host-minted) and §2 (filesystem
  paths are grants in `keld.permissions.jsonc`; the role has no path authority of its own).
- `docs/specs/kel75-principalized-bun-child-roles.md` (`KELD_APP_LINK` is the sole child
  bootstrap variable; requested `env` is applied per key through `RoleConfig::env`,
  `crates/keld-runtime/src/role.rs:456`).
- `docs/specs/kel96-no-flag-host-boot.md` §4.1 (`KEL-96-D3`: no-flag boot reads only the
  compiled `keld.boot.json` schema v1, fields `schema,name,entry,renderer,permissions`;
  `KEL-96-D11`: `keld-core` owns boot validation).
- `docs/specs/gh532-first-proof-evidence-rules.md` rule 3 (red-until-implemented cells)
  and `docs/specs/gh566-corpus-manifest-owner.md` (the corpus owner and exact-bytes digest).
- GH497 (`docs/specs/gh497-first-proof-migration-output.md`, `Status: approved` on the
  PR #1172 branch; not on `origin/main` at `b72e4c19`): the declared role environment
  is "BLOCKED until #452 approves/implements key". GH499
  (`docs/specs/gh499-drawio-hand-authored-migration.md`, approved on the PR #1170
  branch, likewise not on `origin/main`) A08/A13: exactly `DRAWIO_DISABLE_UPDATE=true`,
  and "spec proposals are not permission to guess or invent a field in an unmerged
  parser".

T1 deviates from no architecture section (`No boundary change`: no handle ownership,
crash ownership or principal minting moves). T2 amends KEL-75 and KEL-96 and must update
both specs in its own PR.

## 3. Acceptance criteria (binary, each becomes a test)

T1 (this PR). "Not delivered" means the Host passed no session-facts payload, which is
every role at `b72e4c19`.

1. Given session facts are not delivered, when `app.getPath('userData')` is called at
   module evaluation (before `whenReady`), then it throws an `Error` whose `code` is
   `KELD-COMPAT-040`, whose `fact` is `path.userData`, and whose message names
   `app.getPath('userData')`, `path.userData` and `GH-452`.
2. Same state, `app.getVersion()` throws `KELD-COMPAT-040` with `fact` `version`.
3. Same state, `app.getName()` and a read of `app.name` each throw `KELD-COMPAT-040`
   with `fact` `name`; `name` is a non-enumerable, non-configurable accessor with a
   setter, as Electron defines it (`lib/browser/api/app.ts` at 694f4585), not a data
   property, so `{ ...app }`, `Object.entries(app)` and `JSON.stringify(app)` do not
   invoke it.
4. Same state, `app.getAppPath()` throws `KELD-COMPAT-040` with `fact` `appPath`.
5. Same state, `app.getLocale()` called after the facade `ready` event (the pinned
   app.md: "This API must be called after the `ready` event is emitted.") throws
   `KELD-COMPAT-040` with `fact` `locale`, observed in a child process that reaches
   `ready` through a `ready` listener.
6. Same state, a read of `app.isPackaged` throws `KELD-COMPAT-040` with `fact`
   `isPackaged`; it is neither `false` nor `undefined`, and it is a non-enumerable
   accessor without a setter (Electron's read-only property).
7. Error case: `app.getPath('bogus')` throws `KELD-COMPAT-041`, whose message names
   `bogus`, says the name is not *documented*, and lists the documented names. The
   documented list equals, in order, the names under `### app.getPath(name)` in the
   pinned v44.4.5 snapshot
   `crates/keld-compat/fixtures/doc-snapshots/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md`
   (`home` through `crashDumps`, 17 names). Electron's source at the same commit also
   accepts `cache`, `userCache` and `userDesktop`, excludes `assets` on macOS and
   `recent` outside Windows; the message must not claim Electron rejects a name, and
   whether the payload carries the undocumented names is §10 Q9.
8. Negative control (decoy environment and environment-read oracle): given an
   observer that replaces `process.env` with a recording Proxy and wraps `process.cwd`
   before any facade module loads, spawned with `HOME`, `XDG_CONFIG_HOME`, `APPDATA`,
   `LANG` and `LC_ALL` set to decoy values, when each of the seven members (plus
   `getPath('documents')`) is read at import and again after the facade `ready` event,
   then every outcome in both phases is `KELD-COMPAT-040` with no string or boolean,
   and the recorded reads of `HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`,
   `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `LANG`, `LC_ALL`, `LC_MESSAGES`, `TMPDIR` and of
   the cwd are empty. A facade that reads `HOME` even into an error message, derives
   `name`/`version` from a `package.json`, `locale` from `Intl`, `isPackaged` from a
   constant, or mints any value once `isReady()` is true fails it.
9. Error cases on the call itself: assigning `app.name` (string or Symbol) throws
   `KELD-COMPAT-042` naming the value and `setName`, and the next read still fails
   closed with `040`; `app.getPath` with a non-string argument (`undefined`, `null`, a
   number, a Symbol, an object) throws `KELD-COMPAT-041` naming the received type and
   never a second untyped `TypeError`.
10. The seven `electron-app-v1` cells `app.get-path.user-data-default`,
   `app.get-version.loaded-app`, `app.get-name.package-name`, `app.name.property`,
   `app.get-app-path.app-directory`, `app.get-locale.after-ready` and
   `app.is-packaged.dev-false` keep `expected_verdict: fail` with
   `implementing_ticket: GH-452`; each `test_name` equals the re-recorded today-test that
   asserts criteria 1–6 (read before `ready`; the negative-control texts say so and
   point at criterion 8 for the after-`ready` read); `denominator.json`
   `corpus_sha256` equals the SHA-256 of the exact `corpus.json` bytes. Flipping any
   of them to `pass` in T1 is rejected.
11. `KELD-COMPAT-040`, `041` and `042` are registered in
    `docs/engineering/keld-error-codes.md` with `crate`, `message` and `fix`, and
    `cargo test -p keld-cli --test error_registry` passes.

T2 (Host-minted delivery; blocked on §10 Q1–Q4). Restated from the issue so a fresh
agent can bind tests; values are decided by the owners named in §10.

12. Given a role spawned by the Host with the session-facts payload, when
    `app.getPath('userData')` is called at module evaluation, then it returns
    synchronously the Host-computed Electron default (`appData` + app name) for the
    legacy macOS profile; the facade reads no environment variable to produce it.
13. `app.getVersion()`, `app.getName()`, `app.name`, `app.getAppPath()` return the
    payload values synchronously at module evaluation; `app.getLocale()` returns the
    payload locale after `ready`.
14. `app.isPackaged` is `false` under `keld dev`; its true-half cell is recorded
    `unknown` naming KEL-141. Hard-coding `true` fails the dev cell; a constant `false`
    is rejected at review because the payload must carry the value.
15. Negative controls: removing payload delivery makes criterion 12's cell throw
    `KELD-COMPAT-040` (the T1 branch); spawning with decoy `HOME`/`XDG_CONFIG_HOME`
    leaves every returned path equal to the payload.
16. The payload's user-argument vector is byte-equal to the arguments given to
    `keld dev` (payload-level observable; #570 owns `process.argv`).
17. The seven cells flip to `pass` with inverted today-tests in the same change
    (gh532 rule 3), and no `implementing_ticket` remains on them.

T3 (declared role environment; blocked on §10 Q5). Restated from the issue.

18. A declared key is visible in `process.env` at module evaluation when the operator
    shell does not set it; for first proof the reviewed list is exactly
    `DRAWIO_DISABLE_UPDATE`.
19. Each reserved-floor fixture (`KELD_APP_LINK`, `NODE_OPTIONS`, `BUN_OPTIONS`,
    `NODE_TLS_REJECT_UNAUTHORIZED`, `DYLD_INSERT_LIBRARIES`, `LD_PRELOAD`, `https_proxy`)
    and every non-literal name (`DRAWIO_*`, a name containing `=`) is rejected before
    spawn with a typed `KELD-*` error naming the key; no role process is created.
20. The generated role-policy record and every guard decision are identical with and
    without the declared environment.

## 4. Design

### 4.1 First-principles facts (ownership, trust, lifecycle, I/O, failure)

| Member | Electron source of the value | Keld owner of the value | Why the Bun role cannot mint it honestly at `b72e4c19` |
|---|---|---|---|
| `getPath(name)` | Chromium path service; `userData` = `appData` + app name | Host: paths are grants (`03-security.md` §2); `appData` needs `HOME`, which the issue forbids the facade to read | Reading `HOME`/`XDG`/`APPDATA` mints filesystem authority outside the Host and is decoy-sensitive; `name` is itself undelivered |
| `getVersion()` | loaded app's `package.json` `version` | Host via a schema-v1 field (`version` does not exist in schema v1; adding it is a KEL-96/KEL-15 change) | `keld dev` stages only the entry and the kipc sidecar (`crates/keld-cli/src/boot.rs:475-482`); no `package.json` is staged; walking up from the staged entry crosses the launch root (a layout coincidence under dev, denied under strict `/app`) |
| `getName()` / `name` | `package.json` `name` or `productName` | Host: the compiled descriptor `name` (issue text; `boot.rs:521`) | The role receives no descriptor value; parsing `keld.boot.json` in the role would be a duplicate parser of a Host-owned schema (GH499 A18) |
| `getAppPath()` | app directory | Host: the launch root it staged | `dirname(Bun.main)` is `src/`, not the app directory; the launch root is not delivered |
| `getLocale()` | Chromium `l10n_util` UI locale | Host: the webview engine's locale | `Intl.DateTimeFormat().resolvedOptions().locale` is the Bun process's ICU default; measured `en-US` on Bun 1.4.4/macOS 25.5 under `LANG=de_DE.UTF-8` and under `LC_ALL=fr_FR.UTF-8` (system `AppleLanguages` is `en-US`, so tracking of the macOS preference was not distinguishable from a constant without changing the user's system setting). No contract binds it to the Host webview |
| `isPackaged` | packaging state | Host: knows dev lease vs installed provenance (`KELD_DEV_LEASE`, KEL-254) | Dev vs installed is indistinguishable from inside the role; no installed artifact exists (KEL-141, KEL-103), so a constant `false` is a stub whose cell would prove only the stub |
| `DRAWIO_DISABLE_UPDATE` | app-declared env | Host spawn contract (KEL-75 amendment, permission-model gate) | A facade write to `process.env` would be a second environment path; GH497 M06 and GH499 A08 bind it to the Host declaration |

Failure facts: a throw at `electron.js:30` is caught by draw.io's own try/catch (`store`
becomes `null`); the reads at `:58`/`:70` then throw `KELD-COMPAT-040` inside
`whenReady`, replacing today's `TypeError`. Nothing in T1 can crash independently of the
role; no handle, queue or copy is introduced.

### 4.2 Reuse before rewrite

- Error shape: the facade's existing typed refusals are `Error` values whose message
  begins with the code (`browser-window.ts:127`, `489`; `app.ts` `KELD-COMPAT-012`) and
  carry no `code` field; the public typed shape elsewhere is
  `KeldCallError = Error & { code: string }` (`packages/@keld/kipc/src/transport.ts:1471`,
  recognised by `isCallError`). T1 throws an `Error` subclass that keeps the message
  prefix and additionally carries `code` and `fact`, so it satisfies both conventions.
  That is a superset of the older facade errors, not a second family; giving 010–012 a
  `code` field is parked as adjacent cleanup.
- Closed name set: the pinned v44.4.5 `app.md` snapshot already committed for the app
  corpus is the oracle for the 17 `getPath` names; the test parses it rather than
  copying the list a second time.
- Delivery precedent for T2: the Host already mints one immutable launch fact into the
  role with a Bun define, `--define=process.env.KELD_NATIVE_WINDOW_FACADE:"1"`
  (`crates/keld-host/src/main.rs:491-495`, consumed at
  `packages/@keld/kipc/src/transport.ts:32`). The per-key `RoleConfig::env` setter
  exists for T3. T2 must choose one carrier (§10 Q1) and must not add a second
  environment path (issue text).
- Rejected alternatives, each recorded with the reason: derive `userData` from
  `~/Library/Application Support/<name>` in Bun (authority minted outside the Host;
  reads `HOME`; needs undelivered `name`); read `package.json` by walking up from the
  entry (not a contract; crosses the launch root; decoy-sensitive); `Intl` locale
  (Bun-process observable, not the webview locale; see the measurement above); constant
  `isPackaged = false` (stub; its cell would prove the constant); parse `keld.boot.json`
  in the role (duplicate parser of a Host-owned schema); a `KELD_*` environment carrier
  for the payload (conflicts with KEL-75's sole bootstrap variable unless that spec is
  amended, which is Q1).

### 4.3 Types (T1, `packages/@keld/electron/src/app-session-facts.ts`)

```ts
export const ELECTRON_PATH_NAMES: readonly [
  "home", "appData", "assets", "userData", "sessionData", "temp", "exe", "module",
  "desktop", "documents", "downloads", "music", "pictures", "videos", "recent", "logs",
  "crashDumps",
];
export type ElectronPathName = (typeof ELECTRON_PATH_NAMES)[number];
export type AppSessionFact =
  | "name" | "version" | "appPath" | "locale" | "isPackaged" | `path.${ElectronPathName}`;
export type AppSessionFactCode =
  | "KELD-COMPAT-040" | "KELD-COMPAT-041" | "KELD-COMPAT-042";
export class AppSessionFactError extends Error {
  readonly code: AppSessionFactCode;
  /** The missing fact for 040; undefined for 041/042 (the call itself is the defect). */
  readonly fact: AppSessionFact | undefined;
}
/** Fail-closed read: throws 040 until a Host payload exists (T2 replaces the body). */
export function readSessionFact(fact: AppSessionFact, call: string): never;
/** Non-string or undocumented `name` → 041; otherwise reads `path.<name>` (040). */
export function readSessionPath(name: string): never;
export interface AppSessionFactAccessors { name: string; readonly isPackaged: boolean }
/** Defines `name` (get + refusing set) and `isPackaged` (get) as non-enumerable accessors. */
export function defineSessionFactAccessors<T extends object>(target: T): T & AppSessionFactAccessors;
```

`app.ts` adds `getPath(name)`, `getVersion()`, `getName()`, `getAppPath()` and
`getLocale()` to the existing `app` object literal and wraps the literal in
`defineSessionFactAccessors`, which installs `name` and `isPackaged` as non-enumerable
accessors. The listener table is untouched; `index.ts` is not edited (#565 owns
exports).

Public contract of T1: the seven member names and the runtime `code`/`fact` fields on
the thrown `Error` (readable through `isCallError` and `.code`). `AppSessionFactError`
and its types are reachable from `@keld/electron/src/app-session-facts.ts` but are not
exported from the package entry; adding them to the exports map belongs to #565.

T2 sketch (for the owners, not implemented here): a read-only `SessionFacts` record
`{ name, version, appPath, locale, isPackaged, paths: Record<ElectronPathName, string>,
userArgs: string[] }` minted by the Host before spawn and frozen in the role before the
app entry evaluates; `readSessionFact` returns the field or throws 040 when the record
is absent.

- Capabilities required; manifest changes: none in T1. T2 adds no capability (facts are
  not grants); T3 adds the declared-environment field (permission-model gate).
- Wire/protocol changes: none in T1. T2 is a `keld.boot.json` schema-v1 extension plus a
  KEL-75 spawn amendment (version bump + wire/public-contract review gates).
- Platform notes: macOS first. `recent` is accepted as a *name* on every OS because the
  pinned list contains it; which names have a value on which OS is the payload owner's
  decision (T2). Linux/Windows path values and argv shape stay unknown (issue text).
- Runtime seam: none in T1 (no execution owner changes; the throw happens in the role
  on the caller's stack). T2 moves fact minting to the Host before spawn: OS grants
  unchanged, crash domain unchanged, config captured at spawn.
- Migration unit: callers are the draw.io sites in §1 and the import-time reads of
  `electron-store` (`getPath('userData')`, `getVersion()`) and `electron-log`
  (`name`/`getName`/`getPath`/`getVersion`/`isPackaged`, timing unknown). No adapter;
  the permanent facade is the `app` object.

## 5. Boundaries

- Implement in (T1): `packages/@keld/electron/src/app.ts`,
  `packages/@keld/electron/src/app-session-facts.ts` (new),
  `packages/@keld/electron/src/app-session-facts.test.ts` (new),
  `packages/@keld/electron/src/app-surface.test.ts`,
  `packages/@keld/electron/fixtures/app_surface_locale_after_ready.ts`,
  `packages/@keld/electron/fixtures/app_session_facts_decoy_env.ts` (new),
  `crates/keld-compat/fixtures/app-corpus/corpus.json` and `denominator.json`,
  `docs/engineering/keld-error-codes.md`.
- Must not touch: `packages/@keld/electron/src/index.ts` (#565), any `process.*`
  installation (#570), `crates/keld-runtime/src/role.rs`,
  `crates/keld-core/src/app_session.rs` (#449), `crates/keld-cli/src/boot.rs`,
  `crates/keld-guard/**`, `keld.boot.json` schema, workspace `Cargo.toml`.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 — Fail-closed facade half: the seven members, `KELD-COMPAT-040`/`041`/`042`,
  the re-recorded red cells and the environment-read negative control (criteria 1–11).
  This PR, draft, do-not-merge until this text is approved.
- [ ] T2 — Host-minted session facts: KEL-96 schema-v1 extension and KEL-75 spawn
  amendment approved by their owners; Host mints and delivers the payload; facade reads
  it; the seven cells flip (criteria 12–17). Blocked on Q1–Q4 and Q9.
- [ ] T3 — Declared role environment: literal-name validation, reserved-name floor,
  `RoleConfig::env` application, policy-identity proof (criteria 18–20). Blocked on Q5;
  permission-model gate.
- [ ] T4 — `isPackaged` true half after KEL-141 (an installed macOS artifact); stays
  `unknown` until then.

## 7. Test plan

| Criterion | Test | Kind |
|---|---|---|
| 1–4, 6 | `app-surface.test.ts` red-cell today-tests (exact names recorded in `corpus.json`) | Bun unit, in-process |
| 5 | `app-surface.test.ts` via observer `fixtures/app_surface_locale_after_ready.ts` (reaches `ready` through a `ready` listener; prints one `GH445_LOCALE_AFTER_READY` line) | Bun, child process |
| 7 | `app-session-facts.test.ts`: `getPath('bogus')` is 041 and says "documented"; `ELECTRON_PATH_NAMES` equals the names parsed from the pinned snapshot | Bun unit; oracle is the committed doc bytes |
| 8 | `app-session-facts.test.ts` via `fixtures/app_session_facts_decoy_env.ts` (recording `process.env` Proxy and `process.cwd` counter installed before any facade import; reads at import and after `ready`; decoy `HOME`, `XDG_CONFIG_HOME`, `APPDATA`, `LANG`, `LC_ALL`) | Bun, child process |
| 9 | `app-session-facts.test.ts`: `app.name = …` is 042 (string and Symbol); non-string `getPath` arguments are 041 naming the type | Bun unit |
| 10 | `cargo test -p keld-compat` corpus registry (gh566 owner): digest, red-cell keys, exact Bun case names | Rust integration |
| 11 | `cargo test -p keld-cli --test error_registry` | Rust integration |
| 12–20 | Owned by T2/T3; bound when their specs are approved | — |

Anti-flake: observers use the existing `OBSERVER_KILL_MS` (4 s) as a kill switch only
and exit on their own; no sleeps; the decoy environment is passed through `Bun.spawnSync`
`env`, never set in the test process; `LifecycleLink.connect` is replaced only inside
observer children (bun test shares one module cache per process).

Failure-first proof: the re-recorded today-tests are committed before the facade change
and fail against the unchanged facade (`TypeError`/`undefined` instead of
`KELD-COMPAT-040`); the implementation commit turns them green. The module-contract
file cannot load at that commit (its module does not exist yet), so criteria 7–9 are
proved by mutation instead, recorded in the PR: a `getPath` that reads `process.env.HOME`
into its message fails criterion 8 through the recorded reads; members that return
values once `isReady()` is true fail criterion 8 in the after-`ready` phase; removing the
non-string guard fails criterion 9 with an untyped `TypeError`.

## 8. Review gates triggered

- public API: yes (seven new `app` members, three `KELD-COMPAT` codes, and the runtime
  `code`/`fact` fields on the thrown error; the class itself is not exported from the
  package entry, see §4.3).
- permission model: none in T1 (no grant or policy change); T3 triggers it.
- wire protocol: none in T1; T2 triggers it (schema-v1 field, spawn amendment).
- dependency addition: none.
- unsafe: none.

## 9. Perf impact

none. Cold path; each member throws once on the caller's stack. No kipc frame, no
steady-state allocation.

## 10. Open questions

1. **Payload carrier (KEL-75 owner).** Bun `--define` constants (precedent:
   `KELD_NATIVE_WINDOW_FACADE`), a Host-written file in the launch root read before the
   entry, or a kipc pre-entry frame? The issue requires delivery "beside KELD_APP_LINK"
   without a second environment path. Recommendation: the define carrier for immutable
   scalars, because it already exists and cannot be altered by a runtime config
   (`KELD-CORE-037` refuses staged `bunfig.toml`).
2. **Schema v1 `version`/`productName` (KEL-96/KEL-15 owner).** Add both fields, or
   derive `version` from `package.json` at `keld migrate`/`keld dev` compile time only
   (Host-side) and keep `productName` out?
3. **`name` semantics.** Electron prefers `productName` over `name`. Is the descriptor
   `name` the Electron `app.name`, and does `getName()` equal `name` with no
   `productName` (the #656 cell negative controls assume so)?
4. **`getLocale` source and pre-ready behaviour.** Host webview locale or the OS
   preferred language, and what a pre-ready call returns once the payload exists?
5. **Declared role environment grammar.** Exact literal-name grammar, reserved floor,
   and the Bun-honoured environment names to add to the floor (the issue records this
   as UNKNOWN; cite Bun's primary documentation before approval).
6. **Throwing accessors.** `app.name` and `app.isPackaged` throw on read in T1.
   `electron-is-dev` reads `app.isPackaged` at import; today that read yields
   `undefined` (silently "dev"). Accept the typed throw as the honest first-proof
   behaviour, or keep `isPackaged` absent until T2?
7. **Code numbering.** The registry's "Adding a code" rule says next free number;
   `040`–`042` were taken from the upper-middle of the free range so #565/#570 can take
   `013+` without collision (`origin/agent/gh-570-process-facts-20261031` already uses
   `013`). Accept or renumber at approval.
8. **Rewriting a red cell's today-test without flipping it.** gh532 rule 3 says the
   implementing change inverts the mapped test and flips the cell in one PR; it does
   not say whether the same ticket may first re-record the today-test (here: from
   "throws a TypeError" to "throws KELD-COMPAT-040") while the cell stays `fail`.
   This PR does that. Accept as the rule's intended reading for a two-step ticket, or
   require the cells' texts to stay as at #656 until T2 flips them.
9. **Undocumented `getPath` names.** Electron's source accepts `cache`, `userCache`
   and `userDesktop`, which the pinned page does not document, and limits `assets` and
   `recent` by platform. T1 treats the documented list as the oracle. Should the T2
   payload carry the undocumented names, and per-platform absence?
