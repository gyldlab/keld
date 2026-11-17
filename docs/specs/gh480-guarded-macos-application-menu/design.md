# Design

Part of: [index](index.md)
Owns: section 4

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
currently installed tree alive. **Set
`NSMenu.autoenablesItems = NO` on the root and every nested submenu**,
including replacements during recovery. The SDK defaults to YES;
the [real Mac probe](https://github.com/gyldlab/keld/issues/480#issuecomment-6388101245)
showed an enabling `validateMenuItem:` can otherwise override a
manually disabled item on `[menu update]`. The Host computes each
item's effective `enabled` from its admitted source value **and**
current Host generation/action eligibility. Reapply an authoritative
disable on role loss/revocation; an AppKit update must not revive it.
An unsupported-but-source-enabled GH556 item remains enabled for
**diagnostic-only** action, with a frozen checkbox check mark.
Native targets independently recheck the current Host role/generation
and command eligibility before **any effect**; a disabled native item
and AppKit's key-matching Boolean are **not security gates**. The
verified `app.system.menu` grant is separately required at Host
admission, never inferred from UI enabled state.

Commit the new menu *atomically* only
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
The oracle is actual target/Host-command execution and responder effects,
**not** `NSMenu.performKeyEquivalent:` returning `YES`: the standalone
probe got a **YES** match for synthetic Cmd+W on a disabled Close item
with **zero target calls**; when enabled it invoked the target once.
This is **not** global OS accelerator, KELD `CloseRequested/Veto`
or real WKWebView evidence. An item disabled by source, stale generation
or missing Host authority must execute **zero privileged effects**
even if AppKit reports a key match. A UI enabled bit or key match
never substitutes for the independent Host `app.system.menu` guard.
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
  On role loss, explicitly disable old commands/key equivalents and
  refuse stale native callbacks before any successor can act.
  `[menu update]` may not revive them. An old command generation,
  stale focused `WindowRef`, old role or revoked grant cannot trigger
  a new role's window action even if the same key is pressed. When no
  current primary is authorized, native menu actions cannot target
  an old window. A new current role explicitly installs its menu;
  retained native handles retire on Host Quit.
- **Single observable diagnostic path for the first proof:** reuse
  the existing process stderr exposed by `keld-host` and forwarded
  through `keld dev`'s inherited stderr, observed as a captured
  **Host menu diagnostic JSONL record** in the native acceptance harness.
  This is a **proposed contract on an existing output path**, not a
  claim that a structured async menu collector already ships.
  **FACT at Pin:** keld-cli dev launches the staged keld-host with inherited
  stderr (`crates/keld-cli/src/dev.rs:385-391`). keld-host writes and flushes
  HostRunFailure to stderr (`crates/keld-host/src/main.rs:118-125`).
  Core separately forwards captured primary-process stderr and its bounded
  elision notice (`crates/keld-core/src/app_session.rs:5867-5874`).
  This is a terminal-observable stream, not a subscriber API or durable store.
  The native harness captures the actual `keld dev` parent stderr. A mock
  logger or a Bun-only test does not qualify this observation path.
  Each terminal record is one JSON object on one line with this exact
  proposed field contract:

  | Field | Type and rule |
  | --- | --- |
  | `code` | Registry-assigned stable `KELD-COMPAT-*` code. The next free `KELD-COMPAT-013` is only a candidate; it is not allocated by this spec. |
  | `phase` | `pre-send`, `install` or `activation`. |
  | `result` | `denied`, `native-failed`, `effect-unknown` or `unsupported`. |
  | `correlation` | CALL correlation for install; Host-owned operation correlation for an activation; JSON `null` only for a definitely pre-send failure. |
  | `role_generation` | Optional authenticated role generation; omit when unavailable. |
  | `menu_generation` | Optional Host-assigned menu generation; omit when unavailable. |
  | `ticket` | Owning ticket, such as `GH480`, `GH556` or `GH563`. |
  | `fix` | Static actionable fix text from the error-code registry. |

  The record is proposed as JSONL, not free-form stderr prose. The registry
  owns the exact code and fix; this spec does not allocate `KELD-COMPAT-013`
  or any replacement. Never log menu labels, paths, user text, OS selectors,
  identity secrets or native pointers. The existing
  [error-code registry](../../engineering/keld-error-codes.md) is the sole
  code/fix allocation owner; its owner must approve the record grammar and
  allocate the code before implementation.
- **Outcome observation and exactly one owner:** A definitely pre-send
  Bun-local failure (missing link or invalid local projection) may emit one
  `phase=pre-send` JSONL record with `correlation=null` and zero Host frames.
  Once a CALL may have reached Host, only Host owns the terminal record:
  `phase=install` covers the guarded install outcome and late native failure;
  `phase=activation` covers a Host-native unsupported click or held Quit.
  Host emits exactly one terminal record for denial, native failure,
  effect-unknown or unsupported behavior. Bun must not emit a duplicate
  record or claim not-sent after admission may have occurred. A disappeared
  or ambiguous post-send completion is `effect-unknown`, never automatic
  replay. Successful installation yields a correlated `MenuInstallReceipt`
  internally (not an error log or public getter).
  Tests launch the actual `keld dev`, capture its parent stderr, inject each
  terminal outcome, assert exactly one Host-owned JSONL record after admission,
  assert no duplicate Bun-side record, and prove zero app-link/Bun effects
  for denied or unsupported cases. A mock logger does not qualify.
  This path does **not** expose a new Electron callback, PubSub subscriber,
  menu channel EVENT or private JS diagnostic API.
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
