# Spec: `app` session facts — fail-closed facade half and Host-minted delivery (GH-452)

- Status: approved
- Tracker: GH-452
- Owner: @0monish
- Updated: 2026-11-15
- Pin: 249e7a64d9b7732cf2f1ab629ff1b2e02810d4b5
- Approved scope: T1, T2a, version/path T2b and locale T2c; full T2/T3 pending.
Disposition: **DO NOT MERGE until independent review and all execution gates pass.**
The owner delegated technical decisions and continuation on 2026-11-01; the bounded
decision is recorded on GH452 comment 6549729612. That delegation approves the T1
contract and T2a in §11, not independent review, execution evidence or full T2/T3.
PR1362 comment 6550291308 extends the delegated decision to §12 after independent
architecture review; exact implementation review and execution gates remain required.


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

## Parts

| Part | Owns | Load when |
|---|---|---|
| [contract.md](contract.md) | GH452 session-facts contract and scoped T2 amendments | Inspect approved acceptance and owning design |
| [delivery.md](delivery.md) | GH452 delivery tasks, verification and unresolved decisions | Implement, test or verify scoped delivery |

## Current blockers

- B-1

## 8. Review gates triggered

- public API: yes (seven new `app` members, three `KELD-COMPAT` codes, and the runtime
  `code`/`fact` fields on the thrown error; the class itself is not exported from the
  package entry, see §4.3).
- permission model: none in T1 (no grant or policy change); T3 triggers it.
- wire protocol: none in T1; T2 triggers it (schema-v1 field, spawn amendment).
- dependency addition: none.
- unsafe: none.
