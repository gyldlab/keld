# Spec: Windows and Linux popup interception and child ownership
Status: draft
Linear: GH-539 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Define the remaining Windows/Linux popup contract: run required application handlers, deny by default, admit an allowed child
only through the existing Host window/policy owners, and keep native child creation and release observable.

No duplicate navigation/evaluation adapter: GH1126 and GH1185 retain those contracts. No default-allow quirk, inherited
channel grant, raw engine-owned popup, process rewrite or claim that a denied popup proves handler compatibility. GH461
remains the shared popup-policy owner.

## 2. Spec refs

- [Owning issue GH-539](https://github.com/gyldlab/keld/issues/539); current public owner decisions remain authoritative.
- [Shared popup handler and policy owner](https://github.com/gyldlab/keld/issues/461#issuecomment-6026134489).
- [Current explicit Windows rejection](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/webview2/guarded_navigation.rs).
- [Current Windows engine](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/webview2/mod.rs).
- [Current Linux engine](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/webkitgtk/mod.rs).
- [Native boundary rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/AGENTS.md).
- [Linux navigation/evaluation contract](https://github.com/gyldlab/keld/blob/aaafcbfe69e29ff5ca68bdc9093eea1af85f40cc/docs/specs/gh1126-linux-webcontents-navigation-eval.md).
- [Windows navigation/evaluation contract](https://github.com/gyldlab/keld/blob/aa00b50ecafa7454bc950f7f8487cc28a68df186/docs/specs/gh1185-windows-file-origin.md).
- [Shared Host window identity](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh531-window-registry-close-state-machine.md).
- [Electron setWindowOpenHandler and did-create-window](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-contents.md).
- [Electron child option inheritance](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/window-open.md).

Verified source baseline: Current Windows GuardInstalled construction installs a NewWindowRequested handler that sets
Handled(true) without a NewWindow; Keld's source records this as deny-all. It does not invoke a Bun application handler.
GH1126's first Linux slice explicitly excludes popup creation; GH1185 retains deny-all and excludes opener-adopted windows
from its navigation premise. An engine showing no extra window therefore does not prove the target handler or allowed-child
path.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given script window.open, target=_blank, a relevant modified click or form target=_blank on an admitted document, when the
popup request reaches the adapter, then the real app handler is invoked once even if the guard or handler ultimately denies.

Independent negative control: Short-circuit before app invocation because the target will be denied; the handler's
independently observed allowed side effect must be missing.

### AC2

Given no handler or a null/undefined/unrecognized result, when classified, then deny without an uncaught exception, emit the
approved diagnostic and create zero native, registry or facade children.

Independent negative control: Silently allow, throw a native/JS exception for undefined, or emit did-create-window despite
zero registered child.

### AC3

Given a handler allows a URL the guard denies, when native disposition is finalized, then deny and observe no forbidden
child/navigation/resource effect.

Independent negative control: Use the app allow result alone at the native decision boundary; an independent
disposition/effect observer must fail.

### AC4

Given a fully qualified about:blank allow, when a child is created, then one Host-owned child uses the shared approved
inherited webPreferences behavior, allowed non-web option overrides and zero inherited el: grants.

Independent negative control: Apply override webPreferences to about:blank or copy the opener's channel grants; the
inheritance/grant controls must independently fail.

### AC5

Given a native child whose creation/configuration fails before ownership commits, when unwinding, then no orphan native
object, Host registry entry or did-create-window event survives.

Independent negative control: Publish the child identity/event before native admission succeeds or forget partial cleanup.

### AC6

Given a popup decision held while its opener navigates, is destroyed or changes role generation, when a late handler reply
arrives, then it cannot create a child for the retired document or bind a successor's authority.

Independent negative control: Correlate by URL or numeric window id only and accept the old decision after opener retirement.

### AC7

Given the approved per-engine decision/pending limit, when the limit or deadline is exceeded, then refuse before new
reservations and retain native resources until their completion/release is proven.

Independent negative control: Drop accounting on caller rejection while a native deferral or callback still retains the popup
request.

### AC8

Given a supported native allow path, when the page observes window.open and the actual original application uses the child,
then the declared WindowProxy/closed/opener behavior and did-create-window ordering match the approved per-engine contract.

Independent negative control: Create an unrelated ordinary window and count it as a linked popup, or use only the Host window
count as the full compatibility oracle.

### AC9

Given each native platform lane, when evidence is published, then deny, handler side effect, guard denial, real allow,
stale-decision and cleanup cases are separately qualified; absent allow feasibility stays unsupported.

Independent negative control: Mark all popup cells PASS from the current deny-all engine hook.

## 4. Design

### 4.1 Policy and mechanism are different owners

GH461 owns always-invoke, return classification, guard-and-app, about:blank inheritance and no-default-allow-quirk rules.
GH460 supplies navigation authorization. GH531 plus GH1125/GH1188 own child identity, creation and close. The Windows/Linux
adapter translates actual engine callbacks into those same contracts without creating another policy parser, permission check
or child registry.

### 4.2 Intercept before any child can escape

Keep the existing Windows permission/popup installation witness before first navigation. Its future extension must preserve
fail-closed behavior on partial handler installation. Linux must qualify the actual current GTK3/Wry create/decision seam and
install it through the existing media/construction owner. Do not assume an absent handler or default return is a complete
intercepted handler path. Malformed/optional native fields must be handled without panics before an app handler is called.

### 4.3 Timing feasibility is a blocking contract

The shared policy asks for an app handler before native child admission; that handler runs in Bun, while the native callback
has platform-specific return/deferral requirements. T1 must qualify a supported bounded deferral or an owner-approved
synchronous arrangement for each engine. No blocking UI wait, undocumented nested event loop, forged early handler result or
speculative unregistered child is allowed. If a correct route cannot be proved, retain deny-all with an explicit unsupported
handler/allow row; do not label that as full compatibility.

### 4.4 Native child creation and linkage

After current-document checks and the final guard-and-app decision, the Host mints one child through the existing window
owner and uses the engine's required related-view/controller configuration. Application URL/features/options remain untrusted
input. about:blank uses the shared approved inheritance rule; it does not clone channel grants. Only after native ownership
commits may did-create-window be emitted. The real page's WindowProxy and opener semantics need an independent per-engine
oracle, not an inference from a new visible window.

### 4.5 Handler effects are not popup authority

A draw.io-style handler can call openExternal then return undefined. Invoke the handler once, classify undefined as denial
with the shared diagnostic, and let the existing shell broker independently guard any side effect. A denied popup does not
roll back a handler side effect. A lost reply is outcome unknown for that handler; never automatically repeat it merely to
decide whether a child should be created.

### 4.6 Bounded decision and teardown

Bind each request to the original native view/document and role generation; page-supplied IDs cannot select the opener. Bound
callback/deferral count, captured details and timer state under one owner. On deadline or opener retirement deny when still
possible, settle the public path once, then release native request/handler registrations only at a proven native
completion/teardown. Do not free physical reservations from Promise rejection alone. An old allow response cannot create into
a replacement document.

### 4.7 Platform truth and migration

Keep Windows deny-all as the fallback until the new shared contract and native route are approved. Treat Linux native
relation/return semantics as unqualified until actual engine evidence. Do not edit GH1185's no-opener proof premise silently:
any allowed child must enter its native document/resource contract through an explicit owner amendment. Migrate the actual
handler, Host creation, facade events and per-engine conformance cells together. No shim-only supported result.

## 5. Boundaries

Future changes are confined to shared-policy projection, the existing Windows/Linux native popup hooks and current
Host/facade child creation owners. Native navigation/eval, profile stores, grants, schema allocation and permanent Electron
facades retain their owners. No raw page-native endpoint or broadened grant is introduced.

This publication changes only `docs/specs/gh539-portable-popup-contract.md` and related grouped-PR/issue handoffs. Shared
architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze the shared GH461 policy revision, per-engine callback/deferral feasibility, bounded native lifetime and child linkage contract, with actual primary/native receipts and appropriate owner amendments.

### T2

- [ ] Implement truthful no-handler/invalid/deny behavior plus always-invoke through the existing route; prove actual handler side effects separately from zero-child counts.

### T3

- [ ] Implement one qualified allow path as a complete Host-owned child vertical slice, including option inheritance, no copied grants, native linkage and partial-creation cleanup.

### T4

- [ ] Run original caller and hostile native stale/limit/destroy fixtures on each claimed OS, preserving unsupported rows and navigation/evaluation owners' current proof scopes.

## 7. Test plan

AC1 observes a harmless real guarded side effect from the original handler while child creation is denied. AC2/3 inspect
native decision, native object counts, Host registry and events separately. AC4 uses literal expected option values and a
real denied child channel call. AC5 faults every native creation handoff. AC6 forces old-response delivery after
navigation/destruction. AC7 counts retained native objects beyond caller retirement. AC8 observes the actual page WindowProxy
plus a linked child operation. AC9 separates model and engine results. All native execution is unrun in this session.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Public API, permission/navigation and native unsafe review; wire changes only through the existing generated owner if needed.
No dependency or unsafe expansion is approved by this draft.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

Bound pending native decisions and captured option/detail bytes; no unbounded queue or synchronous UI polling. A handler's
effect and native child cleanup costs are independent. No measured latency or compatibility percentage is claimed.

## 10. Open questions (block implementation)

1. GH461/GH460 and platform owners must approve the exact shared policy and truthful unsupported behavior without a
default-allow quirk.

2. Qualify native return/deferral timing across Bun and each UI thread; if no safe route exists, leave handler/allow support
blocked.

3. Approve native child linkage/configuration and amend GH1185/GH1126 premises explicitly before allowing popup-created
documents.

4. Freeze bounds/error shapes and collect real per-engine handler, WindowProxy, no-grant, stale and cleanup receipts before
publishing support.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.

