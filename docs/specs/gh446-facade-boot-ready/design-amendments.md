# Normative amendments and implementation interfaces
Part of: [index](index.md)
Owns: Exact amendment text, types, platform notes, runtime seam, migration and pinned evidence.

## 4. Design: amendments and implementation interfaces

### Amendment text (exact content of the delegated decision)

**KEL-96 §4.1.** Append one row inside the decision block. This changes `decision_digest`.
Join the next two segments with one ASCII space. Do not include the line break:

```text
| `KEL-96-D12` | `keld.boot.json` schema 2 declares a boot whose app creates every window: closed fields `schema`, `name`, `entry` and `permissions`, and no `renderer`. Its `Ready` follows authenticated `HELLO` and the UI loop's `Init` turn; the host refuses window calls before that `Ready` and
creates no window in any generation on its own. Schema 1 is unchanged and keeps `CreateInitialWindow` before `Ready`. Schema 2 runs on macOS only; Windows and Linux refuse it with `KELD-CORE-034` before any application resource (GH-446). |
```

### KEL-96 D-12 exact decision text

`keld.boot.json` schema 2 declares a boot whose app creates every window: closed
fields `schema`, `name`, `entry` and `permissions`, and no `renderer`. Its `Ready`
follows authenticated `HELLO` and the UI loop's `Init` turn; the host refuses window
calls before that `Ready` and creates no window in any generation on its own. Schema
1 is unchanged and keeps `CreateInitialWindow` before `Ready`. Schema 2 runs on macOS
only; Windows and Linux refuse it with `KELD-CORE-034` before any application resource
(GH-446).

**KEL-96 approval metadata.** Amend in place:

- **Header `Decision digest:`.** Replace it with the digest T-1 recomputes over
  D1–D12, using the §4.1 rule and the command in this spec's §7.
- **§2 provenance.** "The eleven selections below" reads "The eleven original
  selections below (D1–D11)". After the provenance list, append:

  > **Amendment GH-446 (D12).** `KEL-96-D12` was added by GH-446 (spec #657,
  > implementation T1). The new owner-delegated decision and exact technical-content
  > review are recorded on #657, bound to the reviewed technical head and specification
  > blobs; the implementing PR separately records its exact diff review. The header
  > digest covers D1–D12 under the §4.1 rule; the `decision_digest` in the list above
  > remains the value the original review approved for D1–D11. Historical approval
  > does not approve the new semantics.

- **§7 "Decision completeness".** "Exactly one `KEL-96-D1` through `KEL-96-D11`"
  reads "Exactly one `KEL-96-D1` through `KEL-96-D12`".
- **§10.** "The eleven architecture decisions are frozen" reads "The twelve
  architecture decisions (D1–D11, and D12 added by GH-446) are frozen".

**KEL-96 AC-1.** Replace with:

> A valid `keld.boot.json` is a bounded, strict document of schema 1 or schema 2.
> Schema 1 requires `schema`, `name`, `entry`, `renderer`, and `permissions`, and keeps
> its already-approved optional `auth` configuration and validation unchanged.
> Schema 2's exact closed fields are `schema`, `name`, `entry`, and `permissions`.
> `permissions` contains only `file` and `content_sha256`. Duplicate or unknown
> fields, an unknown schema, non-UTF-8 input, or input over 64 KiB is rejected.

**KEL-96 AC-8.** Append:

> This criterion is for schema 1. Given a valid schema-2 descriptor on macOS, launching
> `keld-host` with no diagnostic flag starts the Bun `entry` and runs the event loop with
> no native window until the app creates one (GH-446).

**KEL-96 AC-11.** Replace with:

> Startup follows §4.4's state machine. Lifecycle `Ready` is emitted only after
> authenticated `HELLO` and, for schema 1, initial-window registration/navigation
> readiness, or, for schema 2, the UI loop's `Init` turn; it is never implied by
> connection or handshake.

**KEL-96 §4.4 step 2.** Read "canonicalize/open `entry` and, for schema 1, `renderer`".

**KEL-96 §4.4 step 7.** Replace with:

> 7. `CreateInitialWindow` (schema 1): create/register the window and finish its
>    initial renderer navigation on the UI thread. `StartLoop` (schema 2): install every
>    host component that serves the app's window calls, enter the UI event loop with no
>    window, and wait, bounded by the initial milestone deadline, for its `Init` turn on
>    the UI thread. Window calls are refused until step 8.

**KEL-96 §4.5.** Append:

> For schema-2 boots the host creates no window, so step 1 is not this coordinator's:
> window counting belongs to the GH-531 registry, and the legacy
> `AppWindowEvent::LastWindowClosed` UI milestone is unused. The registry still causes
> the host to write `LifecycleEvent::LastWindowClosed` after the final `Destroyed`
> under GH-531 criterion 32 and GH-531 T-11, preserving `window-all-closed` and default Quit.
> An initially empty facade boot does not synthesize that event. Steps 2–5 apply unchanged.

**KEL-139 "Recovery".** Append:

> Renderer-declared schema-1 recovery continues to preserve both native-window identity
> and document state. In a schema-2 facade boot (GH-446), each surviving native window
> retains its identity under GH-531 D-6. Its document remains unchanged until the
> recovered application explicitly navigates that window. Old-generation authority
> retirement and filesystem-broker quiescence still precede successor Ready and
> privileged work. The host owns native allocation and lifetime; the application
> requests creation and navigation. Registry `Create` adoption (#449) prevents an
> extra allocation when an eligible survivor exists, without restricting legitimate
> later creates. This is a new scoped amendment recorded under the 2026-10-23
> owner delegation in #657; it does not rewrite the original approval's scope.

Falsifiers: any survivor loses native identity; a document changes without explicit
authorized navigation; a `Create` allocates while an eligible survivor exists; or
schema-1 continuity changes. A legitimate extra application `Create` after eligibility
is exhausted is not a failure of this contract.

**KEL-139 AC-3.** Append:

> The same authenticated-generation `Ready` gate holds in schema 2. In its initial
> generation, host Ready precedes every application-created window. A successor may
> inherit surviving windows; it receives Ready after fresh authentication and the
> existing retirement/quiescence gates, and the facade becomes publicly ready only
> after accepted window subscription and complete replay (#449).

**Architecture 02, no-flag primary paragraph.** Replace "Initial `Ready` follows native
renderer navigation;" with:

> Initial `Ready` follows native renderer navigation in a schema-1 boot and the UI
> loop's `Init` turn in a schema-2 boot, which has no host-created window (GH-446);

**Architecture 06 §1, macOS no-flag primary.** Append:

> A schema-2 boot (GH-446) creates no native window on its own: the same router writes `Ready` on
> the UI loop's `Init` turn, and a successor generation still receives `Ready` after its
> fresh HELLO.

**`docs/engineering/keld-error-codes.md`.**

- `KELD-CORE-034`: append to its fix, "For a schema-2 descriptor on Windows or Linux,
  remove `initialWindow` from `keld.config.ts` to boot a renderer window."
- `KELD-CORE-035`: the message reads "The private schema-1 or schema-2 boot descriptor
  is invalid", and the fix reads "Remove duplicate or unknown fields and regenerate a
  bounded strict `keld.boot.json` of schema 1 or schema 2."
- New `KELD-CORE-044`. Crate keld-core. Message: "A UI startup milestone arrived that
  this boot kind does not expect, or arrived twice". Fix: "This is a host defect;
  report it with the host log, then relaunch the staged host."

### Types, platform notes, runtime seam and migration

Types (sketch):

```rust
// keld-core, private
enum InitialWindow { Renderer(PathBuf), None } // from schema 1 or schema 2
// Extend the existing ParsedBoot with `initial`; retain its other current fields,
// including the approved schema-1 auth configuration. This is not a replacement struct.
enum ReadyTrigger { InitialNavigation, HostInitialized } // chosen from InitialWindow
fn coordinate_window_events(
    events: &Receiver<AppWindowEvent>,
    router: &PrimaryRouterHandle,
    commands: &Sender<AppWindowCommand>,
    trigger: ReadyTrigger,
    first_milestone_deadline: Instant,
    pre_init_terminal: fn(&HostAppError) -> !,
) -> Result<(), HostAppError>;

// keld-core, public: the ordinary result preserves retained main-thread custody.
// The binary-owned callback is only for an unrecoverable pre-Init stall.
pub fn run_guarded(
    boot: ValidatedBootSelection,
    pre_init_terminal: fn(&HostAppError) -> !,
) -> Result<(), HostRunFailure>;

// keld-wv, public
pub enum AppWindowEvent {
    NavigationReady,
    LastWindowClosed,
    /// The UI loop delivered its `Init` turn on a backend that created no initial
    /// window (macOS: tao `StartCause::Init`, from `applicationDidFinishLaunching:`).
    /// Emitted at most once per process, never for a pre-`Init` event.
    HostInitialized,
}
impl WkWebViewEngine {
    /// Runs the same main-owner-aware loop with no initial window (GH-446).
    /// `service` and the custody output retain their current contracts.
    pub fn run_app_until_quit_without_window<F>(
        &mut self,
        commands: Receiver<AppWindowCommand>,
        events: Sender<AppWindowEvent>,
        service: &mut F,
        retains_main_custody: &mut bool,
    ) -> Result<(), WvError>
    where
        F: FnMut(
            MainThreadMarker,
            &mut dyn FnMut() -> Option<Retained<NSWindow>>,
            bool,
        ) -> Result<(bool, Option<Instant>), WvError>;
}
```

`run_app_until_quit_with_main_owner` and the new method share one private loop body,
parameterized by the startup milestone: initial navigation with its deadline, or
`Init`. Existing schema-1 wrappers keep their behavior. The new path does not replace
the presentation service with a no-op, move a `!Send` owner or bypass its custody
output. Its initial anchor is absent, not a fabricated native view. Under schema 2,
`run_app` skips the renderer read, the renderer dispatch thread and
`create_app_with_renderer_bridge`. Bridges for app-created windows are F02's.

- **Capabilities and manifest:** none. `keld.permissions.jsonc` and its digest rule are unchanged.
- **Wire:** no change to Ready (D-8). The #449 adoption and subscription-order
  amendments in GH-531 have their own explicit wire review gate.
- **Platforms:** macOS implements. Linux and Windows refuse with 034 (D-9).
- **Runtime seam:** the execution owner is unchanged: keld-core owns startup, keld-wv
  owns the UI loop, and keld-host owns process exit. There are no new OS grants. The
  crash domain is unchanged; only the recovery-arm moment moves, and only for
  schema 2 (D-3).
  - **Callback lifetime:** the backend emits `HostInitialized` from inside the loop
    closure's `Init` arm, through the existing `events` sender. The terminal function
    is a `fn` pointer with no captured state.
  - **Configuration** is captured once at `ResolveBoot`.
- **Migration unit:**
  - **Callers:** `run_app` (macOS), `parse_boot_bytes`, and `keld-host` `main` (the `run_guarded` terminal argument).
  - **Handlers:** `coordinate_window_events`.
  - **Config resolvers:** keld-cli `boot.rs`, `doctor.rs` and `dev.rs` consume the one
    resolver.
  - **Persisted state:** none.
  - **Temporary adapter:** none. The GH-531 legacy teardown arm is the permanent
    schema-1 close path (D-2).
  - **Compat facade:** #446 changes only host timing and adds its tests. #449 composes
    the public readiness barrier in AC-18 over the existing link/mirror; it does not
    replace the shared native API Ready policy. Entry qualification is AC-17, not an
    assumed consequence of an earlier host event.

### Pinned evidence and Electron v44.4.5 comparison

#### Historical source baseline at original approval base `3503a8aa`

The approved technical content was based on [Keld main at
`3503a8aa`](https://github.com/gyldlab/keld/tree/3503a8aa383427dcf50ccbf6653c4298e7fb8aad).
The table records that historical source state only. It does not describe the
current Pin of this split, `ec29b7ce`.

| Evidence | Observed behavior at the historical base / implication |
|---|---|
| `app_session/boot_contract.rs`, `parse_boot_bytes` | Only schema 1 is accepted; `renderer` is required. Schema 2 is an actual missing capability, not a compiler flag that already exists |
| `app_session.rs`, macOS `run_app` and `coordinate_window_events`; `wkwebview/mod.rs`, navigation callback | The host authenticates the role, installs services, then unconditionally creates the initial renderer window. NavigationReady triggers initial Ready. HELLO alone does not emit it |
| `app_session.rs`, `signal_ready` and successor installation | Recovery is armed only after the successful initial write; successor install retires old authority/awaits filesystem quiescence, replays Ready, then starts the new reader. A second call to signal_ready is not automatically ignored |
| `packages/@keld/api/src/app.ts`, `link.ts`; `packages/@keld/kipc/src/transport.ts` | Lazy lifecycle link, raw host-ready state and later-task transport delivery; coalesced-delivery and entry-order assertions still need independent runtime tests |
| `packages/@keld/electron/src/app.ts` and `index.ts` | The facade currently observes the native API Ready gate; BrowserWindow is not exported. An earlier host event alone does not implement the constructor |
| macOS `NativeWindowObserver` and `native_window_probe.swift` | Existing positive observation filters by title/layer. A zero-length result does not prove process-wide absence |
| `app_session.rs`, `HostRunFailure`; `keld-host/src/main.rs` | That base retained unresolved native custody, reported by borrow and conditionally skipped dev-stage removal. T-1 must preserve this contract |

#### Current source reconciliation at Pin ec29b7ce

The following source owners already exist at the index Pin. These are source-presence
facts, not test receipts or native acceptance. T-1 must reuse them and fix only a
gap that a current-Pin test proves.

| Evidence at the current Pin | Existing source behavior and T-1 boundary |
|---|---|
| `crates/keld-core/src/app_session/boot_contract.rs:37-139@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The closed schema-2 document parses to `InitialWindow::None`; schema 1 retains its renderer and optional auth path. Do not add another parser. |
| `crates/keld-cli/src/boot.rs:380-410@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The CLI stages schema 2 without a renderer on macOS and Windows. Linux rejects the declaration before it creates a bootable stage. |
| `crates/keld-core/src/app_session.rs:1345-1383@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` and `crates/keld-core/src/app_session.rs:1473-1507@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | Linux and Windows product hosts refuse schema 2 with `KELD-CORE-034`. Windows has a debug-only test admission; it is compiled out of product builds. |
| `crates/keld-core/src/app_session.rs:2389-2518@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The macOS path selects `HostInitialized`; its coordinator owns the first-Init deadline and terminal call. |
| `crates/keld-core/src/app_session.rs:4670-4881@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The coordinator checks the selected trigger and deadline, then invokes the borrowed terminal on expiry. |
| `crates/keld-wv/src/wkwebview/mod.rs:411-445@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The UI loop emits the genuine Init event. |
| `crates/keld-host/src/main.rs:146-170@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The no-flag binary supplies the borrowed, diverging pre-Init terminal callback. T-1 must preserve it and the retained main-thread custody contract. |
| `crates/keld-host/tests/no_flag/macos/facade_ready.rs:19-134@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` and `crates/keld-host/tests/no_flag/windows/facade_ready.rs:54-122@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | Native macOS Ready/zero-window and Windows product-refusal tests exist. Their presence does not prove they ran on this review head or replace real-device receipts. |
| `packages/@keld/api/src/link.ts:172-184@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` and `packages/@keld/api/src/link.ts:227-251@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The private window source publishes public Ready only after raw Host Ready and accepted Subscribe/replay. #449 owns its constructor and native acceptance gates. |
| `packages/@keld/electron/src/index.ts:13-40@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` and `packages/@keld/electron/src/browser-window.ts:254-410@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The facade exports `BrowserWindow` and its Host-backed implementation. #449 still owns the acceptance of subscription, replay, identity, adoption and first-window behavior. |
| `crates/keld-host/tests/no_flag/macos/facade_ready.rs:19-37@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` | The process-wide native-window census is distinct from the older title-filtered positive observer. A missing capture permission is not interpreted as zero windows. |

The source paths above do not prove that tests ran. The review of this specification
does not claim native acceptance.

The requested oracle is [Electron v44.4.5, commit
`694f4585`](https://github.com/electron/electron/tree/694f45852a0f1726cd23bfd379854de489cccb65):

**Electron v44.4.5 behavior comparison**

- **Main-code loading before initialization completes:** [`lib/browser/init.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/init.ts), `appCodeLoaded`;
  [`electron_browser_main_parts.cc`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/electron_browser_main_parts.cc), `JoinAppCode`; [`esm-spec.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/spec/esm-spec.ts), awaited initialization cases
  CJS and awaited native ESM are distinct checks. A synchronous top-level fixture cannot qualify awaited ESM
- **isReady state, readiness promise and ready observers:** [`browser.cc`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/browser.cc),
  `DidFinishLaunching`;
  [`promise.h`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/common/gin_helper/promise.h) One
  initialized state; late whenReady works. Do not assert that every ready listener precedes every promise continuation
- **No constructor before initialization:** [`electron_api_browser_window.cc`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_browser_window.cc), constructor guard Host enforcement plus a facade-visible refusal; window initialization is not the prerequisite for
  application Ready
- **Supervised main-role replacement:** No corresponding Electron main-process recovery contract is supplied by these sources Keld recovery is its own contract; keep its corpus cell
  unknown rather than report an Electron PASS

The inspected target is [draw.io desktop `31.7.0`, commit
`2edf9fb9`](https://github.com/jgraph/drawio-desktop/tree/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e).
Its [`package.json`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/package.json)
declares native ESM and `src/main/electron.js`. The
[`entry`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js)
registers `whenReady`, then its normal startup path calls the window-creation
function, which constructs BrowserWindow and navigates it. Its local window registry
is initialized afresh in a new main process. These are the target-derived reasons for
testing imported-module evaluation, public readiness, native first creation and
host-backed survivor adoption separately.

The target's [`package-lock.json`](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/package-lock.json)
resolves Electron **44.5.1**, while package.json requests `^44.2.0`. **44.4.5 is the
explicit comparison oracle for this task, not the target lockfile's runtime.** A
shipping compatibility receipt must name which executable actually ran. The admitted
target dependency graph and its evaluation timing remain AC-17's qualification work;
this source inventory alone is not a draw.io launch PASS.

The bounded graph inspection found no top-level await in the entry and its nine
relative main modules; the full transitive closure and integrity-pinned package bytes
remain unqualified. AC-17's imported dependency that calls `whenReady()` early is an
adversarial case, not a claim that an inspected draw.io dependency makes that call.
One concrete target requirement is pre-Ready enumeration: the entry calls
`contextMenu(...)` before its Ready registration, and the locked
[`electron-context-menu` 5.1.0 entry](https://github.com/sindresorhus/electron-context-menu/blob/v5.1.0/index.js)
calls `BrowserWindow.getAllWindows()` and registers `browser-window-created`.
Therefore #449 must keep static mirror enumeration usable before public Ready,
without a Host Create call or synchronous wait for subscription. It reflects facts
already applied to the mirror; only the readiness barrier promises complete replay.
The constructor guard and host pre-Ready call refusal remain unchanged. This
publisher-tag source check is not a full npm-tarball byte or target-runtime receipt.
