# Spec: facade-boot Ready — host initialized and window module available before any app window (F01-T2)
Status: draft
Tracker: GH-446
Owner: @0monish
Updated: 2026-11-16
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

## 1. Goal & non-goals

In an Electron app, readiness is independent of a window, and application code
requests its windows after initialization. `ready` listeners and `whenReady()`
continuations can both create them; this spec does not impose an order between those
callbacks. For the existing schema-1 renderer-declared boot, Keld's host emits
lifecycle `Ready` only after it has created its own window and finished that
window's first navigation:

- `coordinate_window_events` maps `AppWindowEvent::NavigationReady` to
  `PrimaryRouterHandle::signal_ready` (`crates/keld-core/src/app_session.rs`).
- That event fires only for the host-created initial window
  (`crates/keld-wv/src/wkwebview/mod.rs`, `NavigationReady` emission).

So an `@keld/electron` main on a schema-1 boot that calls `new BrowserWindow()`
inside `whenReady` would get a second window next to a host window it never asked
for. This spec decides who creates window #1 and what `Ready` means for each
generation.

The observable outcome is a new boot kind. When the compiled descriptor declares that
the app creates every window, the host on macOS:

1. authenticates the role's `HELLO`;
2. enters its UI loop with no window;
3. the loop emits `HostInitialized` on its `Init` turn; the coordinator receives it and writes `Ready` once afterward.

In the initial generation, the app then sees `ready`/`whenReady` with zero application
windows. At the current Pin the facade composes raw Host Ready with accepted window
subscription and complete replay. #449 owns qualification of that source path. The host refuses non-subscription window calls before its
`Ready` transition. Recovered generations may already have surviving windows (D7).
Renderer-declared boots preserve their existing contract.

Non-goals:

- Qualification and demonstrated repairs of the existing window `Create` call and
  `BrowserWindow` constructor (F02-T2 #449, GH-531).
- Qualification of the existing registry's recovery adoption (GH-531 D-6 and T-1,
  tracked by #449).
- Two-phase quit (F01-T3).
- Launch-time `open-url`/`open-file` delivery (#524, F01-T8).
- Windows and Linux emission points. Those hosts refuse the new boot kind (D9).
- `keld migrate` writing the new config field, and `compat: { electron: true }` (architecture 04 §2 sketch).
- The cold-start delta. It is measured under X05-T1's registered metric and is not
  an acceptance criterion here.


### Approval provenance

The 2026-10-23 owner-delegated decision and technical approval are historical records.
The [approval record](https://github.com/gyldlab/keld/pull/657#issuecomment-6371083658)
binds technical head `a04e1ed889733a64cc33c1b64535e02b4afb0902` and the original
single-file content. This directory layout is a structure repair. It preserves the
approved technical decisions and does not claim native acceptance. The final split
content requires its own exact-head review record before publication.

## Parts

| Part | Owns | Load when |
|---|---|---|
| [contract.md](contract.md) | Governing references and acceptance criteria | You need the contract or an AC |
| [design-boot.md](design-boot.md) | Reuse, schema declaration, host initialization and readiness | You change boot or Ready behavior |
| [design-recovery.md](design-recovery.md) | Failure, recovery, wire and platform rules | You change failure or generation behavior |
| [design-amendments.md](design-amendments.md) | Normative amendments, types, runtime seam and pinned evidence | You implement an approved amendment |
| [implementation.md](implementation.md) | Boundaries and the ordered implementation task | You plan or implement T-1 |
| [verification.md](verification.md) | Test plan and performance impact | You validate the spec or implementation |
| [provenance.md](provenance.md) | Delegated decisions, approval history and remaining evidence | You check approval or outstanding acceptance |

### Historical section map

The original single-file section numbers remain stable. This map is the canonical
destination for historical PR, issue and review references.

- §1 Goal and non-goals: this index, [goal](#1-goal--non-goals)
- §2–§3: [references](contract.md#2-spec-refs) · [ACs](contract.md#3-acceptance-criteria)
- §4.0–§4.5: [reuse](design-boot.md#first-principles-and-reuse-decision) · [D-1](design-boot.md#d-1-how-a-facade-boot-is-declared) · [D-2](design-boot.md#d-2-renderer-declared-boots-keep-createinitialwindow-before-ready)
  · [D-3](design-boot.md#d-3-what-host-initialized-means) · [D-4](design-boot.md#d-4-what-window-module-available-means) · [D-5](design-boot.md#d-5-host-ready-entry-evaluation-and-usable-facade-readiness)
- §4.6–§4.9: [D-6](design-recovery.md#d-6-failure-and-timeout-behaviour) · [D-7](design-recovery.md#d-7-recovered-schema-2-generations) · [D-8](design-recovery.md#d-8-wire) · [D-9](design-recovery.md#d-9-platforms)
- §4.10–§4.12: [amendment](design-amendments.md#amendment-text-exact-content-of-the-delegated-decision) · [types](design-amendments.md#types-platform-notes-runtime-seam-and-migration) · [pinned evidence](design-amendments.md#pinned-evidence-and-electron-v4445-comparison)
- §5–§6: [boundaries](implementation.md#5-boundaries) · [tasks](implementation.md#6-tasks)
- §7 and §9: [test plan](verification.md#7-test-plan--evidence) · [performance](verification.md#9-perf-impact)
- §8 review gates: this index, [review gates](#8-review-gates-triggered)
- §10 delegated decisions and evidence: [provenance](provenance.md#10-open-questions-blockers--unknowns)

## Current blockers

none

## 8. Review gates triggered

**Implementation dependency:** T-1 remains gated on GH-445. Implementation and native acceptance remain separate work.

- **unsafe:** none.
- **public API:** yes.
  - `keld.boot.json` schema 2, which also triggers the manifest-schema gate (KEL-96-D11).
  - The `keld.config.ts` field `initialWindow`.
  - The keld-wv items `AppWindowEvent::HostInitialized` and `WkWebViewEngine::run_app_until_quit_without_window`.
  - The keld-core `run_guarded` terminal parameter.
  - The error-code registry entries.
  - Electron-visible timing of `ready`/`whenReady`/`isReady` in schema-2 boots.
- **permission model:** yes by routing only. KEL-96-D11 sends every `keld.boot.json`
  change here; no capability, policy or guard path changes.
- **dependency:** none.
- **wire protocol:** Ready's bytes are unchanged. The approved gh531 amendment's
  independent review covers `Adopted = 8`, subscription admission/045 precedence
  and replay-before-Subscribed ordering; #449 carries the implementation/vector gate.
