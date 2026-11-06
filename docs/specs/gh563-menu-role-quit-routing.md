# Spec: Menu role quit routing into the app quit owner
Status: draft
Linear: GH-563 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Route native menu Quit and its Cmd+Q key equivalent into GH451's cancelable transaction, so dirty windows can veto and no adapter
directly terminates the process.

Non-goals: GH451 owns before-quit/will-quit/sweep semantics. GH480 owns menu installation and other roles. Generic item click
callbacks remain GH556.

## 2. Spec refs

- [Owning issue GH-563](https://github.com/gyldlab/keld/issues/563) and its latest public owner decisions.
- [Architecture
05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md):
native ownership, engine boundary and capability separation.
- [Lifecycle
owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/lifecycle.rs).
- [Current
facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Scoped native intent decision](https://github.com/gyldlab/keld/issues/563#issuecomment-6563533459).
- [Pending role-quit cell ownership](https://github.com/gyldlab/keld/issues/563#issuecomment-6386193487).
- [Menu owner
draft](https://github.com/gyldlab/keld/blob/83ded4df3c2355d69bf88d6f4202749febd40eb1/docs/specs/gh480-guarded-macos-application-menu.md).

Verified baseline: The published scoped decision allows a single QuitRequested lifecycle payload for the selected original
developer demo and D2 preflight. It explicitly excludes full will-quit, multiwindow and Dock semantics. This future pack preserves
that scope and specifies the later adapter acceptance.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a qualified current menu owner and an approved GH451 implementation, when the user selects Quit, then one intent reaches the
app coordinator and before-quit runs exactly once.

Independent negative control: Call native terminate directly from the menu callback.

### AC2

Given an unsaved window, when Quit is selected from the menu, then its normal close listener can veto and the native window and
app remain alive.

Independent negative control: Destroy the window without the GH531 close handshake.

### AC3

Given the same unsaved fixture, when Cmd+Q is pressed, then it dispatches the installed role-quit item and preserves the same
veto.

Independent negative control: Let the key equivalent bypass the role adapter.

### AC4

Given a stale menu stamp, retired app link or pre-Ready owner, when an old native activation arrives, then the adapter refuses
without native termination or dispatch to a successor.

Independent negative control: Trust the old menu's captured role without checking the current installed owner.

### AC5

Given only GH480 or scoped D2 support, when evidence is generated, then the GH563 full role-quit cell remains pending fail with
this implementing ticket.

Independent negative control: Flip the full role-quit verdict after a mock intent or the narrow demo alone.

### AC6

Given a nested or repeated valid intent during an active quit, when delivery reaches GH451, then that owner coalesces it and the
adapter does not create another sweep.

Independent negative control: Maintain a second quit-in-progress state machine in menu code.

## 4. Design

### 4.1 Native intent adapter

Use the current installed menu stamp, exact qualified app owner, final menu guard and authenticated Main/Ready check. Send the
single existing-owner lifecycle intent. The adapter owns validation and routing only; GH451 owns coalescing, exceptions, veto and
all terminal decisions.

### 4.2 Preserve scoped wire decision

The published local decision appends unit QuitRequested as tag 2 while preserving Ready 0 and LastWindowClosed 1. That is a scoped
owner record, not proof that current main contains the payload. Before implementation, reconcile the current generated schema and
any concurrent amendments; never independently reuse a numeric tag.

### 4.3 Runtime and retirement

Native menu actions occur on the UI owner, then pass through the existing bounded writer/event path. Retired link/menu activation
is terminally refused and not transferred to a successor. No new transport, renderer CompatSource principal, generic MenuSource or
permission is needed.

### 4.4 Migration and cell ownership

Keep the documented three-stage cell transition: absent surface, real unsupported diagnostic after menu support, then pass only
after GH563's real menu/key-equivalent veto acceptance. Existing scoped original-app approval remains valid within its limits.
Full GH451 behavior must be approved separately before this broader cell can pass.

### 4.5 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes stay in the existing native menu producer, generated lifecycle payload/receiver and facade adapter. Do not own the
app quit state, registry close state or other menu roles.

This publication changes only `docs/specs/gh563-menu-role-quit-routing.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Reconcile the current menu/lifecycle schemas with the preserved scoped decision and add pending stage-transition fixtures.

### T2

- [ ] Wire the validated native role intent into GH451 using existing transport and retirement checks.

### T3

- [ ] Run real menu and Cmd+Q dirty-window fixtures, duplicate/stale/pre-Ready controls and natural terminal exit for the clean
case.

### T4

- [ ] Update only the GH563 cell with exact source, device and receipts once all required behavior is proved.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-563;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

A facade call or synthetic JS event does not prove AppKit menu routing. Capture actual menu/key-equivalent source, before-quit,
close/veto, native window survival and process state. Keep Dock acceptance separate under GH451.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Adapter alone introduces no new boundary. Any changed lifecycle payload invokes its existing wire/public-API gate; full GH451
approval remains prerequisite.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

One existing lifecycle event per admitted intent; existing bounded coalescing handles repeats. No new queue, timer or startup work
is proposed.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Reconcile the scoped payload with current generated contracts and the full GH451 approved target.
2. Qualify installed menu and Cmd+Q routing on the selected macOS source/device before claiming pass.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

