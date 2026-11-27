# Spec: macOS guarded native dialog broker (GH-478, F06-T2)

Status: draft
Tracker: GH-478
Owner: gyldlab/keld
Updated: 2026-11-17
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302
## 1. Goal & non-goals

Give one authenticated Bun app role a **host-guarded** way to present a real macOS
file-open, file-save or message dialog. The selected result must return through the
**existing** GH-527 WorkerLink, and the native UI must be owned and retired by the
current host/window generation. The `@keld/electron` async facade provides the
Electron v44.4.5 `showOpenDialog`, `showSaveDialog` and `showMessageBox` result shapes.
Without the exact `app.system` permission literal `dialog`, no panel may appear.
The first proof is macOS, with explicit legacy-profile disclosure and no implicit
filesystem authority minting. A real OS-visible panel and a subsequent healthy call
are part of acceptance; a typed JavaScript error by itself is not cleanup evidence.

Non-goals:
- Implementing `showMessageBoxSync`, `showOpenDialogSync`, `showSaveDialogSync`,
  `showErrorBox` or certificate-trust dialogs; #564 and F06-T14 own those consumers.
  This broker **does** provide the shared native *message-panel* operation used by #564.
- New kipc transport, socket, Worker, sendSync trick, modal thread, global JS pump,
  queue/replay owner or window-ID minter. GH-527, #528 and GH-531 already own them.
- Adding dialog-selected paths to FsBroker's retained scopes, providing sandbox
  access, or changing Strict Bun containment. F06-D1/F06-T15 own future strict
  user-intent grants. Under explicit legacy, `node:fs` is ambient and the
  containment loss must be disclosed, not relabelled as guarded I/O.
- Implementing Windows/Linux dialog backends, setting compatibility scores or
  closing #477/#564/#449. Non-macOS calls are explicitly unsupported until an
  OS-owned implementation is tested.
- Modifying Host 20 s, guardian/reap, existing kipc I/O or filesystem-service
  clocks. New dialog human-interaction custody is a *separate* finite clock.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [contract.md](contract.md) | Sections 2 and 3. | Reviewing upstream contracts and binary acceptance. |
| [design.md](design.md) | Section 4. | Reviewing ownership, policy, native presentation and cancellation. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Planning tasks, tests, performance and open blockers. |

Migration map: legacy A1–A13 are AC-1–AC-13; T1–T8 are T-1–T-8; Q1–Q6 are Q-1–Q-6. Their meanings are unchanged.

## Current blockers

- B-1
- B-2
- B-3
- B-4
- B-5
- B-6
- B-7
- B-8
## 8. Review gates triggered

- **Permission model:** yes — new exact `app.system` evaluator, guard before
  UI, immutable role policy, deny-before-effects and negative grant controls.
- **Public API:** yes — new async `@keld/api` and Electron facade members,
  options, return shapes and errors.
- **Wire protocol:** yes — one new **version-2** channel and typed postcard
  operation schema, ownership/correlation and hostile-frame tests. No frame or
  existing-channel renumbering.
- **Unsafe:** conditional **blocking** gate if objc2/AppKit requires an
  operation not in the existing `keld-native/AGENTS.md` allowlist; obtain
  exact owner, safety proof and independent review first. The spec PR adds
  **no unsafe**.
- **Dependency addition:** no new dependency in this draft; implementation
  may need additional already-pinned objc2 feature gates. Any actual
  feature/dependency delta needs the normal dependency review.
- **Real OS:** Mac UI-thread, native panel creation, cancellation, generation
  cleanup and host recovery; CI-only Rust/Bun is insufficient.
