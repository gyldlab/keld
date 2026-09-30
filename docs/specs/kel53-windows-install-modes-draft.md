# Spec: KEL-53 Windows installation modes and activation authority (proposal)
Status: draft
Linear: KEL-270 · Owner: KEL-53 maintainers · Updated: 2026-09-29

This is a proposed replacement/amendment to Windows T4b. It is not approved and is
not an implementation contract. It records the product's three-mode direction, the
shared updater boundary, the Windows evidence collected for a fixed task, and the
remaining decisions that must be resolved before approval. The currently approved
`docs/specs/kel53-full-package-activation.md` remains authoritative until its amended
content and Architecture 06 are approved together.

This multi-mode proposal supersedes the earlier task-only proposal
`kel53-t4b-windows-task-amendment-draft.md`; that file remains historical evidence, not
the current product architecture.

## 1. Goal & non-goals

Provide one Windows updater state machine with installation-mode-specific authority:
the default per-user install updates seamlessly without UAC; a developer may select a
Program Files machine-wide install that asks for UAC on every protected activation or
opts into an installer-provisioned no-repeat-UAC coordinator; package-manager/enterprise
managed installs defer mutation to their owner. Every supported direct mode preserves
signed-package verification, the anti-downgrade floor, activation journal, exact health,
crash recovery and rollback. The application and Bun roles never run elevated or as
SYSTEM. The no-UAC mechanism remains an unresolved security design until its request,
lease and lifecycle gates are proven.

Non-goals: three updater implementations; inferring mode from install path, registry
guess or environment; a general resident broker; bypassing `keld-guard`; allowing
application roles to write updater state; changing KIPC or app permissions for
convenience; automatic mutation of MSIX/store/enterprise-managed installs; macOS/Linux
mode expansion before KEL-137; delta work; or claiming protection from an administrator
or arbitrary native malware already running as the same Windows user.

## 2. Spec refs and current-documentation receipt

- `docs/architecture/03-security.md` §5–6: signed update admission, strict/distinct OS
  principals, hostile-role denial and the explicit exclusion of administrators and
  arbitrary same-user native malware.
- `docs/architecture/06-runtime-and-tooling.md` §4 and approved
  `docs/specs/kel53-full-package-activation.md` T3b/T4a/T4b: common verifier, install
  provenance, journal/floor/current/LKG ordering, health and recovery.
- KEL-135 owns verified publisher/app identity and profile identity; KEL-53 owns
  install provenance, active selection, updater journal and writer; KEL-254 owns
  installed-boot verification/role reads; KEL-96 owns host boot admission. Their
  boundaries must be reconciled before approval.
- Microsoft primary sources, retrieved 2026-09-29: [FOLDERID_LocalAppData and
  per-user Windows Installer context](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid),
  [Windows Installer installation context](https://learn.microsoft.com/en-us/windows/win32/msi/installation-context),
  [RunAs/UAC helper guidance](https://learn.microsoft.com/en-us/windows/win32/secbp/running-with-administrator-privileges),
  [Task Scheduler DACL execution rights](https://learn.microsoft.com/en-us/windows/win32/taskschd/security-contexts-for-running-tasks),
  [service access rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights),
  and [MSIX App Installer auto-update ownership](https://learn.microsoft.com/en-us/windows/msix/app-installer/auto-update-and-repair--overview).

Context7: used `/websites/learn_microsoft_en-us_windows_win32_api` (Task Scheduler
security contexts, `runas` helper separation, service access rights) and
`/microsoftdocs/msix-docs` (App Installer update ownership), retrieved 2026-09-29.
Official Microsoft sources and real Windows observations are used; the Task Scheduler
observation establishes only the tested `schtasks` path and mask, not every Scheduler
API or a product authorization protocol.

## 3. Acceptance criteria (binary, each becomes a test)

1. A trusted installer explicitly records one immutable install mode. Unsupported,
   missing, inconsistent or unowned mode state fails closed; mode is never inferred
   from path or environment. A mode change requires its own trusted transition.
2. All direct modes use one common signed-candidate state machine: strict manifest and
   full-package verification; exact install/app/channel/target binding; monotonic floor;
   immutable staging; journaled publication; attempt-bound candidate health for 30
   monotonic seconds; LKG commit or exact rollback; crash-cut recovery. Authority
   selection may change how a write lease is obtained, never these transitions.
3. **Per-user default:** trusted setup installs beneath that user's `FOLDERID_LocalAppData`
   application tree (for example `%LOCALAPPDATA%\Programs\Keld`); normal updates need
   no UAC. Keld roles cannot write the updater state through their OS restrictions.
   Signed content, feed replay/floor checks, journal, health and rollback remain tested.
   The contract explicitly does not promise tamper resistance against the owning user or
   arbitrary same-user native malware; the owner can modify a user-owned installation.
4. **Machine-wide/UAC:** installation selects a protected Program Files-style root and
   `MachineUac` authority. Its installer-provisioned protection profile grants writes to
   Administrators and SYSTEM, while ordinary users and Keld roles have read/execute
   access only. This is distinct from KEL-266's SYSTEM-owned profile: an elevated UAC
   process runs as an administrator, not as SYSTEM, and MUST NOT take ownership or
   repair ACLs ad hoc. A standard-user update may stage/verify unprivileged, then a fixed
   signed updater helper is started through UAC for each protected activation.
   Declined/cancelled consent performs no protected write. The elevated helper alone
   mutates machine state and remains the authorized attempt owner through exact
   candidate health/rollback while alive. If that owner exits or the machine reboots
   before commit/rollback, recovery requires fresh UAC consent to resume the journaled
   common recovery state machine; until then it halts with the attempt unresolved and
   makes no claim that the old selection is active. It MUST NOT silently resume elevated
   work. The
   helper never launches the application or a Bun role elevated/SYSTEM. The candidate
   token/session/integrity are observed as the initiating ordinary user, including
   over-the-shoulder UAC where the approving administrator differs from that user.
   One consent covers health/commit/rollback for that live attempt; this is not a promise
   that a later recovery after owner death needs no further consent.
5. **Machine-wide/seamless:** trusted installation explicitly provisions one fixed,
   protected, narrowly privileged coordinator mechanism. The user may request a wake-up
   but cannot change its executable/configuration, authority or security descriptor.
   Wake-up is not update authorization: a bare/repeated request has zero protected
   effects. Before journal mutation, the coordinator authenticates the exact
   installation, host and fresh attempt; it serializes against every other writer and
   follows the common health and rollback state machine. The application and roles
   remain non-SYSTEM/non-elevated. The product contract is mechanism-neutral: a fixed
   Task Scheduler task is the leading tested candidate, while service or another
   Windows-native mechanism remains eligible only if it meets the same narrow authority,
   authentication, replay, writer-ownership and lifecycle contracts with less risk or
   complexity. No mechanism is approved until the remaining falsifiers pass.
6. **Managed:** if trusted install provenance says MSIX/App Installer, Store, enterprise
   deployment or another package-manager owner, `keld-update` refuses before direct
   feed, staging or protected mutation and reports the owning mechanism. It never
   races the package manager.
7. Before publication, negative controls (wrong install/publisher/app/channel/target,
   stale floor, wrong signer, corrupted package, replay, hostile role, task/helper
   modification, UAC denial, lease conflict and unknown/live predecessor) leave
   protected bytes byte-identical. After journal publication, recovery follows the
   recorded phase: resume a valid `PublishPending`; after durable `HealthAccepted`,
   finish LKG publication and commit; after failed health, roll back to the exact prior
   current selection only when its proof is valid; otherwise halt for recovery while
   preserving the monotonic floor and without claiming the old bytes are unchanged.
   Corrupt/missing LKG and every crash cut after floor/current publication must have
   explicit negative controls and a typed fail-closed outcome where required by the
   approved phase contract.
8. Exact-source Windows receipts bind mode, OS/build, installer provenance, task/helper
   DACL/action where present, current token and process family, protected state, command
   transcript, candidate identity/health, crash cuts and cleanup. A worker-local result,
   state-model test or task-run success alone cannot close product acceptance.

## 4.1 KEL-270 disposition: common work and mode-specific cells

KEL-270's verified-candidate and activation work remains common: signed full-package
verification; install/app/channel/target binding; monotonic anti-downgrade floor;
immutable staging; journaled current/LKG publication; single-writer exclusion; exact
candidate health; commit/rollback; and crash-cut recovery. These transitions must have
one owner and identical state semantics across all direct Windows modes. A mode adapter
may provide authority and launch capabilities but cannot redefine trust, commit or
rollback.

Acceptance must split where authority differs: immutable installer-recorded mode and
owner provenance; root/state ACL profile and bootstrap; how the writer lease is granted;
how the candidate is created under the ordinary initiating user's token; what consent or
privileged coordinator authorizes mutation; how the authorized owner survives or recovers
the attempt; and how package-managed ownership refuses direct mutation. Per-user,
Machine-UAC, Machine-seamless and Managed(owner) each need separate OS-level acceptance
cells for those boundaries, while sharing the common state-machine vectors. KEL-270's
SYSTEM task probe is evidence only for the seamless candidate's wake-up/process-launch
feasibility; it does not close any of these common or mode-specific product cells.

## 4. Design

### Atomic facts and shared core

| Atom / owner | Boundary and inputs → output | Failure mode | Observable contract / first falsifier |
|---|---|---|---|
| Install mode / trusted installer + KEL-53 | Trusted install choice and package owner → immutable `PerUserDirect`, `MachineUacDirect`, `MachineSeamlessDirect`, or `Managed(owner)` | path/registry guess selects authority or mode changes silently | substitute path/environment/owner; refuse before feed/write |
| Candidate verification and activation / `keld-update` | signed feed/package + current mode/provenance → one exact journaled attempt and commit/rollback | mode changes trust/ordering or duplicates the updater | run same artifact/state vectors through each authority; exact common transition trace |
| Writer authority / mode adapter + updater | install mode + one authenticated attempt → one temporary writer lease | ordinary role writes, two writers, or UAC/task start is mistaken for auth | wrong caller and competing writer produce zero journal/pointer changes |
| Candidate/process health / host/runtime | attempt + user token/session + private endpoint → exact ordinary candidate health or rollback | app runs elevated/SYSTEM, wrong session, stale health, live predecessor | inspect token/session/image/family and reject substituted/early-exit candidate |
| Managed ownership / installer/package manager | trusted managed-owner provenance → defer to owner | Keld direct updater races manager | managed install returns typed refusal before direct mutation |

Changing install mode does not silently change the candidate/journal algorithm. The
state-machine owner remains `keld-update`; the installation-mode authority boundary is
separate and may not be duplicated into three updaters. The code-level interface is not
specified here; reuse existing package verifier, local-record codec, guard-owned
Windows filesystem policy, `keld-ipc` transport owner and runtime candidate/job owner
before adding a new abstraction. There is no T4b activation-journal implementation in
`records.rs` yet; extend the owned strict local-record machinery only after the journal
contract is approved.

### Mode matrix and recommendation

| Mode | Install location/owner | Update authority | UAC | Product role |
|---|---|---|---|---|
| `PerUserDirect` (default) | user-owned `FOLDERID_LocalAppData` tree | same user's updater | no | simplest, common Keld experience |
| `MachineUacDirect` | Program Files-style root writable by elevated Administrators/SYSTEM; ordinary users read/execute | one explicit, signed `runas` helper per update | yes | dependable machine-wide fallback |
| `MachineSeamlessDirect` | SYSTEM-protected machine root plus trusted installed coordinator | narrowly scoped one-shot coordinator | initial install only | opt-in advanced machine-wide mode; mechanism still unapproved |
| `Managed(owner)` | package-manager-owned | Store/App Installer/enterprise owner | owner-defined | Keld updater refuses direct mutation |

Recommend per-user as the default. Recommend UAC as the default for a machine-wide
install; seamless mode is an explicit install-time opt-in after its additional security
acceptance passes. This avoids silently adding a privileged coordinator to existing
Program Files installs. Existing untagged machine installs use no inferred mode: a
trusted migration records `MachineUacDirect` only after verifying its distinct
Administrators/SYSTEM-writable protection profile; otherwise updates refuse until a
trusted installer records and verifies the mode. Managed installs never become direct
by path inference.

The Windows task evidence supports a fixed task as the current seamless candidate, not
as an approved implementation. On Windows 11 Home Single Language x64 build 26200,
`0x1200A9` read+execute let the non-elevated session run the fixed task while
`/Change /Disable` was denied; the coordinator ran as SYSTEM/session 0 and launched a
same-session medium-integrity candidate with a duplicated event handle. Execute-only
`0x20` was insufficient for the tested `schtasks` path. The tested task is currently
the smallest evaluated wake-up surface: one fixed action can be started by the ordinary
user, while task configuration remains protected. A service adds a service object and
control/installation surface; its DACL could separate start from reconfiguration, but
that alone would not authenticate a caller or attempt. A fixed elevated helper invoked
directly cannot satisfy seamless updates without a separate privileged launcher, so it
is useful for explicit-UAC mode but is not a complete no-UAC mechanism. These are
engineering comparisons, not proof that the task is secure end to end. The task remains
the leading candidate because it is the only one with a real Windows start-vs-change
observation so far; no implementation mechanism is selected until all identity,
replay, lease, lifecycle and recovery atoms pass. An on-demand service remains a fallback
candidate if the task cannot satisfy a named contract; no evidence here assumes a
service must be resident. An installer-provisioned one-shot helper remains the UAC-mode
boundary unless a different approved design is required.

The explicit-UAC path uses a separate signed elevation helper (Windows `runas`/UAC),
not an elevated application process. Consent owns one live activation attempt while the
elevated helper remains alive. A crash/reboot after that helper disappears requires a
new consent or a safe halt, and an over-the-shoulder administrator must launch the
candidate under the initiating user's ordinary token/session. These are acceptance
requirements, not yet proven here.

### Seamless-machine authentication and lifecycle gate

Task `/Run` permission is a wake-up, never update authorization. The coordinator must
prove the exact Keld host, installation and fresh attempt before protected preparation.
It must not trust user SID/session, PID/path alone, argv, environment or a generic pipe
frame. A proposed line for continued design is host-owned IPC to the short-lived SYSTEM
coordinator using a kernel-observed pipe peer identity plus verified active-image and
installation provenance, followed by explicit one-use, attempt-bound endpoint transfer.
This mechanism is a hypothesis only; it must be adversarially tested against a same-user
wrong host, wrong install, role process, stale/replayed request and a fake named-pipe
server before it becomes contract.

The writer lease must be installation-wide, not Task Scheduler `IgnoreNew`. Define how
retained immutable ancestry pins and mutable journal/pointer readers yield without a
replacement race. Bind the coordinator and candidate to an authenticated process-family
owner. Recovery must distinguish dead, live and unknown predecessors after handles or
the original coordinator disappear; unknown/live state refuses. Every transferred
endpoint must be authenticated, consumed once, closed/revoked and incapable of later
replay. Bare/repeated task wake-ups must terminate without accumulating privileged
workers or interfering with an admitted attempt.

### KEL-266 disposition and compatibility

KEL-266 remains the SYSTEM-owned machine-wide protected-baseline/provenance bootstrap;
do not turn its one-shot SYSTEM initializer into the universal updater or require SYSTEM
for per-user installs. Reuse its verified baseline/provenance primitives. Per-user
setup uses a user-owned bootstrap but the same package/signature/state validators.
Machine-UAC installation needs its own installer-provisioned protection profile that
admits elevated Administrators and SYSTEM as writers and ordinary users as read/execute;
it does not inherit KEL-266's SYSTEM-only writer assumption. Seamless machine mode may
add a separately gated coordinator-provisioning step to the trusted installer after
explicit opt-in; it does not change the default. UAC machine mode provisions no
persistent task/service. Any change to KEL-266 task provisioning or existing machine
install defaults requires its own acceptance and migration decision.

Compatibility fallback: a refusal before protected publication (unsupported mode,
unclassified legacy installation, invalid managed owner, failed task/helper signature
or ACL, declined UAC, failed authentication, or invalid lease) leaves protected bytes
unchanged and reports a typed actionable refusal. After journaled publication,
unknown/live process ownership, failed health or failed journal readback follows the
approved recovery transitions; if required proof is unavailable, it halts for explicit
recovery without claiming that the prior selection is active or that rollback completed.
The updater never silently changes an existing install's mode.

## 5. Boundaries

Implement only after approval and ownership refresh:

- `keld-update`: common verification, floor, package, journal, state machine, health
  decision and rollback; not three independent mode-specific updaters.
- Existing host/runtime/guard/IPC owners: mode admission, process identity, role
  rejection, user-token candidate launch and private health channel.
- Trusted installer: owner-recorded mode; machine baseline remains KEL-266; task only
  for explicit seamless-machine opt-in.
- Managed package owner: MSIX/App Installer/enterprise updates and no competing Keld
  mutation.

Must not add a general service, broaden app permissions, let Bun/webviews write update
state, infer mode from filesystem path, change KIPC/manifest wire without its own spec,
or expand this Windows proposal into macOS/Linux activation. Use existing abstractions;
if an owner abstraction cannot meet the acceptance contract, name the exact unmet
requirement before proposing the smallest extension.

### Proposed synchronization to the approved activation spec and Architecture 06

After boundary approval, amend `docs/specs/kel53-full-package-activation.md` so T4b
defines the shared journaled activation state machine and four installation-owner
classes: per-user direct, machine-wide explicit-UAC direct, machine-wide seamless direct
(authority mechanism gated separately), and package/deployment-managed defer. T4b MUST
leave the no-UAC coordinator mechanism unresolved until the identity, attempt, replay,
writer-lease, process-family, health and crash-recovery proof is accepted. KEL-266's
SYSTEM-owned baseline profile remains a predecessor; it is not implicitly the ACL or
writer contract for UAC mode.

Synchronize Architecture 06 §4 in the same approved change: retain the common updater
sequence (verify, journal, acquire exclusive activation ownership, launch the exact
ordinary-user candidate, confirm health, commit/rollback, recover), then describe the
install-mode authority boundary and managed-owner defer. Name the default as per-user,
Machine-UAC as the explicit machine-wide fallback, and seamless machine-wide as opt-in
with an unselected privileged mechanism. Do not describe SYSTEM task execution as the
universal or default Windows activation path. Keep KEL-135 (publisher/app/profile
identity), KEL-53 (mode provenance, journal and active selection), KEL-254 (installed
image verification and role reads), and KEL-96 (host boot admission) as explicit owners;
reconcile any overlapping boot/read contract before approval.

These paragraphs propose the synchronized diff; they do not amend either approved
document until the named owners approve one exact cross-document change.

## 6. Tasks (ordered, no placeholders)

- [ ] T1 — approve this exact multi-mode spec and sync Architecture 06; record KEL-53,
  KEL-135, KEL-254 and KEL-96 ownership and any predecessor blocks.
- [ ] T2 — add immutable install-mode provenance and classify the default per-user,
  machine UAC, machine seamless opt-in and managed cells; define migration of existing
  untagged machine installs.
- [ ] T3 — complete the common signed full-package verifier/state-machine/journal owner
  without mode-specific forks; retain current crash-cut and monotonic-floor rules.
- [ ] T4 — implement per-user installation/bootstrap and no-UAC update cell; prove
  hostile Keld roles cannot mutate user-owned state, disclose same-user malware boundary.
- [ ] T5 — implement explicit-UAC machine activation; prove UAC cancel has zero protected
  writes, helper alone is elevated, candidate stays ordinary, and one consent covers
  health/commit/rollback.
- [ ] T6 — only after exact host/attempt auth, writer lease, process-family, health and
  replay/concurrency contracts are approved, implement the seamless-machine authority
  adapter; no task/service code before that gate.
- [ ] T7 — exercise Store/App Installer/enterprise managed refusal and direct-mode
  isolation; run real Windows crash cuts for each supported mode.
- [ ] T8 — reconcile KEL-53 parent acceptance, hosted gates and independent security/
  architecture review. Keep all unsupported OS/mode rows explicitly unclaimed.

## 7. Test plan

| Mode/atom | Direct proof and negative control |
|---|---|
| Per-user install | clean standard-user install beneath KnownFolder local path; no UAC; signed feed/floor/journal/health/rollback; hostile Bun/webview denied; same-user native tamper explicitly out of scope |
| Machine UAC | installer ACL profile admits elevated Administrators/SYSTEM only; UAC appears only at protected activation; deny/cancel produces zero writes; elevated helper signature/argv fixed; app/candidate token is initiating ordinary user, including over-the-shoulder approval; exact health and rollback complete under the live owner consent; owner death/reboot triggers fresh consent or safe halt |
| Machine seamless coordinator | chosen mechanism's installed image/configuration/authority/security descriptor read-back; trigger can wake but cannot reconfigure or authorize an update; bare/repeated wake-up produces zero protected writes. If Task Scheduler is selected, also prove `schtasks /Change` denied and `/Run` succeeds as standard user on the supported Windows matrix |
| Machine seamless identity | wrong host/image/install/role/fake pipe, stale attempt and replay fail before journal; exact host positive; capture actual token/PID/image/installation identity |
| Common updater state | signature/context substitutions, anti-downgrade, every journal crash cut, corrupt/mixed pointer/floor, failed health, exact rollback and next valid attempt in each mode |
| Lifecycle/lease | competing tasks/writers, retained read pins, task stop/logoff, live candidate descendant, unknown coordinator and repeated wake-up refuse unsafe recovery |
| Managed ownership | MSIX/App Installer/enterprise provenance returns typed refusal before Keld feed/stage/write; manager remains sole mutation owner |

Real Windows receipts must include OS/build, mode/provenance, source hashes, token and
process-family observations, UAC/task transcript, signed artifact identities, protected
state read-backs, health timeline and cleanup. Mocks or a mode adapter pass cannot close
the real OS row.

## 8. Review gates triggered

`permission model`: yes. `unsafe`: conditional on final native diff, exact-scope
independent review. `public API` and internal dependency edge: conditional on the chosen
shared mode/authority API. `dependency addition`: none currently proposed. `wire
protocol`: none currently proposed; any authenticated host/coordinator channel change
must be versioned by its owning protocol spec. These gates remain open until the
concrete cross-crate diff is reviewed.

## 9. Performance impact

No performance claim. Measure update start latency and idle resource cost only after
all mode correctness/authority contracts pass. The per-user path should not carry a
privileged resident process. Seamless coordinator start frequency must be bounded.

## 10. Open questions / owner approval

1. Can the UAC helper retain the one elevated attempt across ordinary candidate launch,
   exact 30-second health, commit and rollback without elevating the app or prompting a
   second time while the helper remains alive? No direct Windows Keld run has proved
   this. Recovery after owner death/reboot is explicitly fresh-consent-or-halt.
2. How does the seamless coordinator authenticate the exact active Keld host,
   installation, role and one-use attempt after independent Task Scheduler launch?
   No exact-host proof exists.
3. How do mutable state readers yield to one writer and how is full process-family death
   established after owner handles disappear?
4. What is the trusted installer owner and migration rule for current Program Files
   installations that have no explicit update-mode provenance?
5. KEL-135/KEL-254/KEL-96 must reconcile publisher/app identity, installed current-image
   verification, role reads and host admission before installed boot/update admission is
   implemented in each direct mode, including per-user and Machine-UAC modes.
6. Approve the exact amended Windows mode contract and Architecture 06 synchronization
   before product implementation. A task DACL pass is not approval of the coordinator
   authentication protocol.

Until those decisions and proofs are resolved, this remains a draft; production update
behavior stays fail-closed.
