# Spec: macOS synchronous message box: Implementation and tests

Part of: [index](index.md)
Owns: Code placement, ordered tasks and original caller controls

## 5. Boundaries

Before §1.1's independent exact-content review and owner receipt, this
amendment permits only assigned documentation and failure-first tests.
After that receipt, the bounded production implementation belongs only in:

- `packages/@keld/api/src/app.ts`: synchronous access to the existing
  Ready singleton and its sticky terminal state.
- `packages/@keld/api/src/link.ts`: blocking Message on the existing
  WorkerLink, shared nonce/codec/deadline/result/cancellation policy.
- `packages/@keld/electron/src/dialog.ts`: existing brand/options
  admission and actual synchronous numeric result mapping.

`src/index.ts` already shares the dialog object. GH478's existing
Native/Core/WV owners and generated dialog codec are consumed unchanged;
this consumer does not implement the missing parentless/hidden fallback.
The mapped GH477 test/corpus may change only after its declared real
native proof; source-only tests cannot qualify a broader cell.

MUST NOT modify `packages/@keld/kipc/src/transport.ts`, GH527/GH531
specs, Host/Core/WV/Native production, Guard policy, channel/schema/error
allocation, generated codec, Windows/Linux production, Guardian timeouts,
CI policy or root dependencies. Preserve sibling async behavior while
sharing its existing private policy. Any further owner change needs its
own assignment and review; this bounded receipt cannot authorize it.

## 6. Tasks

- [ ] T1: obtain §1.1's independent exact-content review and owner receipt
      for the bounded GH564 consumer; consume the existing approved GH478
      v2 contract without re-approving it. Full-target approval remains open.
- [ ] T2: consume the current GH477 mapped expected-fail sync cell;
      keep it fail before qualified implementation, no duplicate
      parser/denominator and no 44.3.0 lifecycle re-pin.
- [ ] T3: add a private **connected-link** sync access on the existing
      singleton/WorkerLink, and tests that distinguish absent/connecting/
      live/terminal states without a second HELLO.
- [ ] T4: consume GH478's guarded real NSAlert operation, actual parent and
      clicked-index mapping, bounded native cleanup on role loss,
      local `callBlocking` expiry and late Host admission, plus both the
      distinct Cancel ACK and original-correlation terminal obligations.
- [ ] T5: compose the sync UI result and GH527 parked event/mirror semantics
      with >8KiB incoming events; test native default-only completion and
      actual Host Quit/role loss separately.
- [ ] T6: flip GH477's single implementing-ticket cell to pass **only in
      this implementing diff** and prove real unmodified draw.io
      Close→Cancel/Save/Discard acceptance on a Mac, with independently
      reopened saved content and a clearly labelled unsupported error-
      recovery repeat-close scenario.
- [ ] T7: run actual final full local/hosted required CI, CodeRabbit or
      independent nonauthor exact-head review, permission/public API/wire
      review and OS acceptance before any merge or compatibility score.

The broader draft target is not an implementation authorization. Only
§1.1's specifically reviewed and recorded bounded slice may proceed after
its owner receipt. No receipt is claimed by this pending amendment.
Do not mark this issue done when T3/T4 or a source fixture passes.

### T-1: Verify one synchronous dialog through the existing guarded WorkerLink

- **Blocked by:** none

Preserve the original main caller, native broker and bounded reply ownership. Run AC-1 and the independent failure controls.

## 7. Test plan & evidence

The bounded source regressions are
`packages/@keld/electron/src/dialog-sync-consumer.test.ts` and
`packages/@keld/api/src/dialog-sync-consumer.test-role.ts`. Their eight
isolated child cases cover unconnected and connected-before-Ready
refusal, exact source options and 0/1/2 primitive mapping, brand-first
admission, parentless refusal, sticky terminal state despite Ready,
Quit admission, original typed-error preservation and wrong-reply refusal.
They control only the LifecycleLink Host seam and establish no native
click, Guard, visibility, WorkerLink park or physical-cleanup acceptance.
They were prepared before production and are **unrun** in this amendment.

Keep the existing unsupported-only test unchanged until stronger real
native positive and negative proof permits its explicit bounded update.
A 257-cycle successful-click test does not satisfy S8's 257 local expiries.
Root owns actual Bun/Cargo/native execution and must retain the observed
RED, corrected GREEN and independent mutation failures. S2 and S13 remain
unqualified as stated; no source-only result flips full compatibility.

- **AC:** S1
  - **Positive observable:** Submit the exact draw.io `type:'question'`, `title:'Confirm'`, `noLink:true` object, observe actual macOS 0/1/2 clicks; sync primitive and no Promise
  - **Independent negative control:** Reject inert `noLink`, drop `title` or substitute Promise/defaultId: real source admission or non-default click fails
- **AC:** S2
  - **Positive observable:** visible parent sheet and hidden/absent fallback
  - **Independent negative control:** Stale WindowRef attaches to recycled parent and fails
- **AC:** S3
  - **Positive observable:** one cached authenticated realm link; pre-Ready throws synchronously
  - **Independent negative control:** Open second Worker/HELLO or await Promise; one-link/return-shape test fails
- **AC:** S4
  - **Positive observable:** main JS suppressed during real modal; async positive control interleaves
  - **Independent negative control:** Execute a timer/IPC listener while parked and parity trace fails
- **AC:** S5
  - **Positive observable:** real >8KiB queued facts, pre-reply state applied before return, listeners later
  - **Independent negative control:** Dispatch listener during wake or drop last EVENT; exact ordering fails
- **AC:** S6
  - **Positive observable:** exact guard denial, native panel creation census zero
  - **Independent negative control:** Remove guard before UI and denial/no-panel fails
- **AC:** S7
  - **Positive observable:** Kill authenticated role while real alert outstanding, panel reaped and next call works; old generation Cancel cannot close successor's panel
  - **Independent negative control:** Drop native cancellation; typed wake still occurs but native census fails
- **AC:** S8
  - **Positive observable:** Transport/Core: on one healthy link repeat **256+1 = 257** sequential timed-out `CALL(c_orig)` plus nonce-bound fresh `Cancel(c_cancel)`; observe separate ACK/`REPLY(c_cancel)` and exactly one typed `ERR(c_orig)` (or prior genuine `REPLY(c_orig)`), abandoned count returns to zero and next CALL succeeds. Native Mac:
    delay Host admission past local timeout, physically dismiss/suppress its panel within the approved bound, enforce nonce/role fencing, and keep link-loss/terminal Host cleanup bounded
  - **Independent negative control:** ACK `c_cancel` but omit `ERR(c_orig)`: the 257th abandoned original triggers `KELD-IPC-027`, even if every Cancel ACKs and every panel closes. Double-terminal the original or treat ACK as its disposal: exact correlation/once-only assertion fails. Independently drop native Cancel relay, restrict UI dispatch
    to default-mode only or synthesize clicked Cancel: visible panel/handle or wrong result fails
- **AC:** S9
  - **Positive observable:** real clicked Cancel=1 while wrong terminal outcome is error
  - **Independent negative control:** Treat kernel/role failure as ESC press; distinguishable witness fails
- **AC:** S10
  - **Positive observable:** Unmodified draw.io dirty text survives Cancel/failed Save; correlate app Save-success ACK to destruction, then independently reopen the saved file and compare modified bytes
  - **Independent negative control:** Treat the app ACK as disk proof, omit immediate preventDefault, cross-bind stale Save or skip the independent file read: fail
- **AC:** S11
  - **Positive observable:** GH566 corpus flips only on real Mac pass, frozen lifecycle unchanged
  - **Independent negative control:** Remove mapped test/keep implementing ticket after pass; validator fails
- **AC:** S12
  - **Positive observable:** exact existing Host/Guardian/link limits, no global buffer growth
  - **Independent negative control:** Inflate existing deadlines/add second link; contract and resource diff fail
- **AC:** S13
  - **Positive observable:** Verify current Ready/grant/parent **before** triggering dirty-close, then force an after-flag dialog error: dirty window/bytes survive, repeat Close's known blocked state is recorded as unsupported
  - **Independent negative control:** Pretend the failed dialog clicked Cancel or score a later close as working without the source-exact recovery adapter: negative fails


Test fixture pins: Electron v44.4.5 and drawio-desktop
`2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`, webapp submodule
`24b76c2cbd55d88e354042e8d329a2e4708972bc`. Three-button
Save/Cancel/Discard each needs a real native click and separate trace.
The two-dirty-window quit sweep is #443/#451's **next-milestone** owner;
first proof uses one dirty window but must not regress its event ordering.

The real Mac harness must observe actual native panel visibility/click,
window identity, renderer dirty bytes, coherent timestamps, Bun/Host role
terminal status and a healthy subsequent operation. Mocked Rust/TypeScript
stubs alone are not acceptance. The external watchdog is only a kill
switch, never the source of Cancel. Negative controls restore production
bytes after failure and are retained as exact evidence. Every test is
reported with selected count and platform.
