# Spec: macOS guarded native dialog broker (GH-478, F06-T2)

- Status: approved
- Tracker: GH-478
- Owner: gyldlab/keld
- Updated: 2026-11-01
- Pin: 6d138429bbcc633c213ae0364380297b3548b760
- Linear: GH-478 (tracker under #517; reference KEL-80)

Approval: owner-delegated acceptance of the exact v2 spec input (SHA-256
`a3137e6d826d93788845b57c4295917b34be54b61f7fcc939903ab3432fc73c2`) and the existing
architecture05 path-only amendment is recorded at
[GH478 comment 6476354568](https://github.com/gyldlab/keld/issues/478#issuecomment-6476354568);
no native/product acceptance is claimed.

Save preferred-suffix correction: bounded owner-delegated decision
[6553879003](https://github.com/gyldlab/keld/issues/478#issuecomment-6553879003); exact
source review and native proof remain separate from that contract decision.

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
  The separately approved GH499 migration contract permits a macOS-only,
  hash-bound dirty-close adaptation to awaited `showMessageBox(...).response`.
  Its exact patch bytes and native reentrancy/Save/Cancel/Discard proof remain
  separate gates. GH564 is required for the unchanged synchronous-source route,
  not automatically for that qualified adapted MVP. No additional app rewrite
  is authorized here.
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
|---|---|---|
| [refs.md](refs.md) | Section 2: authority and exact predecessors | Checking what this spec consumes |
| [acceptance.md](acceptance.md) | Section 3: AC-1 to AC-15 | Writing or reviewing a test |
| [design.md](design.md) | Section 4: ownership, permission, native completion, errors and reuse | Changing the broker, guard, channel, native panel or facade |
| [delivery.md](delivery.md) | Sections 5, 6, 7, 9, 10: boundaries, tasks, evidence, perf and blockers | Planning or implementing a slice |

## Current blockers

- B-1

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
