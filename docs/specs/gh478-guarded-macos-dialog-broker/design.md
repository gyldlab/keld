# Design

Part of: [index](index.md)
Owns: section 4

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
// In the owning IPC/native interface; exact identifiers are chosen on admission.
struct DialogOwner {
    window: Option<WindowRef>, // GH531 host incarnation, never OS window number
    role: RoleInstance,        // authenticated host metadata, not caller payload
    correlation: CorrelationId,
    request_nonce: DialogNonce, // lookup only, never a permission
}

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
}
```

**Important:** This sketch does *not* mint a second `WindowRef` or
`RoleInstance`. It refers to approved GH531 and KEL75 types when their
actual existing owner is connected. Only `parent`, the bounded cancellation
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
The requested parent cannot override the guard.

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
  - **Save:** only `defaultPath?:string` and `filters?:FileFilter[]` for
    the pinned draw.io call, using the same omission/path/filter rules.
    An unimplemented Save `properties` member is rejected before UI.
    `securityScopedBookmarks:true` is also rejected before any OS effect;
    the first-proof Save creates/writes no destination itself.
  - **Filters:** at most 32 `{name:string,extensions:string[]}` elements
    (no extra own keys), each name nonempty and at most 256 UTF-8 bytes,
    each extensions array 1–32 unique entries with 1–128 UTF-8 bytes
    per extension. Admit a simple extension without slash, dot-prefix,
    NUL or wildcard, or the exact `"*"` all-files token; reject other
    globs and malformed objects before UI. An empty filters array is the
    native unfiltered case. These ceilings are proposed first-proof caps,
    not an assertion that Electron has the same caps.
  - **Message (shared with GH564):** `message:string`, optional
    `type` from `none|info|error|question|warning`, `buttons:string[]`,
    bounded `defaultId`/`cancelId` indexes, `normalizeAccessKeys:boolean`,
    `title:string`, `noLink:boolean`, `checkboxLabel:string` and
    `checkboxChecked:boolean`. The actual draw.io values
    `type:'question'`, `title:'Confirm'` and `noLink:true` are **admitted**.
    `title` is passed through according to real AppKit presentation
    support; it may be visually absent on macOS and must not trigger a
    rejection. Pinned Electron documents `noLink` as Windows-specific;
    on macOS validate its boolean type, then give it **no native effect**.
    Unknown message options (including icon, detail, and unsupported
    `AbortSignal` in the first proof) fail before UI. Button and text
    fields stay inside the request's 16 KiB envelope.
- This exact *unmodified source* fixture is an admission gate, not a mock:
  `showOpenDialog(win,{defaultPath,filters,properties})` and
  `showSaveDialog(win,{defaultPath,filters})` at draw.io lines
  4195–4219, plus `showMessageBoxSync(mainWindow,{type:'question',
  buttons:['Save','Cancel','Discard Changes'],defaultId:0,cancelId:1,
  noLink:true,normalizeAccessKeys:true,title:'Confirm',message:...})`
  at lines 876–887. Record real runtime optional values, test both
  present-`undefined` and absent separately, then require actual Mac
  selected/Cancel/open/save and clicked 0/1/2 outcomes. A source parse
  without real panel observation does not flip #477 to pass.
  A later option is admitted only with its native positive and independent
  negative control; the limits above and all unsupported flags remain
  explicit divergences until separately approved.
- UI presentation is exactly once after authorization and parent lookup.
  Prefer OS completion callbacks to blocking the Host UI loop. A visible
  parent uses a sheet; parentless/hidden-parent behavior uses the AppKit
  path that reproduces the pinned oracle and keeps cancellation deliverable.
  Electron's internal use of `runModal` is evidence about Electron, not an
  instruction to add a blocking Host implementation or second event loop.
- At most **one panel per authenticated role** may be outstanding, whether
  invoked by the sync facade, an async facade or another role-owned Worker.
  An overlap (including a nested callback or a same-window retry) fails
  `DialogBusy` before allocating a second native object; globally retained
  panels have a **proposed Host-wide cap of 8** (subject to capacity review)
  and typed refusal. This is a Host-owned reservation released exactly
  once on completion/cancel.
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
  nonce was never admitted cannot allocate UI or affect a different role.
  Cleanup is authorized by matching existing ownership even if a previous
  dialog grant was revoked; no new operation gets authorized by Cancel.
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
