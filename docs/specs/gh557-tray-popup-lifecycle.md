# Spec: Generation-bound Tray and ordered popup menu lifecycle
Status: draft
Linear: GH-557 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Define the future macOS Tray and popup-menu slice through the existing native menu, window, guard and role owners. Native
handles have generation-bound custody; an admitted popup preserves menu-will-show, menu-will-close, item click and final
callback order.

Non-goals: Implementation stays parked until its Zettlr tranche and predecessor approvals. No application-controlled GUID/AppUserModelID,
arbitrary native selectors, global-shortcut expansion, new menu policy, general Windows/Linux parity or unsupported Tray method
family. This user-authorized document does not change the parked milestone.

## 2. Spec refs

- [Owning parked issue](https://github.com/gyldlab/keld/issues/557)
- [Architecture 03 tray literal](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md)
- [Architecture 05 native ownership](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md)
- [Existing application-menu owner](https://github.com/gyldlab/keld/blob/83ded4df3c2355d69bf88d6f4202749febd40eb1/docs/specs/gh480-guarded-macos-application-menu.md)
- [Existing click/state proposal](https://github.com/gyldlab/keld/blob/092ba4ff958ad401bf4d7d70c49492f0f6fcff4d/docs/specs/gh556-non-role-menu-click.md)
- [Pinned Menu contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu.md)
- [Pinned Tray contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/tray.md)
- [Native popup owner](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_menu_mac.mm)
- [Native close/click/callback ordering](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/cocoa/electron_menu_controller.mm)
- [Actual Zettlr Tray caller](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/tray/index.ts)
- [Badge contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/menu-item-badge.md)
- [Badge platform availability](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu-item.md)
- [Actual Zettlr popup caller](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/menu/index.ts)

Verified source and limits: Electron's pinned macOS controller emits MenuWillClose before posting a deferred popup-close callback, with itemSelected running
before that deferred callback deletes the controller. Zettlr creates a Tray with a template-image path AND its app config UUID,
then sets tooltip/context menu and click activation. GH557 explicitly excludes app-selected GUIDs; that real caller conflict
blocks an unmodified Zettlr Tray compatibility claim until a recorded disposition. No native proof was run here. The same pinned
Zettlr menu provider also passes frame to popup for Writing Tools; an unsupported frame cannot be silently removed while
claiming the original popup caller passed.

All proposed API, schema, ownership and permission changes below are future contracts.
This documentation PR changes no runtime boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a current authenticated role with tray authority and an admitted resource image, when the supported Tray constructor
completes, then one native icon and one Host-owned incarnation exist; no raw native pointer or app-selected authority crosses
the link.

Independent negative control: Construct native UI before checking the tray grant; the denied-call icon/handle census fails.

### AC2

Given the qualified Tray subset, when tooltip/image/context-menu values change, then the same native incarnation reflects them;
a genuine click reaches the captured live callback once with the reference-qualified event fields.

Independent negative control: Update only facade state or synthesize a JS click; the independent native appearance/action witness fails.

### AC3

Given a live Tray, when destroy or role retirement occurs, then native teardown and callback quiescence complete exactly once;
stale/recycled IDs cannot reach a successor. Dropping a JS reference alone is not the declared lifetime owner.

Independent negative control: Keep the native icon after retiring only the Bun wrapper, or accept the old incarnation after ID reuse; native census and
callback assertions fail separately.

### AC4

Given an admitted macOS popup with one selectable normal item, when it is shown and selected, then the trace is menu-will-show,
menu-will-close, that item's click, then the final popup callback, once each.

Independent negative control: Emit click before menu-will-close or run the final callback before click; the literal ordered transcript fails.

### AC5

Given the same popup with no selection, when the user cancels or closePopup targets its original window, then will-close and the
final callback occur once with zero item click; closing again creates no synthetic selection.

Independent negative control: Reuse the last selected item on Cancel or settle callback twice; counters and order fail independently.

### AC6

Given a popup bound to window A and menu incarnation M, when focus shifts to B or M is replaced before deferred delivery, then
the existing admitted lifetime rules prevent retargeting to B or successor menu callbacks.

Independent negative control: Resolve the current focused window or current menu by public item ID at delivery; a distinct B/M2 callback identifies the wrong
owner.

### AC7

Given callback reentrancy, role loss, native presentation failure or event pressure, when the popup transaction retires, then
existing bounded delivery/cancel rules release native custody, preserve already executed effects and allow a healthy later
popup.

Independent negative control: Hold a registry lock across user code, replay a possibly executed click, or release reservation before native teardown;
reentrancy/effect/resource controls fail separately.

### AC8

Given invalid coordinates, unsupported frame/option, malformed badge or an unapproved GUID, when construction/popup occurs, then
typed refusal precedes native effect; no value is silently dropped to manufacture compatibility.

Independent negative control: Ignore the supplied GUID or unsupported option and return success; argument-to-effect/diagnostic assertions fail.

### AC9

Given the original pinned Zettlr Tray call containing its UUID and popup call containing frame, when coverage is reported under
the current unsupported contracts, then both cells remain blocked/unsupported with their exact caller conflicts; edited
UUID-free/frame-free clones cannot count as original-app acceptance.

Independent negative control: Delete the conflicting argument only in the harness and mark its unmodified caller passing; source-equality and coverage checks
fail.

### AC10

Given the approved macOS badge subset and supported OS version, when a MenuItem badge changes while its popup uses the existing
menu projection, then the native badge matches the admitted value without changing item callback authority.

Independent negative control: Retain badge only on the Bun object or use badge text as an identity; native display and identity controls fail independently.

## 4. Design

### 4.1 Shared owners and incarnation

The Host native owner holds Tray handles on the UI thread; the existing role registry supplies generation. Reuse GH480 menu
projection and GH556 item/state/callback bindings. One private tray incarnation plus current role identity addresses requests;
app IDs, labels and OS handle numbers are data, not authority. Extend one canonical generated family after owner allocation; no
new event connection, callback bus, focus registry or second menu policy.

### 4.2 Tray subset and image admission

First proposed subset: constructor with an admitted image and no app-selected GUID, destroy/isDestroyed, setImage, setToolTip,
setContextMenu, genuine click, and existing popup-menu integration. An image filename does not grant filesystem access. Resolve
packaged template images through the existing resource/FS admission owner or accept a separately validated bounded image value;
retain macOS template semantics. Other Tray events/methods are explicitly unqualified until their actual demand and platform
oracle are specified.

### 4.3 Lifetime and authority

Retain native Tray custody until explicit destroy, role retirement or Host termination, a declared divergence from a
garbage-collection-driven reference lifetime. JS object collection is not authorization to resurrect or transfer a native
handle. The tray grant admits Tray effects only; attached menu and selected callback effects still need their existing
authority. Quiesce native callback sources before releasing/reusing an identity. A caller exception is not native teardown.

### 4.4 Popup transaction

Capture eligible parent WindowRef, menu installation/incarnation, callback, item command binding and coordinate inputs at
admission. Use GH480/GH556 validation/state-before-click rules and GH531 window identity. On macOS native close first commits
will-close, then the selected GH556 click, then final popup completion; post completion so it cannot delete native controller
state before selection processing. Cancel has no selected click. Hold no internal lock while invoking user code.

### 4.5 Deferred ordering and replacement

Bound retained popup callbacks, native controller references and deferred activations under the existing role-wide menu budget.
Preserve the original item/window through allowed delivery; after its retirement fence, drop unstarted stale work with a
terminal disposition, never redirect it to the successor. Already-running callback effects cannot be recalled or replayed.
Extend the same physical cancellation/retirement owner for lost role/window and native failures; merely returning from
closePopup does not prove native quiescence.

### 4.6 Options and badge scope

Freeze x/y paired-coordinate, positioningItem, absent/focused-window, no-window, callback and reentrancy behavior with the
reference oracle. No frame pointer, arbitrary WebFrameMain or Writing Tools capability is synthesized. Pinned badges are macOS
14+: types alerts/updates/new-items require count without content; type none (default) requires content without count. Setting
badge undefined removes it; Dock menus do not display it. Extend GH480's single validation/projection, with approved
numeric/text bounds and native observations, rather than a facade-only property. Unsupported versions/options receive a
documented refusal/divergence. Windows/Linux popup order and sourceType remain separate.

### 4.7 Original-app conflict and migration

Zettlr's app-controlled UUID conflicts with the owning issue's never-app-chosen GUID rule. Its popup also supplies frame to
access native Writing Tools. Preserve these exact caller inputs as acceptance blockers until the product/security and
webContents owners select a documented caller migration or explicit approved identity/frame contract amendment. A UUID-free or
frame-free test clone cannot establish original-app parity. Migrate facade handles, menu operations, generated schema and
conformance cells together, preserving GH556 click semantics and GH451 app.quit for the normal Quit callback.

### 4.8 Limits and cleanup

Approve active/retiring Tray counts, menu tree bytes, image decoded/encoded storage, popup records and sequence exhaustion
before activation. Inherit shared transport pressure policy; no unbounded close-callback queue. Include native resources in the
census until actual callbacks are quiescent. A healthy follow-up is required after canceled/failed/retired operations. No
existing timeout, global native state or other owner's cleanup policy changes here.

## 5. Boundaries

Future implementation stays with existing native/menu/Core/API/Electron and generated-contract owners. No runtime code or shared
contracts are changed by this publication. GH480/GH556 menu, GH531 window identity and GH451 quit decisions remain separate
dependencies; unapproved drafts are not promoted.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze the exact Tray/popup/badge/platform table, source oracles, bounds and GUID caller-conflict disposition; add pending
conformance cells while leaving implementation parked.

### T2

- [ ] Implement the guarded generation-bound Tray subset and admitted resource-image path under the existing native owner, with
deny/teardown/late-callback controls.

### T3

- [ ] Extend one shared menu projection/delivery path with popup ordering, close/cancel, replacement, approved badge and bounded
callback retirement; preserve GH556 state-before-click.

### T4

- [ ] After the product schedule and contract gates permit it, run real macOS native controls and the exact approved Zettlr
migration/original fixture; document remaining unsupported members.

## 7. Test plan

AC1–AC3 need actual native icon/handle/action witnesses and independently injected stale identities. AC4–AC7 record literal
event order under real Cocoa menu tracking, programmatic close, replacement and role loss; a JavaScript emitter mock is
insufficient. AC8 validates every accepted option and refusal before effects. AC9 binds tested caller bytes to the actual source
and prevents an edited fixture from claiming original-app parity. AC10 requires the supported OS/version badge witness. Do not
alter the user's menu bar or run native UI in this GitHub-only authoring session.

Every acceptance criterion has an independently observable outcome and named negative control.
Record source commit, pinned reference, exact platform/runtime and raw receipt for every claimed result.
Documentation validation covers immutable source/readback, ten sections, controls and file limits.
Runtime, native and original-app tests are planned, not executed by this drafting session.
GitHub documentation CI cannot establish platform acceptance. Local `just ci`, hooks and machine
closeout were not run: the user restricted this work to GitHub. Official upstream GitHub sources
were read within that restriction; no Context7 query or external-machine evidence is claimed.

## 8. Review gates triggered

Future public API, permission model and wire protocol; independent native unsafe review for any newly sanctioned UI FFI. Image
decoding dependencies trigger separate review. GH557 remains parked and draft; no implementation or merge authority is supplied.

Status remains draft. Implementation requires the section 10 decisions and approval of the exact contract.
Draft publication does not approve an upstream dependency or change a parked implementation schedule.

## 9. Perf impact

Measure native image copies, active/retiring handles, callback queue bytes and menu-open/close tail independently. No
performance claim, numeric pressure limit or permanent background poll is introduced by this draft.

## 10. Open questions (block implementation)

1. Approve the bounded subset and exact native option/event/badge behavior; qualify each claimed macOS version.

2. Resolve the observed Zettlr UUID/no-app-GUID and frame/Writing Tools caller conflicts with existing
product/security/webContents owners.

3. Approve shared schema/admission/aggregate limits and native callback-quiescence handling without replacing existing owners.

4. Schedule the parked Zettlr implementation tranche and bind the exact approved predecessor revisions before implementation.

Independent review must refute or fix concrete findings on the published revision. Platform gaps
remain explicit; no unexecuted acceptance test is reported as passed.
