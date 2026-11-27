# Spec: Developer-tools facade with truthful native state
Status: draft
Linear: GH-536 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Map supported developer-tools members through the existing engine seam in development mode while preserving release denial and
refusing to report an inspector as open without an actual observation.

Non-goals: A new web.devtools release grant, private API use in release, printing and Windows/Linux qualification are separate.
Inspectability alone is not inspector-window state.

## 2. Spec refs

- [Owning issue GH-536](https://github.com/gyldlab/keld/issues/536) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Engine devtools action](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/engine.rs).
- [macOS action implementation](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/mod.rs).
- [Engine rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/AGENTS.md).
- [Electron devtools members](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-contents.md).
- [Policy decision owner](https://github.com/gyldlab/keld/issues/397).
- [Facade issue](https://github.com/gyldlab/keld/issues/536).

Verified baseline: Current macOS WebEngine.devtools dispatches Open/Close to wry and returns Result<(), WvError>. That return does
not establish an open-state observation. The issue proposes a release grant but says its policy amendment does not exist; this
draft keeps release behavior closed until that owner approves it.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a qualified keld dev window, when openDevTools is called, then the actual inspector opens and isDevToolsOpened becomes true
only from the approved native observation.

Independent negative control: Set the mirror true immediately after the Open command returns.

### AC2

Given an actually opened inspector, when it closes or is closed by the user, then the mirror becomes false and one corresponding
closed event is emitted.

Independent negative control: Track only facade closeDevTools calls and ignore native user closure.

### AC3

Given a release build without an approved grant, when any open/toggle/inspect path is called, then it performs no
inspector-opening action, emits the documented diagnostic and reports false.

Independent negative control: Allow inspectElement or toggle to bypass the release check.

### AC4

Given a destroyed or stale view, when a command or callback arrives, then it has the existing typed lifetime failure and cannot
affect a new view.

Independent negative control: Rebind by a recycled numeric ID alone.

### AC5

Given isInspectable is available but no open/close observation exists, when the backend qualifies support, then it reports only
inspectability and does not claim opened events or true open state.

Independent negative control: Treat isInspectable=true as evidence a window opened.

### AC6

Given inspectElement coordinates, when the qualified backend supports the operation, then it inspects the requested approved
coordinate space; otherwise a typed unsupported outcome preserves policy.

Independent negative control: Silently map inspectElement to open-at-default or accept invalid coordinates.

## 4. Design

### 4.1 Action versus observation

Reuse DevtoolsAction Open/Close and the existing UI-owned view. Extend only the necessary observation seam with a live backend
after design review. isDevToolsOpened is a mirror of actual native state; an accepted command is not that state. toggle uses the
same current observed state and one serialized owner.

### 4.2 Policy capture

The host captures dev/release mode and approved policy at boot; request arguments or mutable environment cannot enable release
tools. All facade paths share that admission. A future web.devtools key belongs to the permission/manifest owner and is blocked
here; the docs do not create a grant by naming it.

### 4.3 Availability and honest fallback

GH397 must qualify the source/OS method for open, close, inspect and state observation. Public isInspectable availability is not
proof of programmatic opening or an opened callback. If an operation cannot be observed or supported, keep the exact API
divergent/unsupported with a diagnostic instead of fabricating success.

### 4.4 Lifetime and migration

Native callbacks stay on the UI owner and pass existing host identity to the facade. Destruction unregisters them and retires
mirror state. Add action, observable and consumer in the same slice; no standalone state flag or second inspector manager. Release
paths retain current denial while development support is qualified.

### 4.5 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes stay in WebContents facade and the existing native devtools owner. Do not expose private native APIs in release,
grant web.devtools implicitly or alter other debug facilities.

This publication changes only `docs/specs/gh536-devtools-facade.md` and grouped-PR/issue metadata. Shared policies, architecture,
generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Resolve GH397's current OS/source/observation matrix and record pending/divergent cells per member.

### T2

- [ ] Implement shared dev-mode admission and real open/close observations with native user-closure controls.

### T3

- [ ] Implement toggle/inspect only where qualified, with lifetime and coordinate tests and truthful unsupported outcomes.

### T4

- [ ] Run dev and release binaries against real inspector visibility and event receipts; update supported rows individually.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-536;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Observe the actual native inspector and user close action; a mocked command counter cannot prove open state. Exercise every entry
point in a release binary. Record OS/API availability, view lifetime and native callback teardown.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and WebEngine design review; permission-model gate for any future release key remains blocked. Unsafe review only for
newly sanctioned native operations.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

No new work in release beyond existing refusal; callbacks only for qualified development views. Measure retained observer state
after repeated open/close/destroy; no performance benefit claimed.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Qualify native open/close/inspect and reliable state observations under GH397.
2. Keep any release grant blocked until the permission owner approves exact semantics.
3. Approve coordinate mapping and unsupported-operation diagnostics without false success.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

