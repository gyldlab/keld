# Contract and acceptance

Part of: [index](index.md)
Owns: section 3

## 3. Acceptance criteria: observable and binary

### AC-1 - Missing menu grant is effect-free

Given: The Host has an existing native menu. The current app RoleInstance lacks the exact menu grant.
When: Bun submits one valid Menu.setApplicationMenu snapshot.
Then: The shared Guard denies before NSMenu allocation or replacement. The existing menu remains functional. The Host emits one diagnostic to inherited stderr. The facade returns void and consumes REPLY/ERR. It does not retry.
Negative control: The bypass must cause a native menu effect.
- Single fault: Bypass the shared Guard before the native menu builder.
- At-risk precondition: A current app RoleInstance has no exact app.system menu grant.
- Expected RED: The native builder receives the request or the captured Host stderr has no denial record.
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
On **every** Host-built `NSMenu` (root and recursively nested submenus),
set `autoenablesItems = NO` before publication and keep native
`NSMenuItem.enabled` under explicit Host projection/generation control.
A later `[menu update]` must not re-enable a Host-disabled item or
flip its frozen checkbox check mark.
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
without the OS action is not a pass. `performKeyEquivalent:`
returning `YES` is **not** proof of target execution: assert
real native callback/Host-command counts, responder effects and
disabled/no-focus controls.

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
Neither `[menu update]` nor a disabled item's mouse/key activation
may manufacture a callback or flip that check mark. Role items with
click callbacks execute only the role.

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
