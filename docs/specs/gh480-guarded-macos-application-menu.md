# Spec: guarded macOS application menu and host-native roles (GH-480)

Status: draft
Linear: GH-480 (tracker under #517; KEL-102 reference) · Owner: gyldlab/keld · Updated: 2026-10-24

## 1. Goal & non-goals

Let the **unmodified first-proof draw.io macOS application** build a bounded
Electron-compatible `Menu`/`MenuItem` tree **synchronously in Bun**, including
the real update-preference checkbox and its initially checked state, then
request installation of the projected native macOS application menu by a
**single guarded app-to-Host CALL**. Once installed, recognized host-native
Edit/App roles and their **built-in default keyboard equivalents** execute
on the current macOS first responder without a Host-to-Bun action frame.
A `role: 'close'`, whether activated by mouse or Cmd+W, enters the existing GH-450
**vetoable Host CloseRequested → facade close/preventDefault path**, never a
raw native destroy. A `role: 'quit'` remains a **nonterminating** diagnosed
compatibility gap until GH-563/GH-451 owns real cancelable Quit; native Cmd+Q
must not bypass dirty-document protection. Every native effect, callback and
command identity stays with the current authenticated app generation and
AppKit/UI owner. Missing exact `app.system` literal `menu` creates **zero**
native menu installation effects. The checkbox and non-role click actions
remain explicitly classified as unsupported **interaction** until GH-556 owns
activation delivery; construction and installation must still succeed.

Non-goals:
- Implementing the canonical `keld-guard` system-literal parser a second time,
  creating a second window registry, app link, transport Worker or UI event loop.
- Sending non-role click callbacks or activated MenuItem state back to Bun;
  menu-click EVENTs and callback mirror belong to GH-556 (F06-T9). In the
  first proof, unsupported clicks issue a Host-owned typed diagnostic;
  checkbox selection leaves its projected checked state unchanged, and
  must not masquerade as a successful preference toggle.
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
  Exact [`lib/browser/api/menu-item.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/menu-item.ts)
  lowercases role strings; the tutorial
  explicitly defines roles as case-insensitive and supplies default labels
  **and accelerators**. Pinned
  [`menu-item-roles.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/menu-item-roles.ts)
  assigns the built-in
  macOS Cmd+W/Z/C/V/H/Q equivalents, among others. A recognized role
  takes priority over its `click` callback. Source-derived
  no-focused-window fallback remains **unknown/unscored** until a
  dedicated pinned source/native witness.
- Pinned draw.io [`jgraph/drawio-desktop@2edf9fb:src/main/electron.js`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb/src/main/electron.js#L1949-L2069):
  its actual Mac template has **three top-level menus**, twelve role
  items, custom-click items, separators, and `autoCheckForUpdates` with
  `type:'checkbox'`, `checked: store == null || store.get('dontCheckUpdates') !== true`
  and a non-role `click(menuItem)`. Under `disableUpdate`, three update
  items including that checkbox are removed before construction.
  The exact source spells `role:'hideothers'` in lowercase; role-only
  items supply **no explicit keyboard accelerators**.
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
object graph from **both** real draw.io Mac template variants
(`disableUpdate` false/true), with nested submenus, stable object
identity, normalized case-insensitive roles, separators, source order,
default role label/accelerator and the update-preference
`type:'checkbox'` with its initial boolean `checked` value. With
updates enabled, changing the source store's `dontCheckUpdates` fixture
changes the initial checked state. No Promise or Host frame is emitted
by construction. The Host receives **one immutable, bounded projection**
of current state at installation. Later Bun item mutation has no
effect on the existing native projection (dynamic sync belongs to GH556).

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
menu bar with all **three top-level menus** and the pinned draw.io tree
(twelve recognized role items, custom-click items, separators, and when
updates are enabled the checkbox). Preserve labels, enabled state, order,
role-default shortcuts and the **real initial NSMenuItem check mark**.
An unchecked fixture must render unchecked; an enabled checkbox must
not disappear because its custom click cannot yet be delivered.
The Host AppKit main-thread owner assigns bounded, nonreused
generation-bound command identities. A successful Bun tree or fake menu
snapshot is not positive native OS proof.

**M4 — Electron call shape and errors.** `Menu.setApplicationMenu(menu)`
returns **void**, not a Promise. The guarded CALL/installation is
asynchronous to Bun because the native UI owner runs elsewhere. One
Host-owned, correlation-bound **diagnostic record** on the existing
`keld-host` stderr observation path reports each denied, failed,
unknown-effect or unsupported outcome with the owning `KELD-GUARD`,
`KELD-NATIVE` or `KELD-COMPAT` code and fix guidance, without app
content. Success has a separate internal correlated native-install
receipt; the facade does not turn it into a synchronous guarantee.
The Bun fire-and-observe transport continuation explicitly consumes
its REPLY/ERR so no rejected Promise escapes unhandled. A lost/ambiguous
post-write installation is never silently replayed or counted as
native success. §4.5 states the proposed error-observation contract;
the implementation owner must prove this runtime path before approval.

**M5 — exact host-native recognized roles.** After one real installed menu,
macOS mouse **and built-in default keyboard** activation of supported
`hide`, `hideOthers`, `unhide`,
`undo`, `redo`, `cut`, `copy`, `paste`, `pasteAndMatchStyle` and
`selectAll` affects the correct application/actual focused WKWebView
first responder, not a default unrelated window, and sends **zero
Host-to-Bun kipc action frames per invocation**. It must not invoke
an attached Bun `click` callback for a recognized role. Native
first-responder actions and default Cmd+Z/C/V/H and Hide Others variants
are tested on real Mac text/edit fixtures; recording a key equivalent
without the OS action is not a pass.

**M6 — one vetoable role close.** Given a focused dirty draw.io native
window, activating its `role:'close'` menu item by **mouse and
Cmd+W** requests GH-450's
**same two-phase close**. The actual Host retains the native window
until the role-owned facade receives `CloseRequested(close_seq)`,
invokes `close`, immediately `preventDefault()`, and replies `Veto`;
Cancel retains the **same** dirty native window/document and the
next healthy close still works. A key-equivalent handler which calls
`performClose`/destroys the NSWindow without the GH450 gate, a synthetic
`closed`, skipped CloseRequested or guessed Save is a failure.

**M7 — role quit is explicitly held.** While GH-563's OS-initiated
cancelable Quit is unimplemented, macOS `role:'quit'` selected
by mouse or the real Cmd+Q key equivalent **does not** call
`NSApplication.terminate`, close any window, emit a fake `before-quit`
or exit Host. They record one canonical typed **KELD-COMPAT** diagnostic
naming **GH-563** as the missing next-milestone owner. This is a
scored pending/divergent behavior, **not** Electron Quit parity.
GH-563 later flips its own role-quit expected-fail cell after real
dirty-window Quit/Cancel acceptance.

**M8 — no silent non-role callbacks.** A normal or checkbox menu item
with a custom Bun `click` callback and no supported role is **accepted
for construction and native projection**, but activation is unsupported
until GH-556. The Host records one typed `KELD-COMPAT` diagnostic naming
GH-556, sends zero unadmitted activation EVENT frames and invokes no
user callback. An initially checked checkbox stays checked and an
unchecked one stays unchecked after unsupported activation; the user
must not be led to believe `dontCheckUpdates` changed. This is
explicitly scored as partial visual parity and behavioral divergence.
Role items with click callbacks execute only the role.

**M9 — bounded tree and malformed refusal.** A first-proof installation
admits at most **256 nodes**, **8 nested submenu levels**, a total
**128 KiB** encoded immutable snapshot, and a proposed at-most
**4 KiB** single label. Too deep, max+1 items, duplicate command
identity, unknown role/type, invalid checkbox checked type or checked
on a non-checkbox node, invalid role-default accelerator, cycle,
malformed string/encoding,
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
  `@keld/electron`. Template conversion, lowercase role normalization,
  default role labels and accelerators, checkbox initial state, separators
  and submenu order are synchronous public values. Their **shared immutable
  role metadata** is generated from one approved first-proof role table;
  Host activation and Bun getter behavior must not maintain divergent
  hand-written role definitions. The created menu and item objects are
  the authoritative **user-facing**
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
  Host-side `KELD-COMPAT` diagnostic owner each, with a stable stderr
  observation contract in §4.5. A diagnostic is a receipt of
  *missing behavior*, not a success event or a user-click reply.

### 4.2 Canonical system-literal grant and channel

The only first-proof permission declaration for menu is

```json
{"app":{"system":["menu"]}}
```

The **shared** `keld-guard` system-literal evaluator chosen by F06-D3
is an exact-value-array check for the actual current AppProcess role on
one immutable Host-selected verified policy digest. It is *not* current
`evaluate("fs.*",path)`
with the path string `"menu"`; current main does not yet implement
this grammar. GH478's draft contract requires the identical evaluator
for `dialog`. The permission-model owner must approve which first
implementation lands the evaluator, and the second must reuse it,
without two parsers/permission-predicate copies or wildcard bypass. An
unrecognized, repeated, wildcard, nonstring or otherwise invalid system
entry does not silently authorize `menu`; do not salvage a grant from a
partially invalid array. Recheck the **current** authenticated primary
RoleInstance and captured policy digest for each install, not a cached
Bun assertion or a menu-node-supplied identity.

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
reports unsupported non-role callbacks through Host diagnostics on
stderr, not a fabricated/event-permissive channel. Neither a role
activation nor its diagnostic is permission to request a new native
operation from the application.

### 4.3 Bounded projection and native materialization

The source-grounded first-proof `MenuItem` cases are:

| Input in pinned Mac draw.io | Bun-local state | Immutable Host projection | On native activation |
|---|---|---|---|
| `{role:'hideothers'}` and other role-only items | Electron-compatible case-folded canonical role, default label and shortcut | Whitelisted typed role id; Host derives trusted native action/shortcut | Real native role, no Bun callback |
| `autoCheckForUpdates`: `type:'checkbox'`, `checked: bool`, `click` | Checkbox with boolean initial state and retained Bun callback | Checkbox with **same initial check mark**, frozen until explicit later reinstall | GH556 typed unsupported diagnostic; **no toggle**, no callback |
| `checkForUpdates`, Zoom, About, Support and other non-role `click` items | Normal MenuItem with retained Bun callback | Normal visible item tagged unsupported activation | GH556 typed unsupported diagnostic; no callback |
| Separator/submenu | Source order and hierarchy | Separator/submenu, no executable selector | No action |

With `disableUpdate` true, the three update items are removed by
draw.io before `buildFromTemplate`; do not reconstruct or synthesize
the checkbox. With updates enabled, the checkbox's native checked
state matches `store == null || store.get('dontCheckUpdates') !== true`
at the **moment of installation**. It stays visually unchanged on a
click until GH556 provides an approved update/callback path; do not
pretend user preference was saved. A new deliberate installation may
capture a new initial `checked` value. This is an explicit first-proof
behavior divergence, so the menu **construction/installation** tests
may pass while checkbox-activation parity remains pending.

Proposed wire representation (sketch, no numeric id or new schema file):

```rust
enum MenuKind { Normal, Separator, Submenu, Checkbox }
enum MenuAction {
    NativeRole(RecognizedMenuRole), // closed enum, never a selector string
    UnsupportedClick,              // never serializes callable code
    Inert,
}
struct MenuNodeV1 {
    label: String,
    kind: MenuKind,
    enabled: bool,
    checked: Option<bool>,    // Some only on Checkbox; false is not absence
    action: MenuAction,
    children: Vec<MenuNodeV1>,
}
struct InstallMenuV1 {
    roots: Vec<MenuNodeV1>,   // total nodes <= 256; depth <= 8
}
struct MenuInstallReceipt { // host transport reply, NOT facade return
    host_menu_generation: u64,
}
```

Validate mutually exclusive node combinations: separators/submenus
cannot contain executable actions or checked state; checkbox
`checked` must be boolean (omitted input defaults false before capture);
a `NativeRole` action must have a recognized role and the Host-minted
expected key equivalent; a checked non-checkbox or unknown enum must
fail before AppKit. A present Bun `click` for a role is **ignored**,
not promoted to `UnsupportedClick`. A custom-click item without a
recognized role is **valid and projected**, not rejected as an unknown
command. Electron's public `MenuItem.checked` getter may exist and
default to false even on a normal item; the projection must encode
`None` for that item's `checked`, never `Some(false)` unless its
`kind` is `Checkbox`. The `UnsupportedClick` enum is a diagnostic marker only,
never a call target. No untrusted raw selector or arbitrary accelerator
crosses the link.

The authenticated **role generation** is Host metadata, never a field
trusted from `InstallMenuV1`. The Host assigns one monotonic
`host_menu_generation` and each native command identifier in the
current generation. A `MenuItem` click function and mutable Bun
object are not part of the serialized tree. The snapshot is produced
synchronously and bounded on the Bun side, then fully validated again
at the trusted Host boundary. A circular submenu reference is rejected
rather than recursively traversed forever; unknown roles/types cannot
fall back to arbitrary ObjC selectors or command strings.

**Built-in role table owner:** the GH480 menu compatibility seam owns
one closed, source-controlled role/default-label/default-accelerator
registry whose generated read-only Bun metadata and Host native table
are checked for byte-for-byte semantic drift. It is reviewed together
with the public API and wire enum; a new generator is permitted only
through the incumbent generated-input owner. The Host derives default
native key equivalents from the typed role ID using that table; the
wire **does not carry a caller-selected accelerator or ObjC selector**.
The Bun object's `role` and `accelerator` getters return the corresponding
Electron-compatible public values. Normalize documented role identifiers
with ASCII case folding (`hideothers`, `hideOthers`, `HIDEOTHERS` map
to one `HideOthers` enum), preserving lowercase public role spelling
as Electron does; unknown Unicode/lookalike/nonstring roles cannot
gain arbitrary native dispatch.

The pinned 44.4.5 macOS role defaults needed by this slice are:
`close` Cmd+W, `undo` Cmd+Z, `redo` Shift+Cmd+Z, `cut` Cmd+X,
`copy` Cmd+C, `paste` Cmd+V, `pasteAndMatchStyle`
Cmd+Option+Shift+V, `selectAll` Cmd+A, `hide` Cmd+H,
`hideothers` Cmd+Option+H and `quit` Cmd+Q. `unhide` has
no built-in default key equivalent. Check labels (including the
running app name), menu text, macOS Command/Option/Shift modifiers
and actual focused responder against the pinned upstream `menu-item-roles.ts`
table on the physical Mac. Explicit caller-supplied custom accelerators
and dynamic retargeting are out of scope and must be rejected with a
typed unsupported result, not dropped silently. A key equivalent
routes to the **same authenticated Host menu-generation command**
as a mouse activation; it never directly asks AppKit to close/quit.

On the UI thread, build the replacement NSMenu tree while keeping the
currently installed tree alive. Commit the new menu *atomically* only
after the entire accepted tree is serviceable. A partial/cancelled
installation never leaves an incomplete native menu bar or wrong
command ids. Source-enabled custom-click and checkbox items remain
**visibly enabled** and are intercepted as diagnosable native no-ops;
the checkbox must not toggle itself. This preserves source-visible
`enabled` and `checked` values but deliberately defers callback behavior
until GH556. If an ambiguous transport outcome
occurs after Host may
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
actual current role and result **without raw menu/user data**; §4.5
defines the observation location, correlation, terminal cases and
effect-unknown rule.

### 4.4 Recognized role dispatch, focused webview and cancellation

Native menu commands carry only Host-internal current menu generation
and command ids. Both `NSMenuItem` selection and its default
`keyEquivalent` invoke the **same** generation-checked lookup; neither
shortcut dispatch nor direct keyboard handling may bypass this router.
The command lookup maps recognized roles to a fixed
allowlist of **generic native actions** (native application hide/show,
Undo/Redo and edit responders). The mapping is owned in the menu
compat/native seam, not string-matched by `keld-core`, `keld-ipc`
or renderer webviews. For text edit roles, the existing native focused
WKWebView/responder is the target. These edit/hide/unhide roles emit
no Host→Bun action EVENT and invoke no Bun `click`; `close` intentionally
uses GH450's authenticated `CloseRequested` lifecycle EVENT.
An unknown role, unsupported first-responder
action or missing focused window follows a recorded denial/
unsupported/no-effect policy, never a role-string-to-selector escape.
An unfocused or disabled menu item cannot target a different window.
For source-recognized `hideothers`, mouse and Cmd+Option+H must
select the **Hide Others** native action; case normalization is settled
before the enum crosses to Host.

`role:'close'` is a **host-owned window request**, not an edit role.
It enters GH450's installed `WindowRef`/current RoleInstance
`CloseRequested` path and awaits the true `Allow|Veto` outcome.
The window remains on screen until GH450's accepted physical release
contract proves destruction. This preserves the unsaved dialog route
with GH478/GH564 consumers when those separate requirements land.
Mouse selection and **Cmd+W** use the same `RequestClose` entry,
same close sequence and same focused window; a raw `performClose`,
`orderOut` or `NSWindow.close` from the key equivalent is forbidden.
The Host must show the exact same document and window after Cancel.

`role:'quit'` is deliberately **held**: it must suppress native OS
terminate/Cmd+Q before the operating system can end the Host, and
record a typed diagnostic pointing to GH563. The GH563 owner later
routes app-quit through the GH451 veto/will-quit state machine. A
first-proof nonterminating diagnostic is a known recorded divergence;
it is **not** a general macOS Quit implementation. Specifically,
the native `Quit` item and its **Cmd+Q key equivalent** resolve to one
`UnsupportedQuit` Host handler, which consumes the activation without
calling `NSApplication.terminate`, emitting `before-quit` or closing
any dirty/clean window. Native OS termination from a separate Dock/system
quit is the separate GH563 admission/lifecycle gate, not falsely
qualified by the menu intercept; the native implementation must
prove it cannot be reached by this shortcut path.

For source custom-click items, Host action is
`UnsupportedClick`: output the GH556 diagnostic on a native
activation, leave checkbox check marks unchanged, and send no
Host→Bun EVENT. Menu-projection success is **not** evidence of
user preference update, Zoom action, external-browser navigation or
the other deferred custom callbacks. GH556 must add an explicitly
reviewed receiver/permission/callback state protocol before claiming
those functions work.

### 4.5 Generation, permission, effect and error boundaries

- One authenticated Host role's menu install, one immutable typed
  tree, one native generation and one outstanding transport correlation.
  On role loss, disable old commands, keyboard equivalents and native
  callbacks before any successor can act. An old command generation,
  stale focused `WindowRef`, old role or revoked grant cannot trigger
  a new role's window action even if the same key is pressed. When no
  current primary is authorized, native menu actions cannot target
  an old window. A new current role explicitly installs its menu;
  retained native handles retire on Host Quit.
- **Single observable diagnostic path for the first proof:** reuse
  the existing process stderr exposed by `keld-host` and forwarded
  through `keld dev`'s inherited stderr, observed as a captured
  **Host menu diagnostic record** in the native acceptance harness.
  This is a **proposed contract on an existing output path**, not a
  claim that a structured async menu collector already ships.
  Each record contains the registry-owned stable `KELD-*` code,
  machine-readable phase (`install` or `activation`), result
  (`denied`, `native-failed`, `effect-unknown` or `unsupported`),
  correlation, current role/menu generation when valid, implementing
  ticket for an unsupported item, and one actionable `fix`. No raw
  labels, paths, user text, OS selectors, identity secrets or native
  pointers are logged. The existing
  [error-code registry](../engineering/keld-error-codes.md) is the
  sole code/fix allocation owner; new codes are reserved/reviewed with
  the implementing source, never guessed here.
- **Outcome observation and exactly one owner:** Host writes one
  terminal stderr diagnostic for a received/refused install, late
  native failure, effect-unknown completion, held Quit or unsupported
  custom click. Successful installation yields a correlated
  `MenuInstallReceipt` internally (not an error log or public getter).
  Once a CALL may have reached Host, Bun does **not** invent a second
  duplicate diagnostic or assume not-sent; a disappeared/ambiguous
  completion remains **effect-unknown**. A definitely pre-send
  Bun-local failure (missing link, invalid local projection) is
  synchronously typed or emits the same canonical code/fix to Bun's
  inherited stderr, with a distinct pre-send phase and zero Host frame.
  Tests capture actual output from each owner rather than relying on
  mocked log arrays. This path does **not** expose a new Electron
  callback, PubSub subscriber, menu channel EVENT or private JS
  diagnostic API.
- **Void-return safety:** `@keld/api` schedules exactly one bounded
  authenticated install CALL for a valid snapshot; its internal
  continuation observes both REPLY and ERR, accounts for
  role-generation retirement, and consumes all rejections.
  `Menu.setApplicationMenu(menu)` returns JavaScript `undefined`
  immediately for an admitted valid call. A guard denial, native
  failure, late callback or link failure never creates an unhandled
  Promise rejection and never silently overwrites the prior native
  menu. No silent catch or automatic retry counts as successful
  installation. A hostile deadline/partial write cannot cause a
  repeated native effect.
- Missing `menu` grant: the shared `keld-guard` matcher returns a
  canonical typed `KELD-GUARD` denial naming `/app/system`,
  **zero native effects**, and a captured Host diagnostic as above.
  A malformed/duplicate/wildcard array, foreign role or wrong
  policy digest is independently denied before any NSMenu allocation;
  a verifier that accepts a nonliteral menu string fails the gate.
- Unsupported click/Quit: a typed registry-owned `KELD-COMPAT`
  diagnostic naming GH556/GH563 respectively, with no callback or
  native terminate. Native install failure belongs to the typed
  `KELD-NATIVE` namespace (if the registry admits that exact code),
  not a fabricated `KELD-GUARD` denial. Typed `CallError.code`
  and `message` remain the existing GH527 transport fields; menu
  logging does not alter KIPC wire framing.
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
| Add a new application-facing event/Promise for async install diagnostics | Reject: `setApplicationMenu` has a void return and GH508 admits no menu activation EVENT; keep Host stderr records and internal correlated CALL receipts, with exact observer tests |
| Accept a checkbox but omit checked or secretly flip it without calling Bun | Reject: source-visible `dontCheckUpdates` state diverges; an unsupported click must preserve the frozen initial mark and emit GH556 diagnostic |
| Maintain separate hard-coded Bun and Host native role shortcut tables | Reject: case aliases/labels/Cmd+W can drift; review one role registry and its generated representations |
| Handle Cmd+W via AppKit's direct close selector | Reject: the menu mouse click could pass while the keyboard path bypasses GH450's dirty veto |
| First-proof unbounded recursive menu graph | Reject: cyclic template/oversized tree max+1 tests fail before UI allocation |

No new transport thread or measured speedup is claimed.

## 5. Boundaries

After explicit spec approval and winning source claims:
- `packages/@keld/electron/src/menu.ts`, `menu-item.ts`, `index.ts`
  for synchronous authoritative Menu/MenuItem objects, typed checkbox
  and initial check state, role case folding, generated default labels/
  accelerators, source-level click priority and a void facade that
  observes its internal asynchronous CALL without unhandled rejection.
- `packages/@keld/api/src/menu.ts`, existing authenticated singleton
  adapter over GH527 WorkerLink, no new client connection or mirroring
  of the Bun authoritative Menu tree. It keeps correlation/result
  custody for each outstanding void installation.
- Existing `keld-guard` system-literal owner (GH478 or verified single
  alternate owner), `keld-ipc/src/channel_table.rs`/`channel_allocations.txt`
  append-only, and generated TS channel constant through GH508 owner.
  The GH480 native/facade seam has one reviewed source registry for
  supported role names/default labels/default shortcuts, with generated
  Host/Bun representations (no duplicate hand-maintained tables).
- `crates/keld-native/src/menu.rs` typed native NSMenu construction
  and command operations, checkbox checked display, Host-derived
  key equivalents and shared generation-checked role dispatch, with
  pinned objc2 (no deprecated `cocoa` crate), integrated only on
  the existing `keld-wv` macOS UI loop.
- GH531/GH450 current Host Core window request/close router,
  not an independent registry, and the already approved role-generation
  retirement mechanism. Existing `keld-host` stderr is the **proposed**
  externally captured typed menu-diagnostic observation path; assign
  its sole emitter/error-registry owner before declaring the spec
  approved, prove actual CLI stderr capture, and avoid an app event
  or a duplicate logger.
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
      GH531/GH450 close path, approve the single role registry and
      Host stderr diagnostic observation contract. Resolve §10 before
      changing production.
- [ ] T2. Reuse GH477's four GH480 expected-fail conformance cells;
      retain GH563 role-quit pending/fail and other ticket cells. Add
      **additional** source-exact checkbox, lower-case `hideothers`,
      custom-click-divergence and real-keyboard negative controls owned
      by GH480; do not invent GH477 pass cells before native proof.
- [ ] T3. Bun-local real draw.io Mac template (updates on/off),
      checkbox with both true/false initial states, MenuItem object
      identity, bounded hierarchy and case-insensitive role/default
      label/accelerator table; `role` overrides `click`. Explicitly
      reject unimplemented custom accelerators and malformed/cyclic
      graph, with zero frames on construction.
- [ ] T4. One GH508 guarded menu-install CALL, shared exact `menu`
      evaluator, current RoleInstance/policy-digest admission,
      correlated REPLY/ERR and stderr diagnostic observation; actual
      Mac NSMenu tree/check mark and post-denial no-effect witnesses.
- [ ] T5. First-responder native edit/HIDE roles via mouse **and**
      default Cmd+Z/C/V/H/Option+H equivalents, case-folded
      `hideothers`, with zero Bun activation EVENT frames. Native
      custom-click/checkbox activation produces GH556 diagnostic and
      preserves the initial check mark without executing Bun.
- [ ] T6. Compose role-close through the **existing** GH450 cancelable
      close path using both mouse and **real Cmd+W** on a dirty
      window; ensure dirty Save/Cancel/Discard stay separate GH478/
      GH564 implementation responsibilities. Native Quit mouse/Cmd+Q
      remains bounded and nonterminating with captured GH563 diagnostic.
- [ ] T7. Mac source/native tests, role-loss and old callback cleanup,
      native keyboard shortcut and error-effect-negative controls,
      invert GH480's four red tests only on actual proof, re-record
      GH563 pending cell, run all independent security/wire/public API/
      unsafe gates and final full hosted CI before merge consideration.

T3 can proceed after spec approval without GH450 source edits; T4/T6
wait for authenticated native owners. GH563 is **not** implemented in T6.

## 7. Test plan (failure-first, native OS distinguished)

| AC | Positive observable | Independent falsifier |
|---|---|---|
| M1 | Run unmodified draw.io template fixture in both `disableUpdate` branches; updates enabled has `autoCheckForUpdates` checkbox, store false/true yields checked true/false, role lowercases including `hideothers`; Bun tree is synchronous, preserves identities/order, uses pinned default accelerators and emits zero frames | Remove Checkbox or default checked field; alter `hideothers` to an unknown role; replace constructor with Promise: source census/traffic tests fail |
| M2 | Verified current primary role + exact grant emits one admitted CALL; empty, duplicate, wildcard, malformed or stale grant emits typed Host stderr denial with `/app/system` and zero native allocations or menu change | Bypass shared guard or accept wildcard/duplicate: denied real menu appears or no Host diagnostic |
| M3 | Actual Mac NSMenu bar matches source's three menu headings, role/default shortcuts, custom-click items and checked/unchecked checkbox, and `disableUpdate` removes the exact three update items | Drop checkbox/check state or silently remove custom-click items: source fixture/native screenshot census fails |
| M4 | Valid `setApplicationMenu` returns `undefined` immediately; Host correlated success receipt separate; inject guard rejection, delayed native failure, partial write/unknown effect, role retirement and capture stderr `code/phase/result/fix`, no unhandled rejection or auto-resend | Return Promise, omit a typed Host terminal diagnostic, double-report across stages or automatically replay after a partial write: result/capture/effect tests fail |
| M5 | Actual focused WKWebView edits via native menu and Cmd+Z/C/V and Cmd+H, Cmd+Option+H `hideothers`; count zero Host→Bun activation EVENTs | Delete default keyEquivalent, map lowercase role wrongly, target stale WebView or invoke Bun callback: actual keyboard/focus/callback/traffic tests fail |
| M6 | Dirty real draw.io native window mouse `Close` **and Cmd+W** both enter GH450 `CloseRequested(close_seq)` and receive Veto; Cancel preserves same window/document and next close works | Cmd+W native `performClose`/destroy bypasses sequence even while menu click passes: dirty-window/sentinel test fails |
| M7 | Real native Quit item **and keyboard Cmd+Q** each record GH563 diagnostic in captured stderr, no `before-quit`, no terminate, dirty window and Host remain alive | Restore direct NSApplication terminate or only guard mouse selection: PID and dirty sentinel/keyboard test fails |
| M8 | Actual native checkbox and ordinary source custom-click items remain displayed and enabled; activating each emits GH556 diagnostic, zero Bun frames/callbacks; checkmark and store remain unchanged (behavior marked divergent) | Silently flip checkmark, dispatch callback, remove unsupported source item or drop diagnostic: visual/state/captured-stderr/traffic tests fail |
| M9 | 0/max/max+1 nodes/depth/bytes, cycles, malformed enum, nonboolean checked and checked on normal nodes, invalid role/shortcut source, duplicate command id | Remove type/cycle/bounds checks: mutant allocates forbidden native tree |
| M10 | Bun role death→old menu mouse/key handlers disabled; fresh primary reinstall works; old Cmd+W cannot operate new role/window | Reuse old command-generation id or keep shortcut enabled: stale role action executes |
| M11 | Missing menu never authorizes dialog/fs/webview; explicit Legacy disclosed | Treat menu grant as all system calls; unauthorized effect passes and test fails |
| M12 | GH480 red cells flipped only with real native proof; GH563 red diagnostic retained | Flip role quit to pass / leave absent-constructor test after adding MenuItem; corpus fails |

Native acceptance uses the exact pinned draw.io menu fixture
(enabled and disabled updates, checked and unchecked store values),
`Menu.buildFromTemplate` + `setApplicationMenu` from its unmodified main,
real Mac mouse **and keyboard** activation, OS-visible item hierarchy,
initial checkbox NSMenuItem check mark and actual Host/Bun
frame/captured-stderr logs. The native Edit tests require a real focused
WKWebView text surface and a disabled/no-focus negative control. Capture
the Host's actual stderr independently of a Bun mock logger; classify
missing native diagnostic output as a failure, not a test pass. A
transport Unknown outcome cannot be called installation success from
the local `void` method return.
Accessibility/TCC denial is recorded as **unavailable**, never a
native pass or permission bypass. Sync uses observable callbacks and
bounded watchdogs, not sleeps; an external timeout is not a success
criterion. Where app action is intentionally deferred, negative outcome
is a typed stable diagnostic, not fake Electron parity.

## 8. Review gates triggered

- **Permission model:** yes — one system-literal `menu` grant, verified
  app RoleInstance, immutable policy digest and zero native effects on
  denial; reuse GH478's shared evaluator, never broader Wildcard.
- **Public API:** yes — synchronous Menu/MenuItem shapes, role priority,
  checkbox `checked`, lowercase role semantics, built-in defaults,
  bounded options, `void` installation and typed stderr observation.
- **Wire protocol:** yes — new append-only GH508 guarded menu channel,
  typed immutable tree/receipt with `Checkbox`/`UnsupportedClick`
  discriminants and boolean checked, hostile codec tests, no
  activation EVENTs.
- **Unsafe:** conditional and **blocking** — current
  `keld-native/AGENTS.md` production unsafe allowance covers specific
  Auth operations only; any new NSMenu/ObjC unsafe needs a documented
  narrow allowlist/source proof and independent review first.
- **Dependency:** none planned; pinned objc2 crate features may need
  explicit reviewed additions at implementation, not silently inferred.
- **Platform:** real macOS menu/checkbox, focused mouse + native keyboard
  responder, Cmd+W close-veto, Cmd+Q suppression, stderr diagnostics
  and physical cleanup; no Windows/Linux claims.

## 9. Perf impact

No measured speedup or latency result is claimed. Native menu builds
are cold user interactions; first-proof acceptance counts **zero
Host→Bun frames per native role or unsupported custom-click activation**.
Record total tree bytes, max heap/handle retention, install latency
vs source, repeated replacement, stderr diagnostic volume and actual
native menu callback/key-equivalent teardown. A >5% regression still needs
architecture 01 §5's established waiver. Native role dispatch cannot
raise Host20/Guardian/KIPC clocks or turn into an unbounded Bun worker.

## 10. Open questions (blocking for Status: approved)

1. **Shared guard admission:** approve the GH478/GH480 **single**
   `keld-guard` system-literal evaluator and exact
   `Principal::AppProcess + current RoleInstance + verified policy digest`
   check before native effects. GH478 is DRAFT; choose the one source
   implementation owner/precedence, maintain negative controls for
   malformed/duplicate/wildcard entries, and allocate the guarded
   GH508 channel only at the winning source head.
2. **Void diagnostics observation:** accept or revise §4.5's
   **existing Host stderr** observation proposal, including one
   correlated typed record with code/fix for rejection, native failure,
   unknown effect and unsupported activation, separate from the internal
   successful native-install receipt. Verify the chosen owner can emit
   late callback diagnostics in `keld-host` and that `keld dev`/native
   acceptance really captures them. No approved standalone async
   diagnostics subscriber exists today; this must **not** be asserted
   as shipped. Confirm the `@keld/api` internal continuation consumes
   every REPLY/ERR without unhandled JS rejection, a Promise return or
   retry, and allocate code/fix only with error-registry review.
3. **Single role table and checked-state admission:** approve the
   closed canonical role/default-label/default-accelerator registry
   with generated Host/Bun representations; validate pinned
   `hideothers` case fold, default Cmd+W/Z/C/V/H/Q and actual focused
   WKWebView. Admit the real `Checkbox` + boolean `checked` and
   `UnsupportedClick` projection from both update-toggle fixtures.
   The no-focused-window role-to-click fallback remains a
   source/native **unknown**; neither a custom callback nor a
   wrong-window shortcut may be executed as a workaround.
4. **Limits, callbacks and atomic update:** approve/revise max 256
   nodes, depth 8, 128 KiB request, 4 KiB label and monotonic menu
   generation. Require native atomic replacement, frozen unsupported
   checkbox state, one diagnostic per permitted activation, and stale
   command/key-equivalent retirement during Bun recovery. The
   non-role callback GH556 parity cell stays pending.
5. **GH450/GH563 lifecycle:** verify the winning GH450
   `RequestClose/CloseRequested/Veto` physical-wake seam for **both**
   mouse Close and real keyboard Cmd+W, including Cancel retention of
   the same dirty native window. Prove native Quit item **and** Cmd+Q
   cannot reach `NSApplication.terminate` or trigger `before-quit`,
   and that each reports the GH563 diagnostic. GH563/GH451 remain
   responsible for later real cancelable Quit and separate Dock/system
   Quit paths.

This is **Status: draft**. It does not approve an unimplemented native
menu, channel or permission evaluator, make menu/Cmd+Q work in KELD,
flip a conformance cell, change the source-owner merge state or close
GH480. Actual implementation begins only after exact approved designs
and winning native OS/source ownership claims.
