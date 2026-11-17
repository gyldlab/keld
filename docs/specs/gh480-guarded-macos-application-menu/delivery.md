# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5-7, 9 and 10

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

## 6. Tasks: one bounded slice per PR after approval

### T-1 - Independently review/approve **this** GH480 exact-content spec,

Blocked by: none

reconcile current GH508/GH478 system-literal guard ownership and
GH531/GH450 close path, approve the single role registry and
Host stderr diagnostic observation contract. Resolve §10 before
changing production.
### T-2 - Reuse GH477's four GH480 expected-fail conformance cells;

Blocked by: T-1

retain GH563 role-quit pending/fail and other ticket cells. Add
**additional** source-exact checkbox, lower-case `hideothers`,
custom-click-divergence and real-keyboard negative controls owned
by GH480; do not invent GH477 pass cells before native proof.
### T-3 - Bun-local real draw.io Mac template (updates on/off),

Blocked by: T-1, B-3

checkbox with both true/false initial states, MenuItem object
identity, bounded hierarchy and case-insensitive role/default
label/accelerator table; `role` overrides `click`. Explicitly
reject unimplemented custom accelerators and malformed/cyclic
graph, with zero frames on construction.
### T-4 - One GH508 guarded menu-install CALL, shared exact `menu`

Blocked by: B-1, B-2

evaluator, current RoleInstance/policy-digest admission,
correlated REPLY/ERR and stderr diagnostic observation; actual
Mac NSMenu tree/check mark, recursive `autoenablesItems=NO`,
disabled-state stability across `[menu update]` and post-denial
no-effect witnesses.
### T-5 - First-responder native edit/HIDE roles via mouse **and**

Blocked by: B-3, B-4

default Cmd+Z/C/V/H/Option+H equivalents, case-folded
`hideothers`, with zero Bun activation EVENT frames. Native
custom-click/checkbox activation produces GH556 diagnostic and
preserves the initial check mark without executing Bun.
### T-6 - Compose role-close through the **existing** GH450 cancelable

Blocked by: B-5

close path using both mouse and **real Cmd+W** on a dirty
window; ensure dirty Save/Cancel/Discard stay separate GH478/
GH564 implementation responsibilities. Native Quit mouse/Cmd+Q
remains bounded and nonterminating with captured GH563 diagnostic.
### T-7 - Mac source/native tests, role-loss and old callback cleanup,

Blocked by: T-2, T-3, T-4, T-5, T-6

native keyboard shortcut **action/effect counts** (not merely
`performKeyEquivalent:YES`), post-update disabled-callback
and error-effect-negative controls,
invert GH480's four red tests only on actual proof, re-record
GH563 pending cell, run all independent security/wire/public API/
unsafe gates and final full hosted CI before merge consideration.

T3 can proceed after spec approval without GH450 source edits; T4/T6
wait for authenticated native owners. GH563 is **not** implemented in T6.

## 7. Test plan & evidence: failure-first, native OS distinguished


### M1 - synchronous Bun tree

Positive observable: Run unmodified draw.io template fixture in both `disableUpdate` branches; updates enabled has
`autoCheckForUpdates` checkbox, store false/true yields checked true/false, role lowercases
including `hideothers`; Bun tree is synchronous, preserves identities/order, uses pinned default
accelerators and emits zero frames

Independent falsifier: Remove Checkbox or default checked field; alter `hideothers` to an unknown role; replace constructor
with Promise: source census/traffic tests fail

### M2 - exact admission and no UI on denial

Positive observable: Verified current primary role + exact grant emits one admitted CALL; empty, duplicate, wildcard,
malformed or stale grant emits typed Host stderr denial with `/app/system` and zero native
allocations or menu change

Independent falsifier: Bypass shared guard or accept wildcard/duplicate: denied real menu appears or no Host diagnostic

### M3 - native menu materialization

Positive observable: Real Mac NSMenu matches source's three menus, shortcuts, custom-click items and initial On/Off
marks; `disableUpdate` removes three update items. Verify `autoenablesItems=NO` on root **and all
submenus**; `[menu update]` preserves Host-disabled role and frozen check mark

Independent falsifier: Leave a nested menu at AppKit's default autovalidation YES with a validator returning YES: `[menu
update]` re-enables a Host-disabled item and fails the native-state test

### M4 - Electron call shape and errors

Positive observable: Valid `setApplicationMenu` returns `undefined` immediately; Host correlated success receipt
separate; inject guard rejection, delayed native failure, partial write/unknown effect, role
retirement and capture stderr `code/phase/result/fix`, no unhandled rejection or auto-resend

Independent falsifier: Return Promise, omit a typed Host terminal diagnostic, double-report across stages or automatically
replay after a partial write: result/capture/effect tests fail

### M5 - host-native recognized roles

Positive observable: Real focused WKWebView edit/hide **effects** via mouse and default Cmd+Z/C/V/H/Option+H; assert
target/Host command counts and zero Host→Bun activation EVENTs. Disabled/no-focus item has zero
effect even if a synthetic `performKeyEquivalent:` returned YES

Independent falsifier: Match shortcut but suppress eligible target, or execute a disabled role after `[menu update]`:
effect/count/focus assertions fail regardless of match Boolean

### M6 - vetoable role close

Positive observable: Dirty real draw.io mouse Close **and Cmd+W** each enter GH450 `CloseRequested(close_seq)`/Veto;
Cancel preserves window/document. Disabled Close produces zero GH450 requests even if a synthetic
key matches

Independent falsifier: Direct native destroy bypasses sequence or treating `performKeyEquivalent:YES` as proof of an absent
GH450 command: dirty-window/close-seq control fails

### M7 - role quit is held

Positive observable: Real native Quit item **and keyboard Cmd+Q** each record GH563 diagnostic in captured stderr, no
`before-quit`, no terminate, dirty window and Host remain alive

Independent falsifier: Restore direct NSApplication terminate or only guard mouse selection: PID and dirty
sentinel/keyboard test fails

### M8 - non-role callbacks remain unsupported

Positive observable: Native checkbox/custom-click items remain displayed and source-enabled; activation emits GH556
diagnostic, zero Bun frames/callbacks. Forced `[menu update]` never toggles frozen On/Off nor
enables a Host-disabled callback; store unchanged (divergence)

Independent falsifier: Autovalidation revives a disabled item, silently flips mark, invokes unsupported Bun callback or
drops diagnostic: state/effect/traffic tests fail

### M9 - bounded tree and malformed refusal

Positive observable: 0/max/max+1 nodes/depth/bytes, cycles, malformed enum, nonboolean checked and checked on normal
nodes, invalid role/shortcut source, duplicate command id

Independent falsifier: Remove type/cycle/bounds checks: mutant allocates forbidden native tree

### M10 - generation and replacement custody

Positive observable: Bun role death disables old mouse/key handlers; `[menu update]` before/after successor install
leaves old callback/Host-command effects at zero even if a synthetic key matches. Fresh primary
install works

Independent falsifier: Default AppKit validator revives retired item, trusting match Boolean or reusing an old command id
allows a stale native target/close effect

### M11 - default-deny and first-proof profile

Positive observable: Missing menu never authorizes dialog/fs/webview; explicit Legacy disclosed

Independent falsifier: Treat menu grant as all system calls; unauthorized effect passes and test fails

### M12 - GH477 conformance and product evidence

Positive observable: GH480 red cells flipped only with real native proof; GH563 red diagnostic retained

Independent falsifier: Flip role quit to pass / leave absent-constructor test after adding MenuItem; corpus fails


Native acceptance uses the exact pinned draw.io menu fixture
(enabled and disabled updates, checked and unchecked store values),
`Menu.buildFromTemplate` + `setApplicationMenu` from its unmodified main,
real Mac mouse **and keyboard** activation, OS-visible item hierarchy,
initial checkbox NSMenuItem check mark and actual Host/Bun
frame/captured-stderr logs. Check recursive
`autoenablesItems=NO` and force `[menu update]` on disabled roles
and frozen checkboxes; record actual native target/Host command counts.
The standalone probe's synthetic `performKeyEquivalent:YES` for a
disabled Cmd+W had **zero target calls**, so matching alone cannot
score native KELD role parity. The native Edit tests require a real focused
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

## 9. Perf impact

No measured speedup or latency result is claimed. Native menu builds
are cold user interactions; first-proof acceptance counts **zero
Host→Bun frames per native role or unsupported custom-click activation**.
Record total tree bytes, max heap/handle retention, install latency
vs source, repeated replacement, stderr diagnostic volume and actual
native menu callback/key-equivalent teardown. A >5% regression still needs
architecture 01 §5's established waiver. Native role dispatch cannot
raise Host20/Guardian/KIPC clocks or turn into an unbounded Bun worker.

## 10. Open questions, blockers & unknowns: blocking for Status: approved

### Q-1 - Shared guard admission

 approve the GH478/GH480 **single**
   `keld-guard` system-literal evaluator and exact
   `Principal::AppProcess + current RoleInstance + verified policy digest`
   check before native effects. GH478 is DRAFT; choose the one source
   implementation owner/precedence, maintain negative controls for
   malformed/duplicate/wildcard entries, and allocate the guarded
   GH508 channel only at the winning source head.
### Q-2 - Void diagnostics observation

 accept or revise §4.5's
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
### Q-3 - Single role table and checked-state admission

 approve the
   closed canonical role/default-label/default-accelerator registry
   with generated Host/Bun representations; validate pinned
   `hideothers` case fold, default Cmd+W/Z/C/V/H/Q and actual focused
   WKWebView. Admit the real `Checkbox` + boolean `checked` and
   `UnsupportedClick` projection from both update-toggle fixtures.
   The no-focused-window role-to-click fallback remains a
   source/native **unknown**; neither a custom callback nor a
   wrong-window shortcut may be executed as a workaround.
### Q-4 - Limits, callbacks and atomic update

 approve/revise max 256
   nodes, depth 8, 128 KiB request, 4 KiB label and monotonic menu
   generation. Require native atomic replacement, frozen unsupported
   checkbox state, recursive `NSMenu.autoenablesItems=NO`,
   one diagnostic per permitted activation and stale command/key
   retirement **even after `[menu update]`**. Prove actual
   target/Host effects rather than `performKeyEquivalent:YES` alone;
   Host guard authorization remains independent of the UI bit. The
   non-role callback GH556 parity cell stays pending.
### Q-5 - GH450/GH563 lifecycle

 verify the winning GH450
   `RequestClose/CloseRequested/Veto` physical-wake seam for **both**
   mouse Close and real keyboard Cmd+W, including Cancel retention of
   the same dirty native window. Prove native Quit item **and** Cmd+Q
   cannot reach `NSApplication.terminate` or trigger `before-quit`,
   and that each reports the GH563 diagnostic. GH563/GH451 remain
   responsible for later real cancelable Quit and separate Dock/system
   Quit paths.

### B-1 - Shared system-literal Guard approval
Status: open
Owner: GH478 and GH480 shared Guard owners
Impact: No menu-install authority can be implemented or approved.
Check: Record one shared resource-less system-literal evaluator and exact owner approval.

### B-2 - Host stderr diagnostic contract
Status: open
Owner: Host and error-registry owners
Impact: Denial, late native failure and effect-unknown have no approved menu record grammar.
Check: Approve the exact stderr record, registry code/fix allocation and real keld dev capture test.

### B-3 - Role registry and checked-state API
Status: open
Owner: GH480 facade and native-menu owners
Impact: Bun and Host role/checkbox projections cannot be implemented as one approved contract.
Check: Approve the closed role table, default labels/accelerators and checked/unchecked projection.

### B-4 - Native bounds and atomic menu custody
Status: open
Owner: GH480 native-menu owner
Impact: Native allocation, replacement, callback and stale-command behavior remain unapproved.
Check: Approve node/depth/byte limits, recursive auto-enable controls and generation retirement.

### B-5 - Close and Quit lifecycle
Status: open
Owner: GH450 close owner and GH563/GH451 quit owners
Impact: Role Close and Cmd+W cannot ship without the dirty-window veto; Quit remains held.
Check: Approve real Mac CloseRequested/Veto and nonterminating Cmd+Q evidence.

This is **Status: draft**. It does not approve an unimplemented native
menu, channel or permission evaluator, make menu/Cmd+Q work in KELD,
flip a conformance cell, change the source-owner merge state or close
GH480. Actual implementation begins only after exact approved designs
and winning native OS/source ownership claims.
