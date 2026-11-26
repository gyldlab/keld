# Spec: macOS synchronous Electron message box over the existing WorkerLink (GH-564)

Status: draft
Tracker: GH-564
Owner: gyldlab/keld
Updated: 2026-11-17
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

## 1. Goal & non-goals

Make the pinned draw.io application's **unmodified**
`dialog.showMessageBoxSync([window, ]options)` usable under KELD's explicit
macOS legacy compatibility profile. It must return the **actual integer
button index synchronously** to a Bun main-process `ipcMain` listener while
the **one existing GH527 WorkerLink** continues draining host frames. The
Host presents the real guarded dialog through **GH478's native broker** and
retains native cancellation/cleanup independent of Bun lifecycle. This
closes only the synchronous message-dialog compatibility atom; it does not
by itself make the whole draw.io app or two-phase close shippable.

Non-goals:
- Inventing a second Worker, socket, IPC link, ring, credit protocol,
  Node synchronous request trick or nested JS event pump. Never modify the
  GH527 transport just because a dialog consumer needs to park JS.
- Implementing the native dialog broker, permission grammar or host window
  registry again. GH478, #449/#450 and GH531 are their sole owners.
- Renderer-side `ipcRenderer.sendSync`, `showOpenDialogSync`,
  `showSaveDialogSync`, `showErrorBox`, `showCertificateTrustDialog`, a
  universal Electron module or an app-specific draw.io IPC workaround.
- Returning an invented Cancel or clicked default on link loss, role death,
  denial or expiry. Such a guess can discard unsaved user data.
- Changing Host 20 s / Guardian 10 s / cleanup 5 s / kipc 2 s and other
  existing service deadlines or public `app.quit()` Promise divergence.
- Windows/Linux real native support or permission-sandbox containment
  claims; only macOS first-proof is accepted here.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [contract.md](contract.md) | Sections 2 and 3. | Reviewing dependencies and synchronous acceptance. |
| [design.md](design.md) | Section 4. | Reviewing WorkerLink, Host, native UI and lifecycle design. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Planning tasks, tests, performance and open blockers. |

Migration map: legacy S1–S13 are AC-1–AC-13; T1–T7 are T-1–T-7; Q1–Q6 are Q-1–Q-6. Their meanings are unchanged.

## Current blockers

- B-1
- B-2
- B-3
- B-4
- B-5
- B-6
- B-7

## 8. Review gates triggered

- **Public API:** yes — sync overloads, numeric result, typed errors and
  named/default Electron exports.
- **Permission model:** no new grant in this consumer; must independently
  verify that GH478 exact guard admission is reused, not bypassed.
- **Wire protocol:** GH478 owns the new channel/typed message schema;
  this consumer reuses it unchanged, but its exact consumer decode and
  generated constant need wire/receiver-contract review.
- **Unsafe:** none in the consumer; a GH478 native unsafe change needs its
  separately approved explicit allowlist and review, not inferred here.
- **Dependency:** no new package or library required by this spec.
- **Platform:** real macOS native + Bun/WorkerLink and Hosted CI, without
  converting Electron upstream oracle evidence into product proof.
