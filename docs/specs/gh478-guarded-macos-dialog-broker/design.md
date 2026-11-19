# Guarded native dialog design

Part of: [index](index.md)
Owns: Section 4

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

The first Message vertical assigns postcard request and reply discriminants
`Message = 0` and `Cancel = 1`; unknown discriminants are rejected. Open and
Save are appended only with their real payload contracts and MUST NOT renumber
these values. The sketch above describes the eventual operations, not their
wire order. The rejected empty Open/Save placeholders and the owner decision
are recorded in [GH478 comment 6477314551](https://github.com/gyldlab/keld/issues/478#issuecomment-6477314551).

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
Promises of the AC-2/AC-3/AC-4 values. Explicit null/undefined parent means absent;
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
    **Native Save suffixes:** when the proposed filename has no extension,
    `NSSavePanel.allowedContentTypes` uses its first preferred extension. Save
    admission may therefore also accept the preferred filename extensions from
    the exact `UTType` objects already installed for its one selected group.
    Derive and retain those strings before cloning the existing native delegate's
    filter model. Both the live delegate and final Save reply validate against
    that same active group's values. Never union inactive groups, infer arbitrary
    type-conformance aliases, or admit unrelated suffixes. Open's exact declared
    suffix predicate remains unchanged. Directory classification, path-only
    results, no-write behavior, default-deny Guard and native cleanup ownership
    remain unchanged; no implicit bookmark or filesystem grant is introduced.
    The regression must use an extensionless native default name and require the
    exact OS-appended preferred suffix, while both the stem and resulting path
    remain absent. Separate controls cover declared/native Save positives,
    unrelated Save rejection in the active group, unchanged Open rejection,
    inactive-group aliases and invalid indexes. Pure model acceptance is not a
    substitute for that real shipping-Host panel and subsequent-operation proof.
    Primary native contracts (retrieved 2026-11-01):
    [NSSavePanel.allowedContentTypes](https://developer.apple.com/documentation/appkit/nssavepanel/allowedcontenttypes)
    and [UTType.preferredFilenameExtension](https://developer.apple.com/documentation/uniformtypeidentifiers/uttypereference/preferredfilenameextension).
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

Ownership atoms are independently falsifiable: guard before UI (AC-1), parent
identity/visibility (AC-5), native lifetime/cancellation (AC-6–AC-7), transport
correlation (AC-9), path-only output (AC-8), real native evidence (AC-11),
source-exact option parsing (AC-12), and timeout/reentrancy cleanup (AC-13).
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
