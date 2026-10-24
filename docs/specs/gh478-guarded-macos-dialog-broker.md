# Spec: macOS guarded native dialog broker (GH-478, F06-T2)

Status: draft
Linear: GH-478 (tracker under #517; reference KEL-80) · Owner: gyldlab/keld · Updated: 2026-10-24

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
- [#478 acceptance brief](https://github.com/gyldlab/keld/issues/478),
  [#564 consumer handoff](https://github.com/gyldlab/keld/issues/564#issuecomment-6381751522),
  [#528 transport acceptance](https://github.com/gyldlab/keld/issues/528#issuecomment-6370844113).

The approved architecture is not amended by this **draft**. `keld-native::MODULES`
listing `dialog` means planned surface, **not** that a broker currently exists.
`keld-guard` currently supports scoped arrays under `app.<group>.<action>`;
`app.system` literal-list admission does **not** exist yet and must be implemented
by its owning crate. No call may pretend `evaluate("system.dialog", "dialog")`
implements that different grammar.

## 3. Acceptance criteria (binary observations)

**A1 — exact grant and source.** Given one real app role and verified manifest
`{"app":{"system":["dialog"]}}`, a valid dialog CALL reaches native admission
**once**. With an absent, empty, malformed, duplicated or wildcard-like literal,
non-app principal, unverified digest or wrong role, admission returns an existing
canonical **typed KELD-GUARD denial** whose fix names `/app/system`, with **zero**
NSPanel/NSAlert creation. No developer shortcut and no automatically enabled legacy.

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
native UI object, not by caller-controlled numeric OS handle. A shown parent
uses an attached modal **sheet**; no parent or a valid but hidden parent uses
an independent alert/panel. A stale, destroyed, cross-app or forged parent is
rejected **before** native presentation (not silently retargeted to a different
window). The hidden-parent fallback itself is not a grant widening.

**A6 — independent native custody.** Each admitted dialog owns exactly one
native object, one generation-bound outstanding CALL and one terminal outcome.
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
non-bookmark profile* carry **only** canonical macOS absolute POSIX paths,
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
}

enum DialogRequestV1 {
    Open { parent: Option<WindowRef>, options: OpenOptions },
    Save { parent: Option<WindowRef>, options: SaveOptions },
    Message { parent: Option<WindowRef>, options: MessageOptions },
}
enum DialogReplyV1 {
    Open { canceled: bool, file_paths: Vec<String> },
    Save { canceled: bool, file_path: String },
    Message { response: u32, checkbox_checked: bool },
}
```

**Important:** This sketch does *not* mint a second `WindowRef` or
`RoleInstance`. It refers to approved GH531 and KEL75 types when their
actual existing owner is connected. Only `parent` and validated options
are on the wire; `role`, verified manifest and effective generation are
injected by Host acceptance from the authenticated link. The requester is
never trusted to supply the generation, a principal, an AppKit pointer or
a guard-permit index. Use the existing declared postcard/kipc frame and
`CallError` model, not ad-hoc JSON frames.

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
broker itself returns no FsBroker handle/scope.

### 4.3 Native presentation and completion

- Enforce a finite **request** and **reply** envelope bound before posting UI
  work. The first-proof proposed ceiling is 16 KiB encoded options,
  32 returned file paths and 16 message buttons, with a bounded total response
  of 256 KiB. The shared kipc frame ceiling also applies. Reject limit+one
  before native presentation where known, and before reply allocation on
  native selection. These are **proposed limits**, not shipped API claims.
- The first-proof accepted option set is **explicitly enumerated and tested**:
  open-file/open-directory selection, multiple selection, ordinary extension
  filters and visible/hidden/absent parent; save destination and normal filters;
  message buttons, default/cancel indexes and checkbox. A requested Electron
  property whose behavior is not implemented (including bookmark creation,
  unsupported alias flags or asynchronous `AbortSignal` cancellation) produces
  a typed unsupported result **before UI**, never silent feature loss. The
  actual pinned Electron `dialog.md` documents `noResolveAliases` and
  `securityScopedBookmarks`; neither is inferred supported from ordinary
  open/save success. A later option is admitted only with its own native
  positive test and negative control.
- UI presentation is exactly once after authorization and parent lookup.
  Prefer OS completion callbacks to blocking the Host UI loop. A visible
  parent uses a sheet; parentless/hidden-parent behavior uses the AppKit
  path that reproduces the pinned oracle and keeps cancellation deliverable.
  Electron's internal use of `runModal` is evidence about Electron, not an
  instruction to add a blocking Host implementation or second event loop.
- A panel's retained handle/callback and its owning generation are enrolled
  in a Host-owned custody table before native presentation. Completion first
  validates the still-current owner/parent, records a single terminal result,
  removes UI custody, then sends the correlated REPLY/ERR if a live
  authorized caller still exists. Losing REPLY does not undo a user's choice.
- Role retirement/parent destruction/Host ending cancels UI effects on the
  **real UI thread** without waiting for application JS or treating a typed
  `WorkerLink` terminal wake as native dismissal. The native completion may
  arrive after retirement; it is ignored, never sent to a successor.
- The dialog's human-interaction lifetime uses a distinct finite monotonic
  deadline. **Proposed design default: 600_000 ms**; it is not an increase to
  any existing Host (20 s), Guardian (10 s), cleanup, kipc I/O, FS or owned
  shutdown deadline. On expiry close the panel and reject with a typed
  *expired* outcome; never claim the user clicked Cancel/Save/Discard.
  Terminal Quit uses existing bounded shutdown, not the full dialog budget.
- The host does not block on Bun waiting for the dialog to finish. GH-564's
  synchronous facade **later** uses the Worker-owned `callBlocking` primitive
  with this same native operation and caller-aware deadline; the host stays
  responsive. No second ring, per-dialog Worker or link can be introduced.

### 4.4 Result, errors and no-effect distinctions

Async Electron shapes follow pinned `dialog.md`. A genuine button/destination
selection is a value; deliberate native Cancel is a **value** with
`canceled:true`; guard denial, stale/forged parent, role loss, native failure
and expiry are **typed errors**, not fabricated Cancel/Save values.

- Guard denial: existing `KELD-GUARD*` call error, with fix at `/app/system`.
- Transport/link lifecycle: preserve existing `KELD-IPC-022/023` effect
  uncertainty. A partially sent request must not be replayed by the facade.
- Parent missing, stale or owned by another role: typed *InvalidParent*
  outcome before panel; native parent destruction after admission produces
  *ParentRetired* with physical dismissal, not silent cross-window fallback.
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
  Never use `dispatch_async(main)` alone as the UI-completion oracle.

### 4.6 Reuse, failure independence, rejected alternatives

Ownership atoms are independently falsifiable: guard before UI (A1), parent
identity/visibility (A5), native lifetime/cancellation (A6–A7), transport
correlation (A9), path-only output (A8), and real native evidence (A11).
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
  module export, UI-affinity/native handle custody.
- `crates/keld-core/src/app_session/` existing guarded router/role owner;
  no second ownership table for window generations or a new session clock.
- `crates/keld-wv/src/engine.rs` and **macOS-only** event-loop adapter to
  deliver guarded UI requests and owned cleanup to current native windows.
- `packages/@keld/api/src/dialog.ts` and **one existing app singleton**;
  `packages/@keld/electron/src/dialog.ts` thin async compatibility facade;
  `packages/@keld/electron/src/index.ts` exports the facade.
- `crates/keld-compat/fixtures/native-corpus` and mapped #477 tests as
  approved by its source/test owner. The already published first #477
  synchronous red baseline remains GH564-owned and stays fail for #478.

Must not touch: `crates/keld-core/src/app_session.rs` or GH449 active writer's
regions without a separately won claim/owner coordination; GH824 runtime
shutdown, `keld-ipc` transport Worker implementation, GuardedPrimary and
Guardian timeouts, `#834` owner regions, Windows/Linux native production,
root workspace dependency pins, `docs/specs/gh527*` or `docs/specs/gh531*`
without a new expressly approved amendment. Shared `tools/ci-inputs.json`
needs its existing single-writer/branch-specific review and canonical rebind.

## 6. Tasks (ordered vertical slices; each gets its own owner/gates)

- [ ] **T1:** Approve exact-content GH478 spec and resolve open questions below.
      No code or channel allocation before this gate.
- [ ] **T2:** Complete *only* missing async #477 expected-fail cells using
      the existing GH566 registry. No fabricated pass, no second parser.
- [ ] **T3:** One exact `app.system:["dialog"]` guard admission with
      negative grant/principal/duplicate/wildcard controls, and one new
      append-only channel table entry using current maximum id + 1.
- [ ] **T4:** Real macOS asynchronous **message** panel behind the guard,
      actual visible-parent and no/hidden-parent mapping; compile/fault tests,
      OS panel witness and exact result. This supplies GH564's shared broker.
- [ ] **T5:** Real Mac open/save panels, selected/cancelled shapes, canonical
      path output and no-FsBroker-grant property; actual alias/symlink rows
      labelled observed, unknown or divergence.
- [ ] **T6:** Generation-bound native cancellation/reap, deadline, stale
      callback and healthy next operation, exercised on an actual Mac.
- [ ] **T7:** Thin `@keld/api` and Electron async facades, red-cell flips
      in implementing PR, strict/public-API, guard and exact-head CI gates;
      keep public compatibility score honest.
- [ ] **T8:** GH564 independently consumes T4's shared broker and GH527's
      existing `callBlocking` for synchronous clicked-index behavior,
      after its **own approved spec**, never in this branch.

T4/T5 may be independently tested after the shared T3 gate; T6 validates their
composition, not just their individual wrappers. No incomplete T4-only
implementation may be labelled a complete GH478 acceptance.

## 7. Test plan (failure-first; source/oracle/OS distinguished)

| Criterion | Test owner and observed positive | Independent falsifier (one fault) |
|---|---|---|
| A1 | Guard/Core: verified permission vs zero NSPanel construction on denied call | Remove guard evaluation: denied call shows native UI and test fails |
| A2 | Real Mac: select one file, then Cancel; observe exact filePaths and zero scopes | Return last selected path on Cancel: exact result test fails |
| A3 | Real Mac: select save target, then Cancel; file unchanged until app writes; `securityScopedBookmarks:true` denied before UI | Create/truncate target in supported dialog mode or silently accept bookmarks: sentinel/denial control fails |
| A4 | Real Mac: distinct button index 0/1/2, checkbox off/on; async Promise value shape | Always return defaultId: a non-default click fails |
| A5 | Real Mac: shown parent attached sheet, hidden/absent fallback, stale/forged window refusal | Accept recycled WindowId with old generation: UI attaches and fails |
| A6 | Subprocess + real panel: kill role and inject late callback, next call succeeds | Drop callback owner without closing panel: native census detects leak |
| A7 | Actual event-loop delivery while modal active; host Quit remains bounded | Deliver UI cleanup only in default mode: watchdog/control fails |
| A8 | Path from panel; guarded FS read denied with no grant; zero grant snapshots change | Treat selected path as FS scope: denied read succeeds and fails |
| A9 | Boundaries for 0/max/max+1, malformed/unknown enum, oversized reply | Remove selected-path ceiling: max+1 is accepted and fails |
| A10 | Actual non-macOS typed unsupported, strict vs explicit legacy disclosure | Return fabricated Cancel on unsupported platform: cell fails |
| A11 | GH566 v44.4.5 expected-fail cells actually run; flip only against native proof | Remove mapped test from execution: manifest admission fails |

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

## 10. Open questions (must be explicitly resolved before Status: approved)

1. **Human-interaction timeout and abort semantics:** approve or change the
   proposed distinct **600,000 ms** upper bound and its behavior on user
   expiry. Must remain finite, must physically close the panel, and must not
   change existing Host20/Guardian/IPC cleanup clocks.
2. **Permission grammar for exact system literals:** confirm the proposed
   `{"app":{"system":["dialog"]}}` syntax, canonical denial at
   `/app/system`, duplicate/unknown literal handling, and the `keld-guard`
   method ownership. This is intentionally distinct from current
   `evaluate("group.action", path)` behavior.
3. **Error-code allocation:** assign stable new dialog-specific codes through
   the current error-code registry *at implementation time*. The typed
   categories here are normative; no numeric codes are reserved in advance.
4. **macOS native modal fallback and objc2 safety:** choose the concrete
   parentless/hidden-parent panel API after one compiling/native-dispatch
   prototype proves cancellation and UI command delivery. If a new unsafe
   operation is necessary, obtain a narrowly scoped allowlist amendment and
   source/FFI review **before** implementation.
5. **Selection/alias and symlink limits:** the first-proof file-open/save
   tests must observe alias resolution and POSIX symlink behavior before
   declaring Electron equivalence. The pinned Electron 44.4.5 documentation
   provides `noResolveAliases` and Mac App Store bookmark options. The first
   proof **refuses** unimplemented option variants; adding them requires
   separate native evidence. Unsupported variants stay explicit unknown/
   recorded divergence, not silent path normalization or a forged bookmark.

Approval authorizes the bounded **macOS** design and test tasks only; it does
not imply the unrelated #564 synchronous consumer, #449 window registry,
GH528 cross-platform transport, real KELD product acceptance or a PR merge
is complete.
