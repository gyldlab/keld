# Spec: Windows explicit-profile and descendant qualification

- Status: draft
- Tracker: GH-1187 (Windows program W6); shared policy remains KEL-78 / GH-586 / GH-588
- Owner: Monish Khandelwal with existing Windows runtime and profile owners
- Updated: 2026-11-06
- Keld source: `0eb3ce203187f35c5276a4237189c906a5220d5a`

## 1. Goal & non-goals

Qualify the actual Windows primary Bun profile and descendants used by pinned draw.io.
A passing explicit Legacy development run must show one verified declaration, visible authority
forfeiture, authenticated and guarded Keld calls, and observed process-family retirement.
Windows Job lifetime custody is reported separately from operating-system containment.

This file plans platform qualification under existing shared policy. It does not approve
shipping LPAC integration, a new profile state, an installed Legacy route or new privileged code.
LPAC means less-privileged AppContainer, the existing Windows containment candidate.
A Job is the Windows object used by the existing runtime to own process lifetime.

Non-goals:

- Change the shared profile key/parser, boot version mapping, guard grants or role identities.
- Relax zero-capability LPAC, replace the supervisor or add broad package/network capabilities.
- Claim Strict from a Job, a configured flag, a renderer sandbox or successful HELLO.
- Solve the separate LPAC descendant limitation by silently replacing application effects.
- Qualify direct installed Legacy boot, macOS/Linux behavior or every Electron application.

## 2. Spec refs

At the header source pin, architecture 01 owns process/lifecycle and budgets; architecture 02
owns authenticated IPC; architecture 03 owns permission and containment distinctions.
Architecture 06 section 1 owns the primary runtime seam.

Governing specifications:

- `docs/specs/kel78-strict-profile-sandbox.md`: profile states, Windows containment and hostile proof.
- `docs/specs/kel210-windows-lpac-primary.md`: LPAC transport/prepared-child composition.
- `docs/specs/kel75-principalized-bun-child-roles.md`: generation and supervisor authority.
- `docs/specs/kel74-compat-evidence-schema.md`: provenance and authority-specific evidence.
- KEL-254's approved direct-installed contract: retain its Legacy refusal; no amendment here.

The explicit-profile contract inspected is PR-1120's
`docs/specs/kel78-explicit-legacy-profile.md` at
`5f66d50ff88c578ef651ebe4d121a19830355524`.
GH-588 / PR-1319 owns its shared implementation. GH-587 owns the trusted product collector.
Their final approved and implemented revisions are execution prerequisites.

W6's corrected owner direction [D2] supersedes contradictory parts of the earlier decision:
omission follows KEL-78 as Unverified; LPAC descendant error 5 is an unexplained observed
limitation, not intended spawn denial; grant cleanup must survive abnormal Host death.
Legacy draw.io qualification is limited to dev/non-direct routes unless KEL-254 is amended.

### Verified source and historical evidence

| Status | Fact and source |
|---|---|
| Current code | Main's boot reader accepts only closed versions 1/2; it has no explicit-profile 3/4 support. [S1] |
| Current code | Windows primary config uses the shared role; its preparer constructs generic Command, and prepared commands have Direct/LinuxStrict variants. [S2][S3][S4] |
| Current code | Host installs its death Job before app startup; Job creation reads back kill-on-close and forbids breakaway flags. [S5][S6] |
| Current code | LPAC owns suspended creation, inherited-handle copies, zero capabilities and configured opt-out. It is not the ordinary primary route. [S7] |
| Current code | LPAC profile Drop deletes registration and frees its SID; no durable grant-revocation ledger is implemented in that object. [S7] |
| Historical probe | W6 reports Windows 11 / Bun 1.4.4 custody, LPAC spawn error 5 and leftover grant entries. It is not a current-head product run. [D1] |
| Pinned app | Draft/backup saving spawns System32 attrib.exe; font enumeration conditionally executes PowerShell and converts errors to an empty result. [A1] |

The application source is `jgraph/drawio-desktop@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`.
Its webapp baseline from W6 is `24b76c2cbd55d88e354042e8d329a2e4708972bc`.
Refresh and compare any new target explicitly; a different source or runtime is another arm.
WebView persistent-profile selection is unrelated to this primary Bun security profile.

## 3. Acceptance criteria (binary, each becomes a test)

1. **Shared input admission.** Given an approved Windows compiler/Host pair, when each supported
   boot/profile row reaches the actual Windows entry, then the one verified permissions snapshot
   selects that row before app children, listeners or windows exist.
   Unknown schema, duplicate/invalid profile, digest mismatch and version/declaration mismatch
   are separate refusal cases with the owning typed diagnostic and zero resource counters.
   A mutant skipping only digest comparison must fail its case.
   Unsupported profile-bearing target/mode pairs remain explicit refusals.

2. **Omission and disclosure remain distinct.** Given fresh valid profile-free input, when
   the primary starts, then it follows the existing Windows launch behavior and reports Unverified.
   Given an approved explicit Legacy dev input, the canonical forfeiture is observable before
   app execution and the retained Host state identifies the same declared Legacy request.
   Dropping only the declaration must produce Unverified in a fresh session, never retained Legacy.
   Suppressing only the disclosure must fail the explicit Legacy case.

3. **Authentication and authorization survive Legacy.** Given the real primary's generation,
   when intended, wrong-token and stale-generation peers make requests, then only the intended
   peer binds; a valid but ungranted privileged request is denied before its handler.
   A valid granted control succeeds after each refusal.
   Bypassing only guard dispatch must fail the forbidden-handler counter even if HELLO still works.
   Bun's ambient same-user authority is explicitly forfeited; guard denial is not an OS sandbox.

4. **Recovery freezes policy.** Given an initial bound generation G1 and a captured request,
   when Bun crashes after on-disk policy is changed, then permitted G2 uses the original request
   and permissions digest, with a fresh principal/link/token.
   Old credentials cannot address G2. No receipt for G1 may qualify G2.
   Rereading only profile selection on recovery must fail this case.
   Full session retirement and new capture are required to change profile or grants.

5. **Real application descendants and effects.** Given the pinned application's actual
   draft/backup flows under explicit Legacy, when they request hidden-file effects, then
   the observed descendant image is the intended System32 `attrib.exe`, belongs to the
   correct process family, and the resulting file's hidden attribute is independently read back.
   A successful save alone is insufficient because source treats these child failures as best effort.
   For the separately selected font case, observe the intended PowerShell image and returned
   font data; an empty result is not proof that the command ran successfully.
   Replacing only child creation with a successful no-op must fail image/effect observations.

6. **Lifetime custody is real.** Given the actual primary plus an owned blocking child and
   grandchild in the selected Windows route, when success, cancel, Bun crash, accepted Quit,
   CLI loss and Host death run as separate cases, then the named runtime custodian retires the
   appropriate enrolled family and revoked links, and a new legitimate launch succeeds.
   Terminal shutdown admits no successor; crash recovery follows the existing supervisor policy.
   Observe retained process handles/creation identities, actual Job membership and exited state.
   Omitting only required Job enrollment must fail an independent survivor census.
   Exit code zero alone cannot distinguish clean exit from kill-on-close.

7. **Strict cannot fall back or borrow a custody pass.** Given an explicit Strict request,
   when containment preparation, strict proof or LPAC transport admission fails, then application
   execution is not released and no ordinary Command/Legacy retry occurs.
   Injecting only that retry must fail the independent app-entry witness.
   A Strict success additionally requires KEL-78's full matching OS-containment catalog,
   independently observed LPAC behavior and exact handle allowlist.
   Job cleanup and configured opt-out alone must never satisfy the catalog.

8. **LPAC grant cleanup survives Host loss.** Given future approved production LPAC integration,
   when Host is killed before cleanup, then its owner-approved durable grant record and startup
   sweep revoke retired per-generation grants, retain unrelated ACL entries, and permit a new launch.
   Inspect actual security descriptors across the recorded granted subtree.
   Deleting only the startup sweep must fail the abnormal-death row even if normal Drop passes.
   No Windows LPAC product activation passes while this prerequisite is absent.

9. **Trusted scoped evidence.** Given GH-587's trusted collector and actual W6 run, when product
   evidence is assembled, then exact runtime/app/OS identities, mode, role generation, captured
   permissions, requested/effective profile and each behavior witness agree.
   A Bun-printed receipt-shaped line, stale-generation record, macOS result or Job-only
   record relabelled Strict is rejected. Removing only trusted-origin validation must fail
   the forged-output case. Dev Legacy evidence cannot qualify direct installed boot.

## 4. Design

### Decision atoms

| Atom / owner | Input -> output boundary | Failure and independent oracle |
|---|---|---|
| Policy / shared compiler + core | Verified bytes/version -> frozen request | Mismatch admitted; typed refusal and resource counters |
| Identity / role owner | Spawn attempt -> generation/principal | Reuse; retained Host binding |
| Authentication / IPC | Endpoint/token -> accepted stream | Wrong peer admitted; real rejection plus healthy control |
| Authorization / guard | Bound request -> allowed handler | Ungranted handler runs; counter independent of reply |
| Containment / LPAC + guard | Native preparation/catalog -> admitted Strict | Wrong token/handles or direct OS escape; native oracle |
| Custody / Job + supervisor | Enrolled family -> terminated/revoked attempt | Live retained process after retirement |
| Grant lifetime / resource owner | Per-generation grants -> retirement/sweep | Retired SID entry survives Host death; descriptor readback |
| Provenance / GH-587 + KEL-74 | Trusted run -> scoped product record | Child log accepted as Host authority |

SID means security identifier. An ACL is an access-control list; its individual permission
entry is an ACE. A generation's SID, process ID, token and Host-minted principal are
different identities. None may substitute for another.

Admission depends on captured policy. Authentication consumes generation identity.
The trusted record consumes admission and execution facts. Those edges do not collapse
independent containment, cleanup and product-effect oracles.

### Reuse and ownership

Use the existing shared profile dispatcher and frozen session policy.
Use the one primary supervisor, role generation lease, Windows Job and LPAC owners.
KEL-210 already identifies the narrow prepared-child adapter needed to integrate LPAC;
this file must not create a competing adapter or second supervisor.

Current API anchors:

```rust
WindowsLpacProfile::create(name: &OsStr) -> Result<Self, WindowsLpacError>
WindowsProcessJob::create() -> Result<Self, WindowsHostJobError>
WindowsProcessJob::assign_child(&mut self, process: impl Into<WindowsJobProcess<'_>>)
    -> Result<WindowsJobMembership, WindowsHostJobError>
```

These existing primitives are not evidence of completed product composition.
KEL-210's exact adapter/transport signatures and the shared profile request remain owner-gated.
Do not force an LPAC-owned process object into `std::process::Child` or duplicate its waits.

The Windows Host's established Job path and a suspended assign-before-resume path
are different custody routes. Qualify whichever real product route is selected:
prove Host is enrolled before creating application descendants, or prove the suspended
process is assigned and read back before resume. Do not claim one from the other's fixture.

Legacy has ordinary user authority; it grants no administrator privilege and removes no
ordinary operating-system access check. Retain existing handle hygiene and grant no new
privileged Host handles. Do not promise containment of deliberately escaping hostile Legacy
descendants beyond the approved existing enrolled-family boundary.

### Application effect boundaries

Pinned `saveDraft` and backup-save paths do not await an `attrib.exe` exit.
They log child errors and can complete save independently. Qualification therefore awaits
independent process and file-attribute observations; UI/save success is not their oracle.

The font path uses `execFile` with a 30-second timeout, then filters and sorts output;
errors resolve to an empty array. Distinguish empty legitimate output, execution failure
and successful observed command output. Freeze the chosen test font setup.

Source sets `NoDefaultCurrentDirectoryInExePath=1` on Windows.
Historical W6 evidence varied it and found a working-directory decoy could run without it.
Preserve pinned behavior and record the actual executable path; `Bun.which` or a requested
basename is not proof of the image that started.
This spec does not create a new shell/executable-resolution policy.

### LPAC blockers are preserved

The historical native probe found descendant CreateProcessW error 5 under zero-capability
LPAC. Its cause remains unresolved. KEL-78 expects descendants under the same profile,
so failure to start them is not a successful descendant contract.
No ordinary-AppContainer substitution or extra capability is authorized.

Before future LPAC activation, its owner must satisfy corrected W6 cleanup obligations:
unique per-generation profile identities; durable grant recording and startup sweep;
install-time grants for persistent shared paths; per-launch grants limited to ephemeral
generation objects; descriptor-based verification that retired grants are gone.
A Drop-only solution cannot satisfy Host kill or abort.

The `AppContainer\\Mappings` registry observation in W6 is a test oracle only, not a
supported product dependency. Profile deletion status alone is not grant or live-token revocation.
Do not infer that the historical probe proves the current Host's cleanup.

### Runtime seam, types and migration

Before: ordinary Windows primary uses generic prepared command plus existing Host Job custody;
main has no declared-profile 3/4 boot support.
After gated integration: the shared verified request drives the existing Windows runtime
owner, preserving one generation/lifecycle path and binding evidence to actual execution.

Core owns policy capture; runtime owns process/native resource leases; IPC owns endpoint
access and HELLO; guard owns authorization and proof admission.
Capture policy before resources. Recovery retains the request and recaptures only the
candidate-specific facts required by existing strict admission.

Manifest changes: none owned here; consume the approved shared profile key.
Public kipc wire changes: none. Boot schemas 3/4 remain the shared compiler/Host amendment.
No universal receipt schema, app-supplied SID or Windows-only override is proposed.

Migration unit: compiler/preflight, initial and recovered primary, descendant callers,
resource cleanup, disclosure and trusted collection.
No temporary permissive adapter is selected. The Electron facade remains permanent.

## 5. Boundaries

Future work belongs in existing core primary composition, runtime role/prepared-child,
`windows_job.rs`, `windows_lpac.rs`, IPC selected-pipe policy and their current tests.
Extend existing native/source owners; do not put unsafe operations in core to bypass a gate.

This PR changes only this specification. No production implementation, native experiment,
policy baseline, architecture, unsafe allowlist, dependency, app fork, installed-package
contract or global runtime pin is changed.
Do not edit other live Windows or KEL-78 claims without an accepted handoff.

## 6. Tasks (each approximately one PR; ordered vertical slices)

- [ ] **T1 — freeze the shared Windows qualification input.** Refresh live owners and exact
  predecessor heads; approve supported dev mode, parser/profile rows, runtime/app pins and
  trusted evidence seam. Add AC1/2/9 refusal cases to existing shared tests.
  Exit: exact approval and an actual preflight path; unsupported modes still refuse.

- [ ] **T2 — qualify explicit Legacy and real descendants.** Through actual Windows primary/app
  callers, prove AC2-6 with independent profile, image, file effect, guard, Job membership and
  lifecycle observations. Include a current-directory decoy control.
  Exit: original passes, isolated faults fail, restoration passes; no Strict claim.

- [ ] **T3 — complete existing LPAC prerequisites under their owners.** KEL-210 and the resource
  owner qualify adapter/transport, abnormal-death grant cleanup and AC7/8.
  Resolve the descendant limitation or retain that Strict product cell as blocked.
  Exit: independently reviewed prerequisites; no activation from configuration-only evidence.

- [ ] **T4 — publish trusted W6 product evidence.** After GH-587 and shared launch integration,
  collect AC9 for the exact dev/non-direct application run and recovery cases.
  Keep Legacy/Strict records separate and reject forged/mis-scoped records.
  Exit: scoped product evidence. T2's Legacy evidence need not await a Strict PASS.

## 7. Test plan

| Criteria | Existing test/owner family | Independent oracle |
|---|---|---|
| AC1/2 | Shared boot/profile contract and Windows Host subprocess | Typed diagnostics, capture identity, resource counters |
| AC3/4 | Role authentication/recovery and Host integration | Wire rejection, handler counter, frozen digest |
| AC5 | Pinned draw.io caller under supervised Bun | Image identity, file attributes, observed font result |
| AC6 | Windows Host-death/Job/no-flag subprocess fixtures | Retained handles, Job readback, survivor census, fresh launch |
| AC7 | LPAC boundary/app-link tests plus strict admission | Token/handle/direct-API facts and app-entry absence |
| AC8 | Resource-owner native subprocess fixture | Grant-set readback after Host kill and startup sweep |
| AC9 | Trusted collector plus actual application run | Origin/generation/mode binding and forged-record refusal |

Reuse `crates/keld-runtime/tests/windows_host_death_job.rs`,
`windows_lpac_boundary.rs`, `windows_lpac_app_link.rs` and Windows no-flag tests.
Read final source and path owners before extending them.
A Windows exit code is not a standalone cleanup or cancellation oracle.

Use owned temporary files, explicit readiness signals and retained process identities.
No sleep-based readiness; proposed kill switch is 120 seconds per isolated case.
Bound fixtures to 32 owned processes and 4096 event records; overflow fails with an incomplete
census instead of dropping evidence. Do not kill unrelated user processes.
Run normal retirement, Host kill and startup sweep separately.

Current-documentation receipt: inspected exact GitHub source, governing specs and W6's
historical native reports on 2026-11-06. This plan selects no new native API.
Implementation must refresh official Windows contracts for changed operations and record
the Windows build, architecture, Bun/executable digests and tested launch route.

Execution status: GitHub source inspection only. No Windows/native test, format command,
warning-denied workspace Clippy, workspace suite or local gate ran in this session.
Hosted checks are reported separately and cannot supply native qualification.

## 8. Review gates triggered

Future public API, permission-model and native unsafe changes require named independent
review on the exact owning diff. Dependency/wire gates are conditional; none is granted here.
This documentation change adds no executable API or protocol.
KEL-78 and Windows native owners must approve exact slices. Preserve existing holds.
An isolated reviewer must test ownership and negative-control reasoning before approval.

## 9. Perf impact

No performance claim. Native preparation, grant sweep and collection can affect cold
start/recovery. Measure architecture 01 section 5 budgets only after equivalent semantics.
Do not remove admission, custody or cleanup to improve latency.
Timeouts and record caps are failure bounds, not product guarantees.

## 10. Open questions and approval dependencies

- **B1 — shared target support:** pin approved KEL-78/GH-588 compiler, preflight and runtime
  support for Windows. First check: profile-bearing input reaches that real approved route
  or refuses; main's schema-1/2 reader is not represented as 3/4 support.
- **B2 — Windows operator and mode:** obtain an accepted native-owner handoff for the exact
  dev/non-direct environment and pinned draw.io callers.
  First check: real admission, child image/effect and generation-bound evidence.
- **B3 — Strict integration:** KEL-210 owns adapter/transport and resource composition.
  Resolve LPAC descendant error 5 and corrected grant-lifetime obligations before claiming
  the Strict product cell. First check: independent descendant and abnormal-death controls.
- **B4 — trusted collection:** GH-587 must supply the approved boundary.
  First check: forged Bun output is rejected and authentic Host evidence accepted.
  Mechanism evidence may proceed before product collection.
- **B5 — installed scope:** direct-installed Legacy remains refused by KEL-254.
  Any change requires its owner's separate amendment; this draft proposes none.
  All criteria remain unrun and production work remains approval-gated.

[S1]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session/boot_contract.rs
[S2]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session.rs
[S3]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/role.rs
[S4]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/lib.rs
[S5]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-host/src/main.rs
[S6]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/windows_job.rs
[S7]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/windows_lpac.rs
[A1]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js
[D1]: https://github.com/gyldlab/keld/issues/1187#issuecomment-6460212928
[D2]: https://github.com/gyldlab/keld/issues/1187#issuecomment-6492238239
