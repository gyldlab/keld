# Spec: GPU preparation and behavioral media installation
Status: approved
Linear: KEL-132 · Owner: GYLDLAB · Updated: 2026-09-11
Approval: Linear comment `f68e4b33-baba-43f3-8938-c07637ab7fe4` · decision SHA-256 `1e8b5b9915d9d60884fabc87dba6184b4fd3c530b451345f13af431fa5b107f9`

## 1. Goal & non-goals

Reconcile the landed Linux GPU and media fixes with the remaining cross-platform
acceptance. A successful constructor is not evidence that a platform requested
permission from Keld. Each backend must prove that its installed callback receives
the intended adapter inputs, applies an explicit platform decision, and precedes the
first navigation. KEL-102/T4 separately proves policy-sensitive evaluation against
the verified session snapshot. GPU preparation and media installation are
independent contracts with different failure modes and tests.

The September 10 continuation explicitly assigns this backfill and necessary
dependency work to an Astra orchestrator with delegated independent evidence/review.
This document does not retroactively approve the earlier implementations or describe
an old execution frontier as current. Approval of work is distinct from approval of
new API or platform-support decisions below.

Non-goals: repeat #157/#162; change the GPU population or claim a rendering speedup;
implement an additional policy evaluator; introduce window grants, origin policy,
or a global initializer; broaden permissions; equate a mock device with physical
capture acceptance; implement the entire packaging/profile roadmap in this issue.

## 2. Spec refs

- Architecture 01 §§1, 4–5: ownership, UI thread and attributed performance.
- Architecture 03 §§1, 4.3: host-derived webview principal and default denial.
- Architecture 05 §1: exact-self Linux preparation and media installation evidence.
- `kel102-host-guard-enforcement.md` §4 and T4: immutable verified snapshot,
  private binding owner/weak callback lease, quiescing and the shared dispatcher.
- `kel135-persistent-profile-identity.md` §§4, 6–7: browser profile ownership,
  ephemeral development state and per-platform lifetime proof.
- KEL-171 owns the driver/backend/version rendering matrix. KEL-79 owns origin and
  resource policy; neither is inferred from media capture tests.

Reviewed baseline: Keld `2108f1dbac5fd8779711252fb0fc0c84ae8dc311`, Prompt Tracker
`ee7b812adf2128caedec95ffae4f2a000ce2d62a`, research
`2cbf07c2076f62146664e39e334dd93142a530d1`. Source inspection must be refreshed
when these owning paths change. No runtime architecture change is made by T0.
Post-approval refresh: Keld `739c4e8676e92ce4e28075987726153659b8b5a9`;
the only governing-path delta is the reviewed KEL-135 amendment that this contract
now consumes by its exact landed SHA below.

## 3. Acceptance criteria (binary, each becomes a test)

1. **GPU-PREP:** detected risky/unprepared Linux process entry replaces its image
   using explicit argv/envp and exact `/proc/self/exe`; original arguments and PID
   survive, one mitigation override is supplied, and preparation failure returns
   `KELD-WV-010` without GTK initialization. Normal/prepared inputs do not re-exec.
2. **GPU-STATE:** detection does not mutate the environment. Only mitigation-present
   state reports degraded rendering; pending preparation does not. This is a
   best-effort detector, not certification of driver health or WebKit version.
3. **INSTALL:** each live backend invokes Keld through the handler registered on
   the same platform view that receives the request. Deleting/no-oping registration
   fails a behavioral test even if the engine itself defaults to denial.
4. **DECISION:** separate camera and microphone requests record the actual media
   adapter principal, manifest input, returned decision, and successful platform action. Empty
   manifest denies; missing/AppProcess identity returns `KELD-GUARD007`; current
   minted webview principals remain `KELD-GUARD006`, including with `/app` grants.
   That principal-class denial precedes capability and manifest lookup, so this
   current-v0 row does not prove policy-sensitive evaluator behavior; SNAPSHOT does.
5. **ORDER:** navigation cannot occur before registration succeeds on that view.
   A witness for another view or a failed registration cannot authorize navigation.
   A no-op registration that fabricates success must fail the invocation oracle.
6. **OS-EFFECT:** each real OS row records request completion, no capture and no
   platform prompt. Windows records `PermissionRequested` kind/origin, successful
   `SetState(DENY)` and same-callback `State==DENY` before return; returning
   `DEFAULT` is the prompt-selected negative control. macOS records the actual
   delegate decision-handler value `WKPermissionDecisionDeny`; returning `Prompt`
   is the negative control. Linux records the explicit
   `webkit_permission_request_deny` call. Each required device row has a controlled
   Allow that returns a live stream/track and then stops it, so missing hardware or
   an insecure origin cannot supply a denial pass. Windows and Linux may use their
   cited public development/mock facilities. macOS requires physical hardware or a
   separately reviewed OS-level virtual device; private WebKit SPI/TestRunner is not
   accepted. Camera and microphone are independent. Additional physical-device
   results are supplemental only where a supported synthetic/virtual row already passed.
7. **TEARDOWN:** after destroying a view, attempts to navigate it fail with
   `KELD-WV-007`. A new view receives a distinct id and installed handler. Callback
   revocation after retaining the callback is a separate KEL-102/T4 acceptance;
   ordinary view destruction must not be reported as that stronger proof.
8. **SNAPSHOT:** KEL-102/T4 passes the actual session snapshot and host registry
   identity to all backends. Its recorder digest equals the verified session SHA-256.
   A correct reported digest paired with a default manifest fails. Missing policy
   load is a startup failure, never permission recovery through a default manifest.
9. **REVOKE:** KEL-102/T4 retained leases deny after view removal or policy quiescing.
   Navigation generation rotation is proved at the owning registry event when it
   ships; fixed generation zero does not satisfy navigation-revocation acceptance.
10. **SAVED-GRANT:** the KEL-135 dependency proves a seeded browser camera/microphone
    Allow cannot bypass the next session's denying policy. A fresh development
    session does not inherit a previous session's grant. Test the persistent and
    ephemeral cases independently; fresh-store denial proves neither persistent
    revocation nor same-app identity persistence.
11. **MAC-FLOOR:** the supported oldest macOS/build combination either invokes the
    guarded delegate or rejects construction before content loads. Record both
    debug and release behavior; a newer macOS pass cannot prove the oldest row.
12. **ARTIFACT:** every owned row has exact task, platform, revision, executable,
    fixture and negative-control evidence. Missing rows remain open. A T0 contract
    pass is not a KEL-132 issue-completion or three-OS product pass.

## 4. Design

### First principles and reuse

| Atom | Owner and input/output boundary | Failure and independent falsifier |
|---|---|---|
| GPU classification | `webkitgtk` probe: session/driver/env → `GpuSafeMode` | Wrong classification; exact matrix and before/after environment observation |
| GPU preparation | Linux backend exec owner: owned C strings → replacement process | Live environment mutation, wrong argv/PID, fallback after failure; byte tests and real exec trace |
| Construction | Shipping dispatcher then UI-thread engine: prepared state → platform view | GTK reached unprepared; typed failure before event-loop construction |
| Installation | Backend constructor: handler registration → borrowed same-view witness with no additional durable COM owner | No-op/disconnected/wrong-view registration; real callback/API invocation receipt |
| Identity | Host view registry: view and generation → evaluator principal | App/caller/stale identity; non-first-view and substitution controls |
| Adapter decision | Current media adapter, later KEL-102 snapshot owner | Missing/wrong inputs or disconnected adapter; invocation receipt and registration-removal control. Constant evaluator behavior is distinguishable only after KEL-102/T4 supplies policy-sensitive state |
| Platform mapping | Backend-private production mapper: adapter Allow/Deny → platform Allow/Deny | Ignored result or constant Deny; call the same production mapper with both decisions and assert exact platform constants, then prove the live callback applies/read-backs its Deny output |
| Lifecycle | View owner, KEL-102 admission owner, KEL-135 store owner | Stale lease/saved Allow; separate destroy, quiesce and restart controls |
| Evidence | Per-OS fixture/operator: actual request → attributable receipt | Default engine denial, absent device or dead prompt monitor; positive and negative controls |

Classification feeds preparation, which precedes construction. Installation precedes
navigation. A callback consumes identity and policy, but neither proves registration.
Store lifetime and callback lifetime are separate: keeping an old preference is not
the same defect as keeping a Rust callback alive. OS containment remains KEL-78;
neither this callback nor a mocked stream proves sandbox containment. Input HTML,
origin strings and Bun payloads cannot mint authority. All platform handles stay on
their existing UI thread; callback storage and process lifetime stay with the engine.

### Landed behavior retained

PR #157 (`5443c6fed45199cb7ec7a71882928180a5657450`) replaced the unsafe live
environment mutation with backend-owned raw `execve` and a fallible constructor.
The public preparation helper is a process-entry operation: invoking it after
non-repeatable state would discard that state. That lifecycle constraint is not
itself a Rust undefined-behavior precondition. Do not make it `unsafe fn` merely
to document sequencing. The old privacy-only fix was insufficient because safe
constructors still reached `set_var`; it is not an acceptable alternative.

PR #162 (`dcc4676af16146c887e03e5dcbdd824032c10055`) added an opaque guarded
wry builder, a fake installer callback slot, and Linux behavioral proof. Preserve
the raw-builder encapsulation through platform build. Windows keeps its COM
installer and `GuardInstalled`; KEL-168's popup denial remains a prerequisite
to that witness. Reuse these owners rather than adding parallel installers.

The existing Linux FNV debug receipt identifies the manifest used by the current
adapter. It is not the verified SHA-256 session digest required by SNAPSHOT.
Current adapters construct default manifests and generation zero. KEL-102/T4
owns replacing those values; this backfill does not present that integration as live.

### Remaining implementation shape

The next Windows slice exercises the existing production COM installer against a
real WebView2 controller with an isolated fixture-owned user-data directory. A
fixture-specific directory is not an implementation of authenticated application
profiles. It must not use or delete the user's shared `dev.keld` directory.
Registration, actual adapter invocation, adapter-result mapping and successful `SetState` need separate
receipts bound to controller/view identity, process/thread and run nonce.
Receipt observers remain test-only where practical and cannot change authorization.
Do not add a public callback setter, arbitrary-principal API or a shipping allow flag.

Windows and macOS controls must distinguish engine permission from OS device access.
A controlled Allow used to validate the fixture is outside shipping authorization
and is recorded as a mutation/fixture control; it does not grant `/app` permissions
to a webview. The platform permission enum/state is the prompt-selection oracle; a
generic window census is not. On Windows/Linux, the positive stream/track result
qualifies the configured synthetic device path. On macOS, named physical hardware
may satisfy the required device row; a reviewed OS-level virtual-device pass makes
the separately named physical row supplemental. The v0 contract is callback
enforcement for actual engine requests rather than hardware-driver certification.

No new public Rust signature, channel, manifest capability, wire field or dependency
is selected by T0. Existing future `MediaPolicy` signatures stay owned by KEL-102.
Persistent profile types, signing, namespace and purge remain KEL-135. The missing
saved-permission acceptance must be added to that owner's task contract before the
corresponding implementation is considered complete.

## 5. Boundaries

T0 implements `docs/specs/kel132-webview-gpu-media.md` only, plus generated documents
only when the owning generator includes the source. T1 may change the Windows
backend's private test seams, same-view witness binding, and colocated tests/fixtures with a separately reviewed
diff. T2 owns macOS seams and platform tests. T3 is the completion reconciliation.
Do not silently edit KEL-102's approved task order, KEL-135's identity decisions,
the manifest schema, kipc wire, or packaging scope in a media fixture PR.

## 6. Tasks (each approximately one PR)

- [ ] **KEL-132/T0:** review and land this backfill; publish a contract-only artifact
  with the exact remaining rows and dependency predicates. No product OS claim.
- [ ] **KEL-132/T1:** Windows behavioral installation fixture using the production
  COM installer and isolated fixture state. Pass INSTALL, current-v0 DECISION, ORDER,
  TEARDOWN and Windows OS-EFFECT. Use at least two nonconstant host-returned ids;
  require a constructor-initial nonce page gated until observers are installed; bind
  adapter manifest identity to externally fixed expected values; record the exact
  kind/origin and same-callback state after `SetState`; make `DEFAULT` fail as the
  prompt-selected mutation; label synthetic versus physical devices. Factor
  the production adapter-result → COM-state mapping behind one private helper, test
  both Allow and Deny against exact constants, and make the live callback call it.
  Replacing that helper with constant Deny must fail before the real-OS row is accepted.
- [ ] **KEL-132/T2:** macOS behavioral installation fixture and oldest-supported
  OS decision with real execution; record actual decision-handler Deny and make
  Prompt fail; pass the equivalent rows and MAC-FLOOR.
- [ ] **KEL-132/T-LINUX-TEARDOWN:** extend the existing Linux example to navigate
  the destroyed primer id, require `KELD-WV-007`, and prove a fresh id/handler.
  Vary primer count so camera and microphone use different nonconstant ids. Add
  hard-coded-principal and disconnected-adapter controls. A mutation retaining the
  removed view must fail. Existing #162 evidence does not include these assertions;
  run the changed fixture on Linux before closing it. Policy-sensitive evaluation
  remains SNAPSHOT rather than a constant-deny v0 claim.
- [ ] **KEL-132/T3:** consume T1/T2/T-LINUX-TEARDOWN and preserved Linux evidence, plus KEL-102/T4
  SNAPSHOT/REVOKE and KEL-135 SAVED-GRANT artifacts; reconcile all original criteria
  before completing KEL-132. No implementation is duplicated in this final task.

Dependencies retain their own issues and claims. KEL-102's approved order is
T2 → T3 → T4 → T5; T4 consumes passed T3, whose filesystem boundary consumes
KEL-130. This is an integration dependency, not a prerequisite to building T1/T2
installation fixtures under current default-deny behavior. KEL-135 T0 completion
does not prove any of T1–T5 or SAVED-GRANT. The landed #214 amendment assigns the
saved-permission rows to KEL-135 T2/T3/T4; only their exact passed artifacts supply them.

Artifact schema is `keld.execution-artifact/v1`. KEL-132 artifacts bind `issue_id`,
exact `task_id`, `node_id`, approved spec revision/approval receipt, landed `head_sha`,
PR and review evidence, and named acceptance rows with platform and status. T0's
node is `webview-gpu-media-contract`; T1 is `webview-media-windows`; T2 is
`webview-media-macos`; T-LINUX-TEARDOWN is `webview-media-linux-teardown`; T3 is
`webview-media-completion`. T1/T2/T-LINUX-TEARDOWN carry their AC identifiers from
§3 prefixed by `windows/`, `macos/`, or `linux/`, except OS-EFFECT expands into
the exact required per-device rows in the ledger (`camera-synthetic` /
`microphone-synthetic` on Windows/Linux and `camera-device` /
`microphone-device` on macOS); there is no aggregate platform-prefixed OS-EFFECT row.
T0 approval and code bug-fix
acceptance are separate artifacts even when reviewed in the same session.

T3 additionally consumes exact `KEL-102/T4` policy/lifecycle evidence and
`KEL-135/T2:windows-media-saved-grant`, `KEL-135/T3:macos-media-saved-grant`,
`KEL-135/T4:linux-media-saved-grant`, plus the respective `*-dev-ephemeral` rows.
Those additional KEL-135 rows are adopted in the owning PR #214 contract amendment,
landed at `739c4e8676e92ce4e28075987726153659b8b5a9`, with decision receipt
`ac75562f-ee34-46e2-9694-a26224b0f701`; T3 requires the exact passed platform
artifacts rather than the contract merge or decision alone.
Reject wrong-task, unlanded, missing-provenance, or awaiting/failed-row substitutions.
No generic parent Done or earlier T0 artifact satisfies a platform implementation edge.

### T0 acceptance and dependency ledger

The T0 artifact carries this exact set. Set equality is part of acceptance: deleting
an awaiting row or dependency is a failure, not a smaller valid artifact.

| Stable row | Initial status | Owner and evidence/predicate |
|---|---|---|
| `KEL-132/T0-contract` | passed only after landing | T0; approved spec, reviewed landed head and PR |
| `linux/GPU-PREP` | passed, historical receipt | PR #157 merge `5443c6fed45199cb7ec7a71882928180a5657450`; Linear `33774ceb-51cc-4812-be41-88fe27901f9c` |
| `linux/GPU-STATE` | passed, historical receipt | same #157 evidence; broader matrix remains KEL-171 |
| `linux/INSTALL` | passed, historical receipt | PR #162 merge `dcc4676af16146c887e03e5dcbdd824032c10055`; Linear `7bac91a0-b44c-409f-89e3-fadacc2ffb47` |
| `linux/ORDER` | passed, historical receipt | same #162 initial guarded-builder/callback evidence |
| `linux/camera-synthetic`, `linux/microphone-synthetic` | passed, historical receipt | same #162 explicit deny API and controlled Allow stream evidence |
| `linux/camera-physical`, `linux/microphone-physical` | supplemental-awaiting, nonblocking | named future Linux operator; physical device path is not v0 callback completion |
| `linux/DECISION`, `linux/TEARDOWN` | awaiting | T-LINUX-TEARDOWN varies ids, adds adapter/identity controls and stale-id proof; it re-runs/packages all Linux rows so historical receipts cannot disappear from T3 |
| `windows/INSTALL`, `windows/DECISION`, `windows/ORDER`, `windows/TEARDOWN` | awaiting | T1; real Windows engine and actual returned view identity |
| `windows/camera-synthetic`, `windows/microphone-synthetic` | awaiting, required | T1; kind/origin, same-callback DENY state, JavaScript denial and controlled Allow stream |
| `windows/camera-physical`, `windows/microphone-physical` | supplemental-awaiting, nonblocking | named future Windows operator; cannot substitute for required synthetic rows |
| `macos/INSTALL`, `macos/DECISION`, `macos/ORDER`, `macos/TEARDOWN`, `macos/MAC-FLOOR` | awaiting | T2; real macOS debug/release and minimum-boundary evidence |
| `macos/camera-device`, `macos/microphone-device` | awaiting, required | T2; named physical hardware or separately reviewed OS-level virtual device, actual decision-handler DENY and controlled Prompt/Allow cases |
| `macos/camera-physical`, `macos/microphone-physical` | supplemental-awaiting, nonblocking only after a reviewed virtual-device pass | named future macOS operator; cannot substitute for required device rows |
| `policy/SNAPSHOT`, `lifecycle/REVOKE` | awaiting | exact passed KEL-102/T4 artifact and its named recorder/lease rows |
| `windows/SAVED-GRANT`, `macos/SAVED-GRANT`, `linux/SAVED-GRANT` | awaiting | landed PR #214 successor contract, then exact KEL-135/T2, T3 and T4 `*-media-saved-grant` plus `*-dev-ephemeral` rows |
| `KEL-132/completion` | awaiting | T3 exact-set reconciliation; cannot pass while any required row above is awaiting/failed |

Exact predecessor table:

| Task | Required before claim/implementation |
|---|---|
| T1 | landed T0 artifact; fresh KEL-132 claim; Windows operator for claimed real rows |
| T2 | landed T0 artifact; fresh KEL-132 claim; named real macOS operator |
| T-LINUX-TEARDOWN | landed T0 artifact; fresh KEL-132 claim; named Linux operator |
| T3 | exact passed T1, T2 and T-LINUX-TEARDOWN artifacts; exact passed KEL-102/T4; landed KEL-135 amendment plus exact passed T2/T3/T4 saved-grant and dev-ephemeral rows |

Each task also requires a fresh frontier only when its current Prompt Tracker header
requires one. A historical prompt, open PR, decision comment or partial artifact does
not satisfy a predecessor.

## 7. Test plan

| Rows | Test / operator | First negative control |
|---|---|---|
| GPU-PREP, GPU-STATE | Retain `webkitgtk` vector/state/failure tests and #157 real Linux trace | Restore live mutation, corrupt argv/env or allow unprepared construction |
| Linux INSTALL, current-v0 DECISION, ORDER, OS-EFFECT | Existing `linux_media_guard` evidence plus T-LINUX-TEARDOWN identity/adapter repairs | No handler, hard-coded principal, disconnected adapter, missing receipt, transient prompt, dead monitor |
| Windows installation and effect | T1; named Windows operator and immutable run artifact; kind/origin plus same-callback state before return; separate camera/mic synthetic rows | Omit registration; bypass adapter; hard-code view id; replace mapper with constant Deny; return DEFAULT; make Allow produce no stream |
| macOS installation and MAC-FLOOR | T2; named real macOS operator; actual decision-handler enum; separate camera/mic rows using physical or reviewed OS-level virtual devices | Omit delegate; test oldest supported debug/release; return Prompt; make Allow produce no stream; reject private SPI/TestRunner |
| TEARDOWN | Each backend destroy/stale-id/recreate test | Reuse old id or navigation before successful installation |
| SNAPSHOT, REVOKE | KEL-102/T4 tests specified in its §7 | Correct reported digest with default evaluator input; retain callback after binding removal |
| SAVED-GRANT | KEL-135 platform owner; two-session same-profile and fresh-dev-profile tests | Seed persisted Allow then bypass new denying policy |
| ARTIFACT | T0 and T3 operator independently re-fetch the Linear JSON, require exact ledger/dependency set equality, and verify schema/task/node/spec approval/landed-head/review/row status | delete one passed, awaiting or dependency row; wrong task, nonancestor head, missing approval/review, or awaiting completion row is rejected |

Use loopback port zero, nonce-bound exchanges, observable request/monitor fences and
bounded kill-switch deadlines. Never sleep to synchronize. Risky teardown and
capture tests run in subprocesses and assert cleanup plus fresh successful startup.
Do not accept a timeout, `NotAllowedError` alone, or symbol/source presence as proof.
Run `just ci` and every applicable platform test on the final implementation diff.

## 8. Review gates triggered

T0: unsafe contract review for the retained exec boundary; public API review for
witness/constructor constraints; permission-model review for callback, snapshot and
saved-permission ownership. No dependency addition or wire protocol change.
Later platform FFI changes need independent unsafe review of their actual diff.
KEL-102/T4's sibling dependency requires its own gate; this spec grants none.

## 9. Perf impact

T0 has none. Retain #157's disclosed dispatcher re-exec cost and correctness waiver;
do not relabel it first-paint performance. Fixtures and observers do not establish
runtime improvements. Any retained production instrumentation must be evaluated
against the owning path's allocation and latency contract.

## 10. Open questions

None for T0. The approved macOS decision requires runtime 12+ for guarded webview
creation, matching the documented baseline and Apple's delegate availability. Reject
older runtime before window creation. A build missing the delegate also fails before
application navigation: create a blank view, inspect the actual delegate, then load
the requested target. This differs from current target-before-build ordering and
needs reviewed implementation and real debug/release evidence. KEL-135's persistent
store minimum remains 14. Primary sources:
  [WebKit API](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/UIProcess/API/Cocoa/WKUIDelegate.h)
  and [pinned wry build](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/build.rs),
checked 2026-09-10. KEL-135 saved-permission ownership is T2/T3/T4 as recorded above.
Unassigned macOS device/operator access is an acceptance availability gap, not an open
T0 design question. The required macOS device rows remain awaiting. Windows/Linux
physical rows remain supplemental and never substitute for their required real-engine
synthetic-device rows.
