# Spec: macOS guarded native dialog broker (GH-478, F06-T2)

Status: approved
Linear: GH-478 (tracker under #517; reference KEL-80) · Owner: gyldlab/keld · Updated: 2026-10-28
Approval: owner-delegated acceptance of the exact v2 spec input (SHA-256 `a3137e6d826d93788845b57c4295917b34be54b61f7fcc939903ab3432fc73c2`) and the existing architecture05 path-only amendment is recorded at [GH478 comment 6476354568](https://github.com/gyldlab/keld/issues/478#issuecomment-6476354568); no native/product acceptance is claimed.

## 1. Goal & non-goals

Give one authenticated Bun app role a **host-guarded** way to present a real macOS
file-open, file-save or message dialog. The selected result must return through the
**existing** GH-527 WorkerLink, and the native UI must be owned and retired by the
current host/window generation. The `@keld/electron` async facade provides the
Electron v44.4.5 `showOpenDialog`, `showSaveDialog` and `showMessageBox` result shapes.
Without the exact `app.system` permission literal `dialog`, no panel may appear.
The first proof is macOS, with explicit legacy-profile disclosure and no implicit
filesystem authority minting. A real OS-visible panel and a subsequent healthy call
are part of acceptance; a typed JavaScript error by itself is not cleanup evidence.

Non-goals:
- Implementing `showMessageBoxSync`, `showOpenDialogSync`, `showSaveDialogSync`,
  `showErrorBox` or certificate-trust dialogs; #564 and F06-T14 own those consumers.
  This broker **does** provide the shared native *message-panel* operation used by #564.
  The separately approved GH499 migration contract permits a macOS-only,
  hash-bound dirty-close adaptation to awaited `showMessageBox(...).response`.
  Its exact patch bytes and native reentrancy/Save/Cancel/Discard proof remain
  separate gates. GH564 is required for the unchanged synchronous-source route,
  not automatically for that qualified adapted MVP. No additional app rewrite
  is authorized here.
- New kipc transport, socket, Worker, sendSync trick, modal thread, global JS pump,
  queue/replay owner or window-ID minter. GH-527, #528 and GH-531 already own them.
- Adding dialog-selected paths to FsBroker's retained scopes, providing sandbox
  access, or changing Strict Bun containment. F06-D1/F06-T15 own future strict
  user-intent grants. Under explicit legacy, `node:fs` is ambient and the
  containment loss must be disclosed, not relabelled as guarded I/O.
- Implementing Windows/Linux dialog backends, setting compatibility scores or
  closing #477/#564/#449. Non-macOS calls are explicitly unsupported until an
  OS-owned implementation is tested.
- Modifying Host 20 s, guardian/reap, existing kipc I/O or filesystem-service
  clocks. New dialog human-interaction custody is a *separate* finite clock.

## 2. Spec refs and provenance

- `docs/architecture/03-security.md` §2: immutable manifest, exact-capability
  default-deny and application-principal authority.
- `docs/architecture/04-electron-compat.md` §3–4: main-process facade over the
  single app link and honest per-operation compatibility records.
- `docs/architecture/05-webview-and-native.md` §3: native broker, UI-thread
  ownership and separation of host-minted windows from guard capabilities.
- `docs/specs/gh508-kipc-channel-table.md` §§4–5: **already approved** one
  append-only channel table, the proposed guarded `dialog` row and generated TS
  constant (no speculative numeric allocation or version change).
- `docs/specs/gh527-worker-owned-blocking-call-transport.md`: **already
  approved**, one WorkerLink with bounded frames, ordered wake and typed
  terminal errors. The sync consumer is #564, not this broker.
- `docs/specs/gh531-window-registry-close-state-machine.md`: **approved** host
  `WindowRef=(WindowId,WindowGeneration)` and role-owned close/recovery;
  application-window handle ownership is #449/#450, not this broker.
- `docs/specs/gh532-first-proof-evidence-rules.md` and
  `docs/specs/gh566-corpus-manifest-owner.md`: pinned 44.4.5 expected-fail
  cells and one conformance runner (not the frozen 44.3.0 lifecycle corpus).
- Electron v44.4.5 pinned commit
  `694f45852a0f1726cd23bfd379854de489cccb65`,
  [`docs/api/dialog.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/dialog.md)
  and [`message_box_mac.mm`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/message_box_mac.mm).
  Recorded real-Mac oracle: [KEL-237](https://linear.app/gyldlab-keld/document/electron-4445-macos-drawio-closequit-oracle-24-oct-2026-1718a506d0c7),
  [#442](https://github.com/gyldlab/keld/issues/442#issuecomment-6381736061).
- Pinned, unmodified draw.io Desktop
  [`jgraph/drawio-desktop@2edf9fb`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js):
  `src/main/electron.js:4191-4219` constructs Open/Save options with
  `defaultPath`, `filters` and Open `properties`, including keys whose
  value is `undefined`; `:876-887` constructs the synchronous message
  options including `type:'question'`, `title:'Confirm'` and `noLink:true`.
  This is caller-source evidence, not proof that a KELD broker works.
- Pinned [macOS file-dialog implementation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/file_dialog_mac.mm)
  `SetAllowedFileTypes` and `PopUpButtonHandler::selectFormat` determine
  initial/group-switch filter behavior; source observation is not a Keld run.
- [Approved GH499 migration contract](https://github.com/gyldlab/keld/blob/9959cb45608164c789e7e2e7fb7ab0e5eed039ca/docs/specs/gh499-drawio-hand-authored-migration.md)
  §4.4 and [exact owner approval](https://github.com/gyldlab/keld/issues/499#issuecomment-6465227830)
  permit the separately gated macOS dirty-close async adaptation.
- [#478 acceptance brief](https://github.com/gyldlab/keld/issues/478),
  [#564 consumer handoff](https://github.com/gyldlab/keld/issues/564#issuecomment-6381751522),
  [#528 transport acceptance](https://github.com/gyldlab/keld/issues/528#issuecomment-6370844113).

This **draft** PR proposes a narrow change to the existing `dialog`
architecture row to reconcile path-only/no-implicit-grant first-proof
behavior. The architecture/FS owner must independently review that
amendment before it is approved. `keld-native::MODULES` listing `dialog`
means planned surface, **not** that a broker currently exists.
`keld-guard` currently supports scoped arrays under `app.<group>.<action>`;
`app.system` literal-list admission does **not** exist yet and must be implemented
by its owning crate. No call may pretend `evaluate("system.dialog", "dialog")`
implements that different grammar.

## 3. Acceptance criteria (binary observations)

**A1a — immutable-policy boot preflight.** Malformed JSONC, duplicate
object keys, invalid bytes or a mismatched verified digest are rejected by
the existing retained-handle policy loader before starting an app child,
bootstrap listener or application window/panel. Preserve the loader's existing
typed startup error and resource-absence proof. No authenticated dialog CALL
exists at this cut, so no correlated Guard ERR is promised. Do not move
preflight after startup to manufacture one. The already opened policy handle
is an input, not a newly permitted app resource.

**A1b — exact grant on a valid live CALL.** Given a well-formed digest-verified
manifest, an authenticated current app role and a valid dialog CALL, exactly
`{"app":{"system":["dialog"]}}` reaches native admission once. Missing/empty
system list, wrong node type, duplicate array literals, unknown literals and
wildcard-like strings fail the proposed guard-owned evaluator with canonical
typed `KELD-GUARD` ERR on that CALL and fix pointer `/app/system`, with zero
native panel creation. Duplicate JSON object keys belong to A1a, not this atom.
No developer shortcut or automatic legacy enablement.

**A1c — caller identity before authorization.** An unauthenticated, stale or
wrong-role link is refused by its existing owning bootstrap/session rule with
zero dialog admission and no fabricated CALL response. Independently, the guard
evaluator rejects non-AppProcess principals before grant lookup. A valid app
link with an invalid parent is tested separately under A5; it does not bypass
the identity/permission checks or become another principal.

**A2 — open dialog result.** Given grant and a selected ordinary file on a real
Mac, `dialog.showOpenDialog()` resolves a Promise to
`{canceled:false,filePaths:[<actual selected absolute POSIX path>]}`. For native
Cancel (including an empty selection), `{canceled:true,filePaths:[]}`. When
`multiSelections` is allowed, the result preserves selection order; the maximum
number of returned paths is bounded before a response is allocated.

**A3 — save dialog result.** Given a real selected save destination and
`securityScopedBookmarks` absent/false, `dialog.showSaveDialog()` resolves to
`{canceled:false,filePath:<absolute path>}`; Cancel resolves to
`{canceled:true,filePath:""}`. The supported first-proof mode **does not
create or write** a file, open a scope or imply a later `node:fs` write.
`securityScopedBookmarks:true` is **explicitly unsupported and rejected before
presenting** a dialog: the pinned Electron documentation says this option can
create a blank selected file in Mac App Store builds. Do not silently accept
it and claim the no-side-effect guarantee or return fake bookmark data.

**A4 — message dialog result.** Given buttons and no checkbox label, a native
selected button produces `{response:<actual selected zero-based index>,
checkboxChecked:false}` from async `showMessageBox()`. A wrong button index,
Promise-returned-as-value, synthetic click or JS-only mock does **not** pass.
With a checkbox label, the actual native checked state is returned.

**A5 — parent identity.** The host resolves a supplied **host-minted**
`WindowRef` under the *current authenticated caller role generation* and real
native UI object, not by caller-controlled numeric OS handle. A supplied valid
parent uses the asynchronous upstream **sheet** API, whether visible or hidden;
only an absent parent uses an independent alert/panel. This follows the pinned
async native source branch, not the synchronous method's visibility fallback.
Actual hidden-parent async appearance/completion remains **UNKNOWN** until the
pinned Electron and Keld native oracle cases run; source branch equality alone
does not prove parity. Do not invent a hidden parent, retarget to another window
or silently replace the sheet with a detached modal. A stale, destroyed,
cross-app or forged parent is rejected before native presentation. The Host
rechecks current owner/ref at UI claim and again before publishing a result;
retirement cancels/drains the owned panel rather than retargeting its result.

**A6 — independent native custody.** Each admitted dialog owns exactly one
native object, one generation-bound original CALL and its one terminal outcome.
A later cleanup-only Cancel CALL is separate and never creates a second dialog.
App/Bun role death, link failure, parent destruction, user cancellation,
interaction expiry or session terminal Quit causes an observable physical panel
closure and native callback quiescence, even if the caller is gone. A late
completion is ignored after cleanup and cannot answer or mutate the replacement
role/window. The next valid native dialog succeeds in the same surviving host.

**A7 — UI-thread delivery.** No AppKit object is constructed or operated off the
macOS main UI thread. While a dialog is visible, the normal current-host UI
command path can still receive the required terminal cleanup. If any supported
path enters `NSModalPanelRunLoopMode`, delivery of **the real**
`AppWindowCommand`/panel cancellation is witnessed there. A default-mode-only
completion source must fail the test; a simulated CFRunLoop block alone is not
proof of the KELD bridge. Do not hold registry, guard or generation locks
across presenting an OS panel or invoking user callbacks.

**A8 — path-only authority.** Open and save results in the *explicitly supported
non-bookmark profile* carry **only** the OS-selected absolute POSIX paths,
no backslash, NUL, native bookmark, file descriptor, retained capability or
new manifest entry. Explicit `securityScopedBookmarks:true` and any
unimplemented Electron option are rejected with a typed unsupported outcome
**before** panel creation, not silently ignored. Unrepresentable path selections
produce a typed failure with no fabricated path. An immediately following
`@keld/api` guarded `fs.read/write` of the selected path remains denied unless
an independently declared FS grant already authorized that operation. Aliases
and symlinks require separately observed OS fixtures before parity claims.

**A9 — bounded inputs/effects.** Oversized strings, too many buttons/filters,
invalid result index, overly many selected paths, unknown enum discriminants,
wrong correlation and an unallocated dialog channel fail before unbounded
allocation/presentation. One dialog request cannot allocate a second panel on
retry or on an ambiguous transport outcome (IPC-022/023). No unlimited queue.

**A10 — non-macOS and strict behavior.** Without an implemented native backend,
Windows/Linux are typed unsupported, **not** a fake `canceled:true` or a
KELD-compatible pass. Legacy path-only operations must disclose forfeited
`node:fs` containment. Under Strict, *even a granted UI dialog* does not
implicitly authorize arbitrary filesystem access after selection.

**A11 — first-proof conformance.** The actual #477 expected-fail async cells
are registered and executed before corresponding implementation. When this
broker's real Mac result passes, the implementing PR inverts only its owned
cells **together with** observed native acceptance; the one GH566 parser,
platform labels and frozen 44.3.0 lifecycle bytes stay unchanged.

**A12 — source-exact option admission.** The unmodified pinned draw.io
`showOpenDialog` options (`defaultPath`, `filters`, `properties`) and
`showSaveDialog` options (`defaultPath`, `filters`) are accepted with
present-but-`undefined` optional values normalized as absent. The shared
message broker accepts the actual `type:'question'`,
`title:'Confirm'` and `noLink:true` alongside the three buttons and
indexes; `noLink` is validated but inert on macOS. A supported option with
the wrong type or an unknown key, **even when its value is undefined**,
fails before a panel appears. Source-exact fixtures must actually select,
cancel and choose each of the three message buttons on macOS.

**A13 — caller timeout and reentrancy custody.** After a blocking caller's
local deadline, the native panel is physically retired within the approved
short cleanup bound, even if the Host admits the original request late.
Exactly one panel may be outstanding for a role; concurrent/reentrant
attempts are typed `DialogBusy` failures before another panel exists.
A purely local Worker `abandon` or typed `KELD-IPC-006` is not sufficient:
an independently observed same-link Host cancellation and a real AppKit
close/callback-quiescence witness are mandatory. This is an **unimplemented
cross-owner gate**, not a current WorkerLink capability.
The timed-out original `CALL(c_orig)` and subsequent `Cancel(c_cancel)`
are distinct correlations. An ACK/`REPLY(c_cancel)` proves only the Cancel
disposition; for a still-pending admitted original, a live Host must
separately terminate `c_orig` exactly once with a registered `ERR(c_orig)`
if cancellation won, or preserve an already committed genuine
`REPLY(c_orig)` if the native result won. The Worker must
discard that late original terminal frame and release its abandoned entry,
without treating either terminal frame as proof that a native effect was
rolled back. The 257th successful timeout/cancellation on one healthy link
must not hit `KELD-IPC-027` from retained original correlations.

## 4. Design

### 4.1 Ownership and request seam

The *single* host Window Registry (#449) mints/validates parent identity.
The *single* Core primary-router and guard dispatcher authenticate the role
and immutable permission snapshot, reserve one dialog operation and enqueue
one UI command. `keld-wv` owns the macOS event loop and **borrows** the native
parent window only on that loop. `keld-native::dialog` owns the actual
`NSOpenPanel`, `NSSavePanel` or `NSAlert`, native button-to-result mapping,
callback and cancellation. The role's existing WorkerLink owns the one CALL
correlation/wake; `@keld/api` decodes values/typed terminal outcomes; the
`@keld/electron` module only maps public Electron async result shapes.

Proposed internal contract, **not yet an exported production type**:

```rust
// Private Core custody only; it is not a public Native type or wire payload.
struct DialogOwner {
    window: Option<WindowRef>, // GH531 host incarnation, never OS window number
    identity: PrimaryLinkIdentity, // actual Core-authenticated metadata, not wire
    attempt: u32,             // existing Core generation fence, not wire
    correlation: CorrelationId,
    request_nonce: DialogNonce, // lookup only, never a permission
}

// Wire request/reply definitions belong to keld-native/src/dialog.rs, following
// Native Auth's canonical DTO ownership; reuse keld-ipc::codec postcard primitives.
enum DialogRequestV1 {
    Open { nonce: DialogNonce, parent: Option<WindowRef>, options: OpenOptions },
    Save { nonce: DialogNonce, parent: Option<WindowRef>, options: SaveOptions },
    Message { nonce: DialogNonce, parent: Option<WindowRef>, options: MessageOptions },
    Cancel { nonce: DialogNonce }, // same authenticated channel, no new native UI
}
enum DialogReplyV1 {
    Open { canceled: bool, file_paths: Vec<String> },
    Save { canceled: bool, file_path: String },
    Message { response: u32, checkbox_checked: bool },
    Cancel { disposition: CancelDisposition },
}
enum CancelDisposition {
    Requested,        // matched custody is tombstoned; not proof of native drain
    AlreadyCompleted, // one fixed last-completed nonce; original is not answered twice
}
```

**Important:** This sketch does *not* mint a second window or role identity.
It uses the existing `keld_ipc::window::WindowRef { id, incarnation }` and
Core's `PrimaryCallerContext { identity, attempt, reader_stop }` metadata.
`PrimaryLinkIdentity` and attempt remain private Core custody; Native receives
only its admitted operation, never caller-selected role authority. Only `parent`, the bounded cancellation
nonce and validated options are on the wire; `role`, verified manifest and
effective generation are injected by Host acceptance from the authenticated
link. The requester is
never trusted to supply the generation, a principal, an AppKit pointer or
a guard-permit index. Use the existing declared postcard/kipc frame and
`CallError` model, not ad-hoc JSON frames. A facade-generated bounded
`DialogNonce` associates cancellation with an outstanding call *under the
Host's authenticated role identity*. It is not a bearer capability and may
never cancel a different role/generation's panel; duplicates are refused.
The nonce and `Cancel` variant are proposed GH478 wire-schema work and
require the ordinary GH508/receiver-contract review before implementation.

Valid facade calls keep the Electron names and overloads:
`showOpenDialog(options)` / `showOpenDialog(parent, options)`,
`showSaveDialog(options)` / `showSaveDialog(parent, options)`, and
`showMessageBox(options)` / `showMessageBox(parent, options)`. They return
Promises of the A2/A3/A4 values. Explicit null/undefined parent means absent;
a supplied BrowserWindow is resolved through its private brand/identity owner,
not a public numeric id. The single extraction owner is
`packages/@keld/electron/src/browser-window.ts`: a package-private module
reader is installed from BrowserWindow's class lexical scope so it can test
the genuine `#ref` brand, call the existing private `#state()` live-mirror
check, then return that same immutable `#ref`. The reader has no duplicate
WeakMap, Symbol key, generation store or copied liveness predicate. It does
not read public `id`, call `isDestroyed()`, use `instanceof`, invoke getters
or call through caller-controlled methods. Null/undefined omission is handled
by dialog overload normalization; any supplied wrong-brand object, prototype
forgery, Proxy (including a Proxy of a genuine wrapper) or revoked Proxy is
refused before sending. A stale/destroyed wrapper or revoked mirror fails the
existing #state path. The helper is imported only by dialog.ts, never re-exported
from the public Electron index or exposed as a public BrowserWindow method.
This local check does not replace Host identity/parent checks at admission,
UI claim and result publication. Unsupported or wrong-type options fail at the facade
boundary before serialization; input validation may throw synchronously as
the pinned JS boundary does. Host policy/lifecycle/native failures reject the
Promise with canonical typed errors. No timeout manufactures a user result.

The package-private `LifecycleLink.callDialog(payload, deadlineMs)` follows
existing callAuth on that same WorkerLink, checking closed/Quit state; it is
not a new public generic request facility. The API dialog service owns bounded
nonce allocation, typed wire encoding/decoding, and one same-link cleanup CALL
after IPC006. The Electron facade owns option/result compatibility only.
The default caller deadline is finite 600,000 ms and starts before Host
admission; the independent Host deadline and cleanup bounds remain distinct.

### 4.2 Permission, channel and profile

`keld.permissions.jsonc`: **exact first-proof declaration**

```json
{"app":{"system":["dialog"]}}
```

The `keld-guard` owner adds one **system-literal** evaluator for
`Principal::AppProcess` and the "/app/system" array. Exact `dialog` string
only; duplicate/unrecognized/broad wildcard literals must not grant
anything. This is **not** the old prefix/path matcher and not a separate
broker-local permission test. Add one shared `keld-guard::capability::DIALOG`
constant. `dispatch_privileged` borrows the host-selected verified manifest
and evaluates the exact system grant once before allocating a panel.
The requested parent cannot override the guard. Extend the owning `evaluate`
and `json_pointer_for` capability branch; no copied parser or path/glob policy.
Every wire operation, including Cancel, is evaluated against the same immutable
verified generation manifest. A live matching Cancel additionally checks owned
custody. Revocation, link loss and Quit invoke Core-owned retirement directly;
there is no new after-revocation wire permission exception and no weakening of
the channel table's `Authority::Guarded` contract.

At implementation, consume **the present** `crates/keld-ipc/src/channel_table.rs`
and append one `dialog` `GuardedCall` entry using that day's next real numeric
id; append allocation record, regenerate the single TypeScript channel table,
and retain protocol version 2. No reserved speculative id, no separate codec,
no wildcard admission. A new guarded channel's constant is imported by
`@keld/api` from the canonical generated transport. All new guard/receive
rules get their existing hostile-frame and drift-test coverage.

The host must capture the **verified** policy at the current role's admission
under KEL-102. Legacy is never inferred from calling `dialog`; app source,
profile label and later `node:fs` OS enforcement are independent. The dialog
broker itself returns no FsBroker handle/scope. This PR also reconciles the
`dialog` row in `docs/architecture/05-webview-and-native.md`: the **GH478
legacy first-proof** returns a selected path as data, with no scoped-path
grant or change to the verified manifest. Under Strict, a separate approved
F06-T15/user-intent and FS-policy authority must mint any future capability;
the dialog broker must never infer one from a click. A later strict grant
requires its own spec/architecture amendment, OS containment and guard tests.
Thus an immediate guarded `fs.read/write` after selection is denied in the
absence of a pre-existing independent grant. Explicit legacy `node:fs` access
remains ambient and is disclosed as forfeited containment.

### 4.3 Native presentation and completion

- Enforce a finite **request** and **reply** envelope bound before posting UI
  work. The first-proof proposed ceiling is 16 KiB encoded options,
  32 returned file paths and 16 message buttons, with a bounded total response
  of 256 KiB. The shared kipc frame ceiling also applies. Reject limit+one
  before native presentation where known, and before reply allocation on
  native selection. These are **proposed limits**, not shipped API claims.
- The first-proof option grammar is **source-exact, bounded and typed**.
  The public facade checks every own enumerable key before its codec drops
  JavaScript `undefined`. For each *known optional* key,
  **absent and present-but-`undefined` mean the same omission**; `null`,
  a wrong type or an **unknown key (even with `undefined` value)** is a typed
  unsupported/invalid-input refusal before UI. No silent ignored extension
  properties or lossy `JSON.stringify` coercion. The host validates the
  canonical typed payload again before allocating native state.
  - **Open:** only `defaultPath?:string`, `filters?:FileFilter[]` and
    `properties?:string[]` in the pinned first-proof call. `defaultPath`
    accepts an absolute directory, absolute file or a filename hint, as
    the pinned Electron docs specify; no NUL and at most 4,096 UTF-8 bytes.
    Absence keeps the OS default. `properties` accepts exactly
    `openFile`, `openDirectory` and `multiSelections`, with at most three
    distinct entries. Absent/empty uses native single-file default;
    `openFile`+`openDirectory` enables **both** file and directory
    selection on macOS (`canChooseFiles = true` and
    `canChooseDirectories = true`); neither flag overrides the other.
    Directory-selection precedence on Windows/Linux is not a macOS rule. `multiSelections` preserves the selected order; 32 paths
    maximum. Requested `showHiddenFiles`, `createDirectory`,
    `noResolveAliases`, `treatPackageAsDirectory` and other flags remain
    unsupported until independently observed and approved. This deliberately
    limits Electron parity: if the *real* unmodified draw.io fixture requires
    one of them, the matching #477 cell remains fail.
  - **Save:** `defaultPath?:string`, `filters?:FileFilter[]` and the known
    optional `securityScopedBookmarks?:false`, using the same omission/path/filter
    rules. Open admits the same false-only bookmark option. Absent, undefined
    and false request no bookmark; true is unsupported before native effects.
    An unimplemented Save `properties` member is rejected before UI.
    `securityScopedBookmarks:true` is also rejected before any OS effect;
    the first-proof Save creates/writes no destination itself.
  - **Filters:** at most 32 `{name:string,extensions:string[]}` elements
    (no extra own keys), each name nonempty and at most 256 UTF-8 bytes,
    each extensions array 1–32 unique entries with 1–128 UTF-8 bytes
    per extension. Admit a simple extension without slash, dot-prefix,
    NUL or wildcard, or the exact `"*"` all-files token; reject other
    globs and malformed objects before UI. An empty filters array is the
    native unfiltered case. These ceilings are first-proof caps, not Electron caps.
    The pinned macOS `SetAllowedFileTypes` initially applies the first group,
    and for two or more groups creates a labelled `NSPopUpButton` accessory;
    selecting a group replaces the allowed types with that group's extensions.
    The exact `*` token makes only that selected group unrestricted. It does
    not union all groups. It reduces a compound extension to its final suffix
    and deduplicates that group's suffixes. Preserve names/group order and
    actual selector behavior: the original renderer's ordinary Open supplies
    draw.io, VSDX, Mermaid and All Files as four groups. A first-group-only
    implementation and a union of all groups fail different native controls.
    Reuse pinned AppKit bindings and implement actual group-switch behavior;
    any added type-identifier binding or deprecated-API exception requires its
    owning dependency/lint review, not an unreviewed generic filter abstraction.
  - **Message (shared with GH564):** `message:string`, optional
    `type` from `none|info|error|question|warning`, `buttons:string[]`,
    bounded `defaultId`/`cancelId` indexes, `normalizeAccessKeys:boolean`,
    `title:string`, `detail:string`, `noLink:boolean`, `checkboxLabel:string` and
    `checkboxChecked:boolean`. The actual draw.io values
    `type:'question'`, `title:'Confirm'` and `noLink:true` are **admitted**.
    `title` is validated but inert on macOS, matching pinned native Electron.
    `detail` maps to NSAlert informative text: the original parentless updater
    error path supplies it, so it is not silently unsupported in this MVP. Pinned Electron documents `noLink` as Windows-specific;
    on macOS validate its boolean type, then give it **no native effect**.
    Unknown message options (including icon and unsupported `AbortSignal` in
    the first proof) fail before UI. Button and text fields stay inside the
    request's 16 KiB envelope. Omitted/empty buttons use the native OK result
    mapped to response 0; defaultId omission uses the native default (-1 in
    canonical input), and cancelId omission follows pinned Electron's fallback
    calculation. Explicit indexes must address the effective button set.
    Normalize access keys before native labels and preserve actual Return/Escape
    behavior. Checkbox state is read from the actual suppression control;
    checkboxChecked without a nonempty checkboxLabel is invalid.
- This exact *unmodified source* fixture is an admission gate, not a mock:
  `showOpenDialog(win,{defaultPath,filters,properties})` and
  `showSaveDialog(win,{defaultPath,filters})` at draw.io lines
  4195–4219, plus `showMessageBoxSync(mainWindow,{type:'question',
  buttons:['Save','Cancel','Discard Changes'],defaultId:0,cancelId:1,
  noLink:true,normalizeAccessKeys:true,title:'Confirm',message:...})`
  at lines 876–887. GH499's approved migration direction may replace only the
  synchronous call/assignment with awaited async response under its separately
  approved exact patch; the options and this native contract remain unchanged.
  Record real runtime optional values, test both
  present-`undefined` and absent separately, then require actual Mac
  selected/Cancel/open/save and clicked 0/1/2 outcomes. A source parse
  without real panel observation does not flip #477 to pass.
  A later option is admitted only with its native positive and independent
  negative control; the limits above and all unsupported flags remain
  explicit divergences until separately approved.
- UI presentation is exactly once after authorization and parent lookup.
  Open/Save use `beginSheetModalForWindow_completionHandler` for a supplied
  valid parent and `beginWithCompletionHandler` only for an absent parent; the
  latter is modeless, not an application-modal loop. Message uses NSAlert
  sheets for a supplied valid parent and `runModal` deferred in a one-shot
  main CFRunLoop block **outside** Tao's callback only for an absent parent.
  Hidden supplied-parent runtime behavior is a separate upstream/Keld oracle
  gate; the synchronous hidden-parent fallback is not this async contract. Never call runModal
  directly inside the existing main-owner service: Tao's in-callback guard
  prevents its ordinary command delivery there. The actual Keld WV bridge
  positive/negative experiment in §10 establishes this scheduling choice,
  not a shipping Host cancellation implementation. Main native ownership
  survives modal unwind; no state/registry borrow is held across it. Final
  replies are scheduled after callback transaction unwind. Open/Save sheet
  completion may precede invisibility: order out and observe actual closure.
  Extend the existing main-owner resolver to a Core-validated WebviewId through
  the live views map; preserve Auth's original anchor adapter. Reuse existing
  window_registry admitted_entry/admitted_view identity checks, never copy
  them. Observe current visibility on UI for oracle/custody evidence, never
  use a guessed JavaScript mirror to choose an unapproved fallback.
- At most **one panel per authenticated role** may be outstanding, whether
  invoked by the sync facade, an async facade or another role-owned Worker.
  An overlap (including a nested callback or a same-window retry) fails
  `DialogBusy` before allocating a second native object; globally retained
  panels have a **Host-wide cap of 8** and typed refusal. A reservation remains
  charged until physical native closure and callback quiescence, including
  tombstoned delayed UI work. Do not nest an application-modal alert or add
  an implicit AppKit sheet queue when an existing native modal owns admission;
  refuse Busy until that owner drains. A bounded DIALOG command still reaches
  opcode decoding at panel capacity so Cancel is not rejected before its
  cleanup intent can be known. Only new Open/Save/Message consume panel slots.
  No native queue can outlive its role or turn into a second modal loop.
- A panel's retained handle/callback and its owning generation are enrolled
  in a Host-owned custody table before native presentation. Completion first
  validates the still-current owner/parent, records a single terminal result,
  removes UI custody, then sends the correlated REPLY/ERR if a live
  authorized caller still exists. Losing REPLY does not undo a user's choice.
- Role retirement/parent destruction/Host ending cancels UI effects on the
  **real UI thread** without waiting for application JS or treating a typed
  `WorkerLink` terminal wake as native dismissal. The native completion may
  arrive after retirement; it is ignored, never sent to a successor.
- The **Host** owns a distinct monotonic human-interaction timer, proposed
  maximum **600_000 ms after authenticated Host admission**; it is not an
  increase to Host (20 s), Guardian (10 s), cleanup (5 s), kipc I/O or FS
  clocks. On expiry, *physically* dismiss the panel on the UI thread and
  produce a typed *expired* outcome, never a clicked index. Terminal Quit
  follows existing shutdown deadlines, not the full dialog budget.
- A Bun caller's `callBlocking` or `call` deadline starts **before** Host
  admission, on another local clock. Equal 600 s counters therefore do
  **not** ensure timely native cleanup. Present GH527 WorkerLink
  `abandon` only reaches its own Worker and can yield `KELD-IPC-006`
  without telling AppKit. The proposed fix consumes the **same existing
  WorkerLink and guarded dialog channel**: the facade allocates a nonce
  before sending, and on local timeout/abandon posts an asynchronous,
  bounded `DialogRequestV1::Cancel { nonce }` as a **fresh CALL(c_cancel)**
  over that link before rethrowing the original error. It must not await
  Cancel from synchronous app code, reuse `CALL(c_orig)`'s correlation or
  retry its effect. On original CALL receipt the Host binds the authenticated
  role/generation, nonce, `c_orig` and parent in bounded custody **before**
  scheduling UI. The Host binds Cancel to that exact existing owner, marks a
  tombstone and dismisses its owned panel; a cancellation arriving *before*
  queued UI presentation prevents the panel from ever appearing. A later
  success or stale callback cannot overturn the tombstone. A Cancel whose
  nonce was never admitted cannot allocate UI, tombstone storage or affect a
  different role. Nonzero u64 nonces increase per authenticated generation;
  reject reuse/exhaustion, never wrap. Original reservation precedes UI enqueue,
  and same-link FIFO orders a later Cancel after that reservation. Retain the
  one active record/tombstone until callback quiescence; at most one fixed
  last-completed nonce/disposition supports idempotent completed cancellation.
  No unbounded history or caller-created pre-admission tombstone map.
  The wire Cancel still passes the exact dialog guard under that generation's
  immutable verified manifest, then matches custody. Core independently owns
  retirement after revocation/death; it accepts no new after-revocation CALL
  on the strength of a stale grant. No new operation is authorized by Cancel.
  The single Host writer sends an independent ACK/`REPLY(c_cancel)` or
  refusal for the Cancel CALL. For a **matched, admitted original still
  pending**, it additionally sends exactly one original-correlation
  terminal frame: a registered typed `ERR(c_orig)` once cleanup wins
  before a native result commits, or the genuine already-committed
  `REPLY(c_orig)` if the click won first. An unmatched/foreign Cancel
  cannot affect another original correlation or fabricate its terminal.
  Never fabricate a clicked Cancel
  or overwrite a committed result, send duplicate terminals, assume
  cancelled execution was rolled back, or replay an unknown execution.
  GH527 WorkerLink currently retains an abandoned `c_orig` until its own
  `REPLY/ERR(c_orig)` arrives; an ACK for `c_cancel` alone leaves that
  entry counted toward `MAX_ABANDONED_CALLS=256` and eventually
  `KELD-IPC-027`. The late original terminal is discarded by the Worker
  but releases that one reservation exactly once; it does not answer the
  already-timed-out app caller. On a dead link no terminal can be delivered:
  role-generation retirement independently clears Host/native custody,
  with no promise of a wire ACK or original response. Exact typed CallError
  selection and this guard/wire exception require the GH508/GH527 and
  GH478 security/Core owners' approval before implementation.
- **Proposed measured cancellation bound**, requiring joint GH478/GH564
  approval: at most **10 s from a caller's local timeout** to physical
  closure with a healthy existing link, or **5 s from Host-observed role/
  link retirement** to closure if the transport is already terminal.
  These are *new dialog cleanup acceptance bounds*, not increases to
  existing service/shutdown clocks. Lost Cancel delivery is not a success
  case: link liveness/retirement must close native custody, and the
  host-owned interaction timer is only the last-resort cap. If the live
  transport can abandon without delivering Cancel, or late Host admission
  can still create a panel, GH478/GH564 remain **BLOCKED** until their
  transport/Core owners supply a proved same-link relay. Neither an
  elapsed JS timer nor an ERR/ACK is a physical release witness.
- The Host must continue dispatching this existing-channel cancellation
  and Quit while native Cocoa modal behavior is active. Do not hold a
  guard/registry lock across `NSAlert` callbacks. A second synchronous
  invocation inside a state applier already receives GH527
  `KELD-IPC-005`; independent competing async calls receive the Host-owned
  `DialogBusy`. Tests must distinguish both guards, delayed Host admission,
  callback-after-cancel and a healthy subsequent dialog, with real native
  handle counts. No second ring, Worker, link, modal pump or unbounded
  queue may be introduced.

### 4.4 Result, errors and no-effect distinctions

Async Electron shapes follow pinned `dialog.md`. A genuine button/destination
selection is a value; deliberate native Cancel is a **value** with
`canceled:true`; guard denial, stale/forged parent, role loss, native failure
and expiry are **typed errors**, not fabricated Cancel/Save values.

- Guard denial: existing `KELD-GUARD*` call error, with fix at `/app/system`.
- Transport/link lifecycle: preserve existing `KELD-IPC-022/023` effect
  uncertainty. A partially sent request must not be replayed by the facade.
  A same-link Cancel is idempotent cleanup only: if the user already clicked,
  it cannot roll back the choice or manufacture a new answer.
- Parent missing, stale or owned by another role: typed *InvalidParent*
  outcome before panel; native parent destruction after admission produces
  *ParentRetired* with physical dismissal, not silent cross-window fallback.
- Second overlapping dialog for a role: typed *DialogBusy* before UI.
- True OS user Cancel is the public result, not a terminal error.
- Expiry/native failure produce typed `DialogExpired`/`NativeFailure`
  outcomes. The owning error registry decides the **currently unused** stable
  `KELD-NATIVE-*` numbers before implementation; do not guess numeric codes
  or reuse FS-specific numbers in this draft.

No raw path content, bearer token, native pointer, PID or private profile data
is printed in normal diagnostics. Test artifacts with sensitive paths use
isolated explicit fixtures and remain private until redacted.

### 4.5 Platform and interoperability

- **macOS (first proof):** objc2 / objc2-app-kit using pinned crate versions;
  actual GUI main-thread assertions and parent identity via `keld-wv`.
  Existing `keld-native/AGENTS.md` permits production unsafe only in specific
  Auth methods. If this dialog needs a new unsafe operation, its exact code,
  safety proof and owner/instruction change require prior independent gate;
  do **not** insert `unsafe` under the Auth allowance. No new Cocoa crate.
- **Windows/Linux:** no production API behavior changed by macOS source. A
  future backend may use WebView2/Win32 and portal-first GTK, each with
  its own owned spec, real OS acceptance and permission/result review.
- **Path spelling:** macOS returns absolute POSIX paths with `/` separators.
  No synthetic scope/root canonicalization on the output; separate alias
  resolution and symlink behavior in a real open/save fixture before scored
  claims. An unsupported `noResolveAliases` option is refused, not silently
  ignored. No path grants are minted even if the native panel resolves aliases.
- **Thread/reentrancy:** #442's real-mac run shows main-process JS defers
  during Electron's synchronous modal but native common-mode callbacks may
  execute; it does not prove any particular KELD command enters that mode.
  Never use `dispatch_async(main)` alone as the UI-completion oracle. Both
  the request's presentation command and its independent Cancel/retire
  command must demonstrably run with the *real* KELD modal run-loop mode
  active; a default-mode-only cancellation that waits for an alert's
  natural dismissal fails the bounded cleanup test.

### 4.6 Reuse, failure independence, rejected alternatives

Ownership atoms are independently falsifiable: guard before UI (A1), parent
identity/visibility (A5), native lifetime/cancellation (A6–A7), transport
correlation (A9), path-only output (A8), real native evidence (A11),
source-exact option parsing (A12), and timeout/reentrancy cleanup (A13).
A correct typed reply does not prove guard/no-panel; a correct guard does not
prove cleanup; green unit tests do not prove the actual Mac panel.

Rejected alternatives:
1. A new `dialog` socket/Worker, nested JS pump or other blocking transport:
   duplicates GH527's single role link, risks reordering and wider authority.
2. A bare `NSAlert.runModal()` on the Host thread with no measured cleanup
   command delivery: risks deadlock/starvation during role death and Quit.
3. Bypassing the host-owned WindowRef via NSNumber, numeric native handle or
   a pointer supplied by Bun: breaks generation binding and default-deny.
4. Treating a path returned by a dialog as a temporary FsBroker grant: no
   approved grant source, changes the authority set and creates persistence/
   symlink ambiguity. F06-D1 strict work is separate.
5. Returning a default button on expiry, link loss or OS failure: turns an
   unknown/native failure into destructive user consent.
6. Duplicating `keld-guard` permission syntax/constant or kipc channel IDs:
   forks two existing owners; incompatible with GH508/KEL102.

Compatibility fallback for unsupported OS is a **typed unsupported** result,
not an unguarded platform shell chooser or silent async emulation of a sync API.
No performance claim is made in advance of measured real Mac records.

## 5. Boundaries

Implement only after approval, in the existing owners:

- `crates/keld-guard/src/lib.rs` (one system-list evaluator and constant,
  existing dispatch contract), `crates/keld-ipc/src/channel_table.rs`,
  `channel_allocations.txt` and **generated** TS constant through the one
  approved generator, with source/golden-vector negative controls.
- `crates/keld-native/src/dialog.rs` plus allowed crate-local tests,
  module export, canonical request/reply DTOs, UI-affinity/native handle custody.
  Extend the existing `packages/@keld/kipc/scripts/echo-codegen.ts` owner for
  `packages/@keld/api/src/dialog.generated.ts` and drift/golden-vector checks;
  do not create another generator or hand-edit the generated codec.
- `crates/keld-core/src/app_session.rs` is the **existing protected primary
  reader** (`read_primary_frames`); it must win a fresh path-level GH478 claim
  coordinated with current GH824/GH449/GH1073 overlapping writers before the single existing
  `dispatch_privileged`/guarded-CALL seam is extended to route the
  canonical generated dialog channel. Tests may live under
  `crates/keld-core/src/app_session/`, but that directory alone does not
  connect a handler. The same router must bind the live role, verified
  policy, WindowRef, nonce, current generation and Cancel tombstones;
  no second primary reader, fallback dispatcher or bypass of `keld-guard`.
- `crates/keld-wv/src/engine.rs` and **macOS-only** event-loop adapter to
  deliver guarded UI requests and owned cleanup to current native windows.
- `packages/@keld/api/src/dialog.ts` and **one existing app singleton**;
  `packages/@keld/electron/src/dialog.ts` thin async compatibility facade;
  `packages/@keld/electron/src/browser-window.ts` owns the package-private
  genuine-brand extraction of its existing #ref plus reuse of #state liveness;
  `packages/@keld/electron/src/index.ts` exports only the public dialog facade,
  never that internal reader. Coordinate BrowserWindow's active owner first.
- `crates/keld-compat/fixtures/native-corpus` and mapped #477 tests as
  approved by its source/test owner. The already published first #477
  synchronous red baseline remains GH564-owned and stays fail for #478.

Must not touch: `crates/keld-core/src/app_session.rs` or GH449 active writer's
regions without a separately won claim/owner coordination; GH824 runtime
shutdown, `keld-ipc` transport Worker implementation, GuardedPrimary and
Guardian timeouts, current overlapping Core owner regions, Windows/Linux native production,
root workspace dependency pins, `docs/specs/gh527*` or `docs/specs/gh531*`
without a new expressly approved amendment. Shared `tools/ci-inputs.json`
needs its existing single-writer/branch-specific review and canonical rebind.
No existing transport change is implied by the proposed same-link
`DialogRequestV1::Cancel`: if tests show GH527 cannot carry it through
the owned WorkerLink and live modal Host, stop for that owner's explicit
coordinated change and independent wire/lifecycle review.

## 6. Tasks (ordered vertical slices; each gets its own owner/gates)

- [ ] **T1:** Approve exact-content GH478 spec and resolve open questions below.
      No code or channel allocation before this gate.
- [ ] **T2:** Complete *only* missing async #477 expected-fail cells using
      the existing GH566 registry. No fabricated pass, no second parser.
- [ ] **T3:** One exact `app.system:["dialog"]` guard admission with
      negative grant/principal/duplicate/wildcard controls, and one new
      append-only channel table entry using current maximum id + 1.
      Obtain the protected `app_session.rs` single-router owner claim.
- [ ] **T4:** Complete asynchronous **message** vertical behind the guard:
      supplied-parent sheets and deferred absent-parent native operation,
      hidden supplied-parent upstream/Keld oracle and cleanup evidence,
      same-link timeout/Cancel custody, original terminal correlation, physical
      close/quiescence, and healthy next call. No partial UI export before
      cleanup exists. This supplies GH499's qualified async dirty-close route
      and the native broker that GH564 may independently consume.
- [ ] **T5:** Real Mac open/save panels, selected/cancelled shapes, exact
      OS-selected absolute paths and no-FsBroker-grant property; alias/symlink rows
      labelled observed, unknown or divergence.
- [ ] **T6:** Generation-bound native cancellation/reap, deadline, stale
      callback and healthy next operation, exercised on an actual Mac;
      source-exact caller-timeout/cancel tombstone and concurrent-call
      refusal with separate positive and negative native witnesses.
- [ ] **T7:** Thin `@keld/api` and Electron async facades, red-cell flips
      in implementing PR, strict/public-API, guard and exact-head CI gates;
      prove optional present-`undefined` source inputs and keep public
      compatibility score honest.
- [ ] **T8:** For unchanged synchronous app source, GH564 independently
      consumes T4's broker and existing callBlocking under its own approved
      spec. For the approved GH499 adapted route, its owner instead qualifies
      exact async patch bytes and native dirty-close reentrancy. Neither
      route is inferred passed from the other; no additional source hunk.

The first production vertical contains the message portions of T2/T3/T4/T6/T7
together: guard, canonical channel/codec, Core admission/custody, real native
message operation/cleanup and thin async facade with meaningful tests. It must
not land a public dialog export or a routed production stub awaiting later
cleanup. Open/Save is the next complete vertical and retains its filter/path
obligations; the remaining public methods are not invented placeholders.

T4/T5 may be independently tested after the shared T3 gate; T6 validates their
composition, not just their individual wrappers. No incomplete T4-only
implementation may be labelled a complete GH478 acceptance.

## 7. Test plan (failure-first; source/oracle/OS distinguished)

| Criterion | Test owner and observed positive | Independent falsifier (one fault) |
|---|---|---|
| A1a | Existing boot preflight: malformed/duplicate-object policy or mismatched digest retains its typed startup failure and zero new app child/listener/window/panel | Move verification after app startup: independent resource counters become nonzero |
| A1b | Valid verified app-role CALL: exact grant versus absent/wrong/duplicate-array/unknown/wildcard literals and zero native constructors on denial | Remove guard evaluation: denied CALL creates UI |
| A1c | Bootstrap/session rejects untrusted/stale/wrong role; separately evaluator rejects non-AppProcess before grants | Reuse app grants for another principal or admit untrusted link: identity owner check fails |
| A2 | Real Mac: select one file, then Cancel; observe exact filePaths and zero scopes. With both `openFile` and `openDirectory`, separately select an ordinary file and a directory; with `multiSelections` also select both in one result. Observe both native selection flags and exact selected paths. | Return last selected path on Cancel, or force directories-only when both flags are present: ordinary-file selection/result fails; a files-only mutant fails the directory case. |
| A3 | Real Mac: select save target, then Cancel; file unchanged until app writes; `securityScopedBookmarks:true` denied before UI | Create/truncate target in supported dialog mode or silently accept bookmarks: sentinel/denial control fails |
| A4 | Real Mac: distinct button index 0/1/2, checkbox off/on; async Promise value shape | Always return defaultId: a non-default click fails |
| A5 | Supplied parent uses async sheet API; absent uses detached operation; run separate real Electron/Keld hidden-parent oracle without assuming sync fallback. Genuine live wrapper passes package-private brand/#state reader; stale/forged/wrong/proxy/revoked objects fail before send; Host rechecks owner/ref at UI and reply | Public id/getter/instanceof substitution admits forgery or executes a callback; bypass Host incarnation/role check attaches/replies to wrong owner; hidden→detached fallback mutation violates the selected source branch |
| A6 | Subprocess + real panel: kill role and inject late callback, next call succeeds | Drop callback owner without closing panel: native census detects leak |
| A7 | Actual event-loop delivery while modal active; host Quit remains bounded | Deliver UI cleanup only in default mode: watchdog/control fails |
| A8 | Path from panel; guarded FS read denied with no grant; zero grant snapshots change | Treat selected path as FS scope: denied read succeeds and fails |
| A9 | Boundaries for 0/max/max+1, malformed/unknown enum, oversized reply | Remove selected-path ceiling: max+1 is accepted and fails |
| A10 | Actual non-macOS typed unsupported, strict vs explicit legacy disclosure | Return fabricated Cancel on unsupported platform: cell fails |
| A11 | GH566 v44.4.5 expected-fail cells actually run; flip only against native proof | Remove mapped test from execution: manifest admission fails |
| A12 | Run pinned unmodified draw.io Open/Save/Message argument objects, including present-`undefined` and absent, native UI result/cancel/0-1-2 clicks; show each known option is admitted | Treat all present-`undefined` as unsupported or remove `noLink` allowlist: source-exact valid call fails before panel; accept unknown `bogus:undefined`: pre-UI denial control fails |
| A13 transport/correlation | Deterministic real WorkerLink/Core fixture: deliver `CALL(c_orig)`, induce `KELD-IPC-006` local timeout/Worker abandon, then `Cancel(c_cancel)` with distinct ACK/`REPLY(c_cancel)` **and** terminal `ERR(c_orig)` (or genuine prior committed `REPLY(c_orig)`); verify original abandoned-id release exactly once, zero retained abandoned entries and next call success after **256+1 = 257** successful timeouts/cancels on one healthy link; dead-link cleanup has no fabricated wire reply | ACK `c_cancel` but omit `ERR(c_orig)`: original abandoned entries accumulate and 257th cleanup closes link with `KELD-IPC-027`; double-terminal `c_orig` or pretend Cancel ACK retires original: independent correlation/once-only assertions fail |
| A13 cleanup admission | One active dialog and original correlation remain pending while another same-channel Cancel reaches actual Core decode; no second native panel | Reuse one-panel capacity as a pre-payload rejection of every CALL: Cancel is refused and cleanup fails |
| A12 macOS filters | Four actual source groups; first group initially, VSDX and All Files selectable with corresponding allowed types | Union groups accepts VSDX under first-group restriction; first-only implementation cannot change to VSDX/All Files |
| A13 native/owner | Real Mac: delay Host-to-UI admission until after Bun `callBlocking` expiry, same-link Cancel and Host tombstone, actual physical panel dismissal or suppressed late presentation, native handle count zero within proposed bound, stale click fenced and healthy next call; separate original/Cancel correlation captures and denied cross-role/generation Cancel; second concurrent async call denied without a second panel | Remove Host Cancel relay or tombstone, suppress modal-mode physical cleanup or permit stale callback: late panel/leaked handle crosses bound despite transport ACK/ERR; remove `DialogBusy`: two native panels appear |

The upstream October 24 Electron oracle ran 75 pinned observations, including
an *async-positive* reentrancy control. It is **input evidence only**. The
native implementation must have **new** real KELD host logs and actual OS
click/sheet/cleanup observations; mocks test state logic but never replace
macOS acceptance. Tests await observed native events, not sleeps; a bounded
external watchdog is only a kill switch. No CI skip, retry or softened assertion
may count as a pass. Cross-OS lanes run applicable pure tests and retain
unknown native implementation status.

## 8. Review gates triggered

- **Permission model:** yes — new exact `app.system` evaluator, guard before
  UI, immutable role policy, deny-before-effects and negative grant controls.
- **Public API:** yes — new async `@keld/api` and Electron facade members,
  options, return shapes and errors.
- **Wire protocol:** yes — one new **version-2** channel and typed postcard
  operation schema, ownership/correlation and hostile-frame tests. No frame or
  existing-channel renumbering.
- **Unsafe:** conditional **blocking** gate if objc2/AppKit requires an
  operation not in the existing `keld-native/AGENTS.md` allowlist; obtain
  exact owner, safety proof and independent review first. The spec PR adds
  **no unsafe**.
- **Dependency addition:** no new dependency in this draft; implementation
  may need additional already-pinned objc2 feature gates. Any actual
  feature/dependency delta needs the normal dependency review.
- **Real OS:** Mac UI-thread, native panel creation, cancellation, generation
  cleanup and host recovery; CI-only Rust/Bun is insufficient.

## 9. Perf impact

No performance improvement is claimed. New native dialogs are cold user
interactions. During implementation measure request/frame bytes, outstanding
native handles/queued request count, OS panel lifetime, cancellation tail and
retained memory while a caller is parked; compare against architecture 01 §5
and GH527 outbound/inbound caps. Credit extension is conditional on observed
retained traffic (GH528), not automatically introduced. Never raise existing
Host/Guardian/IPC deadlines to compensate for UI blocking.

## 10. Owner-delegated design decisions; implementation gates remain

The repository owner accepts the exact v2 design under the user-delegated
decision, recorded at [GH478 comment 6476354568](https://github.com/gyldlab/keld/issues/478#issuecomment-6476354568). The choices
below are fixed for this spec; implementation, generated channel allocation,
named owner claims/reviews and native acceptance remain separate gates. This
design approval does not claim native/Host/product acceptance has passed.

1. **Clocks:** 600,000 ms absolute Host interaction deadline from original
   admission; physical suppression/cleanup within 10 s of live-link caller
   timeout, or 5 s of Host-observed role/link retirement. Existing Host20,
   guardian, FS and IPC clocks remain unchanged. The bounds are acceptance
   requirements, not measured by the primitive probe. Failed bounded cleanup
   invokes the existing fatal/custody owner, never fabricated user Cancel.
2. **Permission:** exact `app.system:["dialog"]` in the guard-owned evaluator,
   pointer `/app/system`, principal-first default deny, duplicate/unknown/
   wildcard refusal. Every wire Cancel still uses dispatch_privileged and
   matching existing custody. Immutable generation revocation/death is cleaned
   up by Core lifecycle, without a wire permission exception.
3. **Wire/errors:** typed Open/Save/Message/Cancel and CancelDisposition on
   one append-only guarded channel, original and Cancel correlations distinct.
   At inspected main a866a7c5, channel allocations are echo1/fs2/lifecycle3/
   auth4/window5, so6 is the next candidate. NATIVE009–016 have no current
   registered/source use; proposed categories are InvalidDialogRequest,
   Unsupported, InvalidParent, ParentRetired, Busy, Expired, Cancelled and
   NativeFailure in that order. These are not reservations: the designated
   schema/error writer refreshes the actual allocation tables immediately
   before recording them, preserves every existing ID, and regenerates through
   the canonical owner. Role retirement keeps existing IPC023 semantics.
4. **Native:** callback-based file panels/sheets and deferred-outside-Tao
   absent-parent NSAlert as §4.3. A supplied hidden parent remains on the
   pinned async sheet branch; actual upstream/Keld hidden behavior is an
   explicit unrun acceptance gate, not a Keld safety divergence. One actual Keld WV experiment on macOS26.5.1
   with verified main-equivalent F2 source observed visible/modal alert,
   real AppWindowCommand::Quit, service abort, response-1001, invisible/
   nonmodal state and normal drained exit. Direct-in-service entry blocked
   delivery; removing only native abort left the modal hung. This qualifies
   the existing-loop scheduling primitive only, not Host DIALOG routing.
   Exact Native/WV unsafe operations and any feature/dependency delta retain
   their independent owner gates; the private probe grants no production allow.
5. **Paths:** data only, no FS scope/handle/bookmark/file-write effect.
   Default alias resolution is native; unsupported noResolveAliases and true
   bookmarks reject before UI. Symlink/alias selections require separate real
   fixtures before parity claims. Approve the accompanying architecture05
   reconciliation, while strict intent-derived grants remain F06-T15-owned.
6. **Source/public contract:** source-exact Open/Save options and actual Mac
   selectable filter groups; async Message also admits detail and omitted
   buttons. Parentless Message remains supported, including the source's boot
   error form. Adopt GH499's already-approved macOS-only async dirty-close
   direction as the minimal adapted-MVP route; exact patch bytes and real
   repeated-close/two-window/save-failure/late-result tests still belong to
   GH499. The original source needs GH564 if that patch is omitted. Whole-app
   correctness is not inferred from an isolated caller or a configured update
   suppression field: GH452 and the inert GH491 adapter must supply their own
   actual environment/no-side-effect proof.

**Passed versus open evidence.** Pinned Electron44.4.5 file_dialog_mac.mm
explicitly initializes the first filter group and installs a popup accessory
for multiple groups; selectFormat replaces allowed types with the selected
group, including its `*` behavior. It does not union all groups. The actual
original Open renderer passes four groups. This is primary-source proof;
native Keld filter switching is not yet executed.

Current WorkerLink.call has a pending map and supports simultaneous async
correlations; the single blocking reply slot restricts callBlocking nesting,
not all CALLs. A real Bun1.4.4 WorkerLink probe with a mock peer observed
same-channel original1/cancel2 while original1 was pending, separately routed
both replies, then original3 timeout IPC006/cancel4/late original terminal and
healthy5. It used one existing link and no production channel allocation.
This proves producer feasibility only. Actual Core DIALOG capacity, custody,
guard, original-terminal and257-cycle native recovery remain acceptance work.
There is no existing LifecycleLink.asyncRequest API: add only its package-private
dialog method following callAuth/callFs on the same WorkerLink; no replacement
transport or generic new request framework.

**Ownership before first edit.** GH478's latest spec-only claim expired
2026-10-27T22:30Z; GH449 F2's Core claim expired Oct28 11:30Z; GH824's latest
bounded shared-Cargo claim was released. These facts do not grant a new writer.
GH1073 retains a live claim until Oct29 02:30Z on named Windows/Core tests and
GH527 spec rows. Open PR1282 and1018 touch app_session.rs; PR1150 touches window
registry/router. PR834 is merged. Root must designate one actual GH478 Core
writer and the shared channel/schema/error/dependency writer, post/read back
the path-level claim, and reconcile those exact overlapping diffs before code.
No protected file is deemed free merely because an older claim expired.

Approval authorizes the bounded macOS test-first implementation sequence, not
GH564 sync completion, GH499 exact patch/native success, strict containment,
full app scoring, release, or merge. A product operation remains unknown until
its own acceptance passes. Failure of the chosen same-link/native composition
stops that vertical for a named owner correction; it does not authorize a
second principal/link, event pump, queue or broad permission.
