# Spec: guarded macOS application menu and host-native roles (GH-480)

Status: draft
Linear: GH-480 (tracker under #517; KEL-102 reference) · Owner: gyldlab/keld · Updated: 2026-10-24

## 1. Goal & non-goals

Let the **unmodified first-proof draw.io macOS application** build a bounded
Electron-compatible `Menu`/`MenuItem` tree **synchronously in Bun**, then
request installation of the projected native macOS application menu by a
**single guarded app-to-Host CALL**. Once installed, recognized host-native
Edit/App roles execute on the current macOS first responder without a
Host-to-Bun action frame. A `role: 'close'` enters the existing GH-450
**vetoable Host CloseRequested → facade close/preventDefault path**, never a
raw native destroy. A `role: 'quit'` remains a **nonterminating** diagnosed
compatibility gap until GH-563/GH-451 owns real cancelable Quit; native Cmd+Q
must not bypass dirty-document protection. Every native effect, callback and
command identity stays with the current authenticated app generation and
AppKit/UI owner. Missing exact `app.system` literal `menu` creates **zero**
native menu installation effects.

Non-goals:
- Implementing the canonical `keld-guard` system-literal parser a second time,
  creating a second window registry, app link, transport Worker or UI event loop.
- Sending non-role click callbacks or activated MenuItem state back to Bun;
  menu-click EVENTs and callback mirror belong to GH-556 (F06-T9). In the
  first proof, unsupported clicks issue a typed recorded diagnostic instead
  of inventing a second guarded-channel event subscription.
- Actually terminating on `role:'quit'` or Cmd+Q. GH-563 owns the separate
  F01-T3 GH-451 cancelable Host Quit path; until then it must be safely inert
  and report that remaining owner.
- Implementing Menu.popup, menu-will-show/menu-will-close, Tray, menu
  accelerators beyond documented built-in roles, getApplicationMenu, dynamic
  item mutation, default menus, MenuItem sharing or Windows/Linux native
  menu bars (the neighboring GH-556/GH-557 tracks own them).
- Implicit `legacy` activation, bare native Win32/GTK/ObjC permissions,
  unrestricted `node:fs`, broad wildcard menu grants, or test-score
  claims based on a successful TypeScript export alone.
- Changing Host/Guardian/kipc deadlines, GH-531 identity, GH-527 ring,
  or the existing native close/quit policy.

## 2. Spec refs and first-proof source facts

- `docs/architecture/03-security.md` §2 (default deny, verified policy,
  principal authority); `docs/architecture/04-electron-compat.md` §3
  (facade over shared Host app link); `docs/architecture/05-webview-and-native.md`
  §3 (guarded native module, actual UI-thread owns native handles).
- **Approved** `docs/specs/gh508-kipc-channel-table.md` §4.5: the
  projected menu table entry is `menu`, app→Host `GuardedCall` against the
  exact `app.system` `menu` literal. Its existing receive-policy table
  **does not authorize menu activation EVENTs** on that guarded channel;
  hence F06-T9 click delivery is explicitly deferred. The implementing
  owner allocates the next real id only when it lands; no speculative id.
- **Approved** `docs/specs/gh531-window-registry-close-state-machine.md`
  §4.d–4.g: host-minted WindowRef, current focused native window,
  app-link authentication and separate actual GH-450 CloseRequested/Veto
  owner. [GH-449 draft stack](https://github.com/gyldlab/keld/issues/449)
  is **unmerged**; menu cannot mint its own window id or shortcut.
- `docs/specs/gh527-worker-owned-blocking-call-transport.md`: the one
  authenticated role WorkerLink. Host writes and native completion share
  that link, no second HELLO/queue or custom framing.
- Shared `app.system` literal decision F06-D3 A and [GH-478 proposed
  dialog broker spec #1161](https://github.com/gyldlab/keld/pull/1161):
  **one** `keld-guard` resource-less system-literal evaluator owns both
  `dialog` and `menu` grants. GH478's implementation/spec is **not yet
  approved or shipped**. GH480 must reuse its accepted primitive or win a
  separately reviewed single owner; it may not introduce a second parser.
- Pinned Electron v44.4.5 tag commit
  [`694f45852a0f1726cd23bfd379854de489cccb65`](https://github.com/electron/electron/tree/694f45852a0f1726cd23bfd379854de489cccb65),
  official [`menu.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu.md),
  [`menu-item.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu-item.md)
  and [`tutorial/menus.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/tutorial/menus.md).
  The role property suppresses a supplied `click` callback for a
  recognized role. Source-derived no-focused-window fallback behavior
  remains **unknown/unscored** until a dedicated pinned source witness.
- [GH-477 existing DRAFT corpus #1156](https://github.com/gyldlab/keld/pull/1156)
  contains the first-proof `Menu.buildFromTemplate`, `setApplicationMenu`,
  role ignores-click and role-close **expected-fail** cells (current
  head `260974757fe92490d11bbcbf579901a263af32a4`). The role-quit
  cell belongs to [GH-563](https://github.com/gyldlab/keld/issues/563)
  and **must remain expected-fail** until actual GH-563 implementation.
  Those green Bun *absence* tests are not native menu acceptance.

This document does **not** amend an existing approved API, channel or
window contract by itself. Final cross-owner permission and GH-450
close integration must be explicitly reviewed before the specification
can be marked approved/implemented.

## 3. Acceptance criteria (observable and binary)

**M1 — synchronous Bun tree.** Without a Host link, `Menu.buildFromTemplate`
synchronously constructs an authoritative Bun `Menu`/typed `MenuItem`
object graph, with nested submenus, stable object identity, recognized
roles, separators and exact item order. No Promise or Host frame is
emitted by construction. The host receives **one immutable, bounded
projection** of its current state at installation. A caller-mutated
item after the snapshot cannot alter the Host projection without an
explicit later operation (out of scope).

**M2 — exact admission and no UI on denial.** Given a current authenticated
app RoleInstance and verified permissions containing literal
`{"app":{"system":["menu"]}}`, one bounded Menu-install CALL is admitted.
If missing, empty, duplicated, malformed/wildcard, unverified, stale-role
or non-app principal, Host records the existing canonical typed
`KELD-GUARD` denial naming `/app/system` **before creating/replacing
any NSMenu**. The original working native menu and Host generation
remain unchanged. This rule is evaluated by the one shared system-literal
guard owner, **not** Bun text searching or broker-local shadow policy.

**M3 — native menu materialization.** Under an admitted actual macOS
profile, `Menu.setApplicationMenu(menu)` requests one Host-owned native
menu bar that visibly matches draw.io's pinned *twelve* role-bearing
entries (including nested submenus and separators) in labels, enabled
state and order. The Host AppKit main-thread owner assigns bounded,
nonreused generation-bound command identities. A successful Bun local
tree is not a positive native OS witness; inspect the actual NSMenu bar.

**M4 — Electron call shape and errors.** `Menu.setApplicationMenu(menu)`
returns **void**, not a Promise. The guarded CALL/installation is
asynchronous to Bun because the native UI owner runs elsewhere; the
caller-visible Bun tree is local, while Host actual applied-state is
a separate receipt. Denials, native failure, stale generation and
unsupported operations must be recorded as **typed KELD-GUARD or
KELD-COMPAT diagnostics** with the normal fix guidance, even though
the return type is void. A lost/ambiguous post-write installation
is never silently replayed or treated as native success.

**M5 — exact host-native recognized roles.** After one real installed menu,
a macOS-native activation of supported `hide`, `hideOthers`, `unhide`,
`undo`, `redo`, `cut`, `copy`, `paste`, `pasteAndMatchStyle` and
`selectAll` affects the correct application/actual focused WKWebView
first responder, not a default unrelated window, and sends **zero
Host-to-Bun kipc action frames per invocation**. It must not invoke
an attached Bun `click` callback for a recognized role. Native
first-responder actions are tested on real Mac text/edit fixtures;
recording the command without an OS effect is not a pass.

**M6 — one vetoable role close.** Given a focused dirty draw.io native
window, activating its `role:'close'` menu item requests GH-450's
**same two-phase close**. The actual Host retains the native window
until the role-owned facade receives `CloseRequested(close_seq)`,
invokes `close`, immediately `preventDefault()`, and replies `Veto`;
Cancel retains the **same** dirty native window/document and the
next healthy close still works. A direct AppKit destroy, synthetic
`closed`, skipped CloseRequested or guessed Save is a failure.

**M7 — role quit is explicitly held.** While GH-563's OS-initiated
cancelable Quit is unimplemented, macOS `role:'quit'` and the
associated native Cmd+Q menu activation **do not** call
`NSApplication.terminate`, close any window, emit a fake `before-quit`
or exit Host. They record one canonical typed **KELD-COMPAT** diagnostic
naming **GH-563** as the missing next-milestone owner. This is a
scored pending/divergent behavior, **not** Electron Quit parity.
GH-563 later flips its own role-quit expected-fail cell after real
dirty-window Quit/Cancel acceptance.

**M8 — no silent non-role callbacks.** A menu item with only a custom Bun
`click` callback and no supported role remains unsupported until GH-556.
When it is activated on macOS, it records a typed KELD-COMPAT diagnostic
naming GH-556 and does **not** silently drop the activation, fire
an unadmitted Host-to-Bun frame or invoke the user callback in a
context that lacks GH-556's receiver/authority. Role items with click
callbacks do not call those callbacks as part of native role execution.

**M9 — bounded tree and malformed refusal.** A first-proof installation
admits at most **256 nodes**, **8 nested submenu levels**, a total
**128 KiB** encoded immutable snapshot, and a proposed at-most
**4 KiB** single label. Too deep, max+1 items, duplicate command
identity, unknown role/type, cycle, malformed string/encoding,
truncated wire payload and wrong Postcard discriminant fail *before*
AppKit mutation with a canonical typed error. These values are
proposed limits requiring owner approval, not existing KELD capacities.

**M10 — generation and replacement custody.** A newly accepted menu
installation atomically replaces one Host-owned projection after native
materialization. Current source role and per-menu generation are
Host-owned, distinct from display id, process PID and WindowRef. On Bun
role death, stale native callbacks from the old role are quarantined/
disabled and cannot invoke old user code or operate a successor's
window. A fresh current primary role may install a new menu. Retained
NSMenu/NSMenuItem objects are released/quiesced on Host terminal Quit,
and a rejected install leaves the previous admitted menu functional.

**M11 — default-deny and first-proof profile.** The exact `menu` literal
does not widen `dialog`, filesystem, shell or renderer authority.
No unqualified Strict/Legacy activation occurs; an explicit Legacy
app remains disclosed as forfeiting ambient `node:fs` containment,
independent of Menu role execution. Windows/Linux remain typed
unsupported, not a fabricated native menu pass.

**M12 — GH477 conformance and actual product evidence.** The GH480
four red cells are inverted to `expected_verdict:pass` only in the
**implementing PR** after actual Mac menu installation, role click and
close veto tests. The GH563 role-quit cell remains **fail**; because
GH480 will export `MenuItem`, its mapped baseline must be **re-recorded
as the actual typed GH563 diagnostic**, no longer as a missing
constructor, while retaining `implementing_ticket:GH-563`. GH-477
and the frozen 44.3.0 lifecycle manifest, GH566 parser and all other
owners' cells stay unchanged. A green Bun module-import-only test
is not native parity.

## 4. Design

### 4.1 Ownership, trust and shared native path

- **Bun-local state:** one `Menu`/typed `MenuItem` tree in
  `@keld/electron`. Template conversion, standard supported role
  strings, labels, separators and submenu order live in this package.
  The created menu and item objects are the authoritative **user-facing**
  synchronous tree, not an NSMenu proxy for local reads. A `click`
  function is never serialized to Host as callable code.
- **One guarded Call:** generic `@keld/api` uses the **existing**
  authenticated app `WorkerLink`, sends a bounded tree-snapshot request
  through a single newly allocated GH508 `menu` `GuardedCall` channel;
  Host `keld-guard` verifies current RoleInstance and exact system literal
  before any allocation, while preserving the same transport error codes.
- **Native main UI owner:** `keld-native::menu` supplies a typed macOS
  data-to-native-menu implementation, called only on the **existing
  keld-wv/AppKit main event loop**. Native NSMenu and NSMenuItem handles,
  first responder, menu projection and callback disposal are retained by
  the Host UI owner—not by Bun, a new detached thread or a second
  `keld-native` Window registry.
- **Focused close:** current GH-531 WindowRef/focus state is the one
  identity source. The `role:'close'` UI activation goes through the
  existing GH-450 Host RequestClose boundary; no raw AppKit close or
  native object-ID authority path. Actual `before-quit` remains GH-563.
- **Reported unsupported:** native role Quit and non-role click have one
  KELD-COMPAT diagnostic owner each. A diagnostic is a receipt of
  *missing behavior*, not a success event or a user-click reply.

### 4.2 Canonical system-literal grant and channel

The only first-proof permission declaration for menu is

```json
{"app":{"system":["menu"]}}
```

The **shared** `keld-guard` system-literal evaluator chosen by F06-D3
is an exact-value-array check for the actual current AppProcess role on
one verified policy digest. It is *not* current `evaluate("fs.*",path)`
with the path string `"menu"`; current main does not yet implement
this grammar. GH478's draft contract requires the identical evaluator
for `dialog`. The permission-model owner must approve which first
implementation lands the evaluator, and the second must reuse it,
without two parsers/permission-predicate copies or wildcard bypass.

On implementation only, append one named `menu` `GuardedCall` entry to
the **existing** GH508 `channel_table.rs`, using the next available id
at that source head; derive the TypeScript constant through its owning
code generator. Preserve byte framing/version 2 and existing channel
allocations. The guard-required `MENU` capability constant lives in
`keld-guard`, not independently in Bun or keld-native. All invalid
callers are rejected before decoding unbounded payload bytes or native
UI creation.

**No Host→Bun EVENT on the menu guarded channel in this first proof.**
GH508 §4.5 states a `GuardedCall` entry does not automatically
admit host-originating menu-activation EVENTs; doing so needs another
separately approved receiver/channel design owned by GH556. This
first-proof design executes recognized roles **on the native Host**, and
reports unsupported non-role callbacks through existing diagnostics,
not a fabricated/event-permissive channel.

### 4.3 Bounded projection and native materialization

Proposed wire representation (sketch, no numeric id or new schema file):

```rust
struct MenuNodeV1 {
    role: Option<RecognizedMenuRole>,
    label: String,
    kind: MenuKind,           // Normal, Separator, Submenu
    enabled: bool,
    children: Vec<MenuNodeV1>,
}
struct InstallMenuV1 {
    roots: Vec<MenuNodeV1>,   // total nodes <= 256; depth <= 8
}
struct MenuInstallReceipt { // host transport reply, NOT facade return
    host_menu_generation: u64,
}
```

The authenticated **role generation** is Host metadata, never a field
trusted from `InstallMenuV1`. The Host assigns one monotonic
`host_menu_generation` and each native command identifier in the
current generation. A `MenuItem` click function and mutable Bun
object are not part of the serialized tree. The snapshot is produced
synchronously and bounded on the Bun side, then fully validated again
at the trusted Host boundary. A circular submenu reference is rejected
rather than recursively traversed forever; unknown roles/types cannot
fall back to arbitrary ObjC selectors or command strings.

On the UI thread, build the replacement NSMenu tree while keeping the
currently installed tree alive. Commit the new menu *atomically* only
after the entire accepted tree is serviceable. A partial/cancelled
installation never leaves an incomplete native menu bar or wrong
command ids. If an ambiguous transport outcome occurs after Host may
have committed the native menu, do **not** replay automatically;
record effect-unknown and reconcile through the next deliberate user
installation rather than duplicating callbacks.

The public `Menu.setApplicationMenu(): void` call may return before
native installation is observed, unlike an in-process synchronous
Electron native call. This is a **potential compatibility divergence**
and must be tested against pinned draw.io usage; report it as an explicit
limitation if the first-proof app actually requires immediate native
menu effect. Do not lie by returning a Promise or pretending the Host
installation reply is synchronous. An installer diagnostic names the
actual current role and result **without raw menu/user data**.

### 4.4 Recognized role dispatch, focused webview and cancellation

Native menu commands carry only Host-internal current menu generation
and command ids. The command lookup maps recognized roles to a fixed
allowlist of **generic native actions** (native application hide/show,
Undo/Redo and edit responders). The mapping is owned in the menu
compat/native seam, not string-matched by `keld-core`, `keld-ipc`
or renderer webviews. For text edit roles, the existing native focused
WKWebView/responder is the target. No Host→Bun frame or rerouted
Electron `click` runs. An unknown role, unsupported first-responder
action or missing focused window follows a recorded denial/
unsupported/no-effect policy, never a role-string-to-selector escape.

`role:'close'` is a **host-owned window request**, not an edit role.
It enters GH450's installed `WindowRef`/current RoleInstance
`CloseRequested` path and awaits the true `Allow|Veto` outcome.
The window remains on screen until GH450's accepted physical release
contract proves destruction. This preserves the unsaved dialog route
with GH478/GH564 consumers when those separate requirements land.

`role:'quit'` is deliberately **held**: it must suppress native OS
terminate/Cmd+Q before the operating system can end the Host, and
record a typed diagnostic pointing to GH563. The GH563 owner later
routes app-quit through the GH451 veto/will-quit state machine. A
first-proof nonterminating diagnostic is a known recorded divergence;
it is **not** a general macOS Quit implementation.

### 4.5 Generation, permission, effect and error boundaries

- One authenticated Host role's menu install, one immutable typed
  tree, one native generation and one outstanding transport correlation.
  On role loss, disable old commands/callbacks before any successor
  can act. When no current primary is authorized, native menu actions
  cannot target an old window. A new current role explicitly installs
  its menu; retained native handles retire on Host Quit.
- Missing `menu` grant: canonical `KELD-GUARD` denial naming
  `/app/system`, **zero native effects**. The Bun facade returns void;
  the existing diagnostics subsystem records the rejection. No raw
  exceptions from AppKit callbacks or unhandled Promise rejection
  may become a silent process crash.
- Unsupported click/Quit: a typed `KELD-COMPAT` diagnostic with the
  actual implementing ticket GH556/GH563. Stable code allocation and
  reporting API are chosen by the current error-code owner; this
  draft does **not** invent a `KELD-COMPAT-XYZ` literal.
- Live app subcommands do not create FS grant, dialog grant, window
  id, binary child or renderer navigation authority. No `app.system`
  wildcard, ambient Host file access or implicit Legacy activation.
- No existing Host20/Guardian10/Cleanup5/kipc frame deadline is
  widened by menu installation. An unbounded AppKit callback, blocked
  main thread or detached last owner is a test failure, not a
  rationale to raise a deadline.

### 4.6 Reuse decision and rejected alternatives

| Alternative | Verdict and first falsifier |
|---|---|
| Construct every `MenuItem` by synchronous Host IPC | Reject: Bun buildFromTemplate must be synchronous and no Host traffic; an unready Host prevents local menu construction |
| Introduce a duplicate `app.system.menu` guard matcher | Reject: GH478 proposes one literal parser; a malformed/forged grant must fail identically for both dialog and menu |
| Put ObjC NSMenu handles inside Bun | Reject: native UI ownership and callback lifetime are Host/main-thread, not a semi-trusted process |
| Add menu activation EVENTs to guarded menu channel now | Reject: GH508 currently refuses guarded subscriptions and GH556 owns click delivery; reception would widen authority |
| Implement role close via NSWindow direct destroy | Reject: a dirty draw.io CloseRequested veto must retain physical window |
| Implement role Quit by native terminate | Reject: GH451/563 cancelable Quit is not ready; dirty data lost |
| Pretend async native install is synchronously completed because the API returns void | Reject: actual OS receipt vs local Bun tree must be independently observed |
| First-proof unbounded recursive menu graph | Reject: cyclic template/oversized tree max+1 tests fail before UI allocation |

No new transport thread or measured speedup is claimed.

## 5. Boundaries

After explicit spec approval and winning source claims:
- `packages/@keld/electron/src/menu.ts`, `menu-item.ts`, `index.ts`
  for synchronous authoritative Menu/MenuItem objects, restricted
  recognized role mapping and facade typed unsupported diagnostics.
- `packages/@keld/api/src/menu.ts`, existing authenticated singleton
  adapter over GH527 WorkerLink, no new client connection or mirroring
  of the Bun authoritative Menu tree.
- Existing `keld-guard` system-literal owner (GH478 or verified single
  alternate owner), `keld-ipc/src/channel_table.rs`/`channel_allocations.txt`
  append-only, and generated TS channel constant through GH508 owner.
- `crates/keld-native/src/menu.rs` typed native NSMenu construction
  and command operations, with pinned objc2 (no deprecated `cocoa`
  crate), integrated only on the existing `keld-wv` macOS UI loop.
- GH531/GH450 current Host Core window request/close router,
  not an independent registry, and the already approved role-generation
  retirement mechanism.
- GH477 existing pinned v44.4.5 corpus registration, with GH480's
  red cells flipped **only** alongside real native accepted implementation;
  GH563 red cell re-recorded as a pending diagnostic, never flipped
  to pass by GH480.

MUST NOT edit under *this spec-only* branch: any production Rust/TS,
permissions, channel table, generated CI, default profile, GH449/#450
active worktrees, GH478 native dialog implementation, GH563 quit owner,
GH527 transport, root AGENTS/workflow, Windows/Linux native production,
unsafe allowlist or existing deadline. Shared files in the eventual
implementation need their **incumbent winning owner** and exact source
review. No approval or source claim is granted by this Markdown file.

## 6. Tasks (one bounded slice per PR after approval)

- [ ] T1. Independently review/approve **this** GH480 exact-content spec,
      reconcile current GH508/GH478 system-literal guard ownership and
      GH531/GH450 close path. Resolve §10 before changing production.
- [ ] T2. Reuse GH477's four GH480 expected-fail conformance cells;
      retain GH563 role-quit pending/fail and other ticket cells.
- [ ] T3. Bun-local menu template and MenuItem object graph, bounded
      tree, actual `role` suppresses `click` callback, all negative
      malformed/cycle/bounds cases, with no app-link frames.
- [ ] T4. One GH508 guarded menu-install call, verified exact menu grant,
      current RoleInstance admission and Host native NSMenu/UI ownership,
      real Mac menu bar and post-denial no-effect witnesses.
- [ ] T5. First-responder supported edit/HIDE roles with a native click
      through the actual Mac UI and zero extra Bun EVENT frames; detect
      unsupported non-role behavior via stable diagnostics.
- [ ] T6. Compose role-close through the **existing** GH450 cancelable
      close path; ensure dirty Save/Cancel/Discard stay separate GH478/
      GH564 implementation responsibilities. Native Quit/Cmd+Q remains
      bounded and nonterminating with explicit GH563 diagnostic.
- [ ] T7. Mac source/native tests, role-loss and old callback cleanup,
      invert GH480's four red tests only on actual proof, re-record
      GH563 pending cell, run all independent security/wire/public API/
      unsafe gates and final full hosted CI before merge consideration.

T3 can proceed after spec approval without GH450 source edits; T4/T6
wait for authenticated native owners. GH563 is **not** implemented in T6.

## 7. Test plan (failure-first, native OS distinguished)

| AC | Positive observable | Independent falsifier |
|---|---|---|
| M1 | Bun template with 12 first-proof role items synchronously yields typed tree and zero frames | Replace constructor with Promise/Host CALL; value/traffic tests fail |
| M2 | Verified grant + current app role, denied request makes zero NSMenu mutation | Remove exact guard evaluation; denied real menu appears |
| M3 | Real AppKit application NSMenu bar has source-shaped hierarchy/order and no stale commands | Fake/reorder separator or omit role; native UI census differs |
| M4 | `setApplicationMenu` returns void; native success/failure traced separately | Return Promise or drop rejection diagnostic; event/result tests fail |
| M5 | Actual Mac first responder edit operations, ten host-side native roles and zero Host→Bun frames | Invoke Bun click for a role; callback/transport count nonzero |
| M6 | Actual dirty native window close via GH450 CloseRequested/Veto with same window preserved | Replace with direct NSWindow destroy; close handler missing/physical view gone |
| M7 | Native role Quit and Cmd+Q emit GH563 unsupported, Host stays alive, no window closes | Allow NSApplication.terminate; PID lifetime and dirty sentinel fail |
| M8 | Non-role callback records GH556 unsupported without call/event | Drop unsupported callback silently or inject unguarded EVENT; diagnostic/receive-control fails |
| M9 | 0/max/max+1 nodes/depth/bytes, cycles, malformed enum, duplicate command id | Remove cycle or max+1 bound; native allocation starts unexpectedly |
| M10 | Bun role death→old menu callbacks disabled; fresh primary install/replacement works | Reuse old command-generation id; stale role action executes |
| M11 | Missing menu never authorizes dialog/fs/webview; explicit Legacy disclosed | Treat menu grant as all system calls; unauthorized effect passes and test fails |
| M12 | GH480 red cells flipped only with real native proof; GH563 red diagnostic retained | Flip role quit to pass / leave absent-constructor test after adding MenuItem; corpus fails |

Native acceptance uses the exact pinned draw.io menu fixture,
`Menu.buildFromTemplate` + `setApplicationMenu` from its unmodified main,
real Mac menu activation, OS-visible item hierarchy and actual Host/Bun
frame/diagnostic logs. The native Edit tests require a real focused
WKWebView text surface and a disabled/no-focus negative control.
Accessibility/TCC denial is recorded as **unavailable**, never a
native pass or permission bypass. Sync uses observable callbacks and
bounded watchdogs, not sleeps; an external timeout is not a success
criterion. Where app action is intentionally deferred, negative outcome
is a typed stable diagnostic, not fake Electron parity.

## 8. Review gates triggered

- **Permission model:** yes — one system-literal `menu` grant, verified
  app RoleInstance and zero native effects on denial; reuse GH478's
  shared evaluator, never broader Wildcard.
- **Public API:** yes — synchronous Menu/MenuItem shapes, role priority,
  bounded options, void installation result and error reporting.
- **Wire protocol:** yes — new append-only GH508 guarded menu channel,
  typed immutable tree/receipt, hostile codec tests, no activation EVENTs.
- **Unsafe:** conditional and **blocking** — current
  `keld-native/AGENTS.md` production unsafe allowance covers specific
  Auth operations only; any new NSMenu/ObjC unsafe needs a documented
  narrow allowlist/source proof and independent review first.
- **Dependency:** none planned; pinned objc2 crate features may need
  explicit reviewed additions at implementation, not silently inferred.
- **Platform:** real macOS menu, focused responder, close-veto, role
  quit suppression and physical cleanup; no Windows/Linux claims.

## 9. Perf impact

No measured speedup or latency result is claimed. Native menu builds
are cold user interactions; the first-proof acceptance counts **zero
Host→Bun frames per native role activation**. Record total tree bytes,
max heap/handle retention, install latency vs source, repeated replacement
and actual native menu callback teardown. A >5% regression still needs
architecture 01 §5's established waiver. Native role dispatch cannot
raise Host20/Guardian/KIPC clocks or turn into an unbounded Bun worker.

## 10. Open questions (blocking for Status: approved)

1. **Guard owner composition:** GH478's exact `app.system` literal
   evaluator is still DRAFT; assign an approved sole system-literal
   guard implementation owner/precedence before GH480 adds `menu`.
   No duplicate string matcher or speculative channel id.
2. **Void call result and error channel:** decide the exact current
   existing typed diagnostic mechanism for a denied or failed native
   install *after* Bun's void method returned, and define its deterministic
   no-unhandled-rejection behavior. Do not turn a documented void
   Electron API into a Promise for convenience.
3. **Native role mapping and no-focus source branch:** compare pinned
   Electron 44.4.5 role implementation against actual Mac focused
   WKWebView. The no-focused-window role-to-click fallback is
   source-derived **unknown** in GH477; decide how to report it
   explicitly without silently invoking user callbacks or claiming
   doc-only parity.
4. **Limits/atomic update:** approve or revise proposed max 256 menu
   nodes, depth 8, 128 KiB request, 4 KiB label and monotonic host
   menu generation; specify native replacement and retained-menu
   behavior during Bun recovery, including strict no-old-callback.
5. **GH450 and GH563 role lifecycle:** confirm the owner-approved
   existing GH450 RequestClose/Veto physical-wake seam and that the
   first-proof `role:quit`/Cmd+Q OS callback can be safely suppressed,
   with a GH563 typed diagnostic and **no native terminate**. Real
   cancelable Quit remains the next-milestone GH563/GH451 feature.

This is **Status: draft**. It does not approve an unimplemented native
menu, channel or permission evaluator, make menu/Cmd+Q work in KELD,
flip a conformance cell, change the source-owner merge state or close
GH480. Actual implementation begins only after exact approved designs
and winning native OS/source ownership claims.
