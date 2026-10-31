# Spec: link-time Electron named bindings with typed unsupported members (GH-565)

Status: draft
Linear: GH-565 (#517; Linear reference KEL-127) · Owner: @0monish · Updated: 2026-10-31

## 1. Goal & non-goals

draw.io's main process is an ES module whose first statement imports twelve named
bindings from `electron` (`src/main/electron.js:5-6` at `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`):
`Menu, shell, dialog, session, screen, ClipboardItem, clipboard, nativeImage, nativeTheme,
ipcMain, app, BrowserWindow`. `@keld/electron` exports two of them. Under Bun 1.4.4 a
named import of a missing export fails module linking with
`SyntaxError: Export named '<name>' not found in module '…/index.ts'.` and exit 1
(re-measured 2026-10-31 on this branch's base), so nothing of draw.io evaluates. This
spec exports the ten missing bindings so that import links and evaluates, with
default-export parity and no Host traffic at module evaluation. Every member of those
bindings that Keld does not implement raises one typed error, `KELD-COMPAT-013`, that
names the member and its tracking issue, using Electron's documented return kind:
synchronous members throw, Promise-returning members return a rejected Promise,
properties throw on read, and constructors throw. `new ClipboardItem(items)` constructs.
The observable outcome: draw.io's import statement links; the first unsupported member it
touches (`ipcMain.on` at `electron.js:3426`) fails with a typed error that names GH-466
instead of a link-time `SyntaxError` or a bare `TypeError`. This is the F06 epic's L0
rung (#476 § Maturity ladder): nothing silently succeeds.

Non-goals:

- Behaviour for any exported member. Owners: dialog GH-478, GH-564, GH-561; Menu GH-480,
  GH-556, GH-557; screen and nativeTheme GH-481, GH-558; shell GH-479, GH-482;
  clipboard, ClipboardItem and nativeImage GH-559; ipcMain GH-466 (listeners) and GH-547
  (`handle` family); session GH-553 (`defaultSession`, `webRequest`) and GH-474
  (`fromPartition`, `fromPath`). Members that no ticket names yet are listed under
  "Not yet specified" on #476 and refuse naming #476.
- A recorded no-op anywhere. #553 allows `webRequest.onHeadersReceived`/`onBeforeRequest`
  as recorded ▲ no-ops only on a lane where GH-473's containment and CSP have landed;
  no such lane exists, and #553 itself says every other lane throws a typed error.
- An `ipcMain` registration table or `IpcMainEvent`. #466's reconciled contract (its
  2026-10 "Selected minimum contract" comment) is an EventEmitter registry separate from
  the `@keld/api` invoke map, delivered over GH-540/GH-541 routes; a registry without
  delivery would be a stub.
- `MenuItem`. draw.io does not import it by name, and `native-surface.test.ts` holds the
  GH-480 transition gate for the GH-563 role-quit cell, which trips when `MenuItem`
  appears. GH-480 owns that re-record.
- `electron` specifier resolution, the package `exports` map, CJS `require` and the
  installable graph (GH-502, GH-569, GH-1287). The package today has one ESM entry (`"."`);
  default and named parity is the ESM contract this spec can test.
- Scoreboard rows (F06-T6, #482) and new red cells for member behaviour (F06-T1 #477,
  F06-T3 #479, F05-T1 #470, F04-T1 #464 own those).

## 2. Spec refs

- `docs/architecture/04-electron-compat.md` §1 ("unsupported calls fail loudly with
  fix-its") and §3 layers 1-2 (module alias; main-process modules as TypeScript facades
  over `@keld/api`). Unchanged: a typed refusal per member is the §1 contract, and the
  facade opens no second link (§3 v0 note).
- `docs/architecture/07-agent-experience.md` §2 and `docs/engineering/keld-error-codes.md`:
  one `KELD-*` code per failure class with a `fix`; `crates/keld-cli/tests/error_registry.rs`
  scans `packages/@keld/electron/src`.
- `docs/specs/gh532-first-proof-evidence-rules.md` rules 1-4 and
  `docs/specs/gh566-corpus-manifest-owner.md` §4.4 (one registry entry per corpus, one
  snapshot store, red-until-implemented cells assert today's behaviour).
- `crates/keld-compat/AGENTS.md`: Electron documented behaviour is the oracle; the
  conformance entry lands before the implementation.
- Issue #565 body and Agent Brief; #476 § Maturity ladder L0 and § Not yet specified;
  #420 (PANEL-P3, closed): the alias's named exports are part of the pre-evaluation
  descriptor set.
- Pinned Electron v44.4.5 docs at `694f45852a0f1726cd23bfd379854de489cccb65`:
  `docs/api/{menu,shell,dialog,session,screen,clipboard-item,clipboard,native-image,native-theme,ipc-main}.md`
  (the four already in the store plus six added by T2). Each page's `Process: Main` line
  is the citation that the binding belongs to the main process.

No architecture document deviates. No boundary change: handle ownership, crash ownership
and principal minting are untouched; the facade sends no frame.

## 3. Acceptance criteria (binary, each becomes a test)

1. **Link, per binding.** Given a Bun 1.4.4 child process evaluating a module whose only
   import is `import { <name> } from "<index.ts>"` for each of the ten bindings, when it
   runs, then it exits 0 and prints the binding's `typeof` (`function` for `Menu` and
   `ClipboardItem`, `object` otherwise). *Negative control:* removing that one export makes
   the child exit 1 with `SyntaxError: Export named '<name>' not found`.
2. **Link, draw.io's statement.** Given a child module with draw.io's exact twelve-name
   import, when it runs, then it exits 0 in one module graph. *Negative control:* the same
   harness with `MenuItem` exits 1 with the `SyntaxError`; this is also the live proof that
   the oracle detects a missing export.
3. **Default parity.** For each of the twelve names, `electron.default[name]` is the same
   value as the named export. *Negative control:* dropping a key from the default object.
4. **No Host traffic.** Given `KELD_APP_LINK` set and `LifecycleLink.connect` replaced by
   a recorder before the facade loads, when a child imports the facade and touches every
   own member of the ten bindings (calls each function, reads each accessor, constructs
   each class), then `connect` was called zero times. *Negative control:* one refusal
   calling `app.whenReady()` makes the count 1.
5. **Synchronous refusal.** Given a member whose pinned page documents no `Promise`
   return (a method, a property, or a constructor), when it is called, read or
   constructed, then it throws an `UnsupportedMemberError` with `code === "KELD-COMPAT-013"`,
   `member === "<binding>.<member>"` (the class name for a constructor), `ticket === "GH-<n>"`,
   `kind` naming the reach, and a message that starts with the code and names
   `gyldlab/keld#<n>`. *Negative control:* returning `undefined`.
6. **Promise refusal.** Given a member whose pinned page says `Returns \`Promise<…>\``, when
   it is called, then it returns a `Promise` (no synchronous throw) that rejects with the
   same error. *Negative control:* throwing synchronously, or resolving.
7. **Census.** For each binding, the set of own property names (for classes: minus
   `length`, `name`, `prototype`) equals the member list in §4.3, and each member's kind
   and ticket equal that table. *Negative control:* one extra, missing or re-kinded member.
8. **ClipboardItem constructs.** `new ClipboardItem({})` and
   `new ClipboardItem({ "text/plain": "x" })` construct instances without Host traffic;
   `new ClipboardItem()` and a non-object argument throw `TypeError`; reading `types` and
   calling `getType("text/plain")` refuse naming GH-559. *Negative control:* a constructor
   that throws for `{}`, or a `types` that returns `[]`.
9. **Transition gate untouched.** `MenuItem` is absent from the named and default exports;
   the five existing `MenuItem`-based cases in `native-surface.test.ts` pass unchanged.
   *Negative control:* exporting `MenuItem` fails the GH-563 three-stage case.
10. **Registry.** `KELD-COMPAT-013` has a heading with `crate`, `message` and `fix` in
    `docs/engineering/keld-error-codes.md`, and `cargo test -p keld-cli --test error_registry`
    passes. *Negative control:* deleting the heading.
11. **Corpus.** The corpus `electron-main-bindings-v1` registers ten cells, one per
    binding, each citing its page's `Process: Main` line; after T3 they are `pass` cells
    with no `implementing_ticket`. The `electron-native-v1` cells whose mapped Bun cases
    asserted an absent export are re-recorded to assert today's typed refusal and keep
    `expected_verdict: fail` with their implementing tickets. `cargo test -p keld-compat`
    passes on the host. *Negative control:* one flipped manifest byte
    (`DigestMismatch`), or a quote absent from its pinned page (`QuoteAbsent`).
12. **Mutation run.** With one method refusal replaced by `return undefined` and one
    Promise refusal replaced by a synchronous throw, the mapped cases of AC5 and AC6 fail;
    the PR records the exact failing case names.

## 4. Design

### 4.1 First-principles and reuse decision

No boundary change. Facts (FACT unless marked):

- F1. `packages/@keld/electron/src/index.ts` exports `app`, `BrowserWindow`, types,
  `ELECTRON_VERSION_SHIM`, `isCallError`, and `default = { app, BrowserWindow }`;
  `package.json` has `"type": "module"` and `exports: { ".": "./src/index.ts" }`, no CJS.
- F2. Bun 1.4.4 link failure for a missing named export: `SyntaxError: Export named '<name>'
  not found in module '<abs path>'.`, exit 1 (probe on this branch's base, 2026-10-31).
  `file://` URLs and absolute paths are accepted import specifiers.
- F3. Every Host path of the facade funnels through `ensureLink()` →
  `LifecycleLink.connect` in `packages/@keld/api/src/app.ts`; importing the facade does
  not call it (the `app_surface_*` fixtures capture `connect` after import and observe
  it only on `app.on("ready")`/`whenReady`).
- F4. Existing typed errors in the package are plain `Error`s whose message starts with the
  code (`KELD-COMPAT-010..012`); `@keld/kipc` exposes `KeldCallError = Error & { code }`
  and `isCallError`. Next free code: `KELD-COMPAT-013`.
- F5. The pinned v44.4.5 pages list, per binding, the members in §4.3; `Returns
  \`Promise<…>\`` marks the Promise kind; `_Readonly_` marks a read-only property.
  `clipboard-item.md`: `new ClipboardItem(items)` takes a `Record<string, …>` and says
  nothing about an empty record; `types` and `getType` are its instance members.
- F6. Owners per member come from the F06/F04/F05 epics' `Electron members covered` lines
  (#478, #479, #480, #481, #482, #556, #557, #558, #559, #561, #564; #466, #547; #553,
  #474). Members that no ticket covers (`clipboard.read/has/clear/selection`,
  `nativeImage.*` except `createFromDataURL`, `Menu.sendActionToFirstResponder`) are the
  F06 epic's "Not yet specified" list (#476).
- F7. The ecosystem packages draw.io loads use the default import only: electron-store
  11.0.2, electron-context-menu 5.1.0 and electron-is-dev 3.0.1 (`import electron from
  'electron'`, read from the published `index.js` at unpkg, 2026-10-31); electron-log 5.4.4
  and electron-updater 6.8.9 are CommonJS. So the twelve names are the whole link-time
  demand; what those packages touch at import are members owned elsewhere (#452, #466,
  #491).
- F8. `native-surface.test.ts` and `dialog.test.ts` assert today that `Menu.*`, `screen.*`,
  `nativeTheme.shouldUseDarkColors` and `dialog.*` are `undefined`; the native corpus
  negative controls say that adding the export "fails only this absent-method case" and
  that the implementing owner re-records it. PR #1130 (GH-464) asserts `ipcMain.on/once/
  removeListener` absent the same way; PRs #1306/#1308 (GH-478) add a real `dialog`.

Decisions, each with the rejected alternative:

- **D1 one code, one class.** `KELD-COMPAT-013` is the only code; `UnsupportedMemberError`
  carries `code`, `kind`, `member` and `ticket` fields and a message `KELD-COMPAT-013: <subject>
  is not implemented by @keld/electron yet; it is tracked on gyldlab/keld#<n>. <fix>`, where the
  subject is `electron.<member>()` for a callable, `electron.<member>` for a property and
  `new electron.<member>()` for a constructor (`member` is then the class name).
  Rejected: one code per module (the failure class is one: a linked, unimplemented
  member), and bare `Error` strings as in 010-012 (tests and apps would parse messages
  for the ticket). The field shape follows `KeldCallError`; `isUnsupportedMemberError`
  follows `isCallError`.
- **D2 return kinds from the pinned pages.** Method → throw; `Returns Promise` → rejected
  Promise; property → throwing accessor (writable `themeSource` also throws on set);
  constructor → throw. Rejected: `undefined`-valued properties and resolving Promises
  (silent success, forbidden by root `AGENTS.md`), and throwing synchronously from
  Promise members (draw.io `await`s and `.catch`es them; a synchronous throw changes
  control flow).
- **D3 census = every reachable documented member.** Each binding exposes every member
  its pinned page documents at module or static level, plus the EventEmitter
  registration methods (`on, once, off, addListener, removeListener, removeAllListeners`)
  for `screen` and `nativeTheme`, whose pages document events. Rejected: only the members
  draw.io touches (every other member would fail with a `TypeError`, not the typed
  contract), and instance members of refusing constructors (`menu.popup`, `image.toPNG`:
  unreachable, YAGNI).
- **D4 `session.defaultSession` is a throwing accessor naming GH-553.** Rejected: a stable
  `defaultSession` object with eight refusing `WebRequest` methods. That is #553's own
  design (object identity, F05-T5 binding, the ▲ no-op lane); pre-deciding it here would
  duplicate its spec. draw.io reaches the same typed stop at `electron.js:994`.
- **D5 `ipcMain` refuses.** All nine documented methods refuse (GH-466 listeners, GH-547
  `handle` family). Rejected: a registration table without delivery (a stub) and a
  silent no-op `on`. Consequence, recorded: draw.io's evaluation now stops at
  `electron.js:3426` with a typed error naming GH-466. PR #1130's absence baselines are
  re-recorded by whichever PR lands second (its negative controls anticipate exactly
  this change).
- **D6 `MenuItem` stays unexported** (§1). Rejected: exporting it now with refusals, which
  would trip the GH-563 gate that GH-480 owns.
- **D7 layout.** `unsupported.ts` owns the error, the guard and the surface builder;
  `unsupported-bindings.ts` owns the member table (data). An implementing ticket moves
  its module out of the table into its own file (as #1306 does for `dialog.ts`) and
  deletes the rows it implements. Rejected: ten module files now (ten copies of one
  shape) and inlining the table in `index.ts`.
- **D8 corpus.** A new corpus `electron-main-bindings-v1` (kind `activation`: module
  linking is part of boot), one `pass` cell per binding citing the page's `Process: Main`
  line, every cell declaring `macos`, `linux` and `windows` because the facade is
  platform-neutral and the CI matrix runs the Bun targets on all three. Rejected: adding
  the cells to `electron-native-v1` (kind `primary_workflow`, and a file three open PRs
  edit) and macOS-only cells (would hide two lanes CI already runs).
- **D9 red first.** T2 lands the ten cells as `fail`/`GH-565` with Bun cases asserting
  today's link failure; T3 inverts the cases and flips the cells in the same change
  (gh532 rules 3 and 5).

### 4.2 New types and surfaces (`packages/@keld/electron/src/unsupported.ts`)

```ts
export const UNSUPPORTED_MEMBER_CODE = "KELD-COMPAT-013";
export type TrackingIssue = `GH-${number}`;
export type UnsupportedKind = "method" | "promise" | "property" | "constructor";
export class UnsupportedMemberError extends Error {
  readonly code: "KELD-COMPAT-013";
  readonly kind: UnsupportedKind;
  readonly member: string;      // "shell.openExternal", "Menu", "nativeTheme.shouldUseDarkColors"
  readonly ticket: TrackingIssue;
}
export function isUnsupportedMemberError(value: unknown): value is UnsupportedMemberError;
export type MemberKind = "method" | "promise" | "property" | "writableProperty";
export interface UnsupportedMember { readonly kind: MemberKind; readonly ticket: TrackingIssue }
export type UnsupportedSurface<T extends Record<string, UnsupportedMember>> = {
  readonly [K in keyof T]: T[K]["kind"] extends "promise" ? (...args: unknown[]) => Promise<never>
    : T[K]["kind"] extends "method" ? (...args: unknown[]) => never
    : never;
};
export function unsupportedSurface<T extends Record<string, UnsupportedMember>>(binding: string, members: T): UnsupportedSurface<T>;
```

`index.ts` adds `export { ClipboardItem, Menu, clipboard, dialog, ipcMain, nativeImage,
nativeTheme, screen, session, shell }`, re-exports `UnsupportedMemberError`,
`isUnsupportedMemberError`, `UNSUPPORTED_MEMBER_CODE`, `TrackingIssue` and `UnsupportedKind`,
and extends the default object with the same ten bindings. No `any` anywhere.

### 4.3 Member table (binding · member · kind · tracking issue)

| Binding | Members (kind → issue) |
|---|---|
| `shell` (`shell.md`) | `showItemInFolder` method → GH-479 · `openPath` promise → GH-479 · `openExternal` promise → GH-479 · `trashItem` promise → GH-479 · `beep` method → GH-479 · `writeShortcutLink` method → GH-482 · `readShortcutLink` method → GH-482 |
| `clipboard` (`clipboard.md`) | `readText` promise → GH-559 · `writeText` promise → GH-559 · `write` promise → GH-559 · `read` promise → GH-476 · `has` promise → GH-476 · `clear` method → GH-476 · `selection` property → GH-476 |
| `ClipboardItem` (`clipboard-item.md`) | constructor constructs (object argument; `TypeError` otherwise) · `types` property → GH-559 · `getType` promise → GH-559 |
| `nativeImage` (`native-image.md`) | `createFromDataURL` method → GH-559 · `createEmpty`, `createFromPath`, `createFromBitmap`, `createFromBuffer`, `createFromNamedImage`, `createMenuSymbol` method → GH-476 · `createThumbnailFromPath` promise → GH-476 |
| `nativeTheme` (`native-theme.md`) | `shouldUseDarkColors` property → GH-481 · `themeSource` writableProperty → GH-558 · `shouldUseHighContrastColors`, `shouldUseDarkColorsForSystemIntegratedUI`, `shouldUseInvertedColorScheme` property → GH-558 |
| `nativeTheme` (continued) | `inForcedColorsMode`, `prefersReducedTransparency`, `shouldDifferentiateWithoutColor` property → GH-558 · `on`, `once`, `off`, `addListener`, `removeListener`, `removeAllListeners` method → GH-558 (`updated`) |
| `screen` (`screen.md`) | `getPrimaryDisplay`, `getAllDisplays` method → GH-481 · `getCursorScreenPoint`, `getDisplayNearestPoint`, `getDisplayMatching`, `screenToDipPoint`, `dipToScreenPoint`, `screenToDipRect`, `dipToScreenRect` method → GH-558 |
| `screen` (continued) | `on`, `once`, `off`, `addListener`, `removeListener`, `removeAllListeners` method → GH-558 (display events) |
| `Menu` (`menu.md`) | `new Menu()` constructor → GH-556 · `buildFromTemplate`, `setApplicationMenu` static method → GH-480 · `getApplicationMenu` static method → GH-556 · `sendActionToFirstResponder` static method → GH-476 |
| `dialog` (`dialog.md`) | `showOpenDialog`, `showSaveDialog`, `showMessageBox` promise → GH-478 · `showMessageBoxSync` method → GH-564 · `showOpenDialogSync`, `showSaveDialogSync`, `showErrorBox` method → GH-561 · `showCertificateTrustDialog` promise → GH-561 |
| `session` (`session.md`) | `fromPartition`, `fromPath` method → GH-474 · `defaultSession` property → GH-553 |
| `ipcMain` (`ipc-main.md`) | `on`, `off`, `once`, `addListener`, `removeListener`, `removeAllListeners` method → GH-466 · `handle`, `handleOnce`, `removeHandler` method → GH-547 |

- Capabilities required; manifest changes: none.
- Wire/protocol changes: none.
- Platform notes: none; the facade is platform-neutral and sends no frame.
- Runtime seam: none.
- Migration unit: none permanent. Each implementing ticket replaces its rows with the
  real member in the same change that flips its cells; the compat facade is retained.

## 5. Boundaries

- Implement in:
  - `packages/@keld/electron/src/{index.ts,unsupported.ts,unsupported-bindings.ts}`;
  - `packages/@keld/electron/src/{module-bindings.test.ts,native-surface.test.ts,dialog.test.ts}`;
  - `packages/@keld/electron/fixtures/bindings_no_link.ts`;
  - `crates/keld-compat/fixtures/main-bindings-corpus/{corpus.json,denominator.json}`;
  - `crates/keld-compat/fixtures/native-corpus/{corpus.json,denominator.json}` (re-record only);
  - `crates/keld-compat/fixtures/doc-snapshots/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/`
    `{shell,clipboard,clipboard-item,native-image,session,ipc-main}.md`;
  - `crates/keld-compat/tests/support/corpus_manifest.rs` (one `REGISTRY` entry);
  - `docs/engineering/keld-error-codes.md`, regenerated `llms-full.txt`.
- Must not touch: `app.ts`, `browser-window.ts`, `@keld/api`, `@keld/kipc`, any Rust
  crate source, `Cargo.toml`, the kipc channel table, `keld-guard`, the architecture docs,
  the GH-563 transition-gate cases, `tsconfig.json` or `package.json` of the package.

## 6. Tasks (each ≈ one commit of the GH-565 PR; ordered; no placeholders)

- [ ] T1 This spec, `Status: draft`, for exact-text approval.
- [ ] T2 Red cells first: six pinned pages into the snapshot store (URL + SHA-256 in the
  manifest), corpus `electron-main-bindings-v1` with ten `fail`/`GH-565` cells, the
  `REGISTRY` entry, and `module-bindings.test.ts` asserting today's link failure.
- [ ] T3 Implementation: `unsupported.ts`, `unsupported-bindings.ts`, `index.ts`
  exports, `KELD-COMPAT-013` registry entry and `llms-full.txt`, the `bindings_no_link.ts`
  fixture, inverted link cases and flipped cells, re-recorded `electron-native-v1` cells
  and their Bun cases, the mutation run (AC12) recorded in the PR.

## 7. Test plan

| AC | Test |
|---|---|
| 1 | `module-bindings.test.ts`, ten cell cases `<module>.binding.named-import: …`, each spawning one Bun child with a one-name import (isolated temp file, `file://` specifier) |
| 2 | same file, `draw.io's twelve-name import statement links in one module graph` and the `MenuItem` SyntaxError control |
| 3 | same file, `the default export carries every named binding` |
| 4 | same file spawning `fixtures/bindings_no_link.ts` with `KELD_APP_LINK` set; asserts `connect=0` and `touched` equal to the table's member count plus two constructors |
| 5, 6, 7 | same file, one case per table row from the test's own copy of §4.3 (independent of the implementation's table), plus a census case per binding |
| 8 | same file, `ClipboardItem` cases |
| 9 | existing `native-surface.test.ts` `MenuItem` cases, unchanged |
| 10 | `cargo test -p keld-cli --test error_registry` |
| 11 | `cargo test -p keld-compat` (registry: census, digest, citations, Bun admission); re-recorded cases in `native-surface.test.ts` and `dialog.test.ts` |
| 12 | manual mutation run, recorded in the PR with the failing case names |

Anti-flake: every child has a kill-switch timeout below Bun's per-test limit, writes only
to an isolated temp directory, and reports on stdout; no sleeps; no ports; no OS UI.

## 8. Review gates triggered

public API (new named exports and the `UnsupportedMemberError` type). `unsafe`: none.
permission model: none. dependency: none. wire protocol: none.

## 9. Perf impact

none. Module evaluation adds ten frozen objects and two classes; no frame, no allocation
on any kipc or event-loop path.

## 10. Open questions

1. Platform declaration (D8): the cells declare all three lanes on the strength of the CI
   matrix; only macOS is exercised locally. If the hosted Linux or Windows Bun admission
   fails for a harness reason, narrow to `macos` in the same PR rather than skip.
2. Members with no owning ticket refuse naming #476. Should the owner mint tickets for
   `clipboard.read/has/clear/selection`, the seven `nativeImage` constructors and
   `Menu.sendActionToFirstResponder`, or is the epic the intended tracker until demand
   appears?
3. Landing order against PRs #1306/#1308 (GH-478) and #1130 (GH-464): the later PR
   re-records (`dialog` rows replaced by the real broker; `ipcMain` absence baselines
   replaced by the typed refusal). Confirm the first-to-green rule applies, or name an order.
