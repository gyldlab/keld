# Spec: macOS synchronous Electron message box over the existing WorkerLink (GH-564)

- Status: draft
- Tracker: GH-564
- Owner: gyldlab/keld
- Updated: 2026-11-07
- Pin: 151776c72f3615f6c87d8a03ccbab00a0c08e955

Bounded parented consumer decision: pending exact-content independent review
and owner receipt; no production approval is asserted by this amendment.

Original tracker context: GH-564 under #517; reference KEL-80.

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

### 1.1 Bounded parented consumer decision

This amendment is based on the canonical GH564 spec in PR #1163 at
`0853635db8bd9348ee716e9f3b46d548d8c3b46d`. Its spec blob is identical
to `3444c2da1a53b5c63710eca2253583f879ffa26c`:
`01570c2eb3ca1c41f8b5f51ae1a6c40b7334694e`. The full target remains
draft and the formal `CHANGES_REQUESTED` review remains unresolved.

The proposed bounded implementation is only a synchronous consumer of
GH478's existing parented Message operation. Its product acceptance uses
an actual visible, genuine, current Host-owned parent after the existing
authenticated Ready barrier. It does not patch original draw.io main or
preload source. It adds no native owner, Worker, link, channel, protocol
field, global state, grant or deadline.

A supplied parent retains GH478's existing sheet branch. A supplied hidden
parent therefore also reaches that branch; its Electron synchronous
visibility/fallback parity is **unqualified**. This slice does not claim
a typed hidden-parent refusal or implement S2's independent-alert fallback.
The current authenticated window snapshot has no visibility fact. Focus,
caller properties, declared options and guessed visibility must not
substitute for native visibility. Parentless requests retain the existing
Host-owned `KELD-NATIVE-010` refusal without retargeting another window.

Genuine denial, expiry, native failure, role loss and link loss remain
typed errors, never clicked Cancel. S13's original application
`modifiedModalOpen`/repeat-Close limitation remains explicit. Full GH564
compatibility, S2 completion, issue closure, review dismissal and merge
do not follow from this bounded decision.

Implementation of this slice requires an exact-content independent review
and an owner-delegated receipt on GH564 and the integrating task. That
receipt must identify this canonical revision and the three production
files in §5. Until it exists, only the assigned spec and failure-first
tests may be written. After it exists, its authority covers this bounded
slice only; all broader draft criteria and unqualified native acceptance
remain open. It does not dismiss a formal review.

## Parts

| Part | Owns | Load when |
|---|---|---|
| [refs.md](refs.md) | Section 2: source authority and prerequisites | Verify pinned dependencies |
| [acceptance.md](acceptance.md) | Section 3: S1–S13 and AC-1–AC-13 | Review real negative-control tests |
| [design.md](design.md) | Section 4: singleton, native custody and deadlines | Change consumer or IPC design |
| [delivery.md](delivery.md) | Sections 5–7 and 9–10: task and native evidence | Implement or validate delivery |

## Current blockers

B-1
B-2
B-3
B-4

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
