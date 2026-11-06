# Spec: Screen selection, display events and native theme extensions
Status: draft
Linear: GH-558 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Extend the existing GH481 desktop-facts owner with display selection/events, a current cursor query, app-local theme overrides
and qualified accessibility appearance values. The observable result is source-correct values and ordered callbacks on the
admitted primary role, with native and webview effects proved separately.

Non-goals: Do not reimplement GH481 first-snapshot/Ready logic, add a screen polling service, change OS-wide theme settings, or fold GH1356
systemPreferences into this slice. Windows/Linux coordinate conversion remains a separate qualified lane. Source discovery
authorizes this draft, not feature implementation.

## 2. Spec refs

- [Owning issue](https://github.com/gyldlab/keld/issues/558)
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md)
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md)
- [Existing desktop proposal](https://github.com/gyldlab/keld/blob/5de312ec0c8517cbb57733baaebc63e5d9ccd343/docs/specs/gh481-host-pushed-display-theme.md)
- [Shared recovery decision input](https://github.com/gyldlab/keld/issues/481#issuecomment-6679089486)
- [Screen contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/screen.md)
- [Theme contract and platform fields](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/native-theme.md)
- [Theme implementation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_native_theme.cc)
- [Actual Zettlr display consumer](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/windows/index.ts)
- [Actual Zettlr appearance consumer](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/appearance/index.ts)

Verified source and limits: Main 0eb3ce exports app and BrowserWindow only; this complete extension is not a landed facade. GH481/PR1164 supplies a draft
snapshot contract, and later public source handoffs report scoped S1 work; neither is blanket approval of GH558. The original
macOS snapshot and the later Windows recovery-policy decision are distinct. Electron's cursor result is a device-independent
pixel (DIP) point. Zettlr persists getDisplayMatching(window.getBounds()).id and owns nativeTheme changes in its appearance
provider.

All proposed API, schema, ownership and permission changes below are future contracts.
This documentation PR changes no runtime boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a current admitted macOS desktop snapshot with two displays and negative desktop offsets, when getDisplayMatching and
getDisplayNearestPoint run on fixed non-tie rectangles/points, then they synchronously return the same display IDs as the pinned
Electron oracle and cause zero new frames across 1,000 reads each.

Independent negative control: Replace the matcher with the primary display constant; choose an input unambiguously on the secondary display so the ID
assertion fails.

### AC2

Given a real display addition, removal or one-metric change, when the Host commits the next qualified observation, then one
corresponding event carries the correct current or removed Display, and changedMetrics names only bounds, workArea, scaleFactor
or rotation that actually changed.

Independent negative control: Dispatch before applying the snapshot; a listener's immediate getAllDisplays/getDisplayMatching read must disagree and fail.

### AC3

Given an identical repeated native observation and a recovery replay, when each is applied, then getters are initialized without
replaying historical display-added/removed or updated callbacks. A new actual change still emits.

Independent negative control: Treat every replay snapshot as a newly added display; the callback census must fail while the positive live change remains
observable.

### AC4

Given a supported live cursor query on macOS, when the pointer is moved to two independently witnessed positions including the
secondary display, then getCursorScreenPoint returns current absolute DIP coordinates synchronously.

Independent negative control: Return the last desktop snapshot's cursor or raw hardware pixels; a movement/scale-sensitive coordinate assertion fails.

### AC5

Given system/light appearance and themeSource='system', when themeSource becomes 'dark', then the immediate shouldUseDarkColors
read is true and exactly one updated callback for that qualified transition follows. Repeating an unchanged qualified transition
does not invent another event.

Independent negative control: Set only a Bun bool while suppressing the Host mutation; the separate native appearance witness fails even if the bool test
passes.

### AC6

Given dark override, when light and then system are selected in separate barriers, then the app's native UI and WKWebView
prefers-color-scheme reflect each qualified selection; system follows the real OS observation. An unavailable engine mapping
yields typed unsupported behavior before a fake pass.

Independent negative control: Change only native window decoration while retaining the old webview media-query result; the independent CSS witness fails.

### AC7

Given a native appearance accessibility property supported by the claimed platform, when its actual value is sampled, then the
named getter matches the independent native/reference observation. Unsupported or unavailable fields are explicitly refused,
never fabricated as false.

Independent negative control: Hardcode one qualifying boolean to false and run its true fixture; that property fails independently of dark-mode tests.

### AC8

Given a desktop update before a parked GH527 reply and a second update after that reply, when the caller resumes, then it sees
only the appropriate applied epoch before user listeners dispatch; old-role requests and callbacks cannot mutate or target a
successor.

Independent negative control: Invoke updated inside the state applier, or accept a role-bound stale completion; independent reentrancy and generation
assertions fail.

### AC9

Given invalid themeSource, malformed coordinates, missing readiness/admission or over-budget update, when submitted, then a
stable typed refusal precedes native effects and retains the last valid snapshot; a subsequent valid operation succeeds.

Independent negative control: Coerce an unknown theme to system or accept nonfinite geometry; the refusal/no-effect assertion fails.

### AC10

Given the unmodified pinned Zettlr appearance/window callers with appearance mode='system', when an actual Host theme
observation changes and a window moves between displays, then the original updated listener changes application darkMode and the
persisted display selection follows the supported contract.

Independent negative control: Remove the actual provider's nativeTheme listener while leaving synthetic theme events; the application-visible refresh
assertion must fail.

## 4. Design

### 4.1 Ownership and reuse

GH481 remains the only native display/appearance provider, Host snapshot writer and admitted GH531 mirror applier. Host
observers own OS handles; Core serializes observation, subscribe, retirement and publication. Electron owns names and callback
adapters. Extend the same typed representation instead of introducing a second mirror, channel, observer cache or parser. GH531
still owns window identity.

### 4.2 Display operations

Read matching/nearest-point from one immutable applied snapshot, through one shared geometry helper. Freeze tie, zero-area,
rounding, absent-display and invalid-input behavior from the pinned source/native oracle before approval; the broad
documentation phrase alone does not select them. Events derive from consecutive admitted Host epochs, commit values before
dispatch, preserve the removed record, and never reinterpret initial/recovery replay as physical monitor changes. Define
cross-event order and coalescing jointly with GH481 so no required removal is silently lost.

### 4.3 Cursor seam

Cursor movement is not a desktop-layout change. A stale cursor field in an occasional DesktopSnapshot cannot implement a current
query. Propose a bounded synchronous query through the existing Host/UI and GH527 request path; the owning wire/admission
reviewers must approve the exact operation, current-primary eligibility, coordinate conversion and callback custody. No guessed
wire tag, continuous pointer telemetry, new Worker or change to GH481's zero-IPC geometry getters.

### 4.4 Theme override and accessibility

Keep system observation distinct from requested override and effective result. Proposed themeSource accepts exactly
system/light/dark, starts at system, and affects this application only. Synchronous assignment commits only after an approved
Host barrier, with updated deferred out of the parked applier. Compare the complete qualified theme state, not just one dark
bool. A source change and a native change can differ; record the exact event oracle before approval. Each macOS accessibility
getter has its own native source and unsupported rule.

### 4.5 Native and engine effects

Native appearance and webview CSS are two independent acceptance atoms. Electron's Chromium behavior is a reference, not
evidence WKWebView follows the same hook. The macOS engine owner must select and demonstrate the existing supported appearance
API before activation; otherwise retain typed unsupported for the setter. Do not inject application CSS or change global
preferences to imitate a Host appearance contract.

### 4.6 Trust, lifetime and bounded work

GH481 passive-fact admission does not automatically authorize cursor queries or setters. The guard/primary-role owners must
approve the narrow added request admission. Reuse existing quotas and bounded observation storage, and include current,
in-flight and retiring references in the census. Native Host-wide facts and role-bound request completions remain distinct: the
latter are fenced before mutation. Preserve GH481's chosen recovery contract; the Windows retention-versus-resample fork stays
with its existing owners.

### 4.7 Platforms and migration

Mac first. The documented point conversion APIs are Windows/Linux members; rectangle conversions are Windows-only. Wayland has
documented cursor/conversion limits and needs its own disposition. inForcedColorsMode is Windows-only, not a made-up universal
false. Migrate native provider, canonical schema, mirror, thin facade and source-backed conformance cells together after owner
approval. Unsupported methods remain explicit until that member's OS proof passes.

## 5. Boundaries

Future implementation is limited to the existing GH481 provider/mirror and GH531/GH527 request composition, the Electron
screen/nativeTheme facade, and mapped compatibility tests. Current publication changes only this spec and handoff/PR metadata.
Do not edit active desktop/native worktrees, shared schema or architecture in this draft.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze the member/platform table, exact event/geometry oracles, cursor/setter admission, theme-to-WKWebView mapping and GH481
dependency revisions; record pending cells before implementation.

### T2

- [ ] Extend the one desktop provider/mirror with geometry events and qualified appearance properties, including replay/parked/update
negative controls and zero-frame read proof.

### T3

- [ ] Integrate the jointly approved current cursor and app-local theme operations with native effects, synchronous barriers, bounded
failure and independent CSS/native witnesses.

### T4

- [ ] Run paired macOS fixtures plus the original Zettlr appearance/window consumers; flip only evidenced members and leave other
engines/platforms unknown.

## 7. Test plan

AC1–AC4 require paired physical multi-display/coordinate fixtures, with source snapshots for deterministic tie/boundary cases.
AC5–AC7 separately witness native appearance and WKWebView CSS; controlled fixture overrides must not change the user's global
settings. AC8–AC9 use explicit writer/reply/retirement barriers and a healthy follow-up. AC10 executes the real application
consumer, not a recreated handler. Qualify theme notifications against the pinned Electron build before making an exact
unchanged-value event claim.

Every acceptance criterion has an independently observable outcome and named negative control.
Record source commit, pinned reference, exact platform/runtime and raw receipt for every claimed result.
Documentation validation covers immutable source/readback, ten sections, controls and file limits.
Runtime, native and original-app tests are planned, not executed by this drafting session.
GitHub documentation CI cannot establish platform acceptance. Local `just ci`, hooks and machine
closeout were not run: the user restricted this work to GitHub. Official upstream GitHub sources
were read within that restriction; no Context7 query or external-machine evidence is claimed.

## 8. Review gates triggered

Future public API, wire protocol and permission-model review; GH481/GH531/GH446 lifecycle alignment. Any new native FFI needs
its nearest owner's explicit unsafe allowance and independent review. No dependency addition is proposed or authorized.

Status remains draft. Implementation requires the section 10 decisions and approval of the exact contract.
Draft publication does not approve an upstream dependency or change a parked implementation schedule.

## 9. Perf impact

Preserve zero IPC for cached geometry/appearance reads. Measure native observer churn, retained snapshot/event bytes, cursor
call latency and setter UI tail separately. No performance gain is claimed; new event payload limits and aggregate pressure
disposition require the existing owners' approval.

## 10. Open questions (block implementation)

1. Approve exact geometry ties/invalid inputs, event order/coalescing, replay suppression and per-field native availability.

2. Approve cursor/setter request admission and the single schema/transport extension; no new capability or numeric allocation is
assumed.

3. Select and qualify macOS native plus WKWebView appearance behavior and synchronous setter failure semantics.

4. Bind the current approved GH481/GH531/GH446 composition and resolve any claimed platform's recovery-policy fork before
implementation.

Independent review must refute or fix concrete findings on the published revision. Platform gaps
remain explicit; no unexecuted acceptance test is reported as passed.
