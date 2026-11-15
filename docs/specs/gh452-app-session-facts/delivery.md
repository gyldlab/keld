# GH452 delivery tasks, verification and unresolved decisions

Part of: [index](index.md)
Owns: sections 4–7, 9–10

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

## 6. Tasks

### T-1: Approved T1 fail-closed facade slice

- **Blocked by:** none

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

## 7. Test plan & evidence

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

## 9. Perf impact

none. Cold path; each member throws once on the caller's stack. No kipc frame, no
steady-state allocation.

## 10. Open questions, blockers & unknowns

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
