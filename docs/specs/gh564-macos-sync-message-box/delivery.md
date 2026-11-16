# Spec: GH-564 delivery and evidence
Part of: [index](index.md)
Owns: sections 5–7, 9–10 and open blockers

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

## 6. Tasks: one concern per held PR; ordered dependencies


### T-1: obtain independent bounded consumer approval

- **Blocked by:** B-1

- [ ] obtain §1.1's independent exact-content review and owner receipt
      for the bounded GH564 consumer; consume the existing approved GH478
      v2 contract without re-approving it. Full-target approval remains open.


### T-2: retain canonical GH477 expected-fail cell

- **Blocked by:** T-1

- [ ] consume the current GH477 mapped expected-fail sync cell;
      keep it fail before qualified implementation, no duplicate
      parser/denominator and no 44.3.0 lifecycle re-pin.


### T-3: add connected-link synchronous facade

- **Blocked by:** T-1, T-2

- [ ] add a private **connected-link** sync access on the existing
      singleton/WorkerLink, and tests that distinguish absent/connecting/
      live/terminal states without a second HELLO.


### T-4: reuse guarded NSAlert and native cleanup

- **Blocked by:** T-3

- [ ] consume GH478's guarded real NSAlert operation, actual parent and
      clicked-index mapping, bounded native cleanup on role loss,
      local `callBlocking` expiry and late Host admission, plus both the
      distinct Cancel ACK and original-correlation terminal obligations.


### T-5: compose parked delivery and state updates

- **Blocked by:** T-3, T-4

- [ ] compose the sync UI result and GH527 parked event/mirror semantics
      with >8KiB incoming events; test native default-only completion and
      actual Host Quit/role loss separately.


### T-6: qualify unmodified native draw.io close

- **Blocked by:** T-4, T-5

- [ ] flip GH477's single implementing-ticket cell to pass **only in
      this implementing diff** and prove real unmodified draw.io
      Close→Cancel/Save/Discard acceptance on a Mac, with independently
      reopened saved content and a clearly labelled unsupported error-
      recovery repeat-close scenario.


### T-7: run exact-head gates and independent acceptance

- **Blocked by:** T-1, T-2, T-3, T-4, T-5, T-6

- [ ] run actual final full local/hosted required CI, CodeRabbit or
      independent nonauthor exact-head review, permission/public API/wire
      review and OS acceptance before any merge or compatibility score.

The broader draft target is not an implementation authorization. Only
§1.1's specifically reviewed and recorded bounded slice may proceed after
its owner receipt. No receipt is claimed by this pending amendment.
Do not mark this issue done when T3/T4 or a source fixture passes.


## 7. Test plan & evidence: failing-first checks

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

| AC | Positive observable | Independent negative control |
|---|---|---|
| S1 | Submit the exact draw.io `type:'question'`, `title:'Confirm'`, `noLink:true` object, observe actual macOS 0/1/2 clicks; sync primitive and no Promise | Reject inert `noLink`, drop `title` or substitute Promise/defaultId: real source admission or non-default click fails |
| S2 | visible parent sheet and hidden/absent fallback | Stale WindowRef attaches to recycled parent and fails |
| S3 | one cached authenticated realm link; pre-Ready throws synchronously | Open second Worker/HELLO or await Promise; one-link/return-shape test fails |
| S4 | main JS suppressed during real modal; async positive control interleaves | Execute a timer/IPC listener while parked and parity trace fails |
| S5 | real >8KiB queued facts, pre-reply state applied before return, listeners later | Dispatch listener during wake or drop last EVENT; exact ordering fails |
| S6 | exact guard denial, native panel creation census zero | Remove guard before UI and denial/no-panel fails |
| S7 | Kill authenticated role while real alert outstanding, panel reaped and next call works; old generation Cancel cannot close successor's panel | Drop native cancellation; typed wake still occurs but native census fails |
| S8 | See detailed positive S8 oracle below. | See detailed negative S8 oracle below. |
| S9 | real clicked Cancel=1 while wrong terminal outcome is error | Treat kernel/role failure as ESC press; distinguishable witness fails |
| S10 | Unmodified draw.io dirty text survives Cancel/failed Save; correlate app Save-success ACK to destruction, then independently reopen the saved file and compare modified bytes | Treat the app ACK as disk proof, omit immediate preventDefault, cross-bind stale Save or skip the independent file read: fail |
| S11 | GH566 corpus flips only on real Mac pass, frozen lifecycle unchanged | Remove mapped test/keep implementing ticket after pass; validator fails |
| S12 | exact existing Host/Guardian/link limits, no global buffer growth | Inflate existing deadlines/add second link; contract and resource diff fail |
| S13 | Verify current Ready/grant/parent **before** triggering dirty-close, then force an after-flag dialog error: dirty window/bytes survive, repeat Close's known blocked state is recorded as unsupported | Pretend the failed dialog clicked Cancel or score a later close as working without the source-exact recovery adapter: negative fails |

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

### S8 full transport and native negative controls

**Positive observable (complete S8 source oracle).**

Transport/Core: on one healthy link repeat **256+1 = 257** sequential timed-out `CALL(c_orig)` plus
nonce-bound fresh `Cancel(c_cancel)`; observe separate ACK/`REPLY(c_cancel)` and exactly one typed
`ERR(c_orig)` (or prior genuine `REPLY(c_orig)`), abandoned count returns to zero and next CALL
succeeds. Native Mac: delay Host admission past local timeout, physically dismiss/suppress its panel
within the approved bound, enforce nonce/role fencing, and keep link-loss/terminal Host cleanup
bounded

**Independent negative control (complete S8 source oracle).**

ACK `c_cancel` but omit `ERR(c_orig)`: the 257th abandoned original triggers `KELD-IPC-027`, even if
every Cancel ACKs and every panel closes. Double-terminal the original or treat ACK as its disposal:
exact correlation/once-only assertion fails. Independently drop native Cancel relay, restrict UI
dispatch to default-mode only or synthesize clicked Cancel: visible panel/handle or wrong result
fails


## 9. Perf impact

A synchronous user prompt legitimately parks Bun's main JS, but the host
and transport Worker remain responsive. No RTT, speedup or RSS benefit is
claimed. Measure retained ring records/bytes, native panel lifetime,
cleanup tail and Host Quit latency with the real Mac workload. Do not
increase existing deadlines or activate extra GH527 T4 credit machinery
without the owner’s measured evidence. The transport still uses one
bounded queue and record order, no second buffering layer.

## 10. Open questions, blockers & unknowns: full-target approval gates

The full target remains draft. For §1.1, the selected design reuses the
approved GH478 v2 clocks, Guard, Message/Cancel schema, registered native
errors and source options. These are existing owner decisions, not new
questions for repeat approval. Independent review of this canonical
bounded amendment and the recorded owner receipt are still pending.
The historical questions below remain full-target acceptance work where
not answered by that owning contract; they do not authorize new Native,
protocol or application-recovery work.

1. **Shared dialog interaction and cancellation bounds:** reuse GH478's
   approved **600,000 ms** distinct human UI budget, **10 s live-link
   local-timeout native close** and **5 s Host-observed role/link-loss
   native close**. Validate the existing same-channel
   `Cancel { nonce }`, distinct Cancel and original CALL terminal frames,
   257 sequential abandoned-correlation release control, bounded
   tombstone and late-admission ordering on real macOS with GH478/GH527/Core
   owners. Local Worker `abandon` is
   insufficient; never replace or extend Host20/Guardian/KIPC clocks.
2. **GH478 platform-native completion:** the bounded consumer uses the
   current supplied-parent sheet. Parentless and synchronous hidden-parent
   fallback remain open in their native owner, with measured UI-loop cleanup
   and any required independent unsafe review. No visibility inference or
   unqualified CFRunLoop scheduling claim fills that gap.
3. **Pre-Ready synchronous API behavior:** confirm the proposed *synchronous
   typed not-ready refusal* before the existing singleton is connected,
   rather than allowing a call to invent a second connection or return a
   Promise. The first-proof normal draw.io caller runs after Ready.
4. **Native error-code allocation:** consume GH478's existing
   `KELD-NATIVE-009` through `KELD-NATIVE-016` outcomes. They are registered
   in `docs/engineering/keld-error-codes.md` at the owning PR1306 source
   `16e519091992d551853b6d0434b85000499a3d3c`. The inspected integrating
   tree has those native source outcomes but lacks these eight registry
   sections. Its designated root owner must reconcile the existing rows
   before claiming code/spec consistency; this consumer allocates no code
   and this amendment does not edit that separately owned registry.
5. **Effect-unknown application recovery:** the **source-backed first
   proof limitation is explicit**: pinned draw.io's app-owned
   `modifiedModalOpen` can remain true after a thrown dialog failure,
   and repeatable Close is **unsupported** then. Preflight avoids
   deterministic fixture misconfiguration; it cannot repair a later
   runtime error. The normal 0/1/2 path is required. The GH564/compat
   owner must open and approve a separate, source-exact app-recovery
   ticket before claiming repeatable Close after such errors; it may
   not synthesize Cancel, clear app-private state from Host or silently
   count this as a pass.
6. **Pinned message-argument compatibility:** reuse GH478's approved
   admission of `type:'question'`, `title:'Confirm'` and macOS-inert
   `noLink:true` alongside the exact draw.io buttons and indexes.
   Run unchanged caller and invalid-type/unknown-field controls; until
   both specs agree and a native Mac panel reproduces the result,
   keep the GH477 synchronous cell expected-fail.

A bounded receipt under §1.1 does **not** approve the full GH564 target,
complete GH450 native close, qualify Windows/Linux, flip a corpus cell,
resolve formal review or authorize a merge. It permits only the three-file
consumer after independent exact-content approval, the existing approved
dependency contract and winning source/OS assignments. Native acceptance,
generated-doc gates and broader issue completion remain separately required.

### B-1: independent bounded consumer approval

- **Status:** open
- **Owner:** GH-564 contract reviewer and delegated repository owner
- **Impact:** No production consumer implementation is authorized before the exact-content approval receipt.
- **Check:** Link the independent review and owner receipt to the canonical GH-564 revision.

### B-2: hidden-parent and parentless native parity

- **Status:** open
- **Owner:** GH478 native dialog and GH564 compatibility owners
- **Impact:** Full S2 hidden-parent and parentless synchronous parity remains unqualified.
- **Check:** Real macOS observations prove native presentation and correct callback/cleanup.

### B-3: failed-dialog repeat-close application recovery

- **Status:** open
- **Owner:** GH564 draw.io compatibility/recovery owner
- **Impact:** A thrown native error may strand modifiedModalOpen; later close is not accepted.
- **Check:** An independently approved source-exact adapter or upstream fix passes an after-flag failure test.

### B-4: native error registry reconciliation

- **Status:** open
- **Owner:** Existing GH478/GH1404 native-error registry owner
- **Impact:** KELD-NATIVE-009 through -016 integration is not independently complete.
- **Check:** Canonical error registry rows match the approved native owner and exact current source.
