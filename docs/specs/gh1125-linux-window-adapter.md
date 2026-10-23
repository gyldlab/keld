# Spec: Linux MVP native window adapter and facade boot
Status: approved
Linear: GH-1125 (#517) · Owner: @0monish · Updated: 2026-10-24

Approval: the repository owner approved the bounded design in the active user
session on 2026-10-24, conditional on independent engineering cross-checks. The
[approval record](https://github.com/gyldlab/keld/issues/1125#issuecomment-6373060597)
binds the reviewed technical baseline at PR #1133 head
`e2bc7cd6749c56b0568e3ad44d5c1f7d50b3e809`, spec SHA-256
`faa7f7da315fa067df501fb5e0a1dfb6cd7ad9d55af86e41251f6beb9b166c1c`,
Git blob `c39d6de00de05e803f9881c80a80833462a9c819`. This carrier records that
approval and corrects maintenance evidence; it claims no separate GitHub human
review or native acceptance. Implementation prerequisites remain in §6.

## 1. Goal & non-goals

Enable the existing shared BrowserWindow registry and close protocol on Linux using
the current GTK3/WebKitGTK backend. A facade boot has zero application windows at
Ready; its first constructor creates exactly one native window. Native close can be
vetoed without destroying that window, and accepted close releases the webview before
the window. This approved Linux platform extension still requires implementation
and native acceptance evidence.

Non-goals: another registry, codec, mirror, boot schema, principal, recovery policy or
UI thread; a GTK4 rewrite; Windows/macOS behavior changes; chrome (#395), hidden paint
(#394), renderer `beforeunload`, dialogs, window-bound roles, packaging, persistent
profiles (#1120), release qualification, or a complete draw.io compatibility claim.
The Linux program is #1124; #530 remains the cross-platform family owner.

## 2. Spec refs

- Architecture 01 crate ownership and §5 budgets; 02 §2/§4/§7 receive and lifetime
  contracts; 03 principal/guard boundaries; 04 conformance; 05 §1/§3 UI-thread handles
  and Core window ownership; 06 supervised Bun lifecycle.
- Approved `gh531-window-registry-close-state-machine.md`, implemented by #449/#450:
  sole identity/close owner, command/event shapes, replay and facts-before-listeners.
- [PR #657 technical head `a04e1ed8`](https://github.com/gyldlab/keld/pull/657#issuecomment-6371083658),
  full commit `a04e1ed889733a64cc33c1b64535e02b4afb0902`: gh446 and amended gh531,
  schema-2 Ready, subscription barrier, surviving-window adoption. Its approval is
  macOS-scoped; §4.9 explicitly refuses Linux schema 2. The separate approval above
  authorizes the exact Linux extension in §4.2; historical macOS approval stays scoped.
- Current main `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` includes GH1095's
  gh531 O11 amendment: `IPC-022` after a written Create is outcome-unknown and
  must not be replayed; `IPC-024` proves nonexecution. Preserve this later contract
  when consuming #657's older approved snapshot.
- `kel96-no-flag-host-boot.md` startup/cleanup; KEL-75 role/window lifetime;
  KEL-133 receive validation; KEL-136 shared transport. #528, #540/#541 and the
  shared #449/#450 implementation remain dependencies, not code to duplicate here.
- Baseline examined: `ef491ebd67ae794e004977a6d6578cdbb7235d61`. Linux
  `webkitgtk/mod.rs::create_internal` refuses post-loop creation and
  `run_app_until_quit` removes the view on `CloseRequested`; Core's direct runner
  always creates its initial window. Neither behavior proves facade support.

## 3. Acceptance criteria (binary, each becomes a test)

Each cell runs on Linux; native X11 and native Wayland results are recorded separately.
NC means a temporary negative-control fault that must fail that cell's own oracle.

1. **Boot and first Create.** Authenticated schema 2 reaches the real Init milestone
   with zero application toplevels; public readiness waits for accepted Subscribe and
   complete replay. A constructor in the ready callback creates one native window,
   with the exact Host-minted WindowRef. NCs: retain `create_app`, signal before Init,
   skip the subscription barrier, or mint a facade id. Each fails its own assertion.
2. **Create and observed state.** A live-loop Create yields one `WindowBuilt` and an
   OS-read snapshot, followed by ordered facts. Build failure retires its reference
   without `Created` or an orphan window. Native maximize/fullscreen/focus/resize
   changes agree with the mirror inside listeners. Wayland positions are `[0,0]`;
   X11 geometry is compared with the same window's native frame. NCs: request-derived
   state, an early fullscreen fact, or a failed-build handle leak.
3. **Veto.** A compositor/window-manager close request reaches one `close` listener;
   `preventDefault()` leaves the same visible native window and document usable.
   Repeat while pending merges; after veto a new request has a new close sequence.
   Advancing the registry test clock never closes it. NCs: restore today's immediate
   removal, insert a timer, or admit a second pending close. A facade `close()` call
   alone cannot satisfy the native-request cell.
4. **Accepted close and release.** Allow or Destroy produces exactly one webview
   destruction observation before one window destruction observation, then one
   `WindowReleased`. The shared registry writes `Destroyed`, then any
   `LastWindowClosed`, before the correlated reply. No fact follows `Destroyed`;
   callbacks cannot affect a later window. NCs: reverse native destruction order,
   acknowledge on command enqueue, or retain a stale state callback.
5. **Identity and authority.** Destroy A, create B, then send A's WindowRef, a forged
   incarnation, an old-role reply, and a renderer-originated window call. Each is
   refused by its existing owner and B is unchanged. A native number or WebviewId
   cannot substitute for WindowRef. NCs independently remove incarnation comparison,
   role ownership, or receive-policy enforcement; a GTK test is not authority proof.
6. **Mirror and transport.** Existing shared window conformance runs against the Linux
   host: facts precede listeners and blocking-call return, and 1,000 synchronous reads
   produce zero additional app-link frames. NCs: dispatch-before-apply or a getter
   round trip. No Linux mirror or socket reader exists.
7. **Failure and recovery.** Pre-Ready install failure, lost event channel, missing
   Init or terminal shutdown emits no Ready and cannot arm recovery. After Ready,
   the shared approved role-loss/adoption cells retain native identity and document,
   reject old authority, replay before Subscribe completion, and avoid a second
   allocation for an eligible survivor. NCs: early Ready, reusing retired authority,
   skipping replay, or rebuilding an adopted window. Source support is not a pass.
8. **Legacy and cleanup.** Schema 1 retains navigation-before-Ready, immediate legacy
   close, GPU preparation and ordered shutdown. Empty schema 2 emits no synthetic
   last-window event. After close, failed build, Quit and fatal exit, a fresh process
   can create and close its next window; no owned Bun child remains. NCs: reuse the
   facade close arm for schema 1, skip child reap, or fabricate last-window at Init.

## 4. Design

### 4.1 Atomic decomposition and reuse decision

This is a lifecycle/public-contract extension. No new crash domain or principal
minter is proposed. Every atom below is a design input; native outcomes remain
**unknown** until §7 runs. Source-established facts are marked **fact**.

| Atom / owner | Boundary: input → output | Failure / independent observable |
|---|---|---|
| Registry / Core, #449/#450 | admitted requests and UI facts → identities, transitions, ordered replies | second close/identity owner; inspect owning transition and AC5 (fact: gh531 owns it) |
| GTK adapter / keld-wv | Copy commands/native signals → handles and Copy observations | off-thread access, false state or premature drop; AC2–4 (fact: existing UI seam) |
| Boot / Core + gh446 | validated boot, installed services, genuine Init → raw Ready | duplicate initial window or early recovery; AC1/7/8 (Linux runtime unknown) |
| Identity / Core registry | Host-minted WindowRef → exact live incarnation | native-number substitution; AC5 with reused native number (approved owner, integration unknown) |
| Authentication / runtime + IPC | existing authenticated role link → admitted identity | stale/unbound sender; AC5 old-role negative control (unchanged mechanism) |
| Authorization / receive policy + guard | admitted principal/channel → operation or typed denial | renderer gains app authority; AC5 denied-operation oracle (no grant change) |
| OS containment / existing runtime profile | supervised child → existing OS boundary | unsupported containment claim; existing Linux profile receipts only, no claim added here |
| Lifecycle/revocation / registry + runtime | accepted close, role loss, terminal end → tombstone/release or approved adoption | stale callback or authority survives; AC4/7/8 |
| Evidence / test owner | versioned source and raw native observations → scoped receipt | source/mock substituted for native execution; distinct §7 cells and mutation records |

Independence edges: boot depends on installed registry/services, not on a created
window; native release supplies a receipt to the registry, not the close verdict;
the registry depends on shared subscription/replay ordering, not on GTK identity;
permission proof uses the receive boundary, not successful rendering. Falsifying
fullscreen observation cannot invalidate the native close primitive silently.

Reuse the existing `AppWindowCommand`/`AppWindowEvent`, tao proxy/wake bridge,
`View`, guarded `build_initial_gtk`, Core router and approved registry/mirror. Native
handles stay in the host UI thread; Bun receives serialized values only. GTK signal
closures own Copy identifiers and a proxy, never Core state or a strong widget cycle.
No additional thread, socket, wire parser, identity counter or close timer is added.
Configuration is captured at ResolveBoot; Create values at registry admission.

The one approved additional dependency edge is Linux `gtk = 0.18.2`, already locked
transitively by tao/wry, workspace-pinned under the designated manifest owner. It
provides safe `WidgetExt` signal/state access through tao's `WindowExtUnix::gtk_window`
and wry's `WebViewExtUnix::webview`. No direct WebKit binding or production unsafe is
needed. Published 0.18.2 package metadata still marks that release UNMAINTAINED;
current [RustSec RUSTSEC-2024-0415](https://rustsec.org/advisories/RUSTSEC-2024-0415.html)
was withdrawn on 2026-08-14 after upstream development resumed. The independent
dependency review supports this existing-package edge with unchanged features and
the existing GLib backport. Actual candidate manifest, cargo-deny, build and native
gates remain required; this approval adds no manifest edge or policy exception.
Registry receipt: `cargo info gtk@0.18.2 --registry crates-io` (2026-10-23) reports
MIT, Rust 1.70, and unmaintained status; Cargo.lock pins checksum
`fd56fb197bfc42bd5d2751f4f017d44ff59fbb58140c6b49f9b3b2bdab08506a`.
Use the already selected features; do not enable `unsafe-assume-initialized`.

Rejected: a GTK4 rewrite (no named requirement demands it); a second GTK application
or close handler (tao already prevents default destruction); polling requested
fullscreen state (tao `fullscreen()` caches the setter's request); another boot enum
or facade registry (existing owners suffice). No performance claim justifies a rewrite.

### 4.2 Exact approved Linux extension to shared owners

Use gh446's **same** `ParsedBoot.initial: InitialWindow` and `ReadyTrigger`:
schema 1 selects `Renderer`/`InitialNavigation`; schema 2 selects
`None`/`HostInitialized`. The closed schema-2 document, config resolver and lifecycle
Ready bytes are unchanged. Amend gh446 §4.9's Linux refusal only after the Linux
vertical slice passes; Windows continues refusing with its existing typed error.
The shared owner, not this spec branch, applies that amendment and the corresponding
gh531 Linux unsupported-platform/implementation-boundary wording together with code.

Linux's existing runner composes the same startup owner: validate/preflight before
resources; authenticate/install services, registry and wake bridge before loop entry;
no `create_app` for schema 2. Emit the existing `HostInitialized` variant only on
`Event::NewEvents(StartCause::Init)`, once per host process, never on
Poll or an arbitrary first callback. tao's GTK activation sends Init without a window.
The common guarded Ready transition and the facade's raw-Ready plus
Subscribe/replay barrier remain gh446/#449's; no Linux readiness boolean in TypeScript.

Reuse the shared first-milestone deadline/error/terminal owner for failed or absent
Init and event-channel loss. Loop return before Init is a typed startup failure,
not successful empty boot. Do not import AppKit custody objects or assume the macOS
pre-Init implementation proves GTK termination. AC7 must prove the Linux fail-closed
path, including a blocked UI thread, before platform enablement. Recovery/adoption
cannot be omitted as a shortcut: the integrated Linux role owner must satisfy the
approved gh531/gh446 rules or schema-2 enablement remains blocked.

### 4.3 Native command and observation mapping

No new public command/event shape: implement gh531's `CreateWindow`,
`SetMaximized`, `SetFullScreen`, `DestroyWindow`, `WindowBuilt`,
`WindowBuildFailed`, `WindowCloseRequested`, state facts and `WindowReleased`.
`WebEngine` remains unchanged. A private UI event wrapper may multiplex the existing
command with GTK state observations onto the **same** tao queue:

```rust
enum LinuxUiEvent {
    Command(AppWindowCommand),
    State { build: u64, view: WebviewId, maximized: bool, full_screen: bool },
    NativeDestroyed { build: u64, view: WebviewId, webview: bool },
}
```

Extract the existing constructor's live helper to take
`&EventLoopWindowTarget<LinuxUiEvent>`; both pre-loop schema-1 creation and the
live-loop `CreateWindow` arm use it. Keep guarded initial-content construction; no
bare wry builder bypass. On success read `WindowSnapshot` from native GTK/GDK state,
send `WindowBuilt` before buffered facts, then permit ordinary state delivery. If
the native snapshot cannot be read, unwind the partial build and report the shared
`WindowBuildFailed` stage; never fill it from requested geometry. Registry build
tokens and its Opening-state rules own late success after shutdown.
Private callbacks retain that existing opaque build token with WebviewId; match both
against the live/retiring build so a failed build's late signal cannot address a
successor. This is correlation with the registry's token, not another identity minter.

Bounds are the native frame in logical units; read GDK frame extents, not tao's
construction-time outer-size cache or a resize request. On native Wayland report
`x = y = 0`; ignore requested global position, matching pinned Electron and GTK.
Do not infer backend from an environment variable; use tao's actual display backend.
There is no new wire variant or hidden “real position.” X11 and Wayland size/state
facts still come from their native observations, not a promise that the compositor
honors every requested geometry.

Tao supplies `Moved`, `Resized`, `Focused` and `CloseRequested`. A safe
`connect_window_state_event` on `gtk_window()` supplies confirmed MAXIMIZED/FULLSCREEN
bits (`new_window_state`); its callback returns `Propagation::Proceed` and posts
Copy observations through the same proxy, preserving queue order with tao events.
Read initial flags through `GdkWindow::state`. Shared registry deduplication remains
the fact owner. A setter's successful enqueue is not an observed state change.

### 4.4 Close, teardown and callbacks

Tao's GTK `delete-event` callback emits `CloseRequested` and returns
`Propagation::Stop`, preventing GTK's default destroy. For schema 2, translate that
event to `WindowCloseRequested` and retain the View. Core alone owns pending state,
close sequence, veto, allowed close, role loss and terminal transitions. Schema 1
keeps its existing immediate teardown arm as a permanent compatibility path.

Only an admitted shared `DestroyWindow` or terminal session cleanup removes a View.
Retire its observation routing, disconnect Keld state handlers, tear down any owned
bridge, drop wry webview, then tao window. The existing field order is necessary;
native destroy observations independently verify it. Keep destruction observers alive
through their respective destruction, capturing no strong widget reference. Their
`NativeDestroyed` observations share the UI queue; a private retiring-view receipt
records which objects were observed, never close verdicts or app identity. Publish
`WindowReleased` once after both native destructions; stale queued facts are ignored
by exact live-build lookup and the registry's retired-incarnation state. Failed builds
track only objects actually constructed and emit `WindowBuildFailed` after their
release observations; they emit neither `WindowBuilt` nor `WindowReleased`. Unexpected
native destruction is a terminal session fault, not a synthetic approved close.
Terminal cleanup drains its native release observations before exiting the loop;
a failed observation delivery is a typed failure, never a fabricated release receipt.
Do not claim WebKit process exit or profile drain from widget destruction; #1120 owns
that separate resource contract. An empty native map never emits facade last-window:
the shared registry counts Opening entries and owns the lifecycle decision.

Capabilities/manifest changes: none. Wire changes: none beyond consuming the shared
approved window contract; no allocation or copied channel constants. Platform notes:
Linux GTK3/WebKitGTK 4.1 only; macOS/Windows unchanged. Migration unit: existing Core
Linux startup and UI event handlers, backend constructor/loop, shared facade callers
and conformance; no persisted state, generated codec change or temporary adapter.

### 4.5 Current-documentation receipt

- Applicability: applied: Linux close interception, Init, native state and teardown.
- Context7: used: `/tauri-apps/tao`, Linux GTK CloseRequested/delete-event/Init and
  event-loop commands; `/electron/electron`, Wayland getPosition/getBounds;
  retrieved 2026-10-23. Discovery only; pinned primary sources below decide behavior.
- Official primary: tao 0.35.3, commit `5a14e624c81b7a799728129417e9218be25f17d9`,
  [event loop](https://github.com/tauri-apps/tao/blob/5a14e624c81b7a799728129417e9218be25f17d9/src/platform_impl/linux/event_loop.rs),
  [window](https://github.com/tauri-apps/tao/blob/5a14e624c81b7a799728129417e9218be25f17d9/src/platform_impl/linux/window.rs)
  and [safe GTK access](https://github.com/tauri-apps/tao/blob/5a14e624c81b7a799728129417e9218be25f17d9/src/platform/unix.rs);
  wry 0.56.1, commit `14be44842747a62c4110bd982f61f6c1acd705c3`,
  [GTK destruction](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/webkitgtk/mod.rs).
  Source retrieval matched the local published crate files byte-for-byte.
- Supported claim: tao stops default close, GTK activation queues Init, live loop
  targets construct windows, fullscreen getter reflects requested state; wry Drop
  destroys its widget and tao Drop destroys its window. These are API facts, not
  execution or physical-drain receipts.
- Official primary: GTK/GDK 3.24 [delete-event](https://docs.gtk.org/gtk3/signal.Widget.delete-event.html),
  [state signal](https://docs.gtk.org/gtk3/signal.Widget.window-state-event.html),
  [state read](https://docs.gtk.org/gdk3/method.Window.get_state.html),
  [position limits](https://docs.gtk.org/gtk3/method.Window.get_position.html);
  [Electron 44.4.5 BrowserWindow](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/browser-window.md),
  retrieved 2026-10-23. Supported claim: native state notification and Wayland zero
  global coordinates; this platform limitation does not require a new window schema.
- Refresh: independent architecture and dependency reviews on 2026-10-24 support
  the approved source seams. RustSec's current advisory and raw frontmatter confirm
  the withdrawn maintenance advisory; selected-release metadata remains historical.
- Fallback/blocker: no API-source blocker; real compositor, lifecycle/recovery and
  callback-quiescence acceptance are unknown. The actual GTK-edge manifest,
  cargo-deny/build and native checks remain mandatory; documented APIs and design
  approval do not prove Keld integration.

## 5. Boundaries

Implement in: `keld-wv` Linux backend and its native fixtures; `keld-core` existing
boot/direct runner and shared coordinator; shared conformance owner for Linux cells.
GTK workspace pin/direct edge requires the designated Cargo writer. Existing shared
spec amendments and public status updates belong to their owners at integration.
Must not touch in this specification PR: production code, manifests/lockfiles, active
macOS specs, wire/schema generators, identity/guard policy, CI or agent instructions.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1, #1125: owner approval recorded above after independent lifecycle/API and
  authority/dependency review; exact technical content hash and §10 decisions bound.
- [ ] T2, #1125 implementation claim: after shared #446/#449/#450 dependencies, one
  Linux boot-to-Create/state/close vertical slice, regression/conformance first,
  together with the shared-owner platform amendments. No enabled half-adapter.
- [ ] T3, Linux proof owner under #1124: execute the pinned native X11 and Wayland
  cells and negative controls at T2's exact head; publish scoped conformance and
  dependency/OS receipts before advancing any MVP qualification.

## 7. Test plan

AC1/2/3/4/7/8 use isolated real Linux host subprocesses. Reuse the no-flag lifecycle
and KEL-132 native observation harnesses; extend their independent native census,
widget-destruction, document-continuity and child-reap oracles. AC3 starts with the
current immediate-close failure. AC5 uses real authenticated links plus forged/retired
requests; AC6 reuses the shared Bun/window conformance and frame counter. Every NC
names one removed/reversed transition and retains failing output plus restored pass.

Use condition-driven acknowledgements, isolated display/resources and bounded kill
switches; no sleep synchronization. Record exact host, Bun, Electron, GTK/WebKitGTK,
display server/compositor, GPU and source head. Xvfb is hosted X11 evidence only;
native Wayland requires an actual Wayland session and its own close/state oracle.
No source model, screenshot alone, or successful build substitutes for these cells.
Run `just ci`, actual format, warning-denied workspace clippy, workspace tests and
applicable documentation gates. This new spec is outside the LLMS source allowlist;
do not hand-edit generated corpora. Native tests have not run for this approved design.

## 8. Review gates triggered

- unsafe: none; no new production unsafe authorization.
- public API: yes, Linux behavior of the existing window/Ready contract.
- permission model: independent routing review for schema-2 platform enablement;
  no grants or principal rules change.
- dependency addition: yes, direct existing GTK 0.18.2 edge and workspace pin;
  no version bump. Registry/license/MSRV/advisory evidence required before landing.
- wire protocol: independent integration review of unchanged shared vectors,
  subscription and replay order; no Linux wire extension.

## 9. Perf impact

Architecture 01 §5 startup, first-paint and retained-memory budgets may move. Ready is
not first paint. This specification claims no improvement; implementing evidence must
separate native census, work, queues/copies, clocks, statistic and attributed artifact.

## 10. Open questions

None for the current design decisions. The owner approved extending the shared
schema-2/registry contract to Linux with the same recovery/replay rules, safe existing
GTK access and the reviewed direct GTK 0.18.2 edge, and documented Wayland zero global
coordinates with separate native qualification. T2 remains blocked until the shared
implementation dependencies and their ownership handoffs exist. Those are code and
acceptance prerequisites, not another request for the same design approval.
No macOS approval, spec-only merge, or X11 pass supplies a Linux native pass.
