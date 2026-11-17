# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5–7, 9 and 10

## 5. Boundaries

After both required spec approvals, implementation belongs **only** in:
- `packages/@keld/api/src/app.ts` singleton, package-private direct sync
  access to the *same* connected LifecycleLink; no new connection.
- `packages/@keld/api/src/link.ts` thin guarded-channel wrapper on existing
  WorkerLink; no edit to the transport internals.
- `packages/@keld/electron/src/dialog.ts` and `src/index.ts`: matching
  named/default export and strict Electron result mapping.
- GH478's **future** `crates/keld-native/src/dialog.rs`, to be created
  by the GH478 owner **after draft PR #1161 approval**; this file does
  not exist on the reviewed repo head. GH564 later consumes that
  broker and its approved Core/WV native ownership/permission seam,
  rather than treating an unimplemented file as a ready dependency.
- GH477's `electron-native-v1` mapped cell and test when it is approved/
  landed, with the implementing-ticket flip in the same source change.

MUST NOT modify `packages/@keld/kipc/src/transport.ts`, GH527 or GH531
specs, `crates/keld-core/src/app_session.rs` under GH824/#834 or GH449
owner without coordination, `keld-guard` policy, channel allocation
registry, sibling async file dialogs, native Windows/Linux production,
GuardedPrimary/Guardian timeout logic, CI policy or root dependencies.
Cross-owner integration requires the earlier approved ticket/branch and
fresh winning claim, not a direct patch into another issue's worktree.

## 6. Tasks

### T-1: Approve exact-content GH564 and GH478 message-broker contracts

- **Blocked by:** B-1, B-2, B-3, B-4, B-5, B-6, B-7

Approval covers source-exact message options, the architecture path-only rule,
same-link cancellation and shared interaction/cleanup clocks.

### T-2: Consume the GH477 expected-fail synchronous cell

- **Blocked by:** B-7

The draft #1156 cell must remain expected-fail before implementation; do not add a duplicate parser/denominator or re-pin the 44.3.0 lifecycle.

### T-3: Add private connected-link sync access to the existing singleton/WorkerLink

- **Blocked by:** B-3

Tests must distinguish absent, connecting, live and terminal states without a second HELLO.

### T-4: Consume GH478's guarded NSAlert operation, actual parent and clicked-index mapping

- **Blocked by:** B-1, B-2, B-4, B-6

Bounded native cleanup on role loss, local `callBlocking` expiry and late Host admission,
plus both the distinct Cancel ACK and original-correlation terminal obligations.

### T-5: Compose the sync UI result with GH527 parked event/mirror semantics

- **Blocked by:** B-1, B-2, B-3

Test the composed path with >8KiB incoming events; also test native default-only completion and actual Host Quit/role loss separately.

### T-6: Flip the GH477 implementing-ticket cell only in the implementing diff

- **Blocked by:** B-1, B-2, B-5, B-6, B-7

Prove real unmodified draw.io Close→Cancel/Save/Discard acceptance on a Mac,
with independently reopened saved content and a clearly labelled unsupported
error-recovery repeat-close scenario.

### T-7: Run full local/hosted required CI and independent review

- **Blocked by:** B-1, B-2, B-3, B-4, B-5, B-6, B-7

Complete exact-head nonauthor review, permission/public API/wire review and OS acceptance before any merge or compatibility score.

Implementation may not begin from a draft. Do not mark this issue done
when only T-3 or T-4 has a passing mock.


## 7. Test plan & evidence

| Criterion | Positive observable | Independent negative control |
|---|---|---|
| AC-1 | Submit the exact draw.io `type:'question'`, `title:'Confirm'`, `noLink:true` object, observe actual macOS 0/1/2 clicks; sync primitive and no Promise | Reject inert `noLink`, drop `title` or substitute Promise/defaultId: real source admission or non-default click fails |
| AC-2 | visible parent sheet and hidden/absent fallback | Stale WindowRef attaches to recycled parent and fails |
| AC-3 | one cached authenticated realm link; pre-Ready throws synchronously | Open second Worker/HELLO or await Promise; one-link/return-shape test fails |
| AC-4 | main JS suppressed during real modal; async positive control interleaves | Execute a timer/IPC listener while parked and parity trace fails |
| AC-5 | real >8KiB queued facts, pre-reply state applied before return, listeners later | Dispatch listener during wake or drop last EVENT; exact ordering fails |
| AC-6 | exact guard denial, native panel creation census zero | Remove guard before UI and denial/no-panel fails |
| AC-7 | Kill authenticated role while real alert outstanding, panel reaped and next call works; old generation Cancel cannot close successor's panel | Drop native cancellation; typed wake still occurs but native census fails |
| AC-9 | real clicked Cancel=1 while wrong terminal outcome is error | Treat kernel/role failure as ESC press; distinguishable witness fails |
| AC-10 | Unmodified draw.io dirty text survives Cancel/failed Save; correlate app Save-success ACK to destruction, then independently reopen the saved file and compare modified bytes | Treat the app ACK as disk proof, omit immediate preventDefault, cross-bind stale Save or skip the independent file read: fail |
| AC-11 | GH566 corpus flips only on real Mac pass, frozen lifecycle unchanged | Remove mapped test/keep implementing ticket after pass; validator fails |
| AC-12 | exact existing Host/Guardian/link limits, no global buffer growth | Inflate existing deadlines/add second link; contract and resource diff fail |
| AC-13 | Verify current Ready/grant/parent **before** triggering dirty-close, then force an after-flag dialog error: dirty window/bytes survive, repeat Close's known blocked state is recorded as unsupported | Pretend the failed dialog clicked Cancel or score a later close as working without the source-exact recovery adapter: negative fails |

### Test: Original CALL and Cancel correlation plus native panel cleanup (AC-8 / S8)

#### Transport and Core

- **Positive:** on one healthy link repeat **256+1 = 257** sequential timed-out `CALL(c_orig)` plus nonce-bound fresh `Cancel(c_cancel)`; observe separate ACK/`REPLY(c_cancel)`
and exactly one typed `ERR(c_orig)` (or prior genuine `REPLY(c_orig)`), abandoned count returns to zero and next CALL succeeds
- **Negative control:** ACK `c_cancel` but omit `ERR(c_orig)`: the 257th abandoned original triggers `KELD-IPC-027`, even if every Cancel ACKs and every panel closes. Double-terminal
the original or treat ACK as its disposal: exact correlation/once-only assertion fails

#### Native macOS

- **Positive:** delay Host admission past local timeout, physically dismiss/suppress its panel within the proposed bound, enforce nonce/role fencing, and keep
link-loss/terminal Host cleanup bounded
- **Negative control:** Independently drop native Cancel relay, restrict UI dispatch to default-mode only or synthesize clicked Cancel: visible panel/handle or wrong result fails

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

## 9. Perf impact

A synchronous user prompt legitimately parks Bun's main JS, but the host
and transport Worker remain responsive. No RTT, speedup or RSS benefit is
claimed. Measure retained ring records/bytes, native panel lifetime,
cleanup tail and Host Quit latency with the real Mac workload. Do not
increase existing deadlines or activate extra GH527 T-4 credit machinery
without the owner’s measured evidence. The transport still uses one
bounded queue and record order, no second buffering layer.

## 10. Open questions, blockers & unknowns

### Q-1: Shared dialog interaction and cancellation bounds

Approve or revise GH478's proposed **600,000 ms** distinct human UI budget,
**10 s live-link local-timeout native close** and **5 s Host-observed
role/link-loss native close**. Validate the proposed same-channel
`Cancel { nonce }`, distinct Cancel and original CALL terminal frames,
257 sequential abandoned-correlation release control, bounded tombstone
and late-admission ordering on real macOS with GH478/GH527/Core owners.
Local Worker `abandon` is insufficient; never replace or extend
Host20/Guardian/KIPC clocks.
### Q-2: GH478 platform-native completion

Approve the exact alert/parentless callback scheduling with measured UI-loop
cleanup and any required objc2 unsafe allowance; do not assume all native
CFRunLoop modes execute the same callbacks.
### Q-3: Pre-Ready synchronous API behavior

Confirm the proposed *synchronous typed not-ready refusal* before the existing
singleton is connected, rather than allowing a call to invent a second
connection or return a Promise. The first-proof normal draw.io caller runs
after Ready.
### Q-4: Native error-code allocation

Consume GH478's eventual accepted typed errors and KELD's canonical error
registry; never reserve guessed `KELD-NATIVE-*` numeric codes in this draft.
### Q-5: Effect-unknown application recovery

The **source-backed first-proof limitation is explicit**: pinned draw.io's
app-owned `modifiedModalOpen` can remain true after a thrown dialog failure,
and repeatable Close is **unsupported** then. Preflight avoids deterministic
fixture misconfiguration; it cannot repair a later runtime error. The normal
0/1/2 path is required. The GH564/compat owner must open and approve a
separate, source-exact app-recovery ticket before claiming repeatable Close
after such errors; it may not synthesize Cancel, clear app-private state
from Host or silently count this as a pass.
### Q-6: Pinned message-argument compatibility

Approve GH478's typed admission of `type:'question'`, `title:'Confirm'` and
macOS-inert `noLink:true` alongside the exact draw.io buttons and indexes.
Run unchanged-caller and invalid-type/unknown-field controls; until both
specs agree and a native Mac panel reproduces the result, keep the GH477
synchronous cell expected-fail.

Approving this specification does **not** approve or implement GH478,
complete GH450 native close, integrate GH445/GH446 Ready, qualify
Windows/Linux, flip any corpus cell or authorize a merge. It permits
implementation **only after both approved dependency contracts** and the
winning source/OS claims are present.

### B-1: Shared dialog interaction and cancellation bounds

- **Status:** open
- **Owner:** GH478 native broker, GH527 WorkerLink and Core/Host transport owners
- **Impact:** AC-8 cannot pass. Local Worker abandon or Cancel acknowledgement alone does not prove original CALL retirement or physical panel release.
- **Check:** Approve the distinct human UI budget, nonce-bound same-channel Cancel, exactly one original terminal, 257 sequential expiries, delayed Host admission and real AppKit physical cleanup.

### B-2: GH478 platform-native completion

- **Status:** open
- **Owner:** GH478 AppKit/native owner
- **Impact:** No real message-box result, parent-sheet behavior or modal cancellation can be claimed.
- **Check:** Approve the exact alert/parentless callback schedule, main-thread cancellation, physical release and any narrowly required objc2 unsafe allowance.

### B-3: Pre-Ready synchronous API behavior

- **Status:** open
- **Owner:** GH564 public API and singleton owner
- **Impact:** A synchronous caller before the existing singleton is connected has no approved refusal behavior.
- **Check:** Approve and test a synchronous typed not-ready refusal; prove no Promise, second HELLO, second Worker or new connection.

### B-4: Native error-code allocation

- **Status:** open
- **Owner:** KELD native/error registry owner
- **Impact:** The draft cannot ship guessed or unregistered native dialog codes.
- **Check:** Consume only GH478's accepted typed errors and the canonical error registry; verify code/fix mapping and generated docs.

### B-5: Effect-unknown application recovery

- **Status:** open
- **Owner:** draw.io compatibility/recovery owner and GH564 application owner
- **Impact:** After modifiedModalOpen is set, a thrown dialog failure can strand the pinned app flag. Repeat Close remains explicitly unsupported.
- **Check:** Approve a separate source-exact recovery adapter or upstream fix, then force a post-flag error and prove a later Close works without synthetic Cancel or Host mutation of app-private state.

### B-6: Pinned message-argument compatibility

- **Status:** open
- **Owner:** GH478 permission/options owner and GH564 consumer owner
- **Impact:** The exact unmodified draw.io call cannot be implemented until the broker agrees to the required options.
- **Check:** Approve type question, title Confirm, macOS-inert noLink, exact buttons/indexes and invalid-type/unknown-field controls; verify on a real Mac panel.

### B-7: GH477/GH566 conformance evidence

- **Status:** open
- **Owner:** GH477 compatibility owner and GH566 corpus-manifest owner
- **Impact:** The synchronous compatibility cell must remain expected-fail until the real native consumer is proven.
- **Check:** Flip only the implementing ticket in the same diff after real Mac acceptance and keep the lifecycle manifest unchanged.
