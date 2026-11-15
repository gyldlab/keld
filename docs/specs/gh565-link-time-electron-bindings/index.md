# Spec: link-time Electron named bindings with typed unsupported members (GH-565)

- Status: approved
- Tracker: GH-565
- Owner: @0monish
- Updated: 2026-11-15
- Pin: 98b0b84c19ea3765f75efd0db614d53019a83fc2

The [owner's 2026-11-15 approval](https://github.com/gyldlab/keld/pull/1366#issuecomment-6893253680) accepts the corrected
GH-565 design, platform census, and ClipboardItem admission contract.
This approval does not approve the implementation or authorize merging.

## 1. Goal & non-goals

draw.io's main process is an ES module whose first statement imports twelve named
bindings from `electron` (`src/main/electron.js` lines 5–6 at upstream commit `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`):
`Menu, shell, dialog, session, screen, ClipboardItem, clipboard, nativeImage, nativeTheme,
ipcMain, app, BrowserWindow`. `@keld/electron` exports two of them. Under Bun 1.4.4 a
named import of a missing export fails module linking with
`SyntaxError: Export named '<name>' not found in module '…/index.ts'.` and exit 1
(re-measured 2026-10-31 on this branch's base), so nothing of draw.io evaluates. This
spec exports nine of the ten missing bindings so that import links and evaluates, with
default-export parity and no Host traffic at module evaluation; the tenth, `ipcMain`, is
exported by the GH-466 registration slice (PR #1365), not by this spec (D-5). Every member
available on the executing platform that Keld does not implement raises one typed error,
`KELD-COMPAT-013`, that names the member and its tracking issue, using Electron's
documented return kind: synchronous members throw, Promise-returning members return a
rejected Promise, properties throw on read, and constructors throw.
Absent and own-`undefined` members preserve the shapes in §4.3 instead of refusing.
`new ClipboardItem(items)` constructs for inputs admitted by AC-8. Once this spec and
PR #1365 have both landed, draw.io's import statement links, and the first unsupported
member of these nine bindings that it touches fails with a typed error naming its owner
instead of a link-time `SyntaxError` or a bare `TypeError`. This is the F06 epic's L0
rung (#476 § Maturity ladder): nothing silently succeeds.

Non-goals:

- Behaviour for any exported member. Owners: dialog GH-478, GH-564, GH-561; Menu GH-480,
  GH-556, GH-557; screen and nativeTheme GH-481, GH-558; shell GH-479, GH-482;
  clipboard, ClipboardItem and nativeImage GH-559; session GH-553 (`defaultSession`,
  `webRequest`) and GH-474
  (`fromPartition`, `fromPath`). Members that no ticket names yet are listed under
  "Not yet specified" on #476 and refuse naming #476.
- A recorded no-op anywhere. #553 allows `webRequest.onHeadersReceived`/`onBeforeRequest`
  as recorded ▲ no-ops only on a lane where GH-473's containment and CSP have landed;
  no such lane exists, and #553 itself says every other lane throws a typed error.
- `ipcMain` in any form. The GH-466 registration slice (PR #1365) exports it; the
  reconciliation between that slice and this spec is recorded on #466 (2026-10-31,
  "Delegated design decision" comment) and in D-5. This spec declares no `ipcMain`
  binding, cell, snapshot or member row.
- `MenuItem`. draw.io does not import it by name, and `native-surface.test.ts` holds the
  GH-480 transition gate for the GH-563 role-quit cell, which trips when `MenuItem`
  appears. GH-480 owns that re-record.
- `electron` specifier resolution, the package `exports` map, CJS `require` and the
  installable graph (GH-502, GH-569, GH-1287). The package today has one ESM entry (`"."`);
  default and named parity is the ESM contract this spec can test.
- Scoreboard rows (F06-T6, #482) and new red cells for member behaviour (F06-T1 #477,
  F06-T3 #479, F05-T1 #470, F04-T1 #464 own those).

## Parts

| Part | Owns | Load when |
|---|---|---|
| [contract.md](contract.md) | 2. Spec refs; 3. Acceptance criteria; 4. Design | Implementing or reviewing Electron compatibility behavior |
| [delivery.md](delivery.md) | 5. Boundaries; 6. Tasks; 7. Test plan and evidence; 9. Perf impact; 10. Blockers | Testing, integrating or closing the PR |

## Current blockers

B-1
B-2

## 8. Review gates triggered

public API (new named exports and the `UnsupportedMemberError` type). `unsafe`: none.
permission model: none. dependency: none. wire protocol: none.
