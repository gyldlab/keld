# Spec: macOS display and appearance facts on the existing window mirror (GH-481)

Status: draft
Tracker: GH-481
Owner: gyldlab/keld
Updated: 2026-11-17
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

## 1. Goal & non-goals

Publish one authoritative, generation-bound **macOS display and appearance snapshot**
through the *existing* GH-531 window-mirror subscription, so the Electron facade's
`screen.getAllDisplays()`, `screen.getPrimaryDisplay()`, `Display.workArea` and
`nativeTheme.shouldUseDarkColors` are **synchronous local reads**. The first
snapshot must be applied before the facade reports public Ready or constructs its
first BrowserWindow. A role not admitted to the window subscription receives no
such fact. Subsequent system changes must update the mirror through the existing
ordered host EVENT path, not by polling on every getter. The target is the pinned
Electron **44.4.5** first-proof draw.io application, not an invented new KELD
platform-wide API guarantee.

Non-goals:
- A second host app link, channel, Worker, router, mirror primitive or Bun-to-Host
  request per getter. No new `app.system` permission literal or native allowlist.
- Redefining GH-531 `WindowRef`, close state, role ownership, restart and
  `Subscribe`/`Subscribed` handling, or altering the already-approved GH-527
  WorkerLink ring and parked-call ordering.
- All remaining `screen` APIs, display-change listeners and `nativeTheme.updated`
  events, `themeSource` setter, OS accessibility appearance, or a universal
  menu/window-style change. GH-558 separately owns those later members.
- Windows/Linux native backends, native display capture permission, renderer-origin
  requests, arbitrary application-Worker subscriptions or system preference writes.
- An Electron percentage, an installed draw.io compatibility pass, or a purported
  native OS test from a TS import/mocked display. Source-only parity is not OS proof.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [refs.md](refs.md) | Section 2. | Checking upstream contracts and current-source facts. |
| [contract.md](contract.md) | Section 3. | Reviewing the 12 binary acceptance criteria. |
| [design.md](design.md) | Section 4. | Reviewing Host, native, wire and mirror design. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Planning ownership, tests and unresolved decisions. |

**Migration map:** Legacy D1–D12 map one-to-one to AC-1–AC-12. Legacy T1–T7 map to T-1–T-7. Open questions Q1–Q5 map to Q-1–Q-5. The acceptance meaning is unchanged.

## Current blockers

- B-1
- B-2
- B-3
- B-4
- B-5

## 8. Review gates triggered

- **Public API:** yes, new `screen` getters and `nativeTheme` read-only
  property; exact return type/shape and pre-Ready behavior.
- **Wire protocol:** yes, an **append-only** GH531 WindowEvent discriminant,
  new typed snapshot and strict codec. Frame protocol version and existing
  discriminants stay unchanged; golden-vector and hostile-frame reviews.
- **Permission model:** yes, explicit amendment to the existing
  host-internal window mirror admission guarantee; no new `app.system`
  grant, and nonprimary denies all facts.
- **Unsafe:** none authorized by this spec. Any native ObjC/FFI operation
  requiring new production unsafe must be separately permitted by the
  exact nearest crate AGENTS.md and independently reviewed first.
- **Dependency:** no new package is proposed; reuse pinned OS-native
  APIs. Any new feature/dependency discovered requires that owner’s
  exact change/review gate.
- **Real OS:** mandatory physical macOS comparisons for both appearance
  modes, primary/multi-display geometry, AppKit origin/scale, native
  observer teardown and paired Electron version evidence. A stub is not
  OS acceptance.
