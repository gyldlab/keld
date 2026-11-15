# GH-565 contract and design

Part of: [index](index.md)
Owns: acceptance, decisions, platform matrix, upstream oracle

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
  `docs/api/{menu,shell,dialog,session,screen,clipboard-item,clipboard,native-image,native-theme}.md`
  (the four already in the store plus five added by T2; `ipc-main.md` is PR #1365's). Each
  page's `Process: Main` line is the citation that the binding belongs to the main process.

No architecture document deviates. No boundary change: handle ownership, crash ownership
and principal minting are untouched; the facade sends no frame.

## 3. Acceptance criteria

### AC-1: Every named binding links independently
- **Given:** Bun 1.4.4 and each of the nine main-process names in the pinned Electron v44.4.5 API.
- **When:** A fresh Bun child imports exactly one binding by name from the facade.
- **Then:** It exits 0. Menu and ClipboardItem are functions; the other seven names are objects.
- **Negative control:**
  - **Single fault:** Delete the binding's named export.
  - **At-risk precondition:** The child evaluates a fresh named-import module.
  - **Expected RED:** The link case fails with Bun's export-not-found SyntaxError and exit 1.

### AC-2: Draw.io's linked statement succeeds
- **Given:** The eleven draw.io imports owned here, excluding ipcMain owned by PR #1365.
- **When:** A fresh child evaluates all eleven names in one module graph.
- **Then:** The import links and the child exits 0. PR #1365 restores the twelve-name case when composed.
- **Negative control:**
  - **Single fault:** Import the deliberately unexported MenuItem name.
  - **At-risk precondition:** Bun performs ESM named-import linking before evaluation.
  - **Expected RED:** The negative case exits 1 with the documented SyntaxError.

### AC-3: Default and named binding identities match
- **Given:** Each of the eleven named values exported by this facade.
- **When:** The test reads the same key through electron.default.
- **Then:** The default key is strictly identical to the named export.
- **Negative control:**
  - **Single fault:** Remove one key from the default object.
  - **At-risk precondition:** Default parity is checked for every named import.
  - **Expected RED:** The missing key breaks the identity assertion.

### AC-4: Link-only stubs cause no Host traffic
- **Given:** KELD_APP_LINK and a recorder installed on LifecycleLink.connect before module import.
- **When:** A child imports the facade, reads every existing getter, calls methods, and constructs both classes.
- **Then:** The recorder sees zero connect calls, including when present members refuse.
- **Negative control:**
  - **Single fault:** Call app.whenReady() from a refusal path.
  - **At-risk precondition:** The fixture touches each reachable member and watches the only connect seam.
  - **Expected RED:** The recorder observes connect=1 instead of connect=0.

### AC-5: Available synchronous members refuse with a typed error
- **Given:** A method, property, or constructor without a Promise return in the pinned API.
- **When:** An application calls, reads, or constructs that member.
- **Then:** It throws UnsupportedMemberError with KELD-COMPAT-013, exact kind, member, and GH ticket.
- **Negative control:**
  - **Single fault:** Replace one refusal with return undefined.
  - **At-risk precondition:** The mapped case reaches the available synchronous member.
  - **Expected RED:** The missing typed error breaks that member's refusal case.

### AC-6: Promise members reject asynchronously
- **Given:** A present member whose pinned API documents a Promise return.
- **When:** The application invokes that member.
- **Then:** It returns a Promise without synchronous throw, then rejects with the same typed GH-ticket error.
- **Negative control:**
  - **Single fault:** Throw synchronously or resolve instead of rejecting.
  - **At-risk precondition:** The test captures the returned Promise and awaits its rejection.
  - **Expected RED:** The return-kind or rejection assertion fails.

### AC-7: The native platform's member census is exact
- **Given:** The platform table in section 4.3 and a real macOS, Linux, or Windows CI runner.
- **When:** The test enumerates every binding's own members and checks available kinds and tickets.
- **Then:** Its census exactly matches section 4.3 on that OS, including the absent-name rules.
- **Then:** Absent members read undefined without throwing; Menu.sendActionToFirstResponder is own undefined off macOS.
- **Negative control:**
  - **Single fault:** Add an unavailable name or re-kind a present name.
  - **At-risk precondition:** All native CI lanes run the independent census, not a mocked platform value.
  - **Expected RED:** The census or absence assertion fails on the affected OS.

### AC-8: ClipboardItem follows upstream constructor admission
- **Given:** Electron v44.4.5 clipboard-item constructor behavior and GH-559 payload ownership.
- **When:** The caller constructs ClipboardItem with a MIME-keyed record or an invalid input.
- **Then:** A nonempty own-enumerable string-key record constructs locally without Host traffic.
- **Then:** Missing, null, primitive, array, empty, inherited-only and symbol-only records throw TypeError.
- **Then:** Enumerable payload getters run exactly once and their thrown errors propagate synchronously.
- **Then:** types and getType still refuse with GH-559. Do not validate payload MIME or values prematurely.
- **Negative control:**
  - **Single fault:** Accept an empty record or skip a payload getter.
  - **At-risk precondition:** Dedicated tests construct invalid and valid records with observable getters.
  - **Expected RED:** The constructor or getter assertions fail on the mutated behavior.

### AC-9: MenuItem remains absent
- **Given:** The separate GH-480 and GH-563 MenuItem transition gate.
- **When:** Both ESM exports and the existing five MenuItem native-surface cases are evaluated.
- **Then:** MenuItem is absent from named and default exports; existing transition cases pass unchanged.
- **Negative control:**
  - **Single fault:** Add a MenuItem named export.
  - **At-risk precondition:** The three-stage GH-563 case runs on the modified facade.
  - **Expected RED:** Its absent-export expectation fails.

### AC-10: The typed error is registered
- **Given:** The central error registry and KELD-COMPAT-013 documentation.
- **When:** cargo test -p keld-cli --test error_registry runs.
- **Then:** The heading defines crate, message and fix; the registry test passes.
- **Negative control:**
  - **Single fault:** Remove the KELD-COMPAT-013 registry heading.
  - **At-risk precondition:** The error registry scan includes the facade source.
  - **Expected RED:** The registry test fails for the unregistered code.

### AC-11: Both conformance corpora remain meaningful
- **Given:** electron-main-bindings-v1 and the re-recorded electron-native-v1 observations.
- **When:** cargo test -p keld-compat runs against the pinned snapshots and Bun cases.
- **Then:** Nine main-binding cells pass with their Process: Main citations and no implementing ticket.
- **Then:** Unimplemented native-member cells stay RED with their implementing tickets despite linkable names.
- **Negative control:**
  - **Single fault:** Change a manifest byte or remove a quote from the pinned page.
  - **At-risk precondition:** Corpus admission checks both digests and sourced statements.
  - **Expected RED:** DigestMismatch or QuoteAbsent rejects the changed corpus.

### AC-12: Mutations falsify both refusal kinds
- **Given:** Independent synchronous and Promise refusal cases already in the package tests.
- **When:** A method is changed to return undefined and a Promise member is changed to throw synchronously.
- **Then:** The associated cases fail, and the PR records their exact case names.
- **Negative control:**
  - **Single fault:** Mutate either one refusal return kind.
  - **At-risk precondition:** The selected case reaches the specific mutated member.
  - **Expected RED:** That member's case fails instead of accepting a silent stub.

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
  `clipboard-item.md` describes one or more MIME-typed representations. The pinned
  `lib/browser/api/clipboard-item.ts:36-56` rejects arrays and zero-key objects, then
  snapshots `{ ...items }`; doc silence does not justify accepting an empty record.
  `types` and `getType` are its instance members, deliberately unsupported here.
- F6. Owners per member come from the owning epics' `Electron members covered` lines
  (#478, #479, #480, #481, #482, #556, #557, #558, #559, #561, #564; #553,
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
  removeListener` absent the same way; that baseline is PR #1365's to re-record, not this
  spec's. PRs #1306/#1308 (GH-478) add a real `dialog`.

Decisions, each with the rejected alternative:

### D-1: one code, one class.

`KELD-COMPAT-013` is the only code; `UnsupportedMemberError`
  carries `code`, `kind`, `member` and `ticket` fields and a message `KELD-COMPAT-013: <subject>
  is not implemented by @keld/electron yet; it is tracked on gyldlab/keld#<n>. <fix>`, where the
  subject is `electron.<member>()` for a callable, `electron.<member>` for a property and
  `new electron.<member>()` for a constructor (`member` is then the class name).
  Rejected: one code per module (the failure class is one: a linked, unimplemented
  member), and bare `Error` strings as in 010-012 (tests and apps would parse messages
  for the ticket). The field shape follows `KeldCallError`; `isUnsupportedMemberError`
  follows `isCallError`.
### D-2: return kinds from the pinned pages.

Method → throw; `Returns Promise` → rejected
  Promise; property → throwing accessor (writable `themeSource` also throws on set);
  constructor → throw. For available members, rejected: `undefined`-valued properties
  and resolving Promises
  (silent success, forbidden by root `AGENTS.md`), and throwing synchronously from
  Promise members (draw.io `await`s and `.catch`es them; a synchronous throw changes
  control flow).
### D-3: census = each platform's reachable documented member.

Each binding exposes
  the members registered by the pinned implementation on the actual platform, with
  the shape exceptions in §4.3, plus the EventEmitter
  registration methods (`on, once, off, addListener, removeListener, removeAllListeners`)
  for `screen` and `nativeTheme`, whose pages document events. Rejected: only the members
  draw.io touches (every other member would fail with a `TypeError`, not the typed
  contract), and instance members of refusing constructors (`menu.popup`, `image.toPNG`:
  unreachable, YAGNI). Also rejected: exposing every platform's union everywhere;
  that turns safe feature-detection reads into throwing getters or false-positive
  callable checks. An unavailable member is not an unsupported implementation.
### D-4: `session.defaultSession` is a throwing accessor naming GH-553.

Rejected: a stable
  `defaultSession` object with eight refusing `WebRequest` methods. That is #553's own
  design (object identity, F05-T5 binding, the ▲ no-op lane); pre-deciding it here would
  duplicate its spec. draw.io reaches the same typed stop at `electron.js:994`.
### D-5: `ipcMain` is exported by the GH-466 registration slice (PR #1365); this spec does not touch it.

Decision recorded on #466 (2026-10-31, "Delegated design decision"
  comment): the #589 activation criterion scores draw.io's `ipcMain` registrations, so a
  link-time refusal at `electron.js:3426` would stop evaluation before any later criterion
  is observable, and with no sender in the tree a registered listener has no message to
  miss. Rejected: nine refusing `ipcMain` methods here (this spec's first D-5), which #1365
  would have had to delete on rebase. Landing order: this PR first; #1365 rebases onto it
  and brings the `ipcMain` export, its cells and its `ipc-main.md` snapshot (byte-identical
  to the one T2 first added and T4 removes).
### D-6: `MenuItem` stays unexported

(§1). Rejected: exporting it now with refusals, which
  would trip the GH-563 gate that GH-480 owns.
### D-7: layout.

`unsupported.ts` owns the error, the guard and the surface builder;
  `unsupported-bindings.ts` owns the member table (data). An implementing ticket moves
  its module out of the table into its own file (as #1306 does for `dialog.ts`) and
  deletes the rows it implements. Rejected: nine module files now (nine copies of one
  shape) and inlining the table in `index.ts`.
### D-8: corpus.

A new corpus `electron-main-bindings-v1` (kind `activation`: module
  linking is part of boot), one `pass` cell per binding citing the page's `Process: Main`
  line, every cell declaring `macos`, `linux` and `windows` because all nine bindings
  link on all three. Member presence is platform-specific; the CI matrix must verify
  the corresponding census on each actual platform. Rejected: adding
  the cells to `electron-native-v1` (kind `primary_workflow`, and a file three open PRs
  edit) and macOS-only cells (would hide two lanes CI already runs).
### D-9: red first.

T2 lands the cells as `fail`/`GH-565` with Bun cases asserting
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

`index.ts` adds `export { ClipboardItem, Menu, clipboard, dialog, nativeImage, nativeTheme,
screen, session, shell }`, re-exports `UnsupportedMemberError`,
`isUnsupportedMemberError`, `UNSUPPORTED_MEMBER_CODE`, `TrackingIssue` and `UnsupportedKind`,
and extends the default object with the same nine bindings. No `any` anywhere.

### 4.3 Member table (binding · member · kind · tracking issue)

| Binding | Members (kind → issue) |
|---|---|
| `shell` (`shell.md`) | `showItemInFolder` method → GH-479 · `openPath` promise → GH-479 · `openExternal` promise → GH-479 · `trashItem` promise → GH-479 · `beep` method → GH-479 · `writeShortcutLink` method → GH-482 · `readShortcutLink` method → GH-482 |
| `clipboard` (`clipboard.md`) | `readText` promise → GH-559 · `writeText` promise → GH-559 · `write` promise → GH-559 · `read` promise → GH-476 · `has` promise → GH-476 · `clear` method → GH-476 · `selection` property → GH-476 |
| `ClipboardItem` (`clipboard-item.md`) | constructor admits a nonempty non-array record and snapshots its own enumerable properties (AC-8); invalid shape throws `TypeError` · `types` property → GH-559 · `getType` promise → GH-559 |
| `nativeImage` (`native-image.md`) | `createFromDataURL` method → GH-559 · `createEmpty`, `createFromPath`, `createFromBitmap`, `createFromBuffer`, `createFromNamedImage`, `createMenuSymbol` method → GH-476 · `createThumbnailFromPath` promise → GH-476 |
| `nativeTheme` (`native-theme.md`) | `shouldUseDarkColors` property → GH-481 · `themeSource` writableProperty → GH-558 · `shouldUseHighContrastColors`, `shouldUseDarkColorsForSystemIntegratedUI`, `shouldUseInvertedColorScheme` property → GH-558 |
| `nativeTheme` (continued) | `inForcedColorsMode`, `prefersReducedTransparency`, `shouldDifferentiateWithoutColor` property → GH-558 · `on`, `once`, `off`, `addListener`, `removeListener`, `removeAllListeners` method → GH-558 (`updated`) |
| `screen` (`screen.md`) | `getPrimaryDisplay`, `getAllDisplays` method → GH-481 · `getCursorScreenPoint`, `getDisplayNearestPoint`, `getDisplayMatching`, `screenToDipPoint`, `dipToScreenPoint`, `screenToDipRect`, `dipToScreenRect` method → GH-558 |
| `screen` (continued) | `on`, `once`, `off`, `addListener`, `removeListener`, `removeAllListeners` method → GH-558 (display events) |
| `Menu` (`menu.md`) | `new Menu()` constructor → GH-556 · `buildFromTemplate`, `setApplicationMenu` static method → GH-480 · `getApplicationMenu` static method → GH-556 · `sendActionToFirstResponder` static method → GH-476 |
| `dialog` (`dialog.md`) | `showOpenDialog`, `showSaveDialog`, `showMessageBox` promise → GH-478 · `showMessageBoxSync` method → GH-564 · `showOpenDialogSync`, `showSaveDialogSync`, `showErrorBox` method → GH-561 · `showCertificateTrustDialog` promise → GH-561 |
| `session` (`session.md`) | `fromPartition`, `fromPath` method → GH-474 · `defaultSession` property → GH-553 |

The table above names candidate members; the following presence rules are part of
the census, not optional runtime fallbacks. Capture the actual `process.platform`
at module evaluation; do not provide a caller-controlled platform override.

| Member | macOS | Linux | Windows |
|---|---|---|---|
| `clipboard.selection` | absent | property refusal | absent |
| `shell.writeShortcutLink`, `readShortcutLink` | absent | absent | method refusal |
| `screen.screenToDipPoint`, `dipToScreenPoint` | absent | method refusal | method refusal |
| `screen.screenToDipRect`, `dipToScreenRect` | absent | absent | method refusal |
| `nativeImage.createThumbnailFromPath` | promise refusal | absent | promise refusal |
| `nativeTheme.shouldDifferentiateWithoutColor` | property refusal | absent | absent |
| `Menu.sendActionToFirstResponder` | method refusal | own value `undefined` | own value `undefined` |

All other table members retain their listed presence and refusal kind. In particular,
`nativeImage.createFromNamedImage` and `createMenuSymbol` stay present on all three:
upstream registers them unconditionally even where their useful behavior is limited.
`dialog.showCertificateTrustDialog` is likewise exported by upstream JavaScript on
all three. A documentation platform label alone does not prove property absence.
No getter, Host call or invented method is installed for an absent member.

Primary receipt (Electron v44.4.5, immutable commit
`694f45852a0f1726cd23bfd379854de489cccb65`, retrieved 2026-11-07):

- [Clipboard constructor](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/clipboard-item.ts#L36-L56): array/empty checks and the enumerable-property snapshot.
- [Clipboard selection](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_clipboard.cc#L443-L449): Linux-only registration.
- [Shell shortcuts](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/common/api/electron_api_shell.cc#L189-L197): Windows-only registration.
- [Screen coordinates](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_screen.cc#L225-L237): point methods on Windows/Linux, rectangle methods on Windows.
- [Native image factories](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/common/api/electron_api_native_image.cc#L665-L676): thumbnail excluded on Linux, named-image/menu-symbol factories retained everywhere.
- [Native theme property](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_native_theme.cc#L139-L154): only `shouldDifferentiateWithoutColor` is conditionally registered in this table.
- [Menu JavaScript assignment](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/menu.ts#L202-L206) and [native registration](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_menu.cc#L366-L371): own `undefined` outside macOS, not an omitted JavaScript property.
- [Dialog JavaScript export](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/dialog.ts#L325-L344): the certificate-trust function remains exposed.

- Capabilities required; manifest changes: none.
- Wire/protocol changes: none.
- Platform notes: binding names are shared; member presence follows the table above.
  The facade still sends no frame. Windows/Linux runtime execution is not inferred
  from source guards or a macOS test; each CI lane supplies its own evidence.
- Runtime seam: none.
- Migration unit: none permanent. Each implementing ticket replaces its rows with the
  real member in the same change that flips its cells; the compat facade is retained.
